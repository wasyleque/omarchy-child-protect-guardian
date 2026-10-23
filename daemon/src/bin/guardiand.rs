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

/// Fire a host-side screen-time hook (session lock / unlock). Spawned detached; never blocks the tick.
fn run_hook(kind: &str, cmd: &str) {
    match std::process::Command::new("sh").arg("-c").arg(cmd).spawn() {
        Ok(mut child) => {
            let kind = kind.to_string();
            // Reap the child so hooks firing every tick don't pile up zombies (review L6).
            std::thread::spawn(move || match child.wait() {
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
    let schedule = match &policy.schedule {
        Some(s) if s.enabled => {
            let eng = Arc::new(std::sync::Mutex::new(guardian::schedule::TimeEngine::new(s.clone())));
            // Persisted usage survives reboot/restart so the budget isn't reset (review H2).
            load_sched_state(&policy.schedule_state_path, &eng);
            if let Some(ntfy) = &ntfy {
                let (gtx, mut grx) = tokio::sync::mpsc::unbounded_channel::<u32>();
                ntfy.set_grant_sink(gtx);
                let e = Arc::clone(&eng);
                let aud = audit.clone();
                let sp = policy.schedule_state_path.clone();
                tokio::spawn(async move {
                    while let Some(m) = grx.recv().await {
                        let (day, _) = guardian::schedule::local_day_minute();
                        e.lock().unwrap().grant_minutes(day, m);
                        save_sched_state(&sp, &e);
                        if let Some(a) = &aud {
                            a.record(AuditEvent::GrantApplied { minutes: m });
                        }
                        eprintln!("guardiand: applied +{m} min screen-time grant");
                    }
                });
            }
            let tick_secs = s.tick_secs.max(1);
            let lock_cmd = s.lock_command.clone();
            let unlock_cmd = s.unlock_command.clone();
            let e = Arc::clone(&eng);
            let aud = audit.clone();
            let sp = policy.schedule_state_path.clone();
            tokio::spawn(async move {
                let mut last_blocked = false;
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(tick_secs));
                loop {
                    tick.tick().await;
                    let (day, minute) = guardian::schedule::local_day_minute();
                    // active=true is a stub until the logind idle hook lands (host-side).
                    let st = e.lock().unwrap().on_tick(day, minute, true, tick_secs);
                    save_sched_state(&sp, &e);
                    let blocked = !matches!(st, guardian::schedule::Status::Allowed { .. });
                    if blocked && !last_blocked {
                        let reason = format!("{st:?}");
                        if let Some(a) = &aud {
                            a.record(AuditEvent::ScheduleBlocked { reason: reason.clone() });
                        }
                        eprintln!("guardiand: screen-time BLOCKING ({reason})");
                    } else if !blocked && last_blocked {
                        eprintln!("guardiand: screen-time UNBLOCKED");
                        if let Some(c) = &unlock_cmd {
                            run_hook("unlock", c);
                        }
                    }
                    // Re-assert the lock on EVERY tick while blocked (review H2): covers a child who
                    // logs in / reboots after the budget was already spent. (Lock hooks must be idempotent.)
                    if blocked {
                        if let Some(c) = &lock_cmd {
                            run_hook("lock", c);
                        }
                    }
                    last_blocked = blocked;
                }
            });
            Some(eng)
        }
        _ => None,
    };

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
