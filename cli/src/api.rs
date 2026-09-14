//! The HTTP client. Every call returns the decoded JSON body; a 4xx/5xx
//! becomes an [`ApiError`] carrying that body, which `main` prints to stderr
//! with exit code 2.

use std::fmt;
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::Value;
use ureq::Agent;

#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub body: Value,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "server refused the request ({})", self.status)
    }
}

impl std::error::Error for ApiError {}

pub struct Client {
    agent: Agent,
    base: String,
}

impl Client {
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

    pub fn base(&self) -> &str {
        &self.base
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

    pub fn get_review(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{id}"), &[])
    }

    pub fn rounds(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{id}/rounds"), &[])
    }

    pub fn events(&self, id: &str) -> Result<Value> {
        self.get(&format!("/api/v1/reviews/{id}/events"), &[])
    }

    /// `Some(review)` when the server answered 200 (settled, or pending if it
    /// chose to answer early), `None` on 204. Every poll uses a fresh
    /// connection, so a pooled one to a server that has since gone away is
    /// never reused.
    pub fn wait(&self, id: &str, timeout_secs: u64) -> Result<Option<Value>> {
        let timeout_secs = timeout_secs.min(Self::POLL_SECS);
        let url = format!(
            "{}/api/v1/reviews/{id}/wait?timeout={timeout_secs}",
            self.base
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
        self.post(&format!("/api/v1/reviews/{id}/decision"), Some(&body))
    }

    pub fn withdraw(&self, id: &str, reason: Option<String>) -> Result<Value> {
        let body = reason.map(|r| serde_json::json!({ "reason": r }));
        self.post(&format!("/api/v1/reviews/{id}/withdraw"), body.as_ref())
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
        self.post(&format!("/api/v1/reviews/{id}/discard"), body.as_ref())
    }

    pub fn list(&self, query: &[(&str, String)]) -> Result<Value> {
        self.get("/api/v1/reviews", query)
    }

    pub fn plugins(&self) -> Result<Value> {
        self.get("/api/v1/plugins", &[])
    }

    pub fn plugin_versions(&self, name: &str) -> Result<Value> {
        self.get(&format!("/api/v1/plugins/{name}/versions"), &[])
    }

    pub fn plugins_add(&self, dir: &str) -> Result<Value> {
        self.post(
            "/api/v1/plugins/dirs",
            Some(&serde_json::json!({ "dir": dir })),
        )
    }

    pub fn plugins_reload(&self) -> Result<Value> {
        self.post("/api/v1/plugins/reload", None)
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut req = self.agent.get(format!("{}{path}", self.base));
        for (k, v) in query {
            req = req.query(*k, v);
        }
        let mut resp = req.call().context("connecting to the server")?;
        Self::body(resp.status().as_u16(), &mut resp)
    }

    fn post(&self, path: &str, body: Option<&Value>) -> Result<Value> {
        let url = format!("{}{path}", self.base);
        let mut resp = match body {
            Some(json) => self.agent.post(&url).send_json(json),
            None => self.agent.post(&url).send_empty(),
        }
        .context("connecting to the server")?;
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
            serde_json::from_str(&text)
                .with_context(|| format!("the server sent invalid JSON: {text}"))?
        };
        if (200..300).contains(&status) {
            Ok(value)
        } else {
            Err(ApiError {
                status,
                body: value,
            }
            .into())
        }
    }
}
