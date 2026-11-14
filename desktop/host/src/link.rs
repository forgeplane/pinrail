//! The core's link to one host: a framed connection that it starts or
//! adopts, keeps up with backoff, and multiplexes calls, events and
//! terminal channels over.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::net::UnixStream;
use tokio::sync::{Notify, broadcast, mpsc, oneshot, watch};

use crate::frame::{self, CONTROL_CHANNEL, Frame, kind};
use crate::launch::Launcher;
use crate::registration::Registration;

/// The delays between attempts to reconnect, each with up to 20 % jitter;
/// the last repeats.
const BACKOFF: [u64; 7] = [1, 2, 4, 8, 16, 30, 60];
/// Silence after which the connection counts as lost: the host pings every
/// 15 seconds.
const SILENCE: Duration = Duration::from_secs(45);
/// How long the hello may take.
const HELLO_WITHIN: Duration = Duration::from_secs(10);

/// Where the link finds its host.
#[derive(Clone)]
pub enum Connector {
    /// Adopt the running host of this boot, or start one.
    Launch(Arc<Launcher>),
    /// A host someone else started, such as a test.
    Socket { socket: PathBuf, secret: String },
}

/// What the link knows of its host.
#[derive(Debug, Clone, PartialEq)]
pub enum HostState {
    Connecting,
    Ready {
        /// The host instance's id; events are numbered within it.
        instance: String,
    },
    Reconnecting {
        attempt: u32,
        /// Seconds until the next attempt.
        next_in: f64,
        reason: String,
    },
    /// The host speaks another generation of the protocol.
    Incompatible {
        reason: String,
    },
    /// The link was stopped.
    Offline,
}

/// Why a call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// The host is not connected; nothing was sent.
    Offline,
    /// The connection dropped after the call was sent, so whether it took
    /// effect cannot be known. A mutation must not be retried by itself.
    Unknown,
    /// The host answered with an error.
    Refused { code: String, message: String },
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Offline => write!(f, "the terminal host is not connected"),
            HostError::Unknown => write!(f, "the connection to the terminal host dropped"),
            HostError::Refused { code, message } => write!(f, "{code}: {message}"),
        }
    }
}

impl std::error::Error for HostError {}

/// What the link reports about its host.
#[derive(Debug, Clone)]
pub enum HostEvent {
    /// Connected and subscribed: the host's instance and every terminal it
    /// lists, to reconcile with.
    Connected { instance: String, terminals: Value },
    /// An event from the host, such as `exited` or `title`.
    Event { method: String, params: Value },
    /// The connection dropped; calls in flight ended as `Unknown`.
    Disconnected { reason: String },
}

/// A terminal channel opened by [`HostLink::attach`].
pub struct Channel {
    pub channel: u32,
    /// The snapshot format the host chose.
    pub format: String,
    /// The channel's frames, in order; ends when the channel or the
    /// connection closes.
    pub frames: mpsc::UnboundedReceiver<Frame>,
    link: HostLink,
}

impl Channel {
    /// Sends a frame on the channel: input, credit, a resize and so on.
    pub fn send(&self, kind: u8, payload: Vec<u8>) -> Result<(), HostError> {
        self.link.send(Frame {
            channel: self.channel,
            kind,
            payload,
        })
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = self.send(kind::CLOSE, b"{}".to_vec());
        self.link
            .inner
            .channels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.channel);
    }
}

type Reply = oneshot::Sender<Result<Value, HostError>>;

/// What the reader does with a reply as it reads it, before any frame
/// after it.
type OnReply = Box<dyn FnOnce(&Value) + Send>;

struct Pending {
    reply: Reply,
    on_reply: Option<OnReply>,
    /// For an attach: where the channel's frames go, registered as soon as
    /// the reply is read, before any frame of the channel.
    channel: Option<mpsc::UnboundedSender<Frame>>,
}

struct Inner {
    client: String,
    connector: Connector,
    state: watch::Sender<HostState>,
    events: broadcast::Sender<HostEvent>,
    writer: Mutex<Option<mpsc::UnboundedSender<Frame>>>,
    pending: Mutex<HashMap<u64, Pending>>,
    channels: Mutex<HashMap<u32, mpsc::UnboundedSender<Frame>>>,
    next_id: AtomicU64,
    stop: Notify,
}

/// The link to one host. Cheap to clone.
#[derive(Clone)]
pub struct HostLink {
    inner: Arc<Inner>,
}

impl HostLink {
    /// Starts linking to the host in the background. `client` names the
    /// core, such as "pinrail-core/0.6.0". Must be called in a runtime.
    pub fn start(connector: Connector, client: &str) -> Self {
        let (state, _) = watch::channel(HostState::Connecting);
        let (events, _) = broadcast::channel(1024);
        let link = Self {
            inner: Arc::new(Inner {
                client: client.to_string(),
                connector,
                state,
                events,
                writer: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                channels: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                stop: Notify::new(),
            }),
        };
        tokio::spawn(link.clone().run());
        link
    }

    pub fn state(&self) -> HostState {
        self.inner.state.borrow().clone()
    }

    /// Follows the state as it changes.
    pub fn watch(&self) -> watch::Receiver<HostState> {
        self.inner.state.subscribe()
    }

    pub fn events(&self) -> broadcast::Receiver<HostEvent> {
        self.inner.events.subscribe()
    }

    /// Disconnects and stops reconnecting. The host and its terminals keep
    /// running.
    pub fn stop(&self) {
        self.inner.stop.notify_one();
    }

    /// Calls a method and waits for its result.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, HostError> {
        self.request(method, params, None).await
    }

    /// Opens a terminal channel with `terminal.attach`.
    pub async fn attach(&self, params: Value) -> Result<Channel, HostError> {
        let (sink, frames) = mpsc::unbounded_channel();
        let result = self.request("terminal.attach", params, Some(sink)).await?;
        let channel = result["channel"].as_u64().unwrap_or_default() as u32;
        Ok(Channel {
            channel,
            format: result["format"].as_str().unwrap_or_default().to_string(),
            frames,
            link: self.clone(),
        })
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        channel: Option<mpsc::UnboundedSender<Frame>>,
    ) -> Result<Value, HostError> {
        self.request_with(method, params, channel, None).await
    }

    async fn request_with(
        &self,
        method: &str,
        params: Value,
        channel: Option<mpsc::UnboundedSender<Frame>>,
        on_reply: Option<OnReply>,
    ) -> Result<Value, HostError> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(
                id,
                Pending {
                    reply,
                    on_reply,
                    channel,
                },
            );
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(error) = self.send(Frame::json(&message)) {
            self.inner
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            return Err(error);
        }
        answer.await.unwrap_or(Err(HostError::Unknown))
    }

    fn send(&self, frame: Frame) -> Result<(), HostError> {
        let writer = self.inner.writer.lock().unwrap_or_else(|e| e.into_inner());
        match writer.as_ref() {
            Some(writer) if writer.send(frame).is_ok() => Ok(()),
            _ => Err(HostError::Offline),
        }
    }

    /// Connects, serves the connection until it drops, and reconnects with
    /// backoff, until stopped.
    async fn run(self) {
        let mut attempt = 0u32;
        let mut resume: Option<(String, u64)> = None;
        loop {
            let reason = tokio::select! {
                () = self.inner.stop.notified() => break,
                outcome = self.connect_once(&mut resume) => match outcome {
                    // a connection that was ready starts the delays again
                    Ok(reason) => {
                        attempt = 0;
                        reason
                    }
                    Err(Fatal(reason)) => {
                        self.inner.state.send_replace(HostState::Incompatible { reason });
                        // an incompatible host is not retried until the
                        // link starts again
                        self.inner.stop.notified().await;
                        break;
                    }
                    Err(Retry(reason)) => reason,
                },
            };
            self.disconnected(&reason);
            let delay = backoff(attempt);
            attempt += 1;
            self.inner.state.send_replace(HostState::Reconnecting {
                attempt,
                next_in: delay.as_secs_f64(),
                reason,
            });
            tokio::select! {
                () = self.inner.stop.notified() => break,
                () = tokio::time::sleep(delay) => {}
            }
        }
        self.disconnected("the link stopped");
        self.inner.state.send_replace(HostState::Offline);
    }

    /// Ends what was in flight on a connection that dropped.
    fn disconnected(&self, reason: &str) {
        *self.inner.writer.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let pending: Vec<Pending> = self
            .inner
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
            .map(|(_, p)| p)
            .collect();
        for pending in pending {
            let _ = pending.reply.send(Err(HostError::Unknown));
        }
        self.inner
            .channels
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        let _ = self.inner.events.send(HostEvent::Disconnected {
            reason: reason.to_string(),
        });
    }

    /// One connection, from finding the host to the connection's end.
    /// Returns why it ended: `Ok` when it had become ready, `Err` when it
    /// never did.
    async fn connect_once(&self, resume: &mut Option<(String, u64)>) -> Result<String, Failure> {
        self.inner.state.send_replace(HostState::Connecting);
        let (socket, secret) = match &self.inner.connector {
            Connector::Socket { socket, secret } => (socket.clone(), secret.clone()),
            Connector::Launch(launcher) => {
                let launcher = launcher.clone();
                let registration: Registration = tokio::task::spawn_blocking(move || {
                    launcher
                        .ensure(&|_| {})
                        .map(|(registration, _)| registration)
                })
                .await
                .map_err(|e| Retry(e.to_string()))?
                .map_err(|e| Retry(e.to_string()))?;
                (registration.socket, registration.secret)
            }
        };
        let stream = UnixStream::connect(&socket)
            .await
            .map_err(|e| Retry(format!("{}: {e}", socket.display())))?;
        let (mut reader, mut writer) = stream.into_split();
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello", "params": {
            "secret": secret,
            "client": self.inner.client,
            "generation": crate::GENERATION,
            "features": [],
        }});
        frame::write(&mut writer, &Frame::json(&hello))
            .await
            .map_err(|e| Retry(e.to_string()))?;
        let answer = tokio::time::timeout(HELLO_WITHIN, frame::read(&mut reader))
            .await
            .map_err(|_| Retry("the host did not answer hello".to_string()))?
            .map_err(|e| Retry(e.to_string()))?
            .ok_or_else(|| Retry("the host refused the hello".to_string()))?;
        let hello: Value =
            serde_json::from_slice(&answer.payload).map_err(|e| Retry(e.to_string()))?;
        let generation = hello["result"]["generation"].as_u64();
        if generation != Some(u64::from(crate::GENERATION)) {
            return Err(Fatal(format!(
                "the host speaks generation {generation:?}, this app {}",
                crate::GENERATION
            )));
        }
        let instance = hello["result"]["instance"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let (queue, mut outgoing) = mpsc::unbounded_channel::<Frame>();
        *self.inner.writer.lock().unwrap_or_else(|e| e.into_inner()) = Some(queue);
        let write = tokio::spawn(async move {
            while let Some(frame) = outgoing.recv().await {
                if frame::write(&mut writer, &frame).await.is_err() {
                    break;
                }
            }
        });

        // subscribe, resuming when the host is the same instance
        let after = resume
            .as_ref()
            .filter(|(i, _)| *i == instance)
            .map(|(_, seq)| *seq);
        // announced by the reader as it reads the reply, so before the
        // events the host replays after it
        let link = self.clone();
        let ready_instance = instance.clone();
        let announce: OnReply = Box::new(move |snapshot: &Value| {
            link.inner.state.send_replace(HostState::Ready {
                instance: ready_instance.clone(),
            });
            let _ = link.inner.events.send(HostEvent::Connected {
                instance: ready_instance,
                terminals: snapshot["terminals"].clone(),
            });
        });
        let subscribe = self.request_with(
            "events.subscribe",
            json!({"after": after, "instance": instance}),
            None,
            Some(announce),
        );
        let link = self.clone();
        let instance_for_events = instance.clone();
        let read = async move {
            let mut last_seq = after.unwrap_or(0);
            loop {
                let frame = match tokio::time::timeout(SILENCE, frame::read(&mut reader)).await {
                    Err(_) => return (last_seq, "the host went silent".to_string()),
                    Ok(Ok(Some(frame))) => frame,
                    Ok(Ok(None)) => {
                        return (last_seq, "the host closed the connection".to_string());
                    }
                    Ok(Err(error)) => return (last_seq, error.to_string()),
                };
                if frame.channel != CONTROL_CHANNEL {
                    link.channel_frame(frame);
                    continue;
                }
                if frame.kind == kind::CLOSE {
                    let reason = String::from_utf8_lossy(&frame.payload).into_owned();
                    return (
                        last_seq,
                        format!("the host closed the connection: {reason}"),
                    );
                }
                let Ok(message) = serde_json::from_slice::<Value>(&frame.payload) else {
                    continue;
                };
                if let Some(seq) = message["params"]["seq"].as_u64() {
                    last_seq = seq;
                }
                link.control_message(message, &instance_for_events);
            }
        };
        let subscribed = async {
            let _ = subscribe.await;
            std::future::pending::<()>().await
        };
        let (last_seq, reason) = tokio::select! {
            ended = read => ended,
            () = subscribed => unreachable!(),
        };
        write.abort();
        *resume = Some((instance, last_seq));
        if matches!(*self.inner.state.borrow(), HostState::Ready { .. }) {
            return Ok(reason);
        }
        Err(Retry(reason))
    }

    /// A reply or a notification on channel 0.
    fn control_message(&self, message: Value, instance: &str) {
        if let Some(id) = message["id"].as_u64() {
            let Some(pending) = self
                .inner
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id)
            else {
                return;
            };
            let outcome = match message.get("error") {
                Some(error) => Err(HostError::Refused {
                    code: error["data"]["code"].as_str().unwrap_or("").to_string(),
                    message: error["message"].as_str().unwrap_or("").to_string(),
                }),
                None => Ok(message["result"].clone()),
            };
            if let (Ok(result), Some(on_reply)) = (&outcome, pending.on_reply) {
                on_reply(result);
            }
            if let (Ok(result), Some(sink)) = (&outcome, pending.channel)
                && let Some(channel) = result["channel"].as_u64()
            {
                self.inner
                    .channels
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(channel as u32, sink);
            }
            let _ = pending.reply.send(outcome);
            return;
        }
        let method = message["method"].as_str().unwrap_or("").to_string();
        match method.as_str() {
            "ping" => {}
            // too many events went unread: subscribe again for a snapshot
            "missed" => {
                let link = self.clone();
                let instance = instance.to_string();
                tokio::spawn(async move {
                    if let Ok(snapshot) = link
                        .call("events.subscribe", json!({"instance": instance}))
                        .await
                    {
                        let _ = link.inner.events.send(HostEvent::Connected {
                            instance,
                            terminals: snapshot["terminals"].clone(),
                        });
                    }
                });
            }
            _ => {
                let _ = self.inner.events.send(HostEvent::Event {
                    method,
                    params: message["params"].clone(),
                });
            }
        }
    }

    fn channel_frame(&self, frame: Frame) {
        let mut channels = self
            .inner
            .channels
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let closing = frame.kind == kind::CLOSE;
        let channel = frame.channel;
        if let Some(sink) = channels.get(&channel)
            && (sink.send(frame).is_err() || closing)
        {
            channels.remove(&channel);
        }
    }
}

/// Why a connection ended.
enum Failure {
    /// Try again after a while.
    Retry(String),
    /// Trying again cannot help.
    Fatal(String),
}
use Failure::{Fatal, Retry};

/// The delay before attempt `attempt`, with up to 20 % jitter either way.
fn backoff(attempt: u32) -> Duration {
    let base = BACKOFF[(attempt as usize).min(BACKOFF.len() - 1)] as f64;
    let mut byte = [0u8; 1];
    getrandom::fill(&mut byte).expect("the operating system's random numbers");
    let jitter = (f64::from(byte[0]) / 255.0 - 0.5) * 0.4;
    Duration::from_secs_f64(base * (1.0 + jitter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_grows_to_a_minute_with_jitter() {
        for (attempt, base) in [(0, 1.0), (3, 8.0), (6, 60.0), (40, 60.0)] {
            let delay = backoff(attempt).as_secs_f64();
            assert!(
                (base * 0.8..=base * 1.2).contains(&delay),
                "{attempt}: {delay}"
            );
        }
    }
}
