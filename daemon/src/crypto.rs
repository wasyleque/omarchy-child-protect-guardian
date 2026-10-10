//! Ed25519 verification of parent approvals (zero-trust over an untrusted broker).
//!
//! The parent's device holds the private key; the daemon holds only the public key (paired via
//! QR). A held request carries a one-time `nonce`; the parent signs a canonical message binding
//! the request id, the decision, the nonce and a timestamp. Even an attacker who controls the
//! network and the broker and sees the nonce cannot forge a valid signature without the private key.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::Deserialize;
use uuid::Uuid;

use crate::request::{Decision, InstallSource};

/// Domain-separation tag so a signature here can never be replayed into another protocol.
/// v2 binds the install source + package name, so a broker can't swap what the parent approved.
pub const DOMAIN: &str = "OCPG-v2";

/// Domain tag for the daemon's signature over the *challenge* it publishes, so the parent app can
/// reject any approval card the real daemon did not issue (fake/injected cards).
pub const CHALLENGE_DOMAIN: &str = "OCPG-CH-v1";

/// The bytes the daemon signs (and the app verifies) to authenticate a published challenge.
pub fn challenge_message(id: &Uuid, source: InstallSource, package: &str, nonce: &str) -> String {
    format!("{CHALLENGE_DOMAIN}|{id}|{}|{package}|{nonce}", source.as_str())
}

/// Domain tag for a parent-initiated "grant extra screen-time" command.
pub const GRANT_DOMAIN: &str = "OCPG-GRANT-v1";

/// Bytes the parent app signs to grant `minutes` of extra time (nonce + ts give anti-replay).
pub fn grant_message(nonce: &str, minutes: u32, ts: u64) -> String {
    format!("{GRANT_DOMAIN}|{nonce}|{minutes}|{ts}")
}

/// A signed grant-time command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedGrant {
    /// Discriminator so the daemon can tell a grant from a decision on the same channel.
    pub kind: String,
    pub nonce: String,
    pub minutes: u32,
    pub ts: u64,
    pub sig: String,
}

/// Verify a signed grant against the parent key.
pub fn verify_grant(vk: &VerifyingKey, g: &SignedGrant) -> bool {
    let sig_bytes = match STANDARD.decode(g.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    vk.verify_strict(grant_message(&g.nonce, g.minutes, g.ts).as_bytes(), &sig).is_ok()
}

/// Load the daemon's Ed25519 signing key from `path` (base64 of a 32-byte seed), creating a fresh
/// one (0600) on first run. The matching public key is what the parent app pairs to verify challenges.
pub fn load_or_create_daemon_key(path: &Path) -> Result<SigningKey> {
    if let Ok(contents) = std::fs::read_to_string(path) {
        let seed: [u8; 32] = STANDARD
            .decode(contents.trim())
            .context("decoding daemon key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("daemon key must be 32 bytes"))?;
        return Ok(SigningKey::from_bytes(&seed));
    }
    // Create a new key from the OS CSPRNG.
    let mut seed = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut seed))
        .context("reading /dev/urandom for daemon key")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(path, STANDARD.encode(seed)).with_context(|| format!("writing daemon key {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(SigningKey::from_bytes(&seed))
}

/// base64 of a signing key's public half (for pairing / logging).
pub fn pubkey_b64(key: &SigningKey) -> String {
    STANDARD.encode(key.verifying_key().to_bytes())
}

/// Sign an arbitrary message with the daemon key; returns base64 of the 64-byte signature.
pub fn sign_b64(key: &SigningKey, msg: &str) -> String {
    STANDARD.encode(key.sign(msg.as_bytes()).to_bytes())
}

/// The exact bytes both the phone and the daemon sign/verify. Unambiguous and deterministic.
/// Binds the decision to the full displayed intent: id, decision, nonce, timestamp, source, package.
pub fn canonical_message(
    id: &Uuid,
    decision: Decision,
    nonce: &str,
    ts: u64,
    source: InstallSource,
    package: &str,
) -> String {
    let d = match decision {
        Decision::Allow => "allow",
        Decision::Deny => "deny",
    };
    format!("{DOMAIN}|{id}|{d}|{nonce}|{ts}|{}|{package}", source.as_str())
}

/// A signed decision as posted back by the parent's app.
#[derive(Debug, Deserialize)]
pub struct SignedDecision {
    pub id: Uuid,
    pub decision: Decision,
    pub nonce: String,
    pub ts: u64,
    /// The source + package the parent actually saw and approved (bound into the signature).
    pub source: InstallSource,
    pub package: String,
    /// base64 of the 64-byte Ed25519 signature over [`canonical_message`].
    pub sig: String,
}

/// Parse a base64 Ed25519 public key (32 bytes).
pub fn parse_pubkey(b64: &str) -> Option<VerifyingKey> {
    let bytes = STANDARD.decode(b64.trim()).ok()?;
    let arr: [u8; 32] = bytes.try_into().ok()?;
    VerifyingKey::from_bytes(&arr).ok()
}

/// Verify that `sd` was signed by `vk` for exactly this id/decision/nonce/ts.
pub fn verify(vk: &VerifyingKey, sd: &SignedDecision) -> bool {
    let sig_bytes = match STANDARD.decode(sd.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    let msg = canonical_message(&sd.id, sd.decision, &sd.nonce, sd.ts, sd.source, &sd.package);
    vk.verify_strict(msg.as_bytes(), &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signing_key() -> SigningKey {
        // Deterministic test seed (never used outside tests).
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn sign(sk: &SigningKey, id: Uuid, decision: Decision, nonce: &str, ts: u64, source: InstallSource, package: &str) -> SignedDecision {
        let msg = canonical_message(&id, decision, nonce, ts, source, package);
        let sig = sk.sign(msg.as_bytes());
        SignedDecision {
            id,
            decision,
            nonce: nonce.to_string(),
            ts,
            source,
            package: package.to_string(),
            sig: STANDARD.encode(sig.to_bytes()),
        }
    }

    #[test]
    fn valid_signature_verifies() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let id = Uuid::new_v4();
        let sd = sign(&sk, id, Decision::Allow, "nonce123", 1_000, InstallSource::Pacman, "firefox");
        assert!(verify(&vk, &sd));
    }

    #[test]
    fn tampered_decision_fails() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let id = Uuid::new_v4();
        let mut sd = sign(&sk, id, Decision::Deny, "nonce123", 1_000, InstallSource::Pacman, "firefox");
        sd.decision = Decision::Allow; // flip the decision after signing
        assert!(!verify(&vk, &sd), "flipping the decision must break the signature");
    }

    #[test]
    fn tampered_package_fails() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let mut sd = sign(&sk, Uuid::new_v4(), Decision::Allow, "n", 1_000, InstallSource::Aur, "unwanted-software");
        sd.package = "School calculator".to_string(); // broker swaps the displayed package after signing
        assert!(!verify(&vk, &sd), "swapping the package must break the signature");
    }

    #[test]
    fn tampered_nonce_fails() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let sd = {
            let mut s = sign(&sk, Uuid::new_v4(), Decision::Allow, "real-nonce", 1_000, InstallSource::Pacman, "p");
            s.nonce = "attacker-nonce".to_string();
            s
        };
        assert!(!verify(&vk, &sd));
    }

    #[test]
    fn wrong_key_fails() {
        let sk = signing_key();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let sd = sign(&sk, Uuid::new_v4(), Decision::Allow, "n", 1, InstallSource::Pacman, "p");
        assert!(!verify(&other.verifying_key(), &sd), "a different key must not verify");
    }
}
