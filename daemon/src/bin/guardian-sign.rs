//! `guardian-sign` — dev/pairing helper that stands in for the parent app.
//!
//! The real parent app (PWA/native) generates the keypair and signs on the phone; the private key
//! never leaves the device. This CLI mirrors that logic for pairing demos and tests.
//!
//!   guardian-sign keygen
//!   guardian-sign sign --privkey <b64> --id <uuid> --decision allow|deny --nonce <hex> [--ts <unix>]

use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use uuid::Uuid;

use guardian::crypto::canonical_message;
use guardian::request::Decision;

fn urandom_seed() -> [u8; 32] {
    let mut f = std::fs::File::open("/dev/urandom").expect("open /dev/urandom");
    let mut b = [0u8; 32];
    f.read_exact(&mut b).expect("read /dev/urandom");
    b
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keygen") => {
            let seed = urandom_seed();
            let sk = SigningKey::from_bytes(&seed);
            let vk = sk.verifying_key();
            println!("privkey = \"{}\"   # keep secret; in the real app this stays on the phone", STANDARD.encode(seed));
            println!("pubkey  = \"{}\"   # put this in policy.toml [ntfy].parent_pubkey", STANDARD.encode(vk.to_bytes()));
        }
        Some("sign") => {
            let privkey = flag(&args, "--privkey").expect("--privkey <b64> required");
            let id = Uuid::parse_str(&flag(&args, "--id").expect("--id <uuid> required"))
                .expect("invalid --id uuid");
            let decision: Decision = flag(&args, "--decision")
                .expect("--decision allow|deny required")
                .parse()
                .expect("invalid --decision");
            let nonce = flag(&args, "--nonce").expect("--nonce <hex> required");
            let ts = flag(&args, "--ts")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());

            let seed: [u8; 32] = STANDARD
                .decode(privkey.trim())
                .expect("privkey not base64")
                .try_into()
                .expect("privkey must be 32 bytes");
            let sk = SigningKey::from_bytes(&seed);

            let msg = canonical_message(&id, decision, &nonce, ts);
            let sig = sk.sign(msg.as_bytes());

            let d = match decision {
                Decision::Allow => "allow",
                Decision::Deny => "deny",
            };
            let payload = serde_json::json!({
                "id": id,
                "decision": d,
                "nonce": nonce,
                "ts": ts,
                "sig": STANDARD.encode(sig.to_bytes()),
            });
            // Print the exact JSON the parent app POSTs to the response topic.
            println!("{}", payload);
        }
        _ => {
            eprintln!("usage:");
            eprintln!("  guardian-sign keygen");
            eprintln!("  guardian-sign sign --privkey <b64> --id <uuid> --decision allow|deny --nonce <hex> [--ts <unix>]");
            std::process::exit(2);
        }
    }
}
