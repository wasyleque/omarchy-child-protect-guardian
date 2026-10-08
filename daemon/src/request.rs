//! Shared data contract between the daemon, the control client, and (later) the
//! install-interception hook and the remote push-approval layer.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where an install attempt originates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallSource {
    /// `pacman` / system repositories.
    Pacman,
    /// Flatpak (system or user).
    Flatpak,
    /// AUR helper (e.g. `yay`) installing a built package.
    Aur,
    /// Snap package (`snap install`).
    Snap,
    /// Nix package (`nix-env -i` / `nix profile install`).
    Nix,
}

impl std::str::FromStr for InstallSource {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "pacman" => Ok(Self::Pacman),
            "flatpak" => Ok(Self::Flatpak),
            "aur" => Ok(Self::Aur),
            "snap" => Ok(Self::Snap),
            "nix" => Ok(Self::Nix),
            other => Err(format!("unknown install source: {other}")),
        }
    }
}

/// The parent's answer to a held request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Allow,
    Deny,
}

impl std::str::FromStr for Decision {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            other => Err(format!("unknown decision: {other}")),
        }
    }
}

/// Where a decision came from (recorded in the audit log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionVia {
    /// Local `guardian-ctl` on the control socket.
    LocalCtl,
    /// Remote ntfy, one-time token (MVP mode).
    NtfyToken,
    /// Remote ntfy, Ed25519-signed (zero-trust mode).
    NtfySigned,
    /// No decision arrived in time; policy fallback applied.
    Timeout,
}

/// A single install attempt that is being held pending a decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallRequest {
    pub id: Uuid,
    pub source: InstallSource,
    pub package: String,
    #[serde(default)]
    pub reason: Option<String>,
    /// Unix epoch seconds when the request was created.
    pub requested_at: u64,
}
