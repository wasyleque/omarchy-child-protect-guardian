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

use std::collections::{HashMap, HashSet};
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
    /// Where verified grant-time commands (child, minutes) are delivered to the schedule engine.
    grant_tx: Mutex<Option<mpsc::UnboundedSender<(String, u32)>>>,
    /// One-time nonces already honored for grants (anti-replay).
    grant_nonces: Mutex<HashSet<String>>,
    /// Where honored grant nonces are persisted (so a restart can't re-allow a replay).
    grant_nonce_path: Option<std::path::PathBuf>,
    /// Audit log, so rejected/forged approvals are recorded (not just alerted) — review N5.
    audit: Mutex<Option<Arc<crate::audit::Audit>>>,
    /// Optional AI risk-summary config (enriches each challenge, signed). None = off.
    ai: Mutex<Option<crate::config::AiConfig>>,
    /// Parent-initiated "lock now" override. The schedule tick ORs this into "blocked", so a signed
    /// lock command keeps the child locked until a signed unlock (independent of the time budget).
    lock_override: Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
    /// Where verified pushed schedules (child name + override) go to be applied + persisted.
    sched_tx: Mutex<Option<mpsc::UnboundedSender<(String, crate::schedule::ChildSchedule)>>>,
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

fn load_nonces(path: Option<&std::path::Path>) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Some(p) = path {
        if let Ok(s) = std::fs::read_to_string(p) {
            for line in s.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    set.insert(line.to_string());
                }
            }
        }
    }
    set
}

impl Ntfy {
    /// Build the ntfy handle. Returns an error (so the caller can run without ntfy) if a
    /// `parent_pubkey` is configured but cannot be parsed — we never silently downgrade.
    pub fn new(cfg: NtfyConfig, daemon_key: SigningKey, grant_nonce_path: Option<std::path::PathBuf>) -> Result<Arc<Self>> {
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
        // Deterministic response topic derived from the (paired) request topic, NOT random-per-run.
        // A random topic was lost after any daemon restart/reboot: the phone kept posting grants/locks
        // to the previous run's topic and nothing arrived, no error, child stayed frozen (review F15).
        // With `<request_topic>-resp` the app can compute it at pairing time and it survives restarts.
        // Predictability is safe: decisions are Ed25519-signed and nonce-bound, so a known topic can't
        // be used to forge or replay an approval.
        let response_topic = format!("{}-resp", cfg.request_topic);
        Ok(Arc::new(Self {
            cfg,
            client,
            response_topic,
            mode,
            daemon_key,
            pending: Mutex::new(HashMap::new()),
            alert_tx,
            alert_rx: Mutex::new(Some(alert_rx)),
            grant_tx: Mutex::new(None),
            grant_nonces: Mutex::new(load_nonces(grant_nonce_path.as_deref())),
            grant_nonce_path,
            audit: Mutex::new(None),
            ai: Mutex::new(None),
            lock_override: Mutex::new(None),
            sched_tx: Mutex::new(None),
        }))
    }

    /// Attach the audit log so rejected approvals are recorded (N5).
    pub fn set_audit(&self, audit: Arc<crate::audit::Audit>) {
        *self.audit.lock().unwrap() = Some(audit);
    }

    /// Attach the shared "lock now" override flag (the schedule tick reads it).
    pub fn set_lock_override(&self, flag: Arc<std::sync::atomic::AtomicBool>) {
        *self.lock_override.lock().unwrap() = Some(flag);
    }

    /// Deliver verified pushed schedules (child, override) here (the daemon applies + persists them).
    pub fn set_sched_sink(&self, tx: mpsc::UnboundedSender<(String, crate::schedule::ChildSchedule)>) {
        *self.sched_tx.lock().unwrap() = Some(tx);
    }

    /// Enable AI risk-summaries (enriches each published challenge).
    pub fn set_ai(&self, ai: crate::config::AiConfig) {
        *self.ai.lock().unwrap() = Some(ai);
    }

    /// Record a rejected approval in the audit chain (best-effort).
    fn audit_reject(&self, reason: &str) {
        if let Some(a) = self.audit.lock().unwrap().as_ref() {
            a.record(crate::audit::AuditEvent::ApprovalRejected { reason: reason.to_string() });
        }
    }

    /// Deliver verified grant-time minutes here (the daemon applies them to the schedule engine).
    pub fn set_grant_sink(&self, tx: mpsc::UnboundedSender<(String, u32)>) {
        *self.grant_tx.lock().unwrap() = Some(tx);
    }

    fn handle_grant(&self, vk: &VerifyingKey, g: crypto::SignedGrant) {
        // Must be addressed to THIS daemon (binds the target machine; blocks sibling-PC replay, M7).
        if g.daemon_pub != crypto::pubkey_b64(&self.daemon_key) {
            eprintln!("guardiand: ntfy rejected grant (addressed to a different daemon)");
            return;
        }
        let now = now_secs();
        if g.ts > now.saturating_add(60) || now.saturating_sub(g.ts) > MAX_TS_SKEW_SECS {
            eprintln!("guardiand: ntfy rejected grant (timestamp out of window)");
            return;
        }
        if self.grant_nonces.lock().unwrap().contains(&g.nonce) {
            eprintln!("guardiand: ntfy rejected grant (replayed nonce)");
            return;
        }
        if !crypto::verify_grant(vk, &g) {
            eprintln!("guardiand: ntfy rejected grant (bad signature)");
            self.alert("approval-forged", "Guardian: FORGED grant blocked", "A screen-time grant with an invalid signature was rejected.");
            return;
        }
        self.grant_nonces.lock().unwrap().insert(g.nonce.clone());
        // Persist the nonce so a restart within the TTL window can't re-honor it (M7).
        if let Some(p) = &self.grant_nonce_path {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", g.nonce);
            }
        }
        let minutes = g.minutes.min(1440); // cap at a day
        if let Some(tx) = self.grant_tx.lock().unwrap().as_ref() {
            let _ = tx.send((g.child.clone(), minutes));
        }
        eprintln!("guardiand: ntfy grant of {minutes} min accepted for '{}'", g.child);
    }

    /// Apply a verified instant "lock now" / "unlock" command: set/clear the override flag the
    /// schedule tick honors. Same anti-replay + addressing checks as a grant.
    fn handle_lock(&self, vk: &VerifyingKey, l: crypto::SignedLock) {
        if l.daemon_pub != crypto::pubkey_b64(&self.daemon_key) {
            eprintln!("guardiand: ntfy rejected lock (addressed to a different daemon)");
            return;
        }
        let now = now_secs();
        if l.ts > now.saturating_add(60) || now.saturating_sub(l.ts) > MAX_TS_SKEW_SECS {
            eprintln!("guardiand: ntfy rejected lock (timestamp out of window)");
            return;
        }
        if self.grant_nonces.lock().unwrap().contains(&l.nonce) {
            eprintln!("guardiand: ntfy rejected lock (replayed nonce)");
            return;
        }
        if l.action != "lock" && l.action != "unlock" {
            eprintln!("guardiand: ntfy rejected lock (unknown action '{}')", l.action);
            return;
        }
        if !crypto::verify_lock(vk, &l) {
            eprintln!("guardiand: ntfy rejected lock (bad signature)");
            self.alert("lock-forged", "Guardian: FORGED lock blocked", "A lock command with an invalid signature was rejected.");
            return;
        }
        self.grant_nonces.lock().unwrap().insert(l.nonce.clone());
        if let Some(p) = &self.grant_nonce_path {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", l.nonce);
            }
        }
        let lock = l.action == "lock";
        if let Some(flag) = self.lock_override.lock().unwrap().as_ref() {
            flag.store(lock, std::sync::atomic::Ordering::SeqCst);
            eprintln!("guardiand: ntfy {} accepted", if lock { "lock-now" } else { "unlock" });
            if let Some(a) = self.audit.lock().unwrap().as_ref() {
                a.record(crate::audit::AuditEvent::ScheduleBlocked {
                    reason: if lock { "parent locked the computer".into() } else { "parent unlocked the computer".into() },
                });
            }
        } else {
            eprintln!("guardiand: lock command received but no [schedule] is configured — nothing to lock");
        }
    }

    /// Apply a verified pushed schedule for a child (same anti-replay + addressing checks as a grant).
    fn handle_schedule(&self, vk: &VerifyingKey, s: crypto::SignedSchedule) {
        if s.daemon_pub != crypto::pubkey_b64(&self.daemon_key) {
            eprintln!("guardiand: ntfy rejected schedule (addressed to a different daemon)");
            return;
        }
        let now = now_secs();
        if s.ts > now.saturating_add(60) || now.saturating_sub(s.ts) > MAX_TS_SKEW_SECS {
            eprintln!("guardiand: ntfy rejected schedule (timestamp out of window)");
            return;
        }
        if self.grant_nonces.lock().unwrap().contains(&s.nonce) {
            eprintln!("guardiand: ntfy rejected schedule (replayed nonce)");
            return;
        }
        if !crypto::verify_schedule(vk, &s) {
            eprintln!("guardiand: ntfy rejected schedule (bad signature)");
            self.alert("schedule-forged", "Guardian: FORGED schedule blocked", "A schedule with an invalid signature was rejected.");
            return;
        }
        let child_sched: crate::schedule::ChildSchedule = match serde_json::from_str(&s.sched) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("guardiand: ntfy rejected schedule (bad JSON: {e})");
                return;
            }
        };
        self.grant_nonces.lock().unwrap().insert(s.nonce.clone());
        if let Some(p) = &self.grant_nonce_path {
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
                use std::io::Write as _;
                let _ = writeln!(f, "{}", s.nonce);
            }
        }
        if let Some(tx) = self.sched_tx.lock().unwrap().as_ref() {
            let _ = tx.send((s.child.clone(), child_sched));
            eprintln!("guardiand: ntfy schedule for '{}' accepted", s.child);
            if let Some(a) = self.audit.lock().unwrap().as_ref() {
                a.record(crate::audit::AuditEvent::ScheduleBlocked {
                    reason: format!("parent set schedule for '{}'", s.child),
                });
            }
        } else {
            eprintln!("guardiand: schedule received but no [schedule] is configured — ignored");
        }
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
        // Sign the alert so the parent app shows only genuine alerts (review H5). `message` is a JSON
        // blob the app parses + verifies; `title` stays human-readable for the raw ntfy app.
        let ts = now_secs();
        let asig = crypto::sign_b64(&self.daemon_key, &crypto::alert_message(body, ts));
        let message = serde_json::json!({ "kind": "alert", "text": body, "ts": ts, "asig": asig }).to_string();
        let payload = serde_json::json!({
            "topic": self.cfg.request_topic,
            "title": title,
            "message": message,
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
        let respond_to = format!("{}/{}", self.base(), self.response_topic);
        let mut reason = req.reason.clone().unwrap_or_default();

        // Optional AI risk-summary: generated here so it's part of the SIGNED challenge (tamper-proof)
        // and shown to the parent. Clone the config out before awaiting — never hold the lock across .await.
        let ai_cfg = self.ai.lock().unwrap().clone();
        if let Some(cfg) = ai_cfg {
            let source_str = format!("{:?}", req.source).to_lowercase();
            if let Some(summary) = crate::ai::summarize(&self.client, &cfg, &source_str, &req.package).await {
                reason = if reason.is_empty() { summary } else { format!("{reason}\n\n{summary}") };
            }
        }
        // Sign the challenge (incl. reason + respond_to) so the app rejects any card the real daemon
        // did not issue, and a broker can't rewrite the reason or redirect the response (M2).
        let csig = crypto::sign_b64(
            &self.daemon_key,
            &crypto::challenge_message(&req.id, req.source, &req.package, nonce, &reason, &respond_to),
        );
        let data = serde_json::json!({
            "id": req.id,
            "source": req.source,
            "package": req.package,
            "nonce": nonce,
            "reason": reason,
            "respond_to": respond_to,
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
        // A parent-initiated grant-time command rides the same channel; dispatch it first.
        if let Ok(g) = serde_json::from_str::<crypto::SignedGrant>(msg) {
            if g.kind == "grant_time" {
                self.handle_grant(vk, g);
                return;
            }
        }
        // ...so does an instant "lock now" / "unlock" command.
        if let Ok(l) = serde_json::from_str::<crypto::SignedLock>(msg) {
            if l.kind == "lock_now" {
                self.handle_lock(vk, l);
                return;
            }
        }
        // ...and a pushed screen-time schedule for a child.
        if let Ok(s) = serde_json::from_str::<crypto::SignedSchedule>(msg) {
            if s.kind == "set_schedule" {
                self.handle_schedule(vk, s);
                return;
            }
        }
        let sd: SignedDecision = match serde_json::from_str(msg) {
            Ok(p) => p,
            Err(_) => {
                // A decision arrived we can't parse — almost always an old/stale parent app (review
                // N9/B8). Don't drop it silently: surface it so the parent knows to update the app.
                eprintln!("guardiand: ntfy decision in an unrecognised format — parent app likely needs updating");
                self.alert(
                    "decision-format",
                    "Guardian: update the parent app",
                    "A decision arrived in an old/unrecognised format and was ignored. Update (hard-reload) the parent app.",
                );
                self.audit_reject("unknown decision format (old app?)");
                return;
            }
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
                self.audit_reject("signed decision for unknown/stale request");
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
            self.audit_reject("signed decision package/source mismatch");
            return;
        }
        // 2) Timestamp freshness (defends against very old captured signatures).
        let now = now_secs();
        if sd.ts > now.saturating_add(60) || now.saturating_sub(sd.ts) > MAX_TS_SKEW_SECS {
            eprintln!("guardiand: ntfy rejected signed decision for {} (timestamp out of window)", sd.id);
            self.audit_reject("signed decision timestamp out of window");
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
            self.audit_reject("signed decision bad signature (forged)");
            return;
        }
        // Consume the nonce so a replay can't reuse it.
        self.pending.lock().unwrap().remove(&sd.id);
        if queue.resolve(sd.id, sd.decision, DecisionVia::NtfySigned) {
            eprintln!("guardiand: ntfy (signed) {} → {:?}", sd.id, sd.decision);
        }
    }
}
