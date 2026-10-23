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
/// reject any approval card the real daemon did not issue (fake/injected cards). v2 also binds the
/// displayed reason and the response URL so a broker can't rewrite them (review M2).
pub const CHALLENGE_DOMAIN: &str = "OCPG-CH-v2";

/// The bytes the daemon signs (and the app verifies) to authenticate a published challenge.
pub fn challenge_message(
    id: &Uuid,
    source: InstallSource,
    package: &str,
    nonce: &str,
    reason: &str,
    respond_to: &str,
) -> String {
    format!("{CHALLENGE_DOMAIN}|{id}|{}|{package}|{nonce}|{reason}|{respond_to}", source.as_str())
}

/// Domain tag for the daemon's signature over a tamper/security alert (so the app shows only real
/// alerts, not spoofed ones — review H5).
pub const ALERT_DOMAIN: &str = "OCPG-ALERT-v1";

/// The bytes the daemon signs for an alert.
pub fn alert_message(text: &str, ts: u64) -> String {
    format!("{ALERT_DOMAIN}|{text}|{ts}")
}

/// Domain tag for a parent-initiated "grant extra screen-time" command. v2 binds the target daemon's
/// public key so a captured grant can't be replayed onto a sibling's machine paired to the same parent
/// key (review M7).
pub const GRANT_DOMAIN: &str = "OCPG-GRANT-v2";

/// Bytes the parent app signs to grant `minutes` of extra time (daemon_pub binds the target machine;
/// nonce + ts give anti-replay).
pub fn grant_message(daemon_pub: &str, nonce: &str, minutes: u32, ts: u64) -> String {
    format!("{GRANT_DOMAIN}|{daemon_pub}|{nonce}|{minutes}|{ts}")
}

/// A signed grant-time command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedGrant {
    /// Discriminator so the daemon can tell a grant from a decision on the same channel.
    pub kind: String,
    /// The intended daemon's public key (base64) — the daemon rejects a grant not addressed to it.
    pub daemon_pub: String,
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
    vk.verify_strict(grant_message(&g.daemon_pub, &g.nonce, g.minutes, g.ts).as_bytes(), &sig).is_ok()
}

/// Domain for a parent-initiated "lock the computer now" / "unlock" command.
pub const LOCK_DOMAIN: &str = "OCPG-LOCK-v1";

/// Bytes the parent app signs to lock or unlock the child's machine immediately. `action` is "lock"
/// or "unlock"; daemon_pub binds the target machine, nonce + ts give anti-replay.
pub fn lock_message(daemon_pub: &str, nonce: &str, action: &str, ts: u64) -> String {
    format!("{LOCK_DOMAIN}|{daemon_pub}|{nonce}|{action}|{ts}")
}

/// A signed instant-lock command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedLock {
    /// Discriminator ("lock_now") so the daemon can tell it from a grant/decision on the same channel.
    pub kind: String,
    pub daemon_pub: String,
    pub nonce: String,
    /// "lock" or "unlock".
    pub action: String,
    pub ts: u64,
    pub sig: String,
}

/// Verify a signed lock command against the parent key.
pub fn verify_lock(vk: &VerifyingKey, l: &SignedLock) -> bool {
    let sig_bytes = match STANDARD.decode(l.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    vk.verify_strict(lock_message(&l.daemon_pub, &l.nonce, &l.action, l.ts).as_bytes(), &sig).is_ok()
}

/// Load the daemon's Ed25519 signing key from `path` (base64 of a 32-byte seed), creating a fresh
/// one (0600) on first run. The matching public key is what the parent app pairs to verify challenges.
pub fn load_or_create_daemon_key(path: &Path) -> Result<SigningKey> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let seed: [u8; 32] = STANDARD
                .decode(contents.trim())
                .context("decoding daemon key")?
                .try_into()
                .map_err(|_| anyhow::anyhow!("daemon key must be 32 bytes"))?;
            return Ok(SigningKey::from_bytes(&seed));
        }
        // Only a genuinely-missing key triggers creation; any other read error is fatal so we never
        // silently overwrite an existing key we merely failed to read (review L5).
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("reading daemon key {}", path.display()));
        }
    }

    let mut seed = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut seed))
        .context("reading /dev/urandom for daemon key")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    // Create the file 0600 BEFORE writing the secret (review L5).
    use std::io::Write as _;
    let mut f;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        f = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("creating daemon key {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        f = std::fs::File::create(path)?;
    }
    f.write_all(STANDARD.encode(seed).as_bytes())
        .with_context(|| format!("writing daemon key {}", path.display()))?;
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

    fn sign_lock(sk: &SigningKey, dpub: &str, nonce: &str, action: &str, ts: u64) -> SignedLock {
        let sig = sk.sign(lock_message(dpub, nonce, action, ts).as_bytes());
        SignedLock {
            kind: "lock_now".into(),
            daemon_pub: dpub.into(),
            nonce: nonce.into(),
            action: action.into(),
            ts,
            sig: STANDARD.encode(sig.to_bytes()),
        }
    }

    #[test]
    fn valid_lock_verifies() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let l = sign_lock(&sk, &dpub, "n1", "lock", 1_000);
        assert!(verify_lock(&sk.verifying_key(), &l));
    }

    #[test]
    fn tampered_lock_action_fails() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let mut l = sign_lock(&sk, &dpub, "n1", "lock", 1_000);
        l.action = "unlock".to_string(); // flip the action after signing
        assert!(!verify_lock(&sk.verifying_key(), &l), "flipping the action must break the signature");
    }
}
