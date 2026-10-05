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
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/run/guardian/guardian.sock"),
            decision_timeout_secs: 300,
            default_on_timeout: Decision::Deny,
        }
    }
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
