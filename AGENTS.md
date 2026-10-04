# AGENTS.md — status & working notes

**Project:** Omarchy Guardian — parental controls + remote install approval for Omarchy Linux.
**Created by:** wasyleque · Support: PayPal wasyl@o2.pl
**Phase:** 0 — concept draft. No code yet.

## Current status
- Multilingual README (EN/PL/ES/DE/FR/ZH) + concept, architecture, roadmap docs written.
- Brainstorm done with `agy` (Zero-Bypass, Playtest Sandbox, AI-TL;DR adopted).
- Repo scaffold; awaiting user's ideas before Stage 1.

## Open decisions (need user)
- Phone MVP: ntfy (fast start) vs PWA with Ed25519 signing (true zero-trust) first.
- Content-filter scope in MVP: DNS blocklists only vs eBPF per-app from the start.
- License (proposed GPL-3.0) and whether repo is public.
- Final product name (working: "Omarchy Guardian").

## Workflow routing (this machine)
- **Logic / architecture / integrations →** `agy`.
- **Simple implementation micro-tasks →** local Ollama `qwen3-coder:30b` via `aider` (host 192.168.0.11).
- This file is the status file (keep it current).

## Hard principles
- Parent phone app: **simple + transparent + 100% effective** (no silent failures, no bypass).
- Privacy-first: no telemetry; cloud only relays the parent's decision.
- Zero-Bypass enforcement (Polkit, cgroups v2, nftables/eBPF).
- Every build stage must end with something testable.
