# AGENTS.md — status & working notes

**Project:** Omarchy Child Protect Guardian — parental controls + remote install approval for Omarchy Linux.
**Created by:** wasyleque · Support: PayPal wasyl@o2.pl
**Phase:** 1 — daemon core built & tested.

## Current status
- Name DECIDED: **Omarchy Child Protect Guardian**. License: GPL-3.0. Repo public.
- Multilingual README (EN/PL/ES/DE/FR/ZH) + concept, architecture, roadmap docs written.
- Concept adopted (2× `agy` brainstorm): Zero-Bypass, Playtest Sandbox, AI-TL;DR,
  scoped remote-root via Intent Binding, LACS age-rating feed, AI risk assistant,
  usage stats + gentle AI parenting digest, parent client on every leading OS.
- **STAGE 1 DONE** (`daemon/`, Rust crate `guardian`): `guardiand` holds an install request
  and blocks until a decision; `guardian-ctl` lists/allow/deny/request over a Unix socket
  (newline-JSON). TOML policy with fail-closed timeout. Tested end-to-end: hold→allow,
  timeout→DENY (fail-closed), explicit deny, bad-input rejection; 3 unit tests pass; builds clean.
- Decided: phone MVP = **ntfy first** (abstraction lets PWA+Ed25519 land in v2); content filter
  not in Stage 1.

## Stage 1 build notes (routing worked as intended)
- Core (contract/queue/ipc/main) written by me (correctness-critical).
- Delegated to local Ollama via aider (`qwen3-coder:30b`): `config.rs` load, `guardian-ctl.rs`,
  `guardiand.service`, `daemon/README.md`. All gated by `cargo build`/tests.
- Ollama issues caught & fixed: 3 type-shape compile bugs in `guardian-ctl.rs`; `config.rs`
  read-error handling refined; aider's whole-file mode spawned 2 stray junk files from fenced
  code blocks in the README task (removed) — watch for this on doc/markdown delegations.

## Stage 2 — interception (in progress)
- DESIGN (agy): pacman via **ALPM PreTransaction hook**, NOT Polkit (Polkit does not mediate
  terminal `sudo pacman`). `Operation = Install` so `-Syu` upgrades pass through; exit≠0 aborts the
  whole transaction (fail-closed); also catches `yay`/`paru`/`pacman -U` (all use libalpm).
- DONE: `guardian-hook` binary — reads target package names on stdin, sends one `Submit`, exits 0 on
  allow / 1 on deny / 1 (fail-closed) if guardiand unreachable. Tested non-destructively (allow→0,
  deny→1, multi-target join, no-daemon→1). Hook files in `packaging/` (`50-guardian.hook` +
  scoped `99-guardian-test.hook` for the safe `pacman -S sl` test). Arch docs corrected.
- CAVEAT: with `Target = *`, if guardiand is down no new installs succeed (fail-closed by design).
- DONE: **remote push-approval via ntfy** (design by agy; module written by me — integration/security,
  not delegated). `guardiand` publishes Allow/Deny action buttons to `request_topic`; parent's tap
  POSTs a one-time-token decision to a random response topic; daemon verifies token + resolves.
  Tested end-to-end against real ntfy.sh (allow round-trip) and forged-token rejected → fail-closed DENY.
  Config `[ntfy]` section; `reqwest`/rustls; `guardian-ctl` still a local override.
- TODO: real `pacman -S sl` test on a machine (needs sudo — user runs the documented non-destructive
  test); `-Syu` new-dependency edge case; Flatpak mechanism (libflatpak, not libalpm); Intent Binding
  (frozen cache + Ed25519) for true zero-trust; parent app (PWA) replacing the raw ntfy app.

## Security / anti-bypass (see docs/THREAT_MODEL.md)
- Full threat model written (red-teamed with agy). KEY TRUTH: gating the installer is not enough —
  kids run AppImage/`curl|sh`/flatpak --user from $HOME. Real coverage = layers, default-deny:
  L1 install-intercept · L2 execution allowlist (fapolicyd) · L3 network egress deny (nftables,
  DoH/DoT/VPN block) · L4 account/session · L5 Ed25519 signed approvals · L6 hardware baseline
  (UEFI pw + Secure Boot + UKI/locked GRUB + LUKS) · L7 tamper-evidence/alert. No "zero gaps"
  without L6 — stated honestly; CMOS reset / other devices are out of scope.
- DONE hardenings: SO_PEERCRED on IPC (child uid refused); `guardian-hook` ignores GUARDIAN_SOCKET
  in release (fixed path); **Ed25519 signed-approval mode** (daemon/src/crypto.rs + ntfy signed mode;
  `[ntfy].parent_pubkey` → accepts only signed decisions; `guardian-sign` dev helper simulates the
  phone). Verified e2e vs real ntfy.sh: legit allow works; broker decision-flip + bogus signature
  both rejected → fail-closed DENY. 7 unit tests (queue+crypto) + e2e green.
- P1 config SHIPPED (validated, NOT applied to any host): `packaging/nftables/guardian-egress.nft`
  (egress deny; force DNS; drop DoT; block DoH v4/v6; kill UDP/QUIC/VPN — validated via `unshare -rn nft -f`),
  `packaging/fapolicyd/` (exec allowlist rules + trust-seeding + permissive rollout), read-only
  `packaging/harden-audit.sh` (UEFI/SecureBoot/LUKS/bootloader/guardian/account checks), and
  `packaging/README.md` (ordered deploy guide + rollback per layer).
- **Parent app (PWA) DONE** in `parent-app/`: installable, cross-OS; generates+stores Ed25519 key in
  IndexedDB (never leaves device), subscribes to ntfy via SSE, signs Allow/Deny on-device, POSTs the
  signed decision. Vendored `@noble/ed25519` (no runtime CDN; offline shell via sw.js). Signing proven
  byte-compatible with the Rust daemon end-to-end (JS-generated key+sig verified via live ntfy). JS
  syntax + served-file smoke checked. Honest follow-ups: background push (iOS), QR pairing, key-at-rest.
- fapolicyd **auto-trust DONE** (design): PostTransaction pacman hook `packaging/60-guardian-trust.hook`
  + `packaging/fapolicyd/guardian-trust.sh` trust the files of any approved install (PreTransaction
  already blocked the rest); logic verified non-destructively with a stub fapolicyd-cli. No daemon change.
- Parent app hosted via GitHub Pages (.nojekyll at root) — URL under wasyleque.github.io/.../parent-app/.
- **L7 parent alerts DONE**: daemon pushes a rate-limited priority-5 ntfy alert on a blocked IPC
  control attempt (unauthorized uid) or a rejected/forged approval (bad token/sig/nonce). Verified e2e
  (forged signed decision → "FORGED approval blocked" push). chattr +i ops step documented in packaging/README.
- REAL end-to-end test needs a CLEAN system on separate hardware (user will set this up): install hooks,
  nftables, fapolicyd, hardware baseline, pair the PWA. Everything so far verified in isolation/e2e-over-ntfy.
- **IPC split DONE**: two sockets — `submit.sock` (0666, any uid may Submit, per-uid flood cap
  `max_pending_per_uid`=5) and `guardian.sock` (0660, control List/Resolve, SO_PEERCRED owner-only).
  guardian-hook + flatpak wrapper → submit; guardian-ctl request → submit, list/allow/deny → control.
  Verified e2e incl. cross-socket rejection. config: submit_socket_path, max_pending_per_uid.
- **Flatpak DONE**: `packaging/flatpak/guardian-flatpak-wrapper.sh` (/usr/local/bin/flatpak; gates
  `flatpak install` via submit socket, verified allow/deny/passthrough with a stub) + polkit rule
  `49-guardian-flatpak.rules` (blocks child system installs via GUI). Residual: direct /usr/bin/flatpak
  + --user GUI → covered by fapolicyd backstop (documented). fapolicyd trust for flatpak apps = follow-up.
- **Append-only hash-chained AUDIT LOG DONE** (daemon/src/audit.rs): records started/submitted/
  decided(+via: local_ctl|ntfy_token|ntfy_signed|timeout)/submit_refused/control_blocked; SHA-256 chain;
  `guardian-ctl audit-verify <path>` detects edits/deletions. config `audit_path`. 11 unit tests + e2e
  incl. tamper. (audit-verify was the Ollama micro-task; aider again wrote to a wrong `guardian/` path →
  moved into place & stray removed. Lesson: run aider from daemon/ and re-check the target path.)
- **QR PAIRING DONE**: `guardian-ctl pair --topic <t> [--server][--app]` prints a terminal QR of
  `<app>#pair=<base64url {server,topic}>`; phone scans → PWA auto-configures (index.html parses #pair=).
  qrcode crate; verified (payload round-trips). (Ollama micro-task; aider pathed correctly this time.)
- **WATCHDOG DONE**: `packaging/guardian-watchdog.{sh,service,timer}` (systemd timer) restores
  hook/daemon/unit from /usr/lib/guardian/backup if missing/altered, restarts guardiand if down,
  runs `audit-verify`, and alerts the parent via a new control-only `guardian-ctl alert` →
  daemon ClientMessage::Alert → ntfy (+audit WatchdogAlert). Verified e2e (push + audit entry).
- **NETWORK LOCKDOWN DONE (config)**: `packaging/network/` — polkit `49-guardian-nm.rules` (child can't
  add/activate/tether NM connections), Firefox `firefox-policies.json` + Chromium `chromium-dns-policy.json`
  (DoH off+locked). JSON validated. Pairs with nftables egress.
- **SNAP/NIX wrappers DONE**: `packaging/snap/guardian-snap-wrapper.sh` + `packaging/nix/guardian-nix-wrapper.sh`
  gate `snap install` / `nix-env -i…` / `nix profile install|add` via the submit socket; InstallSource
  gained Snap+Nix. Stub-verified (allow/deny/passthrough; option-skipping fixed in the snap one after
  Ollama's version broke on `--classic code`). snap wrapper was the Ollama micro-task; nix I wrote.
- **INSTALLER DONE**: `packaging/install.sh` (guided; safe-by-default: daemon+wrappers+watchdog+scoped
  `sl` test hook; `--enforce` switches in the real hook; prints host-specific next steps) + `uninstall.sh`
  (full rollback incl. chattr -i). bash -n clean. For the user's clean-system test tomorrow.
- P1/P2 remaining: deploy+tune on a real host (user, tomorrow); account/session hardening polish; iOS
  background push; conda/other wrappers; AUR-scriptlet/Intent-Binding; one-scan pubkey return (manual paste).

## External security review (2026-10-09, commit bdd62cf) — see docs/REVIEW-RESPONSE.md
Reviewer verdict: do NOT use that commit as a child's sole protection. All 7 findings addressed:
- #1 CRITICAL hooks lacked `AbortOnFail` (denials didn't abort pacman) → FIXED (added; needs on-host pacman verify).
- #2 fapolicyd checked SUBJECT trust not OBJECT → FIXED (rewrote rules; needs on-host permissive validation).
- #3 token mode silent-default + secret in broker + policy 0644 → FIXED (token now opt-in `allow_insecure_token`,
  else remote disabled; installer policy 0600).
- #4 signature didn't bind package/source (broker could swap the card) → FIXED (OCPG-v2 binds source+package;
  daemon rejects mismatch; verified e2e). Residual: challenge itself not yet daemon-signed.
- #5 guardian-ctl honored socket env in release; wrappers used PATH → FIXED (release ignores env; absolute
  /usr/bin/guardian-ctl; verified release-ignores vs debug-honors).
- #6 ProtectSystem blocked audit; /run 0750 blocked child submit → FIXED (LogsDirectory + RuntimeDirectoryMode 0755).
- #7 unbounded conns/line/read/stream → FIXED (64-conn semaphore, 64KiB take, 30s read timeout, 1MiB stream cap).
- Audit: added `.head` truncation anchor + watchdog alerts on MISSING log; still tamper-evident not -proof vs root.
- README now marks status = experimental prototype; content/DNS filter, SafeSearch, screen-time = PLANNED not built.
- STILL NEEDS ON-HOST VERIFY (sandbox can't): real pacman AbortOnFail, fapolicyd enforce, systemd unit start. 12 unit tests green.

## Open decisions (need user)
- (none blocking) — on-host test (user) will confirm #1/#2/#6.
- POST-REVIEW: **daemon-signed challenge DONE** (closes #4 residual): daemon has its own Ed25519 key
  (crypto::load_or_create_daemon_key, config daemon_key_path=/etc/guardian/daemon.key, writes daemon.pub),
  signs each challenge (OCPG-CH-v1, `csig`); pairing QR carries `dpub`; PWA verifies & shows only genuine
  cards. Verified e2e (noble verifies genuine=true, swapped=false).
- POST-REVIEW: **off-box audit mirror DONE** (agy: local HMAC = theater vs root). `[ntfy].audit_topic` →
  daemon publishes every audit entry signed by its key + a daily Heartbeat (dead-man's-switch) the moment
  written. Verified e2e (heartbeat+submitted+decided arrived signed off-box). Remaining: parent app
  retains mirrored entries + alarms on heartbeat gap; SNI/DPI for 443 tunnels; product features
  (DNS filter/SafeSearch/time schedule) — design with agy, simple bits to Ollama.

## Workflow routing (this machine)
- **Logic / architecture / integrations →** `agy`.
- **Simple implementation micro-tasks →** local Ollama `qwen3-coder:30b` via `aider` (host 192.168.0.11).
- This file is the status file (keep it current).

## Hard principles
- Parent phone app: **simple + transparent + 100% effective** (no silent failures, no bypass).
- Privacy-first: no telemetry; cloud only relays the parent's decision.
- Zero-Bypass enforcement (Polkit, cgroups v2, nftables/eBPF).
- Every build stage must end with something testable.
