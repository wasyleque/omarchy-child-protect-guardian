//! `guardiand` — the control daemon.
//!
//! Stage 1: load the policy, bind the IPC socket, and serve held install requests
//! until a decision (from `guardian-ctl`) or the policy timeout resolves each one.
//! Stage 2+: when `[ntfy]` is enabled, each held request is also pushed to the parent's
//! phone and can be resolved remotely.

use std::sync::Arc;

use anyhow::Result;

use guardian::audit::{Audit, AuditEvent};
use guardian::config::Policy;
use guardian::ipc::Server;
use guardian::ntfy::Ntfy;
use guardian::queue::Queue;

/// Fire a host-side screen-time hook (session lock / unlock) for one child. Spawned detached; never
/// blocks the tick. `GUARDIAN_CHILD=<child>` is set in the environment so a bare command targets this
/// child; a command that bakes its own GUARDIAN_CHILD= still wins (backward compatible).
fn run_hook(kind: &str, cmd: &str, child: &str) {
    let mut c = std::process::Command::new("sh");
    c.arg("-c").arg(cmd);
    if !child.is_empty() {
        c.env("GUARDIAN_CHILD", child);
    }
    match c.spawn() {
        Ok(mut proc) => {
            let kind = kind.to_string();
            // Reap the process so hooks firing every tick don't pile up zombies (review L6).
            std::thread::spawn(move || match proc.wait() {
                Ok(s) if !s.success() => eprintln!("guardiand: screen-time {kind} hook exited {s}"),
                Ok(_) => {}
                Err(e) => eprintln!("guardiand: screen-time {kind} hook wait failed: {e}"),
            });
        }
        Err(e) => eprintln!("guardiand: screen-time {kind} hook failed to start: {e}"),
    }
}

fn save_sched_state(path: &std::path::Path, eng: &std::sync::Mutex<guardian::schedule::TimeEngine>) {
    let (day, used, granted) = eng.lock().unwrap().snapshot();
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).ok();
    }
    let _ = std::fs::write(
        path,
        format!("{{\"day\":{day},\"used_secs\":{used},\"granted_secs\":{granted}}}\n"),
    );
}

/// Per-child schedule-state file: the base path for an unnamed (single) child, else `<base>.<name>`
/// so each child's used/granted minutes persist independently.
fn child_state_path(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    if name.is_empty() {
        base.to_path_buf()
    } else {
        let mut s = base.as_os_str().to_os_string();
        s.push(".");
        s.push(name);
        std::path::PathBuf::from(s)
    }
}

/// File holding a schedule the parent pushed from the app for a child (overrides the config).
fn child_override_path(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let mut s = base.as_os_str().to_os_string();
    s.push(".");
    if !name.is_empty() {
        s.push(name);
        s.push(".");
    }
    s.push("sched");
    std::path::PathBuf::from(s)
}

/// Load a pushed schedule override for a child, if one was saved.
fn load_child_override(base: &std::path::Path, name: &str) -> Option<guardian::schedule::ChildSchedule> {
    let p = child_override_path(base, name);
    let txt = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}

/// Resolve a child username to its uid (startup-time; for logind activity checks).
fn uid_of(name: &str) -> Option<u32> {
    let o = std::process::Command::new("id").arg("-u").arg(name).output().ok()?;
    String::from_utf8_lossy(&o.stdout).trim().parse().ok()
}

/// Whether the child (uid) is actually using the machine right now: an ACTIVE, non-idle logind
/// session. Review F16: the tick used to pass `active=true` always, so the budget drained by wall
/// clock even when the child was logged out or idle. Defaults to `true` if logind can't be queried,
/// so we never UNDER-count on a system without it.
fn child_active(uid: u32) -> bool {
    let state = match std::process::Command::new("loginctl")
        .args(["show-user", &uid.to_string(), "-p", "State", "-p", "Sessions"])
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => return true, // no loginctl → assume active
    };
    if state.trim().is_empty() {
        return true;
    }
    if !state.contains("State=active") {
        return false; // nobody logged in on the seat as this user
    }
    // Active seat: count it only if a session is non-idle.
    let ids = state
        .lines()
        .find_map(|l| l.strip_prefix("Sessions="))
        .unwrap_or("")
        .trim()
        .to_string();
    if ids.is_empty() {
        return true;
    }
    for id in ids.split_whitespace() {
        if let Ok(o) = std::process::Command::new("loginctl")
            .args(["show-session", id, "-p", "IdleHint"])
            .output()
        {
            if String::from_utf8_lossy(&o.stdout).contains("IdleHint=no") {
                return true;
            }
        }
    }
    false
}

/// Seconds until the next local occurrence of `target_hour`:00 (for the daily digest timer).
fn secs_until_hour(target_hour: u8) -> u64 {
    let (_, _, minute) = guardian::schedule::local_now();
    let cur = minute as i64;
    let tgt = (target_hour as i64 % 24) * 60;
    let mut diff = tgt - cur;
    if diff <= 0 {
        diff += 1440;
    }
    (diff as u64) * 60
}

/// Scan a DNS query log for worrying topics the parent should know about (review: AI-digest safety
/// requirement — drugs, self-harm/suicide, weapons, etc.). Best-effort + heuristic; the model
/// interprets. Returns deduped matched domains/lines (capped).
fn scan_dns_for_concerns(path: &str) -> Vec<String> {
    const TERMS: &[&str] = &[
        "suicide", "samobój", "selfharm", "self-harm", "cutting", "depress",
        "narkotyk", "drug", "cocaine", "heroin", "mdma", "weed", "marijuana", "dopalacz",
        "broń", "weapon", "gun", "firearm", "ammo", "bomb", "explosiv",
        "porn", "xxx", "onlyfans", "hentai", "extremis", "nazi", "isis",
    ];
    let mut out: Vec<String> = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        use std::io::BufRead;
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            let low = line.to_lowercase();
            if TERMS.iter().any(|t| low.contains(t)) {
                let token = line
                    .split_whitespace()
                    .find(|w| w.contains('.') && !w.contains('/'))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| line.trim().to_string());
                if !out.contains(&token) {
                    out.push(token);
                }
                if out.len() >= 20 {
                    break;
                }
            }
        }
    }
    out
}

/// Compile today's events (since local midnight) from the audit log + the DNS concern scan into a
/// compact, factual summary the digest model turns into a parent-facing note.
fn build_day_summary(audit_path: &std::path::Path, dns_log: Option<&str>) -> String {
    let (_, _, minute) = guardian::schedule::local_now();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let midnight = now.saturating_sub((minute as u64) * 60);

    let (mut submitted, mut denied, mut refused, mut ctrl, mut rejected, mut blocks, mut grants) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let mut pkgs: Vec<String> = Vec::new();
    if let Ok(f) = std::fs::File::open(audit_path) {
        use std::io::BufRead;
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            let v: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if v.get("ts").and_then(|t| t.as_u64()).unwrap_or(0) < midnight {
                continue;
            }
            match v.get("event").and_then(|e| e.as_str()).unwrap_or("") {
                "submitted" => {
                    submitted += 1;
                    if let Some(p) = v.get("package").and_then(|p| p.as_str()) {
                        pkgs.push(p.to_string());
                    }
                }
                "decided" => {
                    if v.get("decision").and_then(|d| d.as_str()) == Some("deny") {
                        denied += 1;
                    }
                }
                "submit_refused" => refused += 1,
                "control_blocked" => ctrl += 1,
                "approval_rejected" => rejected += 1,
                "schedule_blocked" => blocks += 1,
                "grant_applied" => grants += 1,
                _ => {}
            }
        }
    }
    pkgs.sort();
    pkgs.dedup();

    let mut s = String::new();
    s.push_str(&format!("Install requests today: {submitted} (denied: {denied}).\n"));
    if !pkgs.is_empty() {
        s.push_str(&format!("Packages the child tried to install: {}.\n", pkgs.join(", ")));
    }
    if refused > 0 {
        s.push_str(&format!("Rapid repeated install attempts (flood): {refused}.\n"));
    }
    if ctrl > 0 {
        s.push_str(&format!("Attempts to tamper with the guardian control socket: {ctrl}.\n"));
    }
    if rejected > 0 {
        s.push_str(&format!("Rejected/forged approval attempts: {rejected}.\n"));
    }
    if blocks > 0 {
        s.push_str(&format!("Times screen-time blocked the session: {blocks}.\n"));
    }
    if grants > 0 {
        s.push_str(&format!("Extra-time grants given: {grants}.\n"));
    }
    if let Some(path) = dns_log {
        let hits = scan_dns_for_concerns(path);
        if !hits.is_empty() {
            s.push_str(&format!("Potentially worrying web lookups: {}.\n", hits.join(", ")));
        }
    }
    if s.trim().is_empty() {
        s.push_str("No recorded activity today.");
    }
    s
}

fn load_sched_state(path: &std::path::Path, eng: &std::sync::Mutex<guardian::schedule::TimeEngine>) {
    if let Ok(s) = std::fs::read_to_string(path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
            let g = |k| v.get(k).and_then(|x: &serde_json::Value| x.as_u64()).unwrap_or(0);
            eng.lock().unwrap().restore(g("day"), g("used_secs"), g("granted_secs"));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Optional first argument: path to the policy file.
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/guardian/policy.toml".to_string());

    let policy = Policy::load_or_default(std::path::Path::new(&config_path));
    eprintln!(
        "guardiand: starting — socket={}, decision_timeout={}s, on_timeout={:?}",
        policy.socket_path.display(),
        policy.decision_timeout_secs,
        policy.default_on_timeout
    );

    // Control socket is restricted to root plus the daemon's own uid (so a non-root dev run still
    // works, while a child — a different uid — is always refused). See docs/THREAT_MODEL.md §5.4.
    let own_uid = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0)
    };
    let mut allowed_uids = vec![0u32];
    if own_uid != 0 {
        allowed_uids.push(own_uid);
    }

    // Open the append-only, hash-chained audit log (disabled gracefully if the path isn't writable).
    let audit = match Audit::open(&policy.audit_path) {
        Ok(a) => {
            let a = Arc::new(a);
            a.record(AuditEvent::Started);
            eprintln!("guardiand: audit log at {}", policy.audit_path.display());
            Some(a)
        }
        Err(e) => {
            eprintln!("guardiand: audit disabled — {e:#}");
            None
        }
    };

    // Daemon identity key (signs published challenges so the app can reject fake cards).
    let daemon_key = guardian::crypto::load_or_create_daemon_key(&policy.daemon_key_path)?;
    let daemon_pub = guardian::crypto::pubkey_b64(&daemon_key);
    eprintln!("guardiand: daemon public key (pair into the app) = {daemon_pub}");
    if let Some(parent) = policy.daemon_key_path.parent() {
        let pub_path = parent.join("daemon.pub");
        let _ = std::fs::write(&pub_path, format!("{daemon_pub}\n"));
        // The public key is public — make it world-readable and the state dir traversable so a
        // non-root `guardian-ctl pair` can embed `dpub` in the QR (review F8: /var/lib/guardian was
        // root-only, so pairing silently dropped the key and the app couldn't verify challenges). The
        // private key stays 0600, so a 0755 dir leaks nothing.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&pub_path, std::fs::Permissions::from_mode(0o644));
            let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o755));
        }
    }

    let queue = Arc::new(Queue::new());

    // Parent-initiated "lock now" override (set by a signed ntfy command, read by the schedule tick).
    let lock_override = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Set up remote push-approval if configured and enabled.
    let ntfy = match &policy.ntfy {
        Some(cfg) if cfg.enabled => match Ntfy::new(cfg.clone(), daemon_key.clone(), Some(policy.grant_nonce_path.clone())) {
            Ok(ntfy) => {
                if let Some(a) = &audit {
                    ntfy.set_audit(Arc::clone(a));
                }
                if let Some(ai) = policy.ai.as_ref().filter(|a| a.enabled) {
                    eprintln!("guardiand: AI risk-summary enabled ({} @ {})", ai.model, ai.endpoint);
                    ntfy.set_ai(ai.clone());
                }
                ntfy.set_lock_override(Arc::clone(&lock_override));
                tokio::spawn(Arc::clone(&ntfy).subscribe_loop(Arc::clone(&queue)));
                tokio::spawn(Arc::clone(&ntfy).alert_worker());
                Some(ntfy)
            }
            Err(e) => {
                eprintln!("guardiand: ntfy disabled — {e:#} (local decisions still work)");
                None
            }
        },
        _ => None,
    };

    // Off-box audit mirror + daily heartbeat (dead-man's-switch), if a mirror topic is configured.
    if let (Some(audit), Some(ntfy), Some(topic)) = (
        &audit,
        &ntfy,
        policy.ntfy.as_ref().and_then(|c| c.audit_topic.clone()),
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        audit.set_mirror(tx);
        tokio::spawn(Arc::clone(ntfy).audit_mirror_loop(rx, topic));
        let hb = Arc::clone(audit);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
            loop {
                tick.tick().await; // fires immediately on the first tick, then daily
                hb.record(AuditEvent::Heartbeat);
            }
        });
    }

    // Screen-time engine. Enforcement (session lock/freeze on Wayland) is a host-side hook; here we
    // track usage, expose status to guardian-ctl, and apply parent-granted minutes over the signed channel.
    let schedule: Vec<(String, Arc<std::sync::Mutex<guardian::schedule::TimeEngine>>)> = match &policy.schedule {
        Some(s) if s.enabled => {
            // Which children to track? GUARDIAN_CHILD (space-separated usernames — the same var the
            // lock hooks use, written by setup-child.sh). Unset ⇒ one unnamed engine on the default
            // schedule (preserves the pre-multi-child single-child behavior).
            let names: Vec<String> = std::env::var("GUARDIAN_CHILD")
                .ok()
                .map(|v| v.split_whitespace().map(|x| x.to_string()).collect::<Vec<_>>())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| vec![String::new()]);
            eprintln!("guardiand: screen-time for {} child(ren): {:?}", names.len(), names);
            // Resolve uids once for the logind activity check (F16).
            let uids: std::collections::HashMap<String, u32> = names
                .iter()
                .filter(|n| !n.is_empty())
                .filter_map(|n| uid_of(n).map(|u| (n.clone(), u)))
                .collect();

            // One engine per child, each with its own effective schedule + persisted state file.
            let engines: Vec<(String, Arc<std::sync::Mutex<guardian::schedule::TimeEngine>>)> = names
                .iter()
                .map(|name| {
                    // A schedule the parent pushed from the app (persisted) wins over the config.
                    let eff = match load_child_override(&policy.schedule_state_path, name) {
                        Some(ov) => s.with_child_override(&ov),
                        None => s.effective_for(name),
                    };
                    let eng = Arc::new(std::sync::Mutex::new(guardian::schedule::TimeEngine::new(eff)));
                    load_sched_state(&child_state_path(&policy.schedule_state_path, name), &eng);
                    (name.clone(), eng)
                })
                .collect();

            // Parent grants: applied to EVERY child for now (per-child targeting lands with the app's
            // child selector). Signed + nonce-checked upstream in ntfy.
            if let Some(ntfy) = &ntfy {
                let (gtx, mut grx) = tokio::sync::mpsc::unbounded_channel::<u32>();
                ntfy.set_grant_sink(gtx);
                let engs = engines.clone();
                let aud = audit.clone();
                let base = policy.schedule_state_path.clone();
                tokio::spawn(async move {
                    while let Some(m) = grx.recv().await {
                        let (day, _) = guardian::schedule::local_day_minute();
                        for (name, e) in &engs {
                            e.lock().unwrap().grant_minutes(day, m);
                            save_sched_state(&child_state_path(&base, name), e);
                        }
                        if let Some(a) = &aud {
                            a.record(AuditEvent::GrantApplied { minutes: m });
                        }
                        eprintln!("guardiand: applied +{m} min screen-time grant to all children");
                    }
                });

                // Pushed schedules from the app: apply to the target child's engine + persist (so it
                // survives restart and overrides the config on next boot).
                let (stx, mut srx) = tokio::sync::mpsc::unbounded_channel::<(String, guardian::schedule::ChildSchedule)>();
                ntfy.set_sched_sink(stx);
                let engs = engines.clone();
                let base = policy.schedule_state_path.clone();
                let default_sched = s.clone();
                tokio::spawn(async move {
                    while let Some((child, ov)) = srx.recv().await {
                        // Match the named child; "" or a single-engine daemon targets the one engine
                        // (so the app can send a display name even when GUARDIAN_CHILD is unset).
                        let target = engs.iter().find(|(n, _)| *n == child)
                            .or_else(|| if child.is_empty() || engs.len() == 1 { engs.first() } else { None });
                        match target {
                            Some((name, e)) => {
                                e.lock().unwrap().set_schedule(default_sched.with_child_override(&ov));
                                if let Ok(j) = serde_json::to_string(&ov) {
                                    let _ = std::fs::write(child_override_path(&base, name), j);
                                }
                                eprintln!("guardiand: applied pushed schedule for '{name}'");
                            }
                            None => eprintln!("guardiand: pushed schedule for unknown child '{child}' — ignored"),
                        }
                    }
                });
            }

            let tick_secs = s.tick_secs.max(1);
            let lock_cmd = s.lock_command.clone();
            let unlock_cmd = s.unlock_command.clone();
            let engs = engines.clone();
            let aud = audit.clone();
            let base = policy.schedule_state_path.clone();
            let lo = Arc::clone(&lock_override);
            let uids = uids.clone();
            tokio::spawn(async move {
                let mut last_blocked: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(tick_secs));
                loop {
                    tick.tick().await;
                    let (day, wday, minute) = guardian::schedule::local_now();
                    // "lock now" override blocks every child; per-schedule block is per child.
                    let forced = lo.load(std::sync::atomic::Ordering::SeqCst);
                    for (name, e) in &engs {
                        // Charge the budget only when the child is really using the machine: logged in,
                        // active, not idle — and never while parent-locked (review F16). Falls back to
                        // active=true when there's no uid/logind so we don't under-count.
                        let active = !forced && uids.get(name).map(|u| child_active(*u)).unwrap_or(true);
                        let st = e.lock().unwrap().on_tick(day, wday, minute, active, tick_secs);
                        save_sched_state(&child_state_path(&base, name), e);
                        let blocked = forced || !matches!(st, guardian::schedule::Status::Allowed { .. });
                        let was = last_blocked.get(name).copied().unwrap_or(false);
                        if blocked && !was {
                            let reason = format!("{st:?}");
                            if let Some(a) = &aud {
                                a.record(AuditEvent::ScheduleBlocked { reason: format!("{name}: {reason}") });
                            }
                            eprintln!("guardiand: screen-time BLOCKING for '{name}' ({reason})");
                        } else if !blocked && was {
                            eprintln!("guardiand: screen-time UNBLOCKED for '{name}'");
                            if let Some(c) = &unlock_cmd {
                                run_hook("unlock", c, name);
                            }
                        }
                        // Re-assert the lock every tick while blocked (review H2); hooks are idempotent.
                        if blocked {
                            if let Some(c) = &lock_cmd {
                                run_hook("lock", c, name);
                            }
                        }
                        last_blocked.insert(name.clone(), blocked);
                    }
                }
            });
            engines
        }
        _ => Vec::new(),
    };

    // Daily AI parenting digest (usage + blocked attempts + worrying-lookup warnings), pushed as a
    // signed alert the app already verifies + shows. Off unless [ai].digest_enabled.
    if let Some(ntfy) = &ntfy {
        if let Some(ai) = policy.ai.clone().filter(|a| a.enabled && a.digest_enabled) {
            let ntfy = Arc::clone(ntfy);
            let audit_path = policy.audit_path.clone();
            eprintln!("guardiand: daily AI digest enabled (at {}:00, {})", ai.digest_hour, ai.digest_lang);
            tokio::spawn(async move {
                let client = reqwest::Client::new();
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(secs_until_hour(ai.digest_hour))).await;
                    let events = build_day_summary(&audit_path, ai.dns_query_log.as_deref());
                    match guardian::ai::daily_digest(&client, &ai, &ai.digest_lang, &events).await {
                        Some(d) => ntfy.alert("daily-digest", "Guardian: podsumowanie dnia", &d),
                        None => eprintln!("guardiand: daily digest skipped (AI unavailable)"),
                    }
                    // Step past this minute so we don't fire twice before the hour rolls over.
                    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
                }
            });
        }
    }

    let server = Arc::new(Server {
        queue: Arc::clone(&queue),
        decision_timeout: std::time::Duration::from_secs(policy.decision_timeout_secs),
        default_on_timeout: policy.default_on_timeout,
        ntfy,
        control_uids: allowed_uids,
        max_pending_per_uid: policy.max_pending_per_uid,
        audit,
        schedule,
    });

    server.run(&policy.socket_path, &policy.submit_socket_path).await
}
