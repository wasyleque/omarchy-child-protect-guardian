//! Ed25519 verification of parent approvals (zero-trust over an untrusted broker).
//!
//! The parent's device holds the private key; the daemon holds only the public key (paired via
//! QR). A held request carries a one-time `nonce`; the parent signs a canonical message binding
//! the request id, the decision, the nonce and a timestamp. Even an attacker who controls the
//! network and the broker and sees the nonce cannot forge a valid signature without the private key.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use uuid::Uuid;

use crate::request::Decision;

/// Domain-separation tag so a signature here can never be replayed into another protocol.
pub const DOMAIN: &str = "OCPG-v1";

/// The exact bytes both the phone and the daemon sign/verify. Unambiguous and deterministic.
pub fn canonical_message(id: &Uuid, decision: Decision, nonce: &str, ts: u64) -> String {
    let d = match decision {
        Decision::Allow => "allow",
        Decision::Deny => "deny",
    };
    format!("{DOMAIN}|{id}|{d}|{nonce}|{ts}")
}

/// A signed decision as posted back by the parent's app.
#[derive(Debug, Deserialize)]
pub struct SignedDecision {
    pub id: Uuid,
    pub decision: Decision,
    pub nonce: String,
    pub ts: u64,
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
    let msg = canonical_message(&sd.id, sd.decision, &sd.nonce, sd.ts);
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

    fn sign(sk: &SigningKey, id: Uuid, decision: Decision, nonce: &str, ts: u64) -> SignedDecision {
        let msg = canonical_message(&id, decision, nonce, ts);
        let sig = sk.sign(msg.as_bytes());
        SignedDecision {
            id,
            decision,
            nonce: nonce.to_string(),
            ts,
            sig: STANDARD.encode(sig.to_bytes()),
        }
    }

    #[test]
    fn valid_signature_verifies() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let id = Uuid::new_v4();
        let sd = sign(&sk, id, Decision::Allow, "nonce123", 1_000);
        assert!(verify(&vk, &sd));
    }

    #[test]
    fn tampered_decision_fails() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let id = Uuid::new_v4();
        let mut sd = sign(&sk, id, Decision::Deny, "nonce123", 1_000);
        sd.decision = Decision::Allow; // flip the decision after signing
        assert!(!verify(&vk, &sd), "flipping the decision must break the signature");
    }

    #[test]
    fn tampered_nonce_fails() {
        let sk = signing_key();
        let vk = sk.verifying_key();
        let sd = {
            let mut s = sign(&sk, Uuid::new_v4(), Decision::Allow, "real-nonce", 1_000);
            s.nonce = "attacker-nonce".to_string();
            s
        };
        assert!(!verify(&vk, &sd));
    }

    #[test]
    fn wrong_key_fails() {
        let sk = signing_key();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let sd = sign(&sk, Uuid::new_v4(), Decision::Allow, "n", 1);
        assert!(!verify(&other.verifying_key(), &sd), "a different key must not verify");
    }
}
