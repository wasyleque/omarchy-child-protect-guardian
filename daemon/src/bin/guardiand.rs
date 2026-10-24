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

/// (keyword, parent-facing category) for worrying DNS lookups — the AI-digest safety requirement, and
/// now also the source for real-time DNS concern alerts. Matched case-insensitively against a log line.
const DNS_CONCERN_TERMS: &[(&str, &str)] = &[
    ("suicide", "self-harm"), ("samobój", "self-harm"), ("selfharm", "self-harm"),
    ("self-harm", "self-harm"), ("cutting", "self-harm"), ("depress", "self-harm"),
    ("narkotyk", "drugs"), ("drug", "drugs"), ("cocaine", "drugs"), ("heroin", "drugs"),
    ("mdma", "drugs"), ("weed", "drugs"), ("marijuana", "drugs"), ("dopalacz", "drugs"),
    ("broń", "weapons"), ("weapon", "weapons"), ("gun", "weapons"), ("firearm", "weapons"),
    ("ammo", "weapons"), ("bomb", "weapons"), ("explosiv", "weapons"),
    ("porn", "adult"), ("xxx", "adult"), ("onlyfans", "adult"), ("hentai", "adult"),
    ("extremis", "extremism"), ("nazi", "extremism"), ("isis", "extremism"),
];

/// Pull the hostname-looking token out of a dnsmasq log line (has a dot, no slash).
fn dns_domain_of(line: &str) -> Option<String> {
    line.split_whitespace()
        .find(|w| w.contains('.') && !w.contains('/') && !w.ends_with(']'))
        .map(|s| s.trim_end_matches('.').to_string())
}

/// Classify a DNS log line: a sinkholed reply (the filter blocked it) → "adult/blocked", else a
/// worrying keyword → its category, else None. Returns (category, domain).
fn dns_concern(line: &str) -> Option<(&'static str, String)> {
    let low = line.to_lowercase();
    let cat = if low.contains("is 0.0.0.0") || low.contains(" is ::") {
        "blocked"
    } else {
        DNS_CONCERN_TERMS.iter().find(|(t, _)| low.contains(t)).map(|(_, c)| *c)?
    };
    Some((cat, dns_domain_of(line).unwrap_or_default()))
}

/// Scan a DNS query log for worrying topics the parent should know about. Best-effort + heuristic; the
/// digest model interprets. Returns deduped matched domains (capped).
fn scan_dns_for_concerns(path: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Ok(f) = std::fs::File::open(path) {
        use std::io::BufRead;
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            let low = line.to_lowercase();
            if DNS_CONCERN_TERMS.iter().any(|(t, _)| low.contains(t)) {
                let token = dns_domain_of(&line).unwrap_or_else(|| line.trim().to_string());
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

/// A coarse human category for a window class, so the digest model isn't left to guess (report: it
/// turned the terminal "foot" into "football"/"games"). None → leave the raw class as-is.
fn app_category(class: &str) -> Option<&'static str> {
    match class.to_lowercase().as_str() {
        "foot" | "alacritty" | "kitty" | "wezterm" | "ghostty" | "xterm" | "konsole" | "st" => Some("terminal"),
        "firefox" | "chromium" | "chrome" | "brave" | "librewolf" | "zen" => Some("browser"),
        "steam" | "lutris" | "heroic" | "minecraft" => Some("games"),
        "code" | "code-oss" | "vscodium" | "zed" => Some("editor"),
        "discord" | "telegram" | "signal" | "element" | "whatsapp" => Some("chat"),
        "spotify" | "mpv" | "vlc" => Some("media"),
        _ => None,
    }
}

/// Top foreground apps today from the activity log (approx time = samples × interval). Best-effort.
fn summarize_activity(path: &str, midnight: u64) -> Vec<String> {
    use std::io::BufRead;
    const INTERVAL: u64 = 30; // matches the reporter's default sampling period
    let mut by_class: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    if let Ok(f) = std::fs::File::open(path) {
        for line in std::io::BufReader::new(f).lines().map_while(Result::ok) {
            let mut it = line.splitn(3, '\t');
            let ts: u64 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            if ts < midnight {
                continue;
            }
            if let Some(class) = it.next() {
                if !class.is_empty() {
                    *by_class.entry(class.to_string()).or_insert(0) += INTERVAL;
                }
            }
        }
    }
    let mut v: Vec<(String, u64)> = by_class.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter()
        .take(6)
        .map(|(c, secs)| {
            let m = secs / 60;
            let t = if m >= 60 { format!("{}h{:02}m", m / 60, m % 60) } else { format!("{m}m") };
            match app_category(&c) {
                Some(cat) => format!("{c} ({cat}) ~{t}"),
                None => format!("{c} ~{t}"),
            }
        })
        .collect()
}

/// Load an AI config pushed from the app (persisted), if present — overrides policy `[ai]`.
fn load_ai_override(path: &std::path::Path) -> Option<guardian::config::AiConfig> {
    let txt = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&txt).ok()
}

/// The activity log for a child (written by guardian-activity-reporter in the child's home).
fn activity_log_for(name: &str) -> Option<std::path::PathBuf> {
    let o = std::process::Command::new("getent").args(["passwd", name]).output().ok()?;
    let line = String::from_utf8_lossy(&o.stdout);
    let home = line.trim().split(':').nth(5)?.to_string();
    if home.is_empty() {
        return None;
    }
    Some(std::path::PathBuf::from(home).join(".local/share/guardian/activity.log"))
}

/// The most recent foreground-app class from an activity log (last non-empty line's 2nd column).
fn last_activity_app(path: &std::path::Path) -> Option<String> {
    let txt = std::fs::read_to_string(path).ok()?;
    let last = txt.lines().filter(|l| !l.trim().is_empty()).last()?;
    last.split('\t').nth(1).map(|s| s.to_string()).filter(|s| !s.is_empty())
}

/// Compile today's events (since local midnight) from the audit log + the DNS concern scan into a
/// compact, factual summary the digest model turns into a parent-facing note.
fn build_day_summary(audit_path: &std::path::Path, dns_log: Option<&str>, activity_log: Option<&str>) -> String {
    let (_, _, minute) = guardian::schedule::local_now();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let midnight = now.saturating_sub((minute as u64) * 60);

    let (mut submitted, mut denied, mut refused, mut ctrl, mut rejected, mut blocks, mut grants) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    let mut pkgs: Vec<String> = Vec::new();
    // Requests from uid 0 (root — e.g. the smoke test) and `guardian-smoke-*` dummies are not the
    // child's activity, so they must never appear in the parent-facing digest (D1). Track their ids
    // so their later `decided` events are skipped too.
    let mut excluded_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
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
                    let uid = v.get("uid").and_then(|u| u.as_u64()).unwrap_or(0);
                    let pkg = v.get("package").and_then(|p| p.as_str()).unwrap_or("");
                    let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    if uid == 0 || pkg.starts_with("guardian-smoke-") {
                        if !id.is_empty() {
                            excluded_ids.insert(id.to_string());
                        }
                        continue;
                    }
                    submitted += 1;
                    if !pkg.is_empty() {
                        pkgs.push(pkg.to_string());
                    }
                }
                "decided" => {
                    let id = v.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    if excluded_ids.contains(id) {
                        continue;
                    }
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
    if let Some(path) = activity_log {
        let apps = summarize_activity(path, midnight);
        if !apps.is_empty() {
            s.push_str(&format!("Apps/games used today: {}.\n", apps.join(", ")));
        }
    }
    if let Some(path) = dns_log {
        if std::path::Path::new(path).exists() {
            let hits = scan_dns_for_concerns(path);
            if !hits.is_empty() {
                s.push_str(&format!("Potentially worrying web lookups: {}.\n", hits.join(", ")));
            }
        } else {
            // Be explicit rather than silently skipping (the digest shouldn't imply "nothing found").
            s.push_str(&format!("(DNS query log not found at {path} — web-lookup check skipped.)\n"));
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
                // Effective AI config = a config pushed from the app (persisted) overrides policy [ai].
                let eff_ai = load_ai_override(&policy.ai_config_path).or_else(|| policy.ai.clone());
                if let Some(ai) = eff_ai.as_ref().filter(|a| a.enabled) {
                    eprintln!("guardiand: AI enabled ({} @ {})", ai.model, ai.endpoint);
                    ntfy.set_ai(ai.clone());
                }
                ntfy.set_ai_config_path(policy.ai_config_path.clone()); // enable "set AI" from the app
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
                let (gtx, mut grx) = tokio::sync::mpsc::unbounded_channel::<(String, u32)>();
                ntfy.set_grant_sink(gtx);
                let engs = engines.clone();
                let aud = audit.clone();
                let base = policy.schedule_state_path.clone();
                tokio::spawn(async move {
                    while let Some((child, m)) = grx.recv().await {
                        let (day, _) = guardian::schedule::local_day_minute();
                        let single = engs.len() == 1; // one engine ⇒ a named grant still targets it
                        for (name, e) in &engs {
                            if child.is_empty() || single || *name == child {
                                e.lock().unwrap().grant_minutes(day, m);
                                save_sched_state(&child_state_path(&base, name), e);
                            }
                        }
                        if let Some(a) = &aud {
                            a.record(AuditEvent::GrantApplied { minutes: m });
                        }
                        eprintln!("guardiand: applied +{m} min grant to '{}'", if child.is_empty() { "all" } else { &child });
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
    // Always spawn (when ntfy is up) and re-read the AI config each cycle, so enabling the digest or
    // changing the hour/model from the app takes effect without a daemon restart.
    // `digest_tx` lets the control socket (`guardian-ctl digest-now`) nudge the task to run the
    // digest immediately; None when there is no ntfy to deliver it.
    let mut digest_tx: Option<tokio::sync::mpsc::UnboundedSender<()>> = None;
    if let Some(ntfy) = &ntfy {
        let (tx, mut digest_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        digest_tx = Some(tx);
        let ntfy = Arc::clone(ntfy);
        let audit_path = policy.audit_path.clone();
        let ai_path = policy.ai_config_path.clone();
        let policy_ai = policy.ai.clone();
        tokio::spawn(async move {
            let client = reqwest::Client::new();
            loop {
                let cur = load_ai_override(&ai_path).or_else(|| policy_ai.clone());
                let hour = cur.as_ref().map(|a| a.digest_hour).unwrap_or(20);
                // Wake on whichever comes first: the daily hour, or an on-demand `digest-now`.
                let manual = tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(secs_until_hour(hour))) => false,
                    r = digest_rx.recv() => match r { Some(()) => true, None => break },
                };
                let cur = load_ai_override(&ai_path).or_else(|| policy_ai.clone());
                // A manual trigger runs whenever AI is enabled (explicit test); the timer still
                // honors the `digest_enabled` opt-in.
                if let Some(ai) = cur.filter(|a| a.enabled && (manual || a.digest_enabled)) {
                    let events = build_day_summary(&audit_path, ai.dns_query_log.as_deref(), ai.activity_log.as_deref());
                    match guardian::ai::daily_digest(&client, &ai, &ai.digest_lang, &events).await {
                        Some(d) => ntfy.alert("!daily-digest", "Guardian: podsumowanie dnia", &d),
                        None => eprintln!("guardiand: daily digest skipped (AI unavailable)"),
                    }
                } else if manual {
                    eprintln!("guardiand: digest-now requested but [ai].enabled is false — nothing sent");
                }
                // After a timer fire, guard against re-firing within the same minute; a manual
                // trigger loops straight back to waiting for the next hour.
                if !manual {
                    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
                }
            }
        });
    }

    // Real-time DNS concern alerts: watch the child's DNS query log and push a SIGNED, rate-limited
    // alert the moment the child hits a blocked/worrying domain — so the parent learns immediately, not
    // only in the daily digest (the digest flagged these but only if the log existed). Off unless
    // [ai].dns_query_log is set and [ai].dns_alert (default on).
    if let Some(ntfy) = &ntfy {
        let ntfy = Arc::clone(ntfy);
        let ai_path = policy.ai_config_path.clone();
        let policy_ai = policy.ai.clone();
        tokio::spawn(async move {
            use std::io::{Read, Seek, SeekFrom};
            // Start at the end of the current log so startup doesn't replay history.
            let mut offset: u64 = load_ai_override(&ai_path)
                .or_else(|| policy_ai.clone())
                .and_then(|a| a.dns_query_log)
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                let ai = match load_ai_override(&ai_path)
                    .or_else(|| policy_ai.clone())
                    .filter(|a| a.enabled && a.dns_alert)
                {
                    Some(a) => a,
                    None => continue,
                };
                let path = match ai.dns_query_log.as_deref() {
                    Some(p) => p.to_string(),
                    None => continue,
                };
                let mut f = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => { offset = 0; continue } // not created yet / rotated away
                };
                let len = f.metadata().map(|m| m.len()).unwrap_or(0);
                if len < offset { offset = 0; } // log rotated/truncated → restart from the top
                if len == offset || f.seek(SeekFrom::Start(offset)).is_err() { continue; }
                let mut buf = String::new();
                if f.read_to_string(&mut buf).is_err() { continue; }
                offset = len;
                // Pass 1: which domains did the filter actually sinkhole this cycle?
                let mut blocked: std::collections::HashSet<String> = std::collections::HashSet::new();
                for line in buf.lines() {
                    if let Some(("blocked", d)) = dns_concern(line) {
                        if !d.is_empty() { blocked.insert(d); }
                    }
                }
                // Pass 2: at most one alert per category per cycle (the cooldown de-dups further). A
                // sinkholed domain gets only the "blocked" alert — skip its keyword alert so one visit
                // isn't reported twice (review D13); a worrying keyword on a NON-blocked domain still fires.
                let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
                let pl = ai.digest_lang.to_lowercase().starts_with("pol");
                for line in buf.lines() {
                    if let Some((cat, domain)) = dns_concern(line) {
                        if domain.is_empty() || (cat != "blocked" && blocked.contains(&domain)) {
                            continue;
                        }
                        if !seen.insert(cat) {
                            continue;
                        }
                        let (title, body) = match (cat, pl) {
                            ("blocked", true) => ("Guardian: zablokowana strona".to_string(),
                                format!("⚠ Dziecko próbowało otworzyć zablokowaną stronę: {domain}")),
                            ("blocked", false) => ("Guardian: blocked site".to_string(),
                                format!("⚠ The child tried to open a blocked site: {domain}")),
                            (c, true) => ("Guardian: niepokojące wyszukiwanie".to_string(),
                                format!("⚠ UWAGA: niepokojące wyszukiwanie ({c}): {domain}")),
                            (c, false) => ("Guardian: worrying lookup".to_string(),
                                format!("⚠ ALERT: worrying lookup ({c}): {domain}")),
                        };
                        ntfy.alert(&format!("dns-concern:{cat}"), &title, &body);
                    }
                }
            }
        });
    }

    // Live "current app" pings: when enabled, publish each active child's foreground app (on change)
    // so the parent app can show "now: <app>". Reads the reporter's per-child activity log.
    if let Some(ntfy) = &ntfy {
        if policy.ntfy.as_ref().map(|c| c.publish_activity).unwrap_or(false) {
            let children: Vec<(String, u32)> = std::env::var("GUARDIAN_CHILD")
                .ok()
                .map(|v| v.split_whitespace().map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|n| uid_of(&n).map(|u| (n, u)))
                .collect();
            if !children.is_empty() {
                let ntfy = Arc::clone(ntfy);
                eprintln!("guardiand: live activity pings enabled for {:?}", children.iter().map(|(n, _)| n).collect::<Vec<_>>());
                tokio::spawn(async move {
                    let mut last: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        for (name, uid) in &children {
                            if !child_active(*uid) {
                                continue;
                            }
                            if let Some(app) = activity_log_for(name).and_then(|p| last_activity_app(&p)) {
                                if last.get(name) != Some(&app) {
                                    last.insert(name.clone(), app.clone());
                                    let _ = ntfy.publish_activity(name, &app).await;
                                }
                            }
                        }
                    }
                });
            }
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
        digest_tx,
    });

    server.run(&policy.socket_path, &policy.submit_socket_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn digest_excludes_uid0_and_smoke_requests() {
        // D1 regression: the smoke test's uid-0 `guardian-smoke-*` requests must not appear in the
        // parent-facing digest, and their deny must not inflate the "denied" count — only the real
        // child's (uid 1001) request should.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let dir = std::env::temp_dir();
        let path = dir.join(format!("guardian_digest_test_{}.log", std::process::id()));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            let lines = [
                format!(r#"{{"event":"submitted","ts":{now},"uid":0,"package":"guardian-smoke-allow","id":"aaa"}}"#),
                format!(r#"{{"event":"decided","ts":{now},"id":"aaa","decision":"allow"}}"#),
                format!(r#"{{"event":"submitted","ts":{now},"uid":0,"package":"guardian-smoke-deny","id":"bbb"}}"#),
                format!(r#"{{"event":"decided","ts":{now},"id":"bbb","decision":"deny"}}"#),
                format!(r#"{{"event":"submitted","ts":{now},"uid":1001,"package":"steam","id":"ccc"}}"#),
                format!(r#"{{"event":"decided","ts":{now},"id":"ccc","decision":"deny"}}"#),
            ];
            for l in lines {
                writeln!(f, "{l}").unwrap();
            }
        }
        let summary = build_day_summary(&path, None, None);
        let _ = std::fs::remove_file(&path);

        assert!(!summary.contains("guardian-smoke"), "smoke packages leaked into digest:\n{summary}");
        assert!(summary.contains("steam"), "real child package missing:\n{summary}");
        assert!(summary.contains("Install requests today: 1"), "wrong submitted count:\n{summary}");
        assert!(summary.contains("denied: 1"), "smoke deny inflated the count:\n{summary}");
    }

    #[test]
    fn dns_concern_classifies() {
        // A sinkholed reply → the filter blocked it.
        assert_eq!(
            dns_concern("Oct 24 reply pornhub.com is 0.0.0.0"),
            Some(("blocked", "pornhub.com".to_string()))
        );
        // A worrying keyword in the queried domain → its category.
        assert_eq!(
            dns_concern("Oct 24 query[A] buy-cocaine.example from 192.168.0.11").map(|(c, _)| c),
            Some("drugs")
        );
        // Ordinary lookups don't alert.
        assert!(dns_concern("Oct 24 query[A] archlinux.org from 192.168.0.11").is_none());
    }
}
