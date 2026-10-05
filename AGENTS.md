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

## Next (Stage 2, logic/integration → agy)
- Real interception: Polkit rule + PAM hook + flatpak wrapper feeding `ClientMessage::Submit`.
- Remote push-approval via ntfy; then Intent Binding (frozen cache + Ed25519).

## Open decisions (need user)
- (none blocking) — proceed with ntfy MVP + Polkit interception next.

## Workflow routing (this machine)
- **Logic / architecture / integrations →** `agy`.
- **Simple implementation micro-tasks →** local Ollama `qwen3-coder:30b` via `aider` (host 192.168.0.11).
- This file is the status file (keep it current).

## Hard principles
- Parent phone app: **simple + transparent + 100% effective** (no silent failures, no bypass).
- Privacy-first: no telemetry; cloud only relays the parent's decision.
- Zero-Bypass enforcement (Polkit, cgroups v2, nftables/eBPF).
- Every build stage must end with something testable.
