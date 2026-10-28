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

/// Domain for a live "current foreground app" activity ping the daemon signs (the app shows it).
pub const ACT_DOMAIN: &str = "OCPG-ACT-v1";

/// The bytes the daemon signs for an activity ping (which child is running which app, when).
pub fn activity_message(child: &str, app: &str, ts: u64) -> String {
    format!("{ACT_DOMAIN}|{child}|{app}|{ts}")
}

/// Domain tag for a parent-initiated "grant extra screen-time" command. v2 binds the target daemon's
/// public key so a captured grant can't be replayed onto a sibling's machine paired to the same parent
/// key (review M7).
pub const GRANT_DOMAIN: &str = "OCPG-GRANT-v3";

/// Bytes the parent app signs to grant `minutes` of extra time to `child` (daemon_pub binds the
/// target machine; nonce + ts give anti-replay). `child` is the account name, "" = all/single child.
pub fn grant_message(daemon_pub: &str, nonce: &str, child: &str, minutes: u32, ts: u64) -> String {
    format!("{GRANT_DOMAIN}|{daemon_pub}|{nonce}|{child}|{minutes}|{ts}")
}

/// A signed grant-time command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedGrant {
    /// Discriminator so the daemon can tell a grant from a decision on the same channel.
    pub kind: String,
    /// The intended daemon's public key (base64) — the daemon rejects a grant not addressed to it.
    pub daemon_pub: String,
    pub nonce: String,
    /// Target child account ("" = all/single child).
    #[serde(default)]
    pub child: String,
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
    vk.verify_strict(grant_message(&g.daemon_pub, &g.nonce, &g.child, g.minutes, g.ts).as_bytes(), &sig).is_ok()
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

/// Domain for a parent-pushed screen-time schedule for one child.
pub const SCHED_DOMAIN: &str = "OCPG-SCHED-v1";

/// Bytes the parent app signs to set `child`'s schedule. `sched_json` is the exact JSON string of the
/// child's schedule override — signed verbatim so there's no canonical-JSON ambiguity (the daemon
/// verifies over the string it received, then parses it).
pub fn schedule_message(daemon_pub: &str, nonce: &str, child: &str, ts: u64, sched_json: &str) -> String {
    format!("{SCHED_DOMAIN}|{daemon_pub}|{nonce}|{child}|{ts}|{sched_json}")
}

/// A signed schedule-set command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedSchedule {
    /// Discriminator ("set_schedule").
    pub kind: String,
    pub daemon_pub: String,
    pub nonce: String,
    /// Target child username ("" = the single/default child).
    pub child: String,
    pub ts: u64,
    /// The child's schedule override as a JSON string (parsed into schedule::ChildSchedule).
    pub sched: String,
    pub sig: String,
}

/// Domain for a parent-pushed AI/Ollama configuration.
pub const AICFG_DOMAIN: &str = "OCPG-AICFG-v1";

/// Bytes the parent app signs to set the AI config (`cfg_json` signed verbatim, like a schedule).
pub fn aicfg_message(daemon_pub: &str, nonce: &str, ts: u64, cfg_json: &str) -> String {
    format!("{AICFG_DOMAIN}|{daemon_pub}|{nonce}|{ts}|{cfg_json}")
}

/// A signed AI-config command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedAiCfg {
    pub kind: String,
    pub daemon_pub: String,
    pub nonce: String,
    pub ts: u64,
    /// The AI config as a JSON string (parsed into config::AiConfig).
    pub cfg: String,
    pub sig: String,
}

/// Verify a signed AI-config command against the parent key.
pub fn verify_aicfg(vk: &VerifyingKey, a: &SignedAiCfg) -> bool {
    let sig_bytes = match STANDARD.decode(a.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    vk.verify_strict(aicfg_message(&a.daemon_pub, &a.nonce, a.ts, &a.cfg).as_bytes(), &sig).is_ok()
}

/// Verify a signed schedule command against the parent key.
pub fn verify_schedule(vk: &VerifyingKey, s: &SignedSchedule) -> bool {
    let sig_bytes = match STANDARD.decode(s.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    vk.verify_strict(schedule_message(&s.daemon_pub, &s.nonce, &s.child, s.ts, &s.sched).as_bytes(), &sig).is_ok()
}

/// Domain for a parent-pushed autonomy-mode change (ask|quiet|auto) + optional "ask-about" undo.
pub const AUTONOMY_DOMAIN: &str = "OCPG-AUTONOMY-v1";

/// Bytes the parent app signs to set the local-model autonomy mode. `child` is reserved for a future
/// per-child split ("" = the whole installation today, which is how autonomy state is kept); `mode` is
/// `ask`|`quiet`|`auto`; `ask_about` is an optional package fragment to re-route to the parent (undo a
/// wrong denial), "" when unused. Signed verbatim, like a schedule/AI-config, so there is no ambiguity.
pub fn autonomy_message(daemon_pub: &str, nonce: &str, child: &str, ts: u64, mode: &str, ask_about: &str) -> String {
    format!("{AUTONOMY_DOMAIN}|{daemon_pub}|{nonce}|{child}|{ts}|{mode}|{ask_about}")
}

/// A signed autonomy-set command posted by the parent app.
#[derive(Debug, Deserialize)]
pub struct SignedAutonomy {
    /// Discriminator ("set_autonomy").
    pub kind: String,
    pub daemon_pub: String,
    pub nonce: String,
    /// Target child ("" = the single/global installation; reserved for a future per-child split).
    #[serde(default)]
    pub child: String,
    pub ts: u64,
    /// `ask` | `quiet` | `auto`.
    pub mode: String,
    /// Optional package fragment to add to the always-ask list (undo a wrong denial); "" when unused.
    #[serde(default)]
    pub ask_about: String,
    pub sig: String,
}

/// Verify a signed autonomy command against the parent key.
pub fn verify_autonomy(vk: &VerifyingKey, a: &SignedAutonomy) -> bool {
    let sig_bytes = match STANDARD.decode(a.sig.trim()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let arr: [u8; 64] = match sig_bytes.try_into() {
        Ok(a) => a,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&arr);
    vk.verify_strict(
        autonomy_message(&a.daemon_pub, &a.nonce, &a.child, a.ts, &a.mode, &a.ask_about).as_bytes(),
        &sig,
    )
    .is_ok()
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

/// The parent's X25519 public key, derived from its Ed25519 public key (libsodium
/// crypto_sign_ed25519_pk_to_curve25519), for payload encryption (M4).
fn parent_x25519_public(parent_ed25519_pub_b64: &str) -> Option<crypto_box::PublicKey> {
    let vk = parse_pubkey(parent_ed25519_pub_b64)?;
    Some(crypto_box::PublicKey::from(vk.to_montgomery().to_bytes()))
}

/// Encrypt `plaintext` so ONLY the parent (holder of the paired Ed25519 key) can read it: a
/// libsodium-compatible sealed box (crypto_box_seal) to the parent's X25519 key. The broker sees only
/// ciphertext, so package names / commands no longer travel in the clear (review M4). base64 output.
pub fn seal_to_parent(parent_ed25519_pub_b64: &str, plaintext: &[u8]) -> Option<String> {
    let pk = parent_x25519_public(parent_ed25519_pub_b64)?;
    let ct = pk.seal(&mut crypto_box::aead::OsRng, plaintext).ok()?;
    Some(STANDARD.encode(ct))
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

    fn sign_sched(sk: &SigningKey, dpub: &str, nonce: &str, child: &str, ts: u64, json: &str) -> SignedSchedule {
        let sig = sk.sign(schedule_message(dpub, nonce, child, ts, json).as_bytes());
        SignedSchedule {
            kind: "set_schedule".into(),
            daemon_pub: dpub.into(),
            nonce: nonce.into(),
            child: child.into(),
            ts,
            sched: json.into(),
            sig: STANDARD.encode(sig.to_bytes()),
        }
    }

    fn parent_x25519_secret_from_seed(seed: &[u8; 32]) -> crypto_box::SecretKey {
        use sha2::{Digest, Sha512};
        let h = Sha512::digest(seed);
        let mut s = [0u8; 32];
        s.copy_from_slice(&h[..32]);
        s[0] &= 248;
        s[31] &= 127;
        s[31] |= 64;
        crypto_box::SecretKey::from(s)
    }

    #[test]
    fn seal_roundtrip_to_parent() {
        let seed = [7u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let pub_b64 = pubkey_b64(&sk);
        let secret = "steam / sudo pacman -S steam".as_bytes();
        let ct = seal_to_parent(&pub_b64, secret).expect("seal");
        // The parent (with its Ed25519 seed → X25519 secret) can open it.
        let skx = parent_x25519_secret_from_seed(&seed);
        let bytes = STANDARD.decode(ct).unwrap();
        let pt = skx.unseal(&bytes).expect("unseal");
        assert_eq!(pt, secret);
    }

    #[test]
    fn m4_app_vector() {
        // Interop vector for the Android app's crypto_box_seal_open: a fixed seed + the exact sealed
        // payload shape the daemon emits (M4 step 2). Run with `--nocapture` to print a concrete `enc`
        // to hard-code in the app's CryptoTest (sealed boxes are non-deterministic, so each run differs;
        // any one blob stays openable by the recipient key forever). Here we only assert round-trip.
        let seed = [0x11u8; 32];
        let sk = SigningKey::from_bytes(&seed);
        let pub_b64 = pubkey_b64(&sk);
        let plain = r#"{"source":"pacman","package":"steam","reason":"sudo pacman -S steam"}"#;
        let enc = seal_to_parent(&pub_b64, plain.as_bytes()).expect("seal");
        let skx = parent_x25519_secret_from_seed(&seed);
        let pt = skx.unseal(&STANDARD.decode(&enc).unwrap()).expect("unseal");
        assert_eq!(pt, plain.as_bytes());
        let seed_hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        println!("M4_VECTOR seed_hex={seed_hex}");
        println!("M4_VECTOR parent_pub_b64={pub_b64}");
        println!("M4_VECTOR plaintext={plain}");
        println!("M4_VECTOR enc_b64={enc}");
    }

    #[test]
    fn valid_schedule_verifies() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let s = sign_sched(&sk, &dpub, "n1", "zosia", 1_000, r#"{"daily_budget_minutes":90}"#);
        assert!(verify_schedule(&sk.verifying_key(), &s));
    }

    #[test]
    fn tampered_schedule_fails() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let mut s = sign_sched(&sk, &dpub, "n1", "zosia", 1_000, r#"{"daily_budget_minutes":90}"#);
        s.sched = r#"{"daily_budget_minutes":600}"#.into(); // bump the budget after signing
        assert!(!verify_schedule(&sk.verifying_key(), &s), "editing the schedule must break the signature");
    }

    fn sign_autonomy(sk: &SigningKey, dpub: &str, nonce: &str, child: &str, ts: u64, mode: &str, ask: &str) -> SignedAutonomy {
        let sig = sk.sign(autonomy_message(dpub, nonce, child, ts, mode, ask).as_bytes());
        SignedAutonomy {
            kind: "set_autonomy".into(),
            daemon_pub: dpub.into(),
            nonce: nonce.into(),
            child: child.into(),
            ts,
            mode: mode.into(),
            ask_about: ask.into(),
            sig: STANDARD.encode(sig.to_bytes()),
        }
    }

    #[test]
    fn valid_autonomy_verifies() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let a = sign_autonomy(&sk, &dpub, "n1", "", 1_000, "auto", "");
        assert!(verify_autonomy(&sk.verifying_key(), &a));
    }

    #[test]
    fn tampered_autonomy_mode_fails() {
        let sk = signing_key();
        let dpub = pubkey_b64(&sk);
        let mut a = sign_autonomy(&sk, &dpub, "n1", "", 1_000, "quiet", "");
        a.mode = "auto".into(); // escalate the mode after signing
        assert!(!verify_autonomy(&sk.verifying_key(), &a), "changing the mode must break the signature");
    }
}
