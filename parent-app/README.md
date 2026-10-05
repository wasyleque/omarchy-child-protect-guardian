# Guardian parent app (PWA)

The cross-platform approval app that closes **signed mode** on the phone side: it holds the parent's
Ed25519 **private key on the device**, receives install requests over ntfy, and returns a **signed**
decision. One codebase installs as an app on Android, iOS, Windows, macOS and Linux. It replaces the
dev `guardian-sign` helper with a real UI.

Signing is byte-compatible with the daemon — proven end-to-end: a key + signature produced by this
app's exact logic (`@noble/ed25519`, vendored in `vendor/`) verifies in the Rust daemon
(`ed25519-dalek`) over a live ntfy round-trip.

## Use it

1. **Host it over HTTPS** (on-device signing requires a secure context). Options:
   - GitHub Pages (serve this `parent-app/` folder), any static host, or `localhost` for testing.
   - A quick local test: `cd parent-app && python3 -m http.server 8080` then open `http://localhost:8080`.
2. Open it on the parent's phone and **Add to Home Screen** (it installs as a standalone app).
3. It generates a device key on first run. **Copy the public key** and paste it into the daemon's
   `/etc/guardian/policy.toml` → `[ntfy].parent_pubkey`. Set the same `request_topic` in both.
4. When the child triggers an install, the daemon pushes `{id, package, nonce, respond_to}`; the app
   shows a card; **Allow/Deny** signs the decision and posts it back. The daemon verifies and acts.

## Why it has no approval gap

- The push carries **no secret** — only the request, a package name and a one-time nonce. Approval
  power is the private key, which never leaves the device; the app only ever transmits a signature.
- Each signature is bound to `OCPG-v1|id|decision|nonce|ts`, so it can't be edited, reused for another
  request, or replayed (the daemon enforces one-time nonce + a fresh-timestamp window).
- The library is **vendored locally** (no runtime CDN) → no supply-chain or offline gap. The app works
  offline except for the live ntfy stream/post.

## Honest limits (follow-ups, not hidden)

- **Background delivery:** while the app is open it streams requests live (SSE, auto-reconnect). For an
  instant buzz when it's closed — especially on iOS — keep the native ntfy app subscribed to the same
  topic as a notifier, or add Web Push (VAPID) later. The *decision* always goes through this app.
- **Key-at-rest:** the seed lives in IndexedDB (never transmitted). A future hardening is a
  non-extractable WebCrypto key or OS-keystore binding; today the guarantee is "never leaves the device".
- **Pairing** is copy/paste of the public key today; a QR flow is a planned nicety.

## Files

`index.html` (app) · `vendor/noble-ed25519.js` (vendored signer) · `manifest.webmanifest` + `sw.js`
(installable, offline shell) · `icon.svg` · `tools/ocpg-sign.mjs` (Node mirror used to prove
JS↔Rust signature compatibility).
