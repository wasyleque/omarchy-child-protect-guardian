//! Remote push-approval over [ntfy](https://ntfy.sh) — no own backend required.
//!
//! Two modes:
//!   * **token** (MVP, works with the raw ntfy app): the notification carries Allow/Deny HTTP
//!     action buttons whose body includes a one-time token. Simple, but the token traverses a
//!     public broker, so it is only a stopgap (see docs/THREAT_MODEL.md §7).
//!   * **signed** (enabled by setting `parent_pubkey`): the notification carries only the request
//!     id, package and a one-time `nonce` — nothing that grants approval. The parent app signs the
//!     decision with its Ed25519 private key; the daemon verifies with the paired public key. An
//!     attacker who controls the network and the broker still cannot forge a decision.
//!
//! In both modes the daemon stays fail-closed: no trusted decision ⇒ the request times out to deny.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Minimum spacing between alerts of the same kind (anti-spam + stays well under ntfy.sh's daily cap).
const ALERT_COOLDOWN: Duration = Duration::from_secs(900);

use crate::config::NtfyConfig;
use crate::crypto::{self, SignedDecision};
use crate::queue::Queue;
use crate::request::{Decision, DecisionVia, InstallRequest, InstallSource};

/// How far a signed decision's timestamp may drift from the daemon clock (seconds).
const MAX_TS_SKEW_SECS: u64 = 600;

enum Mode {
    Token,
    Signed(VerifyingKey),
}

/// Per-request secret awaiting a matching response.
enum PendingAuth {
    Token(String),
    /// Signed mode: the one-time nonce plus the exact source+package we asked the parent to approve,
    /// so a broker-swapped display is caught by comparing against the signed fields.
    Challenge {
        nonce: String,
        source: InstallSource,
        package: String,
    },
}

pub struct Ntfy {
    cfg: NtfyConfig,
    client: reqwest::Client,
    /// Random, per-daemon-run topic the parent's response is POSTed to.
    response_topic: String,
    mode: Mode,
    /// The daemon's own key — used to sign each published challenge so the app can reject fake cards.
    daemon_key: SigningKey,
    pending: Mutex<HashMap<Uuid, PendingAuth>>,
    /// Fire-and-forget tamper/security alerts to the parent: (dedupe-key, title, body).
    alert_tx: mpsc::UnboundedSender<(String, String, String)>,
    alert_rx: Mutex<Option<mpsc::UnboundedReceiver<(String, String, String)>>>,
}

#[derive(Deserialize)]
struct StreamEvent {
    event: String,
    #[serde(default)]
    message: Option<String>,
}

/// Token-mode decision payload.
#[derive(Deserialize)]
struct TokenDecision {
    id: Uuid,
    decision: Decision,
    token: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_hex() -> String {
    Uuid::new_v4().simple().to_string()
}

impl Ntfy {
    /// Build the ntfy handle. Returns an error (so the caller can run without ntfy) if a
    /// `parent_pubkey` is configured but cannot be parsed — we never silently downgrade.
    pub fn new(cfg: NtfyConfig, daemon_key: SigningKey) -> Result<Arc<Self>> {
        let mode = match cfg.parent_pubkey.as_deref() {
            Some(pk) => {
                let vk = crypto::parse_pubkey(pk)
                    .context("parent_pubkey is set but is not a valid base64 Ed25519 public key")?;
                eprintln!("guardiand: ntfy running in SIGNED mode (Ed25519 parent key paired)");
                Mode::Signed(vk)
            }
            None if cfg.allow_insecure_token => {
                eprintln!("guardiand: WARNING ntfy running in INSECURE TOKEN mode (allow_insecure_token=true) — \
                           the token crosses a public broker and can be forged. Pair an Ed25519 key for real security.");
                Mode::Token
            }
            None => {
                bail!(
                    "ntfy is enabled but no parent_pubkey is paired. Refusing the insecure token mode; \
                     remote approval is DISABLED (local guardian-ctl still works). Pair the parent app \
                     and set [ntfy].parent_pubkey, or set allow_insecure_token=true only for throwaway testing."
                );
            }
        };

        // Only a connect timeout: the subscription is a long-lived streaming GET, so a
        // total-request timeout would tear it down periodically. Publish gets its own per-request timeout.
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .context("building reqwest client")?;

        let (alert_tx, alert_rx) = mpsc::unbounded_channel();
        Ok(Arc::new(Self {
            cfg,
            client,
            response_topic: format!("guardian-resp-{}", random_hex()),
            mode,
            daemon_key,
            pending: Mutex::new(HashMap::new()),
            alert_tx,
            alert_rx: Mutex::new(Some(alert_rx)),
        }))
    }

    /// Queue a tamper/security alert to the parent (non-blocking, safe from any context). Alerts of
    /// the same `key` are rate-limited by the worker so an attacker can't flood the phone / the broker.
    pub fn alert(&self, key: &str, title: &str, body: &str) {
        let _ = self.alert_tx.send((key.to_string(), title.to_string(), body.to_string()));
    }

    /// Long-lived task that delivers queued alerts, de-duped per key by [`ALERT_COOLDOWN`].
    pub async fn alert_worker(self: Arc<Self>) {
        let mut rx = match self.alert_rx.lock().unwrap().take() {
            Some(r) => r,
            None => return, // already running
        };
        let mut last: HashMap<String, Instant> = HashMap::new();
        while let Some((key, title, body)) = rx.recv().await {
            let now = Instant::now();
            if let Some(t) = last.get(&key) {
                if now.duration_since(*t) < ALERT_COOLDOWN {
                    continue;
                }
            }
            last.insert(key, now);
            if let Err(e) = self.publish_alert(&title, &body).await {
                eprintln!("guardiand: alert publish failed: {e:#}");
            }
        }
    }

    async fn publish_alert(&self, title: &str, body: &str) -> Result<()> {
        let payload = serde_json::json!({
            "topic": self.cfg.request_topic,
            "title": title,
            "message": body,
            "priority": 5,
            "tags": ["rotating_light", "warning"],
        });
        self.post_publish(payload).await
    }

    fn base(&self) -> &str {
        self.cfg.server.trim_end_matches('/')
    }

    /// Drop a request's pending secret (e.g. after it resolved locally or timed out).
    pub fn forget(&self, id: &Uuid) {
        self.pending.lock().unwrap().remove(id);
    }

    /// Register the request's secret and push the appropriate notification to the parent's phone.
    pub async fn push(&self, req: &InstallRequest) -> Result<()> {
        match &self.mode {
            Mode::Token => {
                let token = random_hex();
                self.pending
                    .lock()
                    .unwrap()
                    .insert(req.id, PendingAuth::Token(token.clone()));
                self.publish_token(req, &token).await
            }
            Mode::Signed(_) => {
                let nonce = random_hex();
                self.pending.lock().unwrap().insert(
                    req.id,
                    PendingAuth::Challenge {
                        nonce: nonce.clone(),
                        source: req.source,
                        package: req.package.clone(),
                    },
                );
                self.publish_challenge(req, &nonce).await
            }
        }
    }

    /// Token mode: notification with Allow/Deny HTTP action buttons.
    async fn publish_token(&self, req: &InstallRequest, token: &str) -> Result<()> {
        let resp_url = format!("{}/{}", self.base(), self.response_topic);
        let body = |decision: &str| {
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
                { "action": "http", "label": "Allow", "url": resp_url, "method": "POST", "body": body("allow"), "clear": true },
                { "action": "http", "label": "Deny",  "url": resp_url, "method": "POST", "body": body("deny"),  "clear": true }
            ]
        });
        self.post_publish(payload).await
    }

    /// Signed mode: notification carries only id/package/nonce (no approval secret). The parent
    /// app reads this, signs, and POSTs the signed decision to the response topic.
    async fn publish_challenge(&self, req: &InstallRequest, nonce: &str) -> Result<()> {
        // Machine-readable payload for the parent app to consume and sign.
        // Sign the challenge so the app can reject any card the real daemon did not issue.
        let csig = crypto::sign_b64(
            &self.daemon_key,
            &crypto::challenge_message(&req.id, req.source, &req.package, nonce),
        );
        let data = serde_json::json!({
            "id": req.id,
            "source": req.source,
            "package": req.package,
            "nonce": nonce,
            "reason": req.reason,
            "respond_to": format!("{}/{}", self.base(), self.response_topic),
            "csig": csig,
        })
        .to_string();
        let payload = serde_json::json!({
            "topic": self.cfg.request_topic,
            "title": format!("Approve install: {}", req.package),
            "message": data,
            "priority": 4,
            "tags": ["closed_lock_with_key"],
        });
        self.post_publish(payload).await
    }

    async fn post_publish(&self, payload: serde_json::Value) -> Result<()> {
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

    /// Off-box audit mirror: publish each written audit entry, signed by the daemon key, to a
    /// dedicated topic so it leaves the machine before a local root attacker could rewrite history.
    pub async fn audit_mirror_loop(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<String>, topic: String) {
        eprintln!("guardiand: mirroring signed audit entries to ntfy topic '{topic}'");
        while let Some(entry) = rx.recv().await {
            if let Err(e) = self.publish_audit(&topic, &entry).await {
                eprintln!("guardiand: audit mirror publish failed: {e:#}");
            }
        }
    }

    async fn publish_audit(&self, topic: &str, entry: &str) -> Result<()> {
        let sig = crypto::sign_b64(&self.daemon_key, entry);
        let payload = serde_json::json!({
            "topic": topic,
            "message": serde_json::json!({ "entry": entry, "sig": sig }).to_string(),
            "priority": 2,
            "tags": ["ledger"],
        });
        self.post_publish(payload).await
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
        const MAX_STREAM_LINE: usize = 1024 * 1024; // drop the connection if a line never terminates
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.context("reading ntfy stream")? {
            buf.extend_from_slice(&chunk);
            if buf.len() > MAX_STREAM_LINE {
                anyhow::bail!("ntfy stream line exceeded {MAX_STREAM_LINE} bytes");
            }
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                let line = &line[..line.len() - 1]; // without the trailing '\n'
                if line.is_empty() {
                    continue;
                }
                if let Ok(ev) = serde_json::from_slice::<StreamEvent>(line) {
                    if ev.event == "message" {
                        if let Some(msg) = ev.message {
                            self.handle_message(&msg, queue);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn handle_message(&self, msg: &str, queue: &Queue) {
        match &self.mode {
            Mode::Token => self.handle_token(msg, queue),
            Mode::Signed(vk) => self.handle_signed(vk, msg, queue),
        }
    }

    fn handle_token(&self, msg: &str, queue: &Queue) {
        let payload: TokenDecision = match serde_json::from_str(msg) {
            Ok(p) => p,
            Err(_) => return,
        };
        let valid = {
            let mut map = self.pending.lock().unwrap();
            match map.get(&payload.id) {
                Some(PendingAuth::Token(expected)) if *expected == payload.token => {
                    map.remove(&payload.id);
                    true
                }
                _ => false,
            }
        };
        if !valid {
            eprintln!(
                "guardiand: ntfy rejected a token decision for {} (unknown id or bad/stale token)",
                payload.id
            );
            self.alert(
                "approval-rejected",
                "Guardian: rejected approval attempt",
                "A decision with a wrong or stale credential was rejected. Someone may be trying to approve installs without your phone.",
            );
            return;
        }
        if queue.resolve(payload.id, payload.decision, DecisionVia::NtfyToken) {
            eprintln!("guardiand: ntfy (token) {} → {:?}", payload.id, payload.decision);
        }
    }

    fn handle_signed(&self, vk: &VerifyingKey, msg: &str, queue: &Queue) {
        let sd: SignedDecision = match serde_json::from_str(msg) {
            Ok(p) => p,
            Err(_) => return,
        };
        // 1) The nonce must match the one we issued for this exact request (one-time), AND the signed
        //    source+package must equal what we actually asked the parent to approve. A broker that
        //    swapped the displayed app would produce a signature over the swapped values → mismatch here.
        let expected = {
            let map = self.pending.lock().unwrap();
            match map.get(&sd.id) {
                Some(PendingAuth::Challenge { nonce, source, package }) if *nonce == sd.nonce => {
                    Some((*source, package.clone()))
                }
                _ => None,
            }
        };
        let (exp_source, exp_package) = match expected {
            Some(v) => v,
            None => {
                eprintln!("guardiand: ntfy rejected signed decision for {} (unknown id or stale nonce)", sd.id);
                self.alert(
                    "approval-rejected",
                    "Guardian: rejected approval attempt",
                    "A signed decision referenced an unknown or stale request. Someone may be replaying or probing approvals.",
                );
                return;
            }
        };
        if sd.source != exp_source || sd.package != exp_package {
            eprintln!(
                "guardiand: ntfy rejected signed decision for {} (signed {:?}/{} ≠ requested {:?}/{})",
                sd.id, sd.source, sd.package, exp_source, exp_package
            );
            self.alert(
                "approval-mismatch",
                "Guardian: approval did not match the request",
                "A decision was signed for a different app than the one requested — possible tampering. Denied.",
            );
            return;
        }
        // 2) Timestamp freshness (defends against very old captured signatures).
        let now = now_secs();
        if sd.ts > now.saturating_add(60) || now.saturating_sub(sd.ts) > MAX_TS_SKEW_SECS {
            eprintln!("guardiand: ntfy rejected signed decision for {} (timestamp out of window)", sd.id);
            return;
        }
        // 3) The signature must verify against the paired parent key.
        if !crypto::verify(vk, &sd) {
            eprintln!("guardiand: ntfy rejected signed decision for {} (bad signature)", sd.id);
            self.alert(
                "approval-forged",
                "Guardian: FORGED approval blocked",
                "A decision with an invalid signature was rejected — someone tried to forge an approval. The install was denied.",
            );
            return;
        }
        // Consume the nonce so a replay can't reuse it.
        self.pending.lock().unwrap().remove(&sd.id);
        if queue.resolve(sd.id, sd.decision, DecisionVia::NtfySigned) {
            eprintln!("guardiand: ntfy (signed) {} → {:?}", sd.id, sd.decision);
        }
    }
}
