//! Append-only, hash-chained audit log (Threat model §4 L7).
//!
//! Every security-relevant event (a held request, its decision and origin, a refused submit, a
//! blocked control attempt) is appended as one JSON line. Each line carries the SHA-256 of the
//! previous line's hash plus this line's content, so deleting or editing any entry breaks the chain
//! and is detectable by [`verify`]. On the real host, also make the file immutable/forward it so a
//! root attacker can't silently rewrite history (`chattr +i`, remote shipping) — the chain makes
//! tampering *evident* even then.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::request::{Decision, DecisionVia, InstallSource};

const GENESIS: &str = "genesis";

/// A recorded event. Serialized inline into each log line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AuditEvent {
    Started,
    Submitted {
        id: Uuid,
        uid: u32,
        source: InstallSource,
        package: String,
        #[serde(default)]
        reason: Option<String>,
    },
    Decided {
        id: Uuid,
        decision: Decision,
        via: DecisionVia,
    },
    SubmitRefused {
        uid: u32,
        source: InstallSource,
        package: String,
        why: String,
    },
    ControlBlocked {
        uid: u32,
    },
    /// Raised by the watchdog (via the control socket) when it detects/repairs tampering.
    WatchdogAlert {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    seq: u64,
    ts: u64,
    prev: String,
    #[serde(flatten)]
    event: AuditEvent,
    hash: String,
}

struct Inner {
    file: File,
    seq: u64,
    prev: String,
    path: PathBuf,
}

fn head_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.head", path.display()))
}

pub struct Audit {
    inner: Mutex<Inner>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The content that a line's `hash` commits to: the previous hash + this line's fields.
fn chain_hash(prev: &str, seq: u64, ts: u64, event: &AuditEvent) -> String {
    let event_json = serde_json::to_string(event).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(prev.as_bytes());
    h.update(b"|");
    h.update(seq.to_string().as_bytes());
    h.update(b"|");
    h.update(ts.to_string().as_bytes());
    h.update(b"|");
    h.update(event_json.as_bytes());
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

impl Audit {
    /// Open (creating if needed) the audit log at `path`, recovering the chain head from any
    /// existing content so the chain continues across restarts.
    pub fn open(path: &Path) -> Result<Audit> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let (seq, prev) = recover_head(path).unwrap_or((0, GENESIS.to_string()));
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("opening audit log {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o640));
        }
        Ok(Audit { inner: Mutex::new(Inner { file, seq, prev, path: path.to_path_buf() }) })
    }

    /// Append an event to the chain. Best-effort: logging a failure to stderr, never panicking.
    pub fn record(&self, event: AuditEvent) {
        let mut g = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let seq = g.seq + 1;
        let ts = now_secs();
        let hash = chain_hash(&g.prev, seq, ts, &event);
        let entry = Entry { seq, ts, prev: g.prev.clone(), event, hash: hash.clone() };
        match serde_json::to_string(&entry) {
            Ok(mut line) => {
                line.push('\n');
                if let Err(e) = g.file.write_all(line.as_bytes()).and_then(|_| g.file.flush()) {
                    eprintln!("guardiand: audit write failed: {e}");
                    return;
                }
                g.seq = seq;
                g.prev = hash.clone();
                // External-ish anchor: records the latest (seq, hash) so a later truncation that
                // forgets to also rewrite the anchor is caught by `verify`. (Not tamper-proof against
                // root, which can rewrite both — that needs off-box shipping; see docs/THREAT_MODEL.)
                let _ = std::fs::write(head_path(&g.path), format!("{seq} {hash}\n"));
            }
            Err(e) => eprintln!("guardiand: audit serialize failed: {e}"),
        }
    }
}

/// Read the last entry to recover (seq, hash) for continuing the chain.
fn recover_head(path: &Path) -> Option<(u64, String)> {
    let file = File::open(path).ok()?;
    let mut last: Option<Entry> = None;
    for line in BufReader::new(file).lines() {
        let line = line.ok()?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(e) = serde_json::from_str::<Entry>(&line) {
            last = Some(e);
        }
    }
    last.map(|e| (e.seq, e.hash))
}

/// Verify a log file's chain integrity. Returns Ok(number_of_entries) or an error describing the
/// first break (a deleted, reordered or edited entry).
pub fn verify(path: &Path) -> Result<u64> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut prev = GENESIS.to_string();
    let mut expect_seq = 1u64;
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let e: Entry = serde_json::from_str(&line).context("malformed audit entry")?;
        if e.seq != expect_seq {
            anyhow::bail!("sequence break at entry {} (expected {})", e.seq, expect_seq);
        }
        if e.prev != prev {
            anyhow::bail!("prev-hash break at entry {}", e.seq);
        }
        let recomputed = chain_hash(&e.prev, e.seq, e.ts, &e.event);
        if recomputed != e.hash {
            anyhow::bail!("content tampered at entry {}", e.seq);
        }
        prev = e.hash;
        expect_seq += 1;
    }
    let count = expect_seq - 1;
    // Anchor check: if a head anchor exists, the log must not have fewer entries than it recorded
    // (catches truncation / emptying that didn't also rewrite the anchor).
    if let Ok(head) = std::fs::read_to_string(head_path(path)) {
        if let Some(anchor_seq) = head.split_whitespace().next().and_then(|s| s.parse::<u64>().ok()) {
            if count < anchor_seq {
                anyhow::bail!("log truncated: {count} entries but anchor expects at least {anchor_seq}");
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("ocpg-audit-{}.log", Uuid::new_v4().simple()))
    }

    #[test]
    fn chain_writes_and_verifies() {
        let p = tmp();
        {
            let a = Audit::open(&p).unwrap();
            a.record(AuditEvent::Started);
            let id = Uuid::new_v4();
            a.record(AuditEvent::Submitted { id, uid: 1001, source: InstallSource::Pacman, package: "firefox".into(), reason: None });
            a.record(AuditEvent::Decided { id, decision: Decision::Allow, via: DecisionVia::NtfySigned });
        }
        assert_eq!(verify(&p).unwrap(), 3);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn continues_chain_across_reopen() {
        let p = tmp();
        { let a = Audit::open(&p).unwrap(); a.record(AuditEvent::Started); }
        { let a = Audit::open(&p).unwrap(); a.record(AuditEvent::ControlBlocked { uid: 1001 }); }
        assert_eq!(verify(&p).unwrap(), 2);
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn tampering_is_detected() {
        let p = tmp();
        {
            let a = Audit::open(&p).unwrap();
            let id = Uuid::new_v4();
            a.record(AuditEvent::Submitted { id, uid: 1001, source: InstallSource::Pacman, package: "steam".into(), reason: None });
            a.record(AuditEvent::Decided { id, decision: Decision::Deny, via: DecisionVia::Timeout });
        }
        // Flip a decision from deny→allow in the stored line without fixing the hash.
        let content = std::fs::read_to_string(&p).unwrap().replace("\"decision\":\"deny\"", "\"decision\":\"allow\"");
        std::fs::write(&p, content).unwrap();
        assert!(verify(&p).is_err(), "edited entry must fail chain verification");
        std::fs::remove_file(&p).ok();
    }
}
