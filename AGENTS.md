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
  retains mirrored entries + alarms on heartbeat gap; SNI/DPI for 443 tunnels.
- PRODUCT FEATURES (design via agy, order: schedule → DNS/SafeSearch → Wayland enforcer):
  **schedule engine DONE** (daemon/src/schedule.rs: window + daily budget + grant + day rollover +
  idle-aware; 6 unit tests; config `[schedule]`, not yet enforced).
- **DNS filter + SafeSearch config SHIPPED** in packaging/dns/: dnsmasq-guardian.conf (:5353, upstream
  1.1.1.3), safesearch.conf (exact IPs from agy, v4+v6, only search hosts), 10-guardian-dns-redirect.nft
  (per-uid :53→:5353, ip+ip6, validated via unshare — note: chain can't be named `redirect`, nft keyword),
  guardian-blocklist-update.sh (StevenBlack, validated+atomic+HUP; logic-tested via file://), systemd
  dnsmasq+updater+timer, README with dig tests. (Ollama was asked for the update script but refused —
  returned prose describing the blocklist instead of code — so I wrote it; lesson: adult-content-adjacent
  prompts derail the local model.) No dnsmasq/dig in sandbox → live resolve test is on-host.
- **Enforcer CORE DONE**: daemon holds the TimeEngine, ticks via libc local-time, `guardian-ctl schedule`
  status, and **grant_time over the signed ntfy channel** (crypto::SignedGrant OCPG-GRANT-v1; parent
  grants +X min from phone; Ed25519-verified + nonce anti-replay → TimeEngine.grant_minutes via an mpsc
  sink; audited GrantApplied). Verified e2e (99→129 min, replay rejected). On block transition the daemon
  audits ScheduleBlocked + logs "invoke lock hook". REMAINING (host-side, needs live Hyprland): logind
  idle detection (active= is stubbed true) + actual session lock (loginctl lock-session / systemctl
  freeze user-<uid>.slice). Also SNI/DPI for 443. 18 unit tests total.

## On-host test report (2026-10-21, commit c5b90de) — see guardian-test-report-2026-10-21.md
User ran the real-hardware test (parent `wasyl`, child `dziecko` uid 1001). Core loop works
(pacman hold/allow/deny/timeout fail-closed, forged approval rejected+alerted, watchdog, audit).
Verdict: still NOT a child's sole protection. Fixes shipped v0.2.0 (commit 72561f4):
- B1 policy.example daemon_key_path → /var/lib/guardian (crashed on fresh install under ProtectSystem).
- B8 service worker now network-first + bumped cache (phone was stuck on old OCPG-v1 format forever).
- N9 unparseable/old-format decision now logs + alerts "update the app" + audits ApprovalRejected
  (this silent drop had hidden B8). Verified e2e.
- N5 rejected/forged approvals now write an audit entry (ntfy got an audit handle).
- N2 control socket 0666 so a child's connect is seen → rejected+alerted+audited by SO_PEERCRED.
- B4 install.sh installs Firefox/Chromium DoH-off policies by default (Firefox built-in DoH bypassed
  the nftables IP list). IP list stays best-effort; SNI filtering still the real fix.
- B3 documented honestly: flatpak wrapper is ADVISORY; fapolicyd enforcing is REQUIRED to stop
  `/usr/bin/flatpak --user` + portable binaries. install.sh NEXT step upgraded to *** REQUIRED ***.
- N12 pairing: 'paired' set only when the key is actually stored (cancel no longer lies).
- STILL on-host (user): §7 fapolicyd enforce (closes B3), §12 boot/LUKS snapshot (H4), §9/13 child GUI.

## Native Android parent app (in progress) — android-app/
User: "potrzebujemy aplikacji na androida a nie strony www" (PWA is cumbersome: no background push,
stale cache, key lost in private tabs — B8/N11). Built a NATIVE Kotlin app (no daemon change needed;
same crypto the daemon already trusts). Architecture from agy.
- Crypto.kt: raw Ed25519 via BouncyCastle (bcprov-jdk18on 1.78.1), byte-identical to dalek/noble.
  **Verified on the JVM here** against a shared vector: pubkey + decision sig + grant sig match,
  genuine OCPG-CH-v2 challenge verifies, tampered one rejected. CryptoTest.kt pins it (runs in CI).
- NtfyService.kt: foreground service (type specialUse) + OkHttp WebSocket to ntfy /<topic>/ws with
  45s ping + backoff reconnect; verifies csig before showing anything; notification Allow/Deny →
  DecisionReceiver signs OCPG-v2 on-device and POSTs to respond_to. Signed alerts verified+shown.
- MainActivity: QR-scan pairing (ZXing), copy pubkey, battery-optimisation opt-out, +15/+30 grant
  (OCPG-GRANT-v2 to last respond_to), recent-request history. Prefs.kt = EncryptedSharedPreferences.
- CI: .github/workflows/android.yml runs crypto tests + builds a debug APK every push, attaches an
  APK to a GitHub release on tag v* (like yt-downloader). Signing via KEYSTORE_B64 secret optional.
- NOT compiled on a device here (no Android SDK in sandbox) — crypto core proven, Android wiring is a
  first cut to validate via CI/Android Studio, same iterate-on-report loop as the daemon on-host tests.
- 2026-10-21/22: APK built in CI, downloaded and **installed on the user's phone via adb** (adb at
  ~/Android/Sdk/platform-tools/adb; device over ADB-WiFi) — app launches. To push a new build: download
  the CI artifact `guardian-parent-apk` and `adb install -r`.
- BUILD STACK DECISION: user's Android Studio generated a full wrapper (Gradle 9.3.0) + AGP 8.13.2 and
  built locally. Adopted that stack as the single source of truth (committed gradlew + wrapper jar) and
  switched CI to `./gradlew`, so Studio and CI are identical — no version drift. App bumped to 0.1.1
  (versionCode 2). If CI ever rejects AGP 8.13.2, fall back to the proven AGP 8.5.2 / Gradle 8.9 stack.
- Follow-ups: iOS app; payload encryption to parent key (M4); heartbeat-gap alarm in the app; app icon.

## Child onboarding + multi-child (2026-10-22)
- packaging/setup-child.sh <name> [--name "Full Name"] [--parent <p>]: one command, run once per child.
  Creates a NAMED unprivileged account (no wheel/sudo/root; GECOS display name), adds only desktop
  groups (video/audio/input/render/seat), ADDS the child's uid to the nftables + DNS uid SETS
  (accumulates across kids), writes a guardiand drop-in Environment=GUARDIAN_CHILD=<space list>, and
  prints the pairing QR. Idempotent.
- packaging/set-parent-key.sh <pubkey>: stores the phone's pubkey into policy [ntfy].parent_pubkey,
  enables signed mode, restarts guardiand — removes the manual TOML edit. So pairing = scan QR + one paste.
- MULTI-CHILD: nft egress + dns-redirect switched from `define CHILD_UID = N` to `define CHILD_UIDS =
  { .. }` (uid set); guardian-lock.sh/unlock.sh now loop over the GUARDIAN_CHILD list. nft validated
  with 2 kids via unshare. (Per-child screen-time BUDGETS still global — follow-up; TimeEngine is one.)
- FULL COMMAND to parent (B-advanced): the exact invoked command rides the SIGNED `reason` field
  (OCPG-CH-v2, tamper-proof). guardian-hook reads it from /proc/<ppid>/cmdline (pacman/yay/paru);
  flatpak/snap/nix wrappers pass `$(basename $0) $*`. Android notification shows it expanded ("Command:").
  AI summary of the command = next step (needs the AI assistant pillar; not built yet).
- Versions bumped: daemon 0.2.1, android app 0.1.2 (versionCode 3). 18 unit tests green.

## AI risk-summary (2026-10-22) — daemon/src/ai.rs + config [ai]
- Daemon asks a model for a 2-sentence "what is this package + child-safety/security concern" ending
  in "RISK: low|medium|high", and folds it into the SIGNED challenge reason (OCPG-CH-v2) — so the
  verdict is tamper-proof and already displayed; no protocol change, no new field.
- Provider pluggable: "ollama" (POST /api/generate) default, or "openai" (OpenAI-compatible). Default
  endpoint = user's LAN Ollama http://192.168.0.11:11434, model qwen3.8:latest (think:false to suppress
  reasoning). Privacy-first (local). Off by default. Any failure/timeout → summary omitted, never blocks.
- Enriched reason computed BEFORE signing (don't hold the ai Mutex across .await — cloned out first).
- Android app shows a 🟢/🟡/🔴 risk light (parsed from "RISK:") + full "Details:" (command + AI note).
- VERIFIED LIVE on real Ollama: steam → "RISK: medium" summary in the challenge; csig verifies over the
  enriched reason; downgrading the verdict to "low" breaks the signature. 20 unit tests (added 2 ai).
- Versions: daemon 0.2.2, android 0.1.4.

## Auto-pairing (2026-10-22)
- QR now carries `pair_to` (= `<request_topic>-pair`). The app, right after scanning, POSTs its own
  pubkey there ({kind:"parent_key",pubkey}) and has a "Send my key to PC" button as fallback.
- packaging/pair.sh <topic> [server]: shows the QR, polls the pair topic (~2 min), captures the key,
  prints it + a sha256 fingerprint, the parent confirms it matches the app, then runs set-parent-key.sh.
  So pairing = scan the QR + confirm on the PC — no copy/paste. Injection by a topic-guesser is
  defeated by the parent comparing the fingerprint. Capture logic verified e2e over real ntfy.
- setup-child.sh now points at pair.sh. Versions: daemon 0.2.3, android 0.1.5.

## App localization — EN / PL / ES (2026-10-22)
- All user-facing text in the Android app is now in string resources (was hardcoded). `values/strings.xml`
  = English (default), `values-pl/` = Polish, `values-es/` = Spanish. Covers the whole UI: layout labels
  + buttons, every MainActivity toast/status, and the NtfyService notifications (foreground, install
  challenge title/summary/Details, Allow/Deny action labels, alert, channel names). `req_summary`,
  `approve_title`, `grant_ok` use positional format args (`%1$s` / `%1$d`).
- `build.gradle.kts`: `resourceConfigurations += listOf("en","pl","es")` (ships only these three). The
  phone's locale auto-selects (pl-PL device → Polish; anything else → English fallback). The signed
  protocol/canonical messages are untouched — only presentation is translated.
- Verified: aapt2 dump shows (default)/(pl)/(es) variants compiled in; built + installed in place on the
  user's phone (pl-PL) as 0.1.6 / versionCode 7 — displays Polish. The fixed debug key made it an
  in-place update (no uninstall), confirming the signing fix.
- Follow-up if needed: Android 13+ per-app language picker (locales_config.xml + android:localeConfig)
  to override the system locale; DE/FR/ZH to match the README languages.
- **First public APK download**: GitHub Release **v0.1.6** created (`gh release create`) with
  `guardian-parent-0.1.6.apk` attached — permanent no-login link at /releases/latest. Before this there
  was NO release/tag, only the expiring CI workflow artifact. android-app/README.md now leads with a
  "Download & install" section (Releases link + direct asset URL + 4 phone steps) and documents the
  `git tag v* && git push` → Release flow. Future releases: bump versionCode/Name, tag v*, push — CI
  attaches the APK. The workflow now **renames the asset to `guardian-parent-<version>.apk`** (version =
  tag minus the leading `v`, prefers the signed release APK else debug) and attaches only that one file,
  so there is no raw app-debug.apk duplicate to clean up anymore.

## On-host test report #2 (2026-10-23, commit 8a6e8ae) + fixes — see guardian-{test-report,fixes}-2026-10-23.md
User (the tester) ran fapolicyd end-to-end for the first time + child GUI session. Serious findings;
fixed across 3 batches (daemon 0.2.4→0.2.5, app 0.1.6→0.1.7/vc8). Docs committed to repo.
- **Batch 1 — critical P0** (commits 56d7e00, 01a2bed, b1d13d6):
  - C0: stopped shipping `vm.memfd_noexec=2` (made next boot fail — dbus-broker/API bus). Removed file.
  - C1/F1/F2/F3/F14: new `packaging/fapolicyd/guardian-fapolicyd-setup.sh` — the correct Arch rollout.
    allow_filesystem_mark=1 (closes bwrap/unshare bind-mount bypass), trust.d seeded as `path size
    sha256` dropping `\`/space paths, rules renamed 50→**05-guardian.rules** (precede upstream 42/43
    that denied the parent/services), Arch prereqs (sysuser, trust=file, buildroot/ld_so), trusts
    Guardian's own non-pacman files. README rewritten; install.sh #3 points at it.
  - F4: install.sh `chattr -i`s managed files first (watchdog re-immutables → re-run no longer strands TEST mode).
  - F8: daemon.pub 0644 + /var/lib/guardian 0755; `guardian-ctl pair` warns loudly if it can't read the key.
  - F6: control socket 0666-by-design documented (N2 visibility, not a regression).
- **Batch 2 — session hardening** (commit d08a400): the worst "sole protection" gaps (login-level).
  - F13: setup-child.sh writes logind drop-in NAutoVTs=0/ReserveVT=0 + masks getty@tty2..6/autovt@;
    harden-audit checks configured NAutoVTs (was a false PASS).
  - F18: guardian-lock.sh adds `pkill -STOP -u <uid>` (tty login runs outside user slice); unlock `-CONT`.
  - F17: lock/notify once per episode via /run/guardian/locked-<uid> stamp (was every tick).
  - F12: setup-child.sh detects+offers to remove a display-manager autologin into a non-child; warns
    about Omarchy's single-user SDDM theme; harden-audit FAILs on non-child autologin.
- **Batch 3 — parent-app gaps + F15** (commit e91f2d0):
  - In-app Allow/Deny list (NtfyService records verified challenges → Prefs pending → MainActivity
    cards; shared Decisions.send; 15-min TTL). Decide in the app, not only the push.
  - Instant Lock: signed **OCPG-LOCK-v1** (crypto::SignedLock/verify_lock) → daemon lock_override OR'd
    into the schedule tick's `blocked`; app "Lock/Unlock now" button (Crypto.signLock). Needs
    [schedule].lock_command.
  - F15: response topic now deterministic `<request_topic>-resp` (was random/run) — grants+locks
    survive daemon restart + work right after pairing; app derives it in Prefs.lastRespondTo.
- STILL OPEN (documented): B3 (flatpak --user unaudited install; C1 stops execution), B4 (DoH to
  unlisted resolvers — needs SNI/maintained list). NEEDS ON-HOST VERIFY: fapolicyd setup script,
  console/autologin hardening, lock-now, pkill backstop.
- **Batch 4 — IN PROGRESS** (schedule in-app, multi-child = BOTH topologies per user). Done so far:
  - Step 1 (39e54d6): schedule engine → weekly per-day budgets/windows ([schedule.days.mon..sun]) +
    nightly curfew quiet_start/end (wraps midnight, not charged). local_now() adds weekday. AI default
    → llama3.2 @ 127.0.0.1 (child box is 192.168.0.12, RTX 3070 8 GB; .11 = the 3090 dev box). daemon 0.2.6.
  - Step 2 (e870e52): PER-CHILD (multi-child) engines in the daemon — one TimeEngine per GUARDIAN_CHILD
    name, each its effective schedule (Schedule::effective_for + [schedule.children.<name>]) + own state
    file <base>.<name>; tick iterates all, locks per child (run_hook sets GUARDIAN_CHILD); ipc status
    aggregates. Server.schedule is now Vec<(name,engine)>. Grants/lock-now still apply to ALL children.
    daemon 0.2.7.
  - Step 3 (b419ca7 daemon + ea35318 app): signed schedule-PUSH from the app. crypto OCPG-SCHED-v1
    (SignedSchedule/verify_schedule, signs the sched JSON verbatim). ntfy handle_schedule → sched sink →
    guardiand applies to the child's engine (set_schedule + with_child_override) + persists
    <base>.<name>.sched (overrides config on boot). App: "Set schedule" editor (budget, window HH:MM,
    curfew HH:MM, weekend; optional child name) → Crypto.signSchedule → POST. daemon 0.2.8, app 0.1.8/vc9.
    Covers several kids on ONE PC (child field). 29 daemon tests.
  - REMAINING: multi-PC app (multi-pairing list + per-child ntfy subscriptions + child selector; today
    the app holds ONE pairing); per-child targeting for grant/lock-now (add `child` to OCPG-GRANT/LOCK);
    F16 (feed logind active so budget isn't charged by wall clock / while frozen); live "current app/game"
    reporter (hyprctl → daemon → app); daily AI digest (usage stats → local Ollama → push).

## Workflow routing (this machine)
- **Logic / architecture / integrations →** `agy`.
- **Simple implementation micro-tasks →** local Ollama `qwen3-coder:30b` via `aider` (host 192.168.0.11).
- This file is the status file (keep it current).

## Hard principles
- Parent phone app: **simple + transparent + 100% effective** (no silent failures, no bypass).
- Privacy-first: no telemetry; cloud only relays the parent's decision.
- Zero-Bypass enforcement (Polkit, cgroups v2, nftables/eBPF).
- Every build stage must end with something testable.
