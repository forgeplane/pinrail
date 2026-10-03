//! The HTTP client. Every call returns the decoded JSON body; a 4xx/5xx
//! becomes an [`ApiError`] carrying that body, which `main` prints to stderr,
//! with exit code 2 for a refusal (4xx) and 1 for a server error (5xx).

use std::fmt;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use ureq::Agent;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub body: Value,
    /// What to read or run next, said after the refusal in markdown
    pub hint: Option<String>,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = if self.status >= 500 {
            "the server failed"
        } else {
            "the server refused the request"
        };
        match self.body["message"].as_str() {
            Some(message) => write!(f, "{what} ({}): {message}", self.status),
            None => write!(f, "{what} ({})", self.status),
        }
    }
}

impl std::error::Error for ApiError {}

pub struct Client {
    agent: Agent,
    base: String,
}

/// What an install asks the app for.
#[derive(Debug, Default)]
pub struct InstallRequest<'a> {
    pub source: &'a str,
    pub link: bool,
    /// with `link`: the full name of the installed plugin the link replaces
    pub replace: Option<&'a str>,
}

impl Client {
    /// What a request that reached no server says: where it looked, and
    /// what to do about it.
    pub fn unreachable(&self) -> String {
        crate::server::not_answering(&self.base)
    }

    /// A request that failed before an answer came: nothing listening is
    /// said as `unreachable`, and an answer too slow in coming as such, since
    /// the app is running and may still do what it was asked.
    fn failed(&self, error: ureq::Error) -> anyhow::Error {
        use ureq::Timeout;
        let listening = !matches!(
            &error,
            ureq::Error::ConnectionFailed
                | ureq::Error::HostNotFound
                | ureq::Error::Timeout(Timeout::Resolve | Timeout::Connect)
        ) && !matches!(&error, ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused);
        let context = match &error {
            ureq::Error::Timeout(_) if listening => format!(
                "the server at {} did not finish answering in time; it is running, so what was asked may still go ahead",
                self.base
            ),
            _ if listening => format!("the connection to the server at {} failed", self.base),
            _ => self.unreachable(),
        };
        anyhow::Error::new(error).context(context)
    }

    /// The longest a single wait poll asks the server to hold the request.
    /// Short on purpose: a poll that lands on a server draining connections
    /// after a restart is abandoned within `POLL_SECS + 5`, not minutes.
    pub const POLL_SECS: u64 = 15;

    pub fn new(base: &str) -> Self {
        Client {
            agent: Self::agent(Duration::from_secs(15)),
            base: base.trim_end_matches('/').to_string(),
        }
    }

    fn agent(timeout: Duration) -> Agent {
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(5)))
            .timeout_global(Some(timeout))
            .build();
        Agent::new_with_config(config)
    }

    /// True if something answers at /api/v1/info.
    pub fn reachable(&self) -> bool {
        let quick = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(2)))
            .build();
        Agent::new_with_config(quick)
            .get(format!("{}/api/v1/info", self.base))
            .call()
            .is_ok()
    }

    pub fn submit(&self, body: &Value) -> Result<Value> {
        self.post("/api/v1/reviews", Some(body))
    }

    /// Sends a plugin's sample as a new review; `body` may give a title,
    /// an origin or who asks.
    pub fn sample(&self, plugin: &str, body: &Value) -> Result<Value> {
        self.post(
            &format!("/api/v1/plugins/{}/sample", segment(plugin)),
            Some(body),
        )
    }

    /// The checks a submission gets, with nothing stored.
    pub fn validate(&self, body: &Value) -> Result<Value> {
        self.post("/api/v1/reviews/validate", Some(body))
    }

    pub fn get_review(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{}", segment(id)), &[])
    }

    pub fn rounds(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{}/rounds", segment(id)), &[])
    }

    pub fn events(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{}/events", segment(id)), &[])
    }

    /// `Some(review)` when the server answered 200 (settled, or pending if it
    /// chose to answer early), `None` on 204. Every poll uses a fresh
    /// connection, so a pooled one to a server that has since gone away is
    /// never reused.
    pub fn wait(&self, id: &str, timeout_secs: u64) -> Result<Option<Value>> {
        let timeout_secs = timeout_secs.min(Self::POLL_SECS);
        let url = format!(
            "{}/api/v1/reviews/{}/wait?timeout={timeout_secs}",
            self.base,
            segment(id)
        );
        let agent = Self::agent(Duration::from_secs(timeout_secs + 5));
        let mut resp = agent.get(&url).call().context("connecting to the server")?;
        match resp.status().as_u16() {
            204 => Ok(None),
            status => Ok(Some(Self::body(status, &mut resp)?)),
        }
    }

    pub fn decide(&self, id: &str, data: Value, note: Option<String>) -> Result<Value> {
        let mut body = serde_json::json!({ "data": data });
        if let Some(note) = note {
            body["agent_note"] = Value::String(note);
        }
        self.post(
            &format!("/api/v1/reviews/{}/decision", segment(id)),
            Some(&body),
        )
    }

    pub fn withdraw(&self, id: &str, reason: Option<String>) -> Result<Value> {
        let body = reason.map(|r| serde_json::json!({ "reason": r }));
        self.post(
            &format!("/api/v1/reviews/{}/withdraw", segment(id)),
            body.as_ref(),
        )
    }

    pub fn discard(&self, id: &str, reason: Option<String>, by: Option<String>) -> Result<Value> {
        let mut body = serde_json::Map::new();
        if let Some(reason) = reason {
            body.insert("reason".into(), Value::String(reason));
        }
        if let Some(by) = by {
            body.insert("by".into(), Value::String(by));
        }
        let body = (!body.is_empty()).then_some(Value::Object(body));
        self.post(
            &format!("/api/v1/reviews/{}/discard", segment(id)),
            body.as_ref(),
        )
    }

    pub fn list(&self, query: &[(&str, String)]) -> Result<Value> {
        self.get("/api/v1/reviews", query)
    }

    pub fn plugins(&self) -> Result<Value> {
        self.get("/api/v1/plugins", &[])
    }

    /// Installs the plugin a folder or a zip holds; the plugin's row.
    pub fn plugins_install(&self, request: &InstallRequest) -> Result<Value> {
        let body = serde_json::json!({
            "source": request.source,
            "link": request.link,
            "replace": request.replace,
        });
        self.post("/api/v1/plugins/install", Some(&body))
    }

    pub fn plugins_remove(&self, name: &str) -> Result<Value> {
        self.delete(&format!("/api/v1/plugins/{}", segment(name)))
    }

    pub fn plugins_describe(&self, name: &str) -> Result<Value> {
        self.get(&format!("/api/v1/plugins/{}/describe", segment(name)), &[])
    }

    /// What the app makes of a plugin folder, installing nothing.
    pub fn plugins_reload(&self) -> Result<Value> {
        self.post("/api/v1/plugins/reload", None)
    }

    /// The review rendered as markdown by the server, opening the way a
    /// command's output does.
    pub fn review_markdown(&self, id: &str) -> Result<String> {
        let mut resp = self
            .agent
            .get(format!("{}/api/v1/reviews/{}", self.base, segment(id)))
            .query("format", "markdown")
            .query("head", "command")
            .call()
            .map_err(|e| self.failed(e))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            // body() turns every status outside 2xx into the server's error
            return Err(Self::body(status, &mut resp)
                .err()
                .unwrap_or_else(|| anyhow::anyhow!("the server answered {status}")));
        }
        resp.body_mut()
            .read_to_string()
            .context("reading the response")
    }

    /// Whether the app already has the file with this hash.
    pub fn attachment_stored(&self, sha256: &str) -> Result<bool> {
        let resp = self
            .agent
            .head(format!("{}/api/v1/attachments/{sha256}", self.base))
            .call()
            .map_err(|e| self.failed(e))?;
        match resp.status().as_u16() {
            200 => Ok(true),
            404 => Ok(false),
            status => {
                anyhow::bail!("the server answered {status} to HEAD /api/v1/attachments/{sha256}")
            }
        }
    }

    /// Uploads a file's bytes under their hash. An upload may take a
    /// while, so it gets a clock of its own rather than the usual 15 s.
    pub fn upload_attachment(
        &self,
        sha256: &str,
        size: u64,
        mut body: impl std::io::Read,
    ) -> Result<Value> {
        let mut resp = Self::agent(Duration::from_secs(60 * 60))
            .put(format!("{}/api/v1/attachments/{sha256}", self.base))
            .header("content-type", "application/octet-stream")
            .header("content-length", size.to_string())
            .send(ureq::SendBody::from_reader(&mut body))
            .map_err(|e| self.failed(e))?;
        Self::body(resp.status().as_u16(), &mut resp)
    }

    /// Streams a file a review carries into `out`.
    pub fn download_attachment(
        &self,
        id: &str,
        name: &str,
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let mut resp = Self::agent(Duration::from_secs(60 * 60))
            .get(format!(
                "{}/api/v1/reviews/{}/attachments/{}",
                self.base,
                segment(id),
                segment(name)
            ))
            .call()
            .map_err(|e| self.failed(e))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Self::body(status, &mut resp).map(|_| 0);
        }
        std::io::copy(&mut resp.body_mut().as_reader(), out).context("reading the file")
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut req = self.agent.get(format!("{}{path}", self.base));
        for (k, v) in query {
            req = req.query(*k, v);
        }
        let mut resp = req.call().map_err(|e| self.failed(e))?;
        Self::body(resp.status().as_u16(), &mut resp)
    }

    fn post(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        self.post_with(&self.agent, path, body)
    }

    fn post_with(&self, agent: &Agent, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let mut resp = match body {
            Some(json) => agent.post(&url).send_json(json),
            // the server refuses a write that does not say it is JSON, even an empty one
            None => agent
                .post(&url)
                .header("content-type", "application/json")
                .send_empty(),
        }
        .map_err(|e| self.failed(e))?;
        Self::body(resp.status().as_u16(), &mut resp)
    }

    fn delete(&self, path: &str) -> Result<Value> {
        let mut resp = self
            .agent
            .delete(format!("{}{path}", self.base))
            .call()
            .map_err(|e| self.failed(e))?;
        Self::body(resp.status().as_u16(), &mut resp)
    }

    fn body(status: u16, resp: &mut ureq::http::Response<ureq::Body>) -> Result<Value> {
        let text = resp
            .body_mut()
            .read_to_string()
            .context("reading the response")?;
        let value: Value = if text.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str(&text) {
                Ok(value) => value,
                // a refusal in plain text (from something in front of the
                // server, or an older one) is still a refusal, not bad JSON
                Err(_) if !(200..300).contains(&status) => {
                    serde_json::json!({ "error": "http", "message": text.trim(), "violations": [] })
                }
                Err(_) => anyhow::bail!("the server sent invalid JSON: {text}"),
            }
        };
        if (200..300).contains(&status) {
            Ok(value)
        } else {
            Err(ApiError {
                status,
                body: value,
                hint: None,
            }
            .into())
        }
    }
}

/// A value as one path segment: an id or a name, whatever it holds, never
/// a query or another route.
fn segment(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
