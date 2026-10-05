// ocpg-sign.mjs — Node mirror of the parent app's signing, used to prove JS(noble) ↔ Rust(dalek)
// byte-compatibility over the exact canonical message. The real app uses the identical logic.
//
//   node ocpg-sign.mjs keygen
//   node ocpg-sign.mjs sign --privkey <b64> --id <uuid> --decision allow|deny --nonce <hex> [--ts <unix>]
import * as ed from "../vendor/noble-ed25519.js";

const b64 = (u8) => Buffer.from(u8).toString("base64");
const unb64 = (s) => new Uint8Array(Buffer.from(s, "base64"));
const DOMAIN = "OCPG-v1";

function canonical(id, decision, nonce, ts) {
  return `${DOMAIN}|${id}|${decision}|${nonce}|${ts}`;
}
function flag(args, name) {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}

const [cmd, ...args] = process.argv.slice(2);

if (cmd === "keygen") {
  const seed = new Uint8Array(32);
  globalThis.crypto.getRandomValues(seed);
  const pub = await ed.getPublicKeyAsync(seed);
  console.log("privkey=" + b64(seed));
  console.log("pubkey=" + b64(pub));
} else if (cmd === "sign") {
  const seed = unb64(flag(args, "--privkey"));
  const id = flag(args, "--id");
  const decision = flag(args, "--decision");
  const nonce = flag(args, "--nonce");
  const ts = Number(flag(args, "--ts") ?? Math.floor(Date.now() / 1000));
  if (!["allow", "deny"].includes(decision)) throw new Error("decision must be allow|deny");
  const msg = new TextEncoder().encode(canonical(id, decision, nonce, ts));
  const sig = await ed.signAsync(msg, seed);
  console.log(JSON.stringify({ id, decision, nonce, ts, sig: b64(sig) }));
} else {
  console.error("usage: keygen | sign --privkey <b64> --id <uuid> --decision allow|deny --nonce <hex> [--ts <unix>]");
  process.exit(2);
}
