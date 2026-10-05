//! Remote push-approval over [ntfy](https://ntfy.sh) — no own backend required.
//!
//! When a request is held, the daemon publishes a notification with **Allow/Deny** HTTP
//! action buttons to the parent's `request_topic`. Tapping a button makes the ntfy app
//! POST a small JSON decision to a per-daemon random `response_topic`, which this daemon
//! subscribes to as an NDJSON stream and uses to resolve the held request.
//!
//! MVP security (public broker): the `request_topic`/`response_topic` are unguessable, and
//! each request carries a one-time, per-request `token` that the daemon verifies and then
//! consumes. A malicious broker node could still flip a decision — that is only closed in
//! v2 by Ed25519-signed decisions (see docs/ARCHITECTURE). Until then, the daemon timeout
//! stays fail-closed, so a dropped or withheld decision denies.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use uuid::Uuid;

use crate::config::NtfyConfig;
use crate::queue::Queue;
use crate::request::{Decision, InstallRequest};

pub struct Ntfy {
    cfg: NtfyConfig,
    client: reqwest::Client,
    /// Random, per-daemon-run topic the action buttons POST decisions to.
    response_topic: String,
    /// One-time tokens keyed by request id; consumed on first valid use.
    tokens: Mutex<HashMap<Uuid, String>>,
}

#[derive(Deserialize)]
struct StreamEvent {
    event: String,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize)]
struct DecisionPayload {
    id: Uuid,
    decision: Decision,
    token: String,
}

impl Ntfy {
    pub fn new(cfg: NtfyConfig) -> Arc<Self> {
        let response_topic = format!("guardian-resp-{}", Uuid::new_v4().simple());
        // Only a connect timeout: the subscription is a long-lived streaming GET, so a
        // total-request timeout would tear it down periodically. Publish gets its own per-request timeout.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("building reqwest client");
        Arc::new(Self {
            cfg,
            client,
            response_topic,
            tokens: Mutex::new(HashMap::new()),
        })
    }

    fn base(&self) -> &str {
        self.cfg.server.trim_end_matches('/')
    }

    /// Register a fresh one-time token for a request and return it.
    pub fn register(&self, id: Uuid) -> String {
        let token = Uuid::new_v4().simple().to_string();
        self.tokens.lock().unwrap().insert(id, token.clone());
        token
    }

    /// Drop a request's token (e.g. after it resolved locally or timed out).
    pub fn forget(&self, id: &Uuid) {
        self.tokens.lock().unwrap().remove(id);
    }

    /// Publish the approval notification with Allow/Deny action buttons.
    pub async fn publish(&self, req: &InstallRequest, token: &str) -> Result<()> {
        let resp_url = format!("{}/{}", self.base(), self.response_topic);
        let decision_body = |decision: &str| {
            serde_json::json!({ "id": req.id, "decision": decision, "token": token }).to_string()
        };
        let reason_suffix = req
            .reason
            .as_deref()
            .map(|r| format!(" (reason: {r})"))
            .unwrap_or_default();

        let payload = serde_json::json!({
            "topic": self.cfg.request_topic,
            "title": format!("Install request: {}", req.package),
            "message": format!("{:?} install of \"{}\"{} — approve?", req.source, req.package, reason_suffix),
            "priority": 4,
            "tags": ["closed_lock_with_key"],
            "actions": [
                { "action": "http", "label": "Allow", "url": resp_url, "method": "POST", "body": decision_body("allow"), "clear": true },
                { "action": "http", "label": "Deny",  "url": resp_url, "method": "POST", "body": decision_body("deny"),  "clear": true }
            ]
        });

        let resp = self
            .client
            .post(format!("{}/", self.base()))
            .timeout(Duration::from_secs(15))
            .json(&payload)
            .send()
            .await
            .context("publishing ntfy notification")?;
        if !resp.status().is_success() {
            bail!("ntfy publish returned HTTP {}", resp.status());
        }
        Ok(())
    }

    /// Long-lived task: subscribe to the response topic and resolve the queue on valid decisions.
    pub async fn subscribe_loop(self: Arc<Self>, queue: Arc<Queue>) {
        let url = format!("{}/{}/json", self.base(), self.response_topic);
        eprintln!(
            "guardiand: ntfy enabled — request_topic='{}', response channel ready",
            self.cfg.request_topic
        );
        loop {
            if let Err(e) = self.subscribe_once(&url, &queue).await {
                eprintln!("guardiand: ntfy stream error: {e:#} — reconnecting in 2s");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn subscribe_once(&self, url: &str, queue: &Queue) -> Result<()> {
        let mut resp = self
            .client
            .get(url)
            .send()
            .await
            .context("connecting to ntfy stream")?;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.context("reading ntfy stream")? {
            buf.extend_from_slice(&chunk);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1]; // without the trailing '\n'
                if line.is_empty() {
                    continue;
                }
                if let Ok(ev) = serde_json::from_slice::<StreamEvent>(line) {
                    // ntfy sends "open"/"keepalive"/"message"; only the last carries a decision.
                    if ev.event == "message" {
                        if let Some(msg) = ev.message {
                            self.handle_decision(&msg, queue);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_decision(&self, msg: &str, queue: &Queue) {
        let payload: DecisionPayload = match serde_json::from_str(msg) {
            Ok(p) => p,
            Err(_) => return, // not a decision payload; ignore
        };
        // Verify and consume the one-time token for this exact request id.
        let valid = {
            let mut map = self.tokens.lock().unwrap();
            match map.get(&payload.id) {
                Some(expected) if *expected == payload.token => {
                    map.remove(&payload.id);
                    true
                }
                _ => false,
            }
        };
        if !valid {
            eprintln!(
                "guardiand: ntfy rejected a decision for {} (unknown id or bad/stale token)",
                payload.id
            );
            return;
        }
        if queue.resolve(payload.id, payload.decision) {
            eprintln!("guardiand: ntfy decision {} → {:?}", payload.id, payload.decision);
        }
    }
}
