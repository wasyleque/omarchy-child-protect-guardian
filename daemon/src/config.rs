//! Daemon policy/configuration, loaded from a TOML file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::request::Decision;
use crate::schedule::Schedule;

/// Runtime policy for the daemon.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    /// Path of the Unix-domain CONTROL socket (list/resolve; owner-only).
    pub socket_path: PathBuf,
    /// Path of the Unix-domain SUBMIT socket (install requests; any uid, capped).
    pub submit_socket_path: PathBuf,
    /// How long a held request waits for a decision before the fallback applies.
    pub decision_timeout_secs: u64,
    /// Fallback decision applied when a request times out (fail-closed by default).
    pub default_on_timeout: Decision,
    /// Max concurrently-held requests per submitting uid (anti-flood).
    pub max_pending_per_uid: usize,
    /// Append-only, hash-chained audit log path.
    pub audit_path: PathBuf,
    /// The daemon's own Ed25519 signing key (base64 seed), created on first run. Used to sign the
    /// challenges it publishes so the parent app rejects fake/injected approval cards.
    pub daemon_key_path: PathBuf,
    /// Where daily screen-time usage is persisted so a reboot doesn't reset the budget (review H2).
    pub schedule_state_path: PathBuf,
    /// Where honored grant nonces are persisted so a restart can't allow a replay (review M7).
    pub grant_nonce_path: PathBuf,
    /// Optional remote push-approval via ntfy. Absent/`enabled = false` → local decisions only.
    pub ntfy: Option<NtfyConfig>,
    /// Optional screen-time schedule (window + daily budget). Enforcement wiring is a later slice.
    #[serde(default)]
    pub schedule: Option<Schedule>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/guardian/guardian.sock"),
            submit_socket_path: PathBuf::from("/run/guardian/submit.sock"),
            decision_timeout_secs: 300,
            default_on_timeout: Decision::Deny,
            max_pending_per_uid: 5,
            audit_path: PathBuf::from("/var/log/guardian/audit.log"),
            daemon_key_path: PathBuf::from("/var/lib/guardian/daemon.key"),
            schedule_state_path: PathBuf::from("/var/lib/guardian/schedule-state.json"),
            grant_nonce_path: PathBuf::from("/var/lib/guardian/grant-nonces"),
            ntfy: None,
            schedule: None,
        }
    }
}

/// Remote push-approval settings (ntfy). The parent's phone subscribes to `request_topic`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NtfyConfig {
    /// Master switch for the ntfy push path.
    #[serde(default)]
    pub enabled: bool,
    /// ntfy server base URL (public `https://ntfy.sh` or a self-hosted instance).
    #[serde(default = "default_ntfy_server")]
    pub server: String,
    /// Topic the parent's phone is subscribed to. MUST be long and unguessable.
    pub request_topic: String,
    /// Optional base64 Ed25519 public key of the parent's device (from pairing). When set, the
    /// daemon runs in **signed mode**: it only accepts cryptographically-signed decisions and the
    /// raw one-time-token path is disabled. This closes the public-broker forgery/preemption gap.
    #[serde(default)]
    pub parent_pubkey: Option<String>,
    /// Optional separate topic to mirror signed audit entries + heartbeats to (off-box durability:
    /// even if a root attacker rewrites the local log, each entry already left the machine signed).
    #[serde(default)]
    pub audit_topic: Option<String>,
    /// Opt-in to the INSECURE token mode (no `parent_pubkey`). The token travels through the public
    /// broker, so anyone who can read the topic can forge an approval. Off by default: without a
    /// pubkey the daemon refuses remote approval and runs local-only. Only for throwaway testing.
    #[serde(default)]
    pub allow_insecure_token: bool,
}

fn default_ntfy_server() -> String {
    "https://ntfy.sh".to_string()
}

impl Policy {
    /// Load the policy from `path`, always falling back to defaults so the daemon starts.
    ///
    ///  * missing file → [`Policy::default`], silently;
    ///  * present but unreadable or malformed → **exit the process** (fail loud, review L1): we must
    ///    not silently fall back to defaults and quietly turn off screen-time / remote approval / the
    ///    audit mirror because of a typo. Unknown keys are rejected (`deny_unknown_fields`).
    pub fn load_or_default(path: &Path) -> Policy {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Policy::default(),
            Err(e) => {
                eprintln!("guardiand: cannot read policy file {}: {e} — refusing to start", path.display());
                std::process::exit(1);
            }
        };

        match toml::from_str(&content) {
            Ok(policy) => policy,
            Err(e) => {
                eprintln!("guardiand: invalid policy file {}: {e} — refusing to start (fix it or remove it for defaults)", path.display());
                std::process::exit(1);
            }
        }
    }
}
