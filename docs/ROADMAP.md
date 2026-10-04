# Roadmap — Omarchy Guardian

Build order: small, verifiable steps (suited to delegating micro-tasks to `aider` + local Ollama,
with `agy` for logic/integration stages). Each stage ends with something testable.

> **Parent-app design principle (non-negotiable):** the phone app must be **simple and transparent**
> (a parent decides in ~3 seconds, understands exactly what they're approving) **and at the same time
> totally effective** (a tapped decision *always* reaches and is *reliably enforced* on the PC —
> no silent failures, no bypass). Simplicity must never cost effectiveness, and vice versa.

---

## Stage 0 — Concept (current) ✅ in progress
- [x] Concept, advantages vs competition, architecture draft, multilingual README.
- [ ] Decide: MVP phone path (ntfy vs PWA), content-filter scope, license, final name.

## Stage 1 — Hold & approve, locally (MVP core)
- [ ] `guardiand` skeleton (Rust, systemd service, TOML policy, request queue).
- [ ] Interception path #1: Polkit rule holds `pacman`/system `flatpak` until a decision.
- [ ] Local decision UI (terminal/GUI prompt) — prove hold→allow/deny end-to-end on one machine.
- [ ] Clear child-facing message + timeout/offline handling.

## Stage 2 — Remote push-approval
- [ ] Send push via ntfy with human-readable app description + *Allow/Deny* action buttons.
- [ ] Listen (SSE/WebSocket) and apply the decision to the held request.
- [ ] **AI-TL;DR:** parse AUR/Flathub metadata → plain-language summary (optional local Ollama).
- [ ] Pairing phone↔PC via QR; per-request UUID + TTL + anti-replay.

## Stage 3 — Enforcement that can't be bypassed
- [ ] Network policy: `nftables` + eBPF (DNS enforcement, VPN/proxy/Tor blocking).
- [ ] Time budget & schedule via cgroups v2 (`SIGSTOP`/`SIGCONT`), Hyprland idle hooks.
- [ ] Interception paths #2 & #3: PAM hook (`sudo`/AUR) + `flatpak --user` wrapper.
- [ ] Daemon self-protection (unkillable by child, read-only policies).

## Stage 4 — The two differentiators
- [ ] **Playtest Sandbox:** instant 15-min Bubblewrap run (no home-network / no `$HOME`), then freeze.
- [ ] Parent weekly report (what was used, what was requested) — local, private.

## Stage 5 — Parent phone app (simple · transparent · 100% effective)
- [ ] PWA/native app holding an Ed25519 key → **signed** decisions (full zero-trust).
- [ ] One-glance request card: app name, AI-TL;DR, who/where, *Allow / Deny / Sandbox 1h*.
- [ ] Delivery guarantees: retries, fallback channel, "decision confirmed on PC" receipt.
- [ ] Multi-parent / multi-child; optional "both parents must approve" for sensitive categories.
- [ ] **About screen:** "Created by wasyleque" + PayPal support ([wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Guardian)) + link to share ideas / contribute.

## Stage 6 — Polish & release
- [ ] Omarchy-native install (AUR package, `omarchy`-style setup), docs, threat model review.
- [ ] License finalized; public release.

---

## PL — Skrót

Kolejność budowy: małe, weryfikowalne kroki (pod delegację mikro-zadań do `aider` + Ollama,
`agy` do etapów logiki/integracji). **Zasada apki rodzica:** prostota i przejrzystość **oraz**
pełna skuteczność — decyzja zawsze dociera do PC i jest twardo egzekwowana, bez cichych awarii
i bez możliwości obejścia. Etapy: 0) koncepcja · 1) lokalne wstrzymanie+zgoda · 2) zdalny push ·
3) nieobchodzalna egzekucja (nftables/eBPF, cgroups, PAM) · 4) Sandbox + raporty · 5) apka rodzica
(podpisane zgody, ekran „About" z autorem i wsparciem) · 6) wydanie.
