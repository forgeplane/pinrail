//! A blocking controller connection, for `status` and `stop`.

use std::fmt;
use std::io;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{Value, json};

use crate::frame::{self, CONTROL_CHANNEL, Frame, kind};
use crate::registration::Registration;

pub struct Client {
    stream: UnixStream,
    next_id: u64,
    /// Notifications that arrived while a call waited for its result.
    notifications: std::collections::VecDeque<Value>,
    /// What the host answered to `hello`.
    pub hello: Value,
}

#[derive(Debug)]
pub enum CallError {
    Io(io::Error),
    /// The host closed the connection, with its reason if it gave one.
    Closed(Option<Value>),
    /// The host answered with a JSON-RPC error.
    Rpc(Value),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::Io(error) => write!(f, "{error}"),
            CallError::Closed(Some(reason)) => {
                write!(f, "the host closed the connection: {reason}")
            }
            CallError::Closed(None) => write!(f, "the host closed the connection"),
            CallError::Rpc(error) => write!(f, "the host answered {error}"),
        }
    }
}

impl std::error::Error for CallError {}

impl From<io::Error> for CallError {
    fn from(error: io::Error) -> Self {
        CallError::Io(error)
    }
}

impl Client {
    /// Connects to the host a registration names, and says `hello`.
    pub fn connect(registration: &Registration) -> Result<Self, CallError> {
        let stream = UnixStream::connect(&registration.socket)?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        let mut client = Self {
            stream,
            next_id: 0,
            notifications: std::collections::VecDeque::new(),
            hello: Value::Null,
        };
        client.hello = client.call(
            "hello",
            json!({
                "secret": registration.secret,
                "client": format!("pinrail-host-cli/{}", env!("CARGO_PKG_VERSION")),
                "generation": crate::GENERATION,
                "features": [],
            }),
        )?;
        Ok(client)
    }

    /// Calls a method and waits for its result, skipping notifications.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, CallError> {
        self.next_id += 1;
        let id = self.next_id;
        frame::write_blocking(
            &mut self.stream,
            &Frame::json(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})),
        )?;
        loop {
            let Some(frame) = frame::read_blocking(&mut self.stream)? else {
                return Err(CallError::Closed(None));
            };
            if frame.channel != CONTROL_CHANNEL {
                continue;
            }
            if frame.kind == kind::CLOSE {
                return Err(CallError::Closed(
                    serde_json::from_slice(&frame.payload).ok(),
                ));
            }
            let Ok(message) = serde_json::from_slice::<Value>(&frame.payload) else {
                continue;
            };
            if message.get("id").is_none() {
                self.notifications.push_back(message);
                continue;
            }
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(CallError::Rpc(error.clone()));
            }
            return Ok(message.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// The next notification, waiting up to `within` for it.
    pub fn notification(&mut self, within: Duration) -> Option<Value> {
        if let Some(message) = self.notifications.pop_front() {
            return Some(message);
        }
        self.stream.set_read_timeout(Some(within)).ok()?;
        let found = loop {
            let Ok(Some(frame)) = frame::read_blocking(&mut self.stream) else {
                break None;
            };
            if frame.channel != CONTROL_CHANNEL || frame.kind != kind::DATA {
                continue;
            }
            if let Ok(message) = serde_json::from_slice::<Value>(&frame.payload)
                && message.get("id").is_none()
            {
                break Some(message);
            }
        };
        let _ = self.stream.set_read_timeout(Some(Duration::from_secs(15)));
        found
    }

    /// Waits until the host closes the connection, and returns its
    /// reason if it gave one.
    pub fn wait_closed(mut self) -> Option<Value> {
        while let Ok(Some(frame)) = frame::read_blocking(&mut self.stream) {
            if frame.channel == CONTROL_CHANNEL && frame.kind == kind::CLOSE {
                return serde_json::from_slice(&frame.payload).ok();
            }
        }
        None
    }
}
