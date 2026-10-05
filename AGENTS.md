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

## Open decisions (need user)
- (none blocking) — ntfy MVP is next after interception is wired on a real machine.

## Workflow routing (this machine)
- **Logic / architecture / integrations →** `agy`.
- **Simple implementation micro-tasks →** local Ollama `qwen3-coder:30b` via `aider` (host 192.168.0.11).
- This file is the status file (keep it current).

## Hard principles
- Parent phone app: **simple + transparent + 100% effective** (no silent failures, no bypass).
- Privacy-first: no telemetry; cloud only relays the parent's decision.
- Zero-Bypass enforcement (Polkit, cgroups v2, nftables/eBPF).
- Every build stage must end with something testable.
