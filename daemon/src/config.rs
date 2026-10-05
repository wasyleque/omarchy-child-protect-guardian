//! Daemon policy/configuration, loaded from a TOML file.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::request::Decision;

/// Runtime policy for the daemon.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Policy {
    /// Path of the Unix-domain control socket.
    pub socket_path: PathBuf,
    /// How long a held request waits for a decision before the fallback applies.
    pub decision_timeout_secs: u64,
    /// Fallback decision applied when a request times out (fail-closed by default).
    pub default_on_timeout: Decision,
    /// Optional remote push-approval via ntfy. Absent/`enabled = false` → local decisions only.
    pub ntfy: Option<NtfyConfig>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/guardian/guardian.sock"),
            decision_timeout_secs: 300,
            default_on_timeout: Decision::Deny,
            ntfy: None,
        }
    }
}

/// Remote push-approval settings (ntfy). The parent's phone subscribes to `request_topic`.
#[derive(Debug, Clone, Deserialize)]
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
}

fn default_ntfy_server() -> String {
    "https://ntfy.sh".to_string()
}

impl Policy {
    /// Load the policy from `path`, always falling back to defaults so the daemon starts.
    ///
    ///  * missing file → [`Policy::default`], silently;
    ///  * unreadable file or malformed TOML → warn on stderr, then [`Policy::default`];
    ///  * otherwise → the parsed policy.
    pub fn load_or_default(path: &Path) -> Policy {
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Policy::default(),
            Err(e) => {
                eprintln!(
                    "guardiand: cannot read policy file {}: {} — using defaults",
                    path.display(),
                    e
                );
                return Policy::default();
            }
        };

        match toml::from_str(&content) {
            Ok(policy) => policy,
            Err(e) => {
                eprintln!(
                    "guardiand: invalid policy file {}: {} — using defaults",
                    path.display(),
                    e
                );
                Policy::default()
            }
        }
    }
}
