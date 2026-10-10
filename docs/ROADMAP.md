# Roadmap — Omarchy Child Protect Guardian

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
- [x] Send push via ntfy with human-readable app description + *Allow/Deny* action buttons.
- [x] Listen (NDJSON stream) and apply the decision to the held request (one-time token verified).
- [ ] **AI-TL;DR:** parse AUR/Flathub metadata → plain-language summary (optional local Ollama).
- [ ] Pairing phone↔PC via QR; per-request UUID + TTL + anti-replay.
- [ ] **Intent Binding (scoped remote-root):** frozen root cache + SHA-256, Ed25519-signed grant bound
  to exactly the action the parent saw — no shell, no TOCTOU; append-only audit log.

## Stage 3 — Enforcement that can't be bypassed
- [ ] Network policy: `nftables` + eBPF (DNS enforcement, VPN/proxy/Tor blocking).
- [ ] Time budget & schedule via cgroups v2 (`SIGSTOP`/`SIGCONT`), Hyprland idle hooks.
- [x] Interception path #2: **Flatpak** — CLI wrapper (`/usr/local/bin/flatpak`) submits + waits for
  approval, + polkit rule blocking child system installs via GUI stores (`packaging/flatpak/`). Verified.
- [x] IPC hardening: **two sockets** — submit (any uid, per-uid flood cap) vs control (owner-only,
  SO_PEERCRED) — so a child-run interceptor can submit but can't decide.
- [x] Interception path #3: **Snap & Nix** CLI wrappers (`packaging/snap/`, `packaging/nix/`) gate
  `snap install` / `nix-env -i` / `nix profile install|add`; stub-verified (InstallSource gained Snap/Nix).
- [ ] Daemon self-protection (unkillable by child, read-only policies).

## Stage 4 — Differentiators
- [ ] **Playtest Sandbox:** instant 15-min Bubblewrap run (no home-network / no `$HOME`), then freeze.
- [ ] **LACS age-rating feed:** signed metadata feed (TUF/Sigstore-style) + local offline cache;
  decision card shows the 5 dimensions instead of a single number.
- [ ] **AI risk assistant:** on-device small model + optional on-demand API; traffic light + 3-point
  TL;DR + permission translator. Public metadata only — zero child data leaves the device.
- [ ] **Usage stats:** per-app foreground time + per-category network counters, local SQLite,
  child-visible (categories + durations, not raw URL/keystroke logs).
- [ ] **AI parenting digest:** gentle weekly summary that surfaces topics to worry about / be glad
  about and suggests how to start a conversation — coach the parent, don't police the child (local-first).

## Stage 5 — Parent phone app (simple · transparent · 100% effective)
- [x] Parent client on **every leading OS** (Android, iOS, Windows, macOS, Linux) from one shared
  core — an installable **PWA** in `parent-app/` with on-device **Ed25519** signing (vendored
  `@noble/ed25519`, key in IndexedDB, SSE from ntfy). Signing proven byte-compatible with the daemon
  end-to-end. **QR pairing done** (`guardian-ctl pair` prints a QR of a `#pair=` link → phone scans →
  app pre-configured with server+topic). Follow-up: background push (Web Push/VAPID or the native ntfy app).
- [ ] One-glance request card: app name, AI-TL;DR, who/where, *Allow / Deny / Sandbox 1h*.
- [ ] Delivery guarantees: retries, fallback channel, "decision confirmed on PC" receipt.
- [ ] Multi-parent / multi-child; optional "both parents must approve" for sensitive categories.
- [ ] **About screen:** "Created by wasyleque" + PayPal support ([wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)) + link to share ideas / contribute.

## Hardening — no-bypass (cross-cutting, see [`THREAT_MODEL.md`](THREAT_MODEL.md))
Installer-gating alone is not enough; enforcement must be default-deny at execution + network.
- [x] IPC peer-credential check (SO_PEERCRED) — only the daemon owner (root) may send control msgs.
- [x] `guardian-hook` ignores `GUARDIAN_SOCKET` in release builds (fixed root-owned path).
- [~] **P1 — execution allowlisting** (fapolicyd): rules + trust-seeding + safe permissive rollout
  **shipped in `packaging/fapolicyd/`**, plus **auto-trust of approved installs** (PostTransaction
  hook `60-guardian-trust.hook` + `guardian-trust.sh`, logic verified). Deploy & tune on host; noexec backstop: TODO.
- [~] **P1 — network egress default-deny** (nftables): ruleset **shipped & syntax-validated** in
  `packaging/nftables/guardian-egress.nft` (force DNS, drop DoT, block DoH v4/v6, kill UDP/QUIC/VPN,
  default-deny). **NetworkManager polkit lockdown + browser DoH policies shipped** in
  `packaging/network/` (child can't add VPN/tether; Firefox/Chromium DoH forced off+locked). TODO: SNI/DPI for 443 tunnels.
- [x] **P1 — Ed25519 signed approvals** (challenge-response; private key stays on phone) — daemon-side
  done & verified e2e (flip + bogus-sig rejected → fail-closed); only the parent app that holds the key remains.
- [~] **P1 — hardware/boot baseline** (UEFI admin password, Secure Boot, signed UKI or locked GRUB, LUKS2 FDE): read-only **audit script shipped** (`packaging/harden-audit.sh`) + deploy guide (`packaging/README.md`); firmware steps are the owner's to apply.
- [~] **P2 — tamper-evidence & self-healing**: **parent alert on tamper DONE** (priority-5 push on a
  blocked control attempt or rejected/forged approval, rate-limited; verified e2e) and
  **append-only hash-chained audit log DONE** (every request/decision+origin & blocked attempt;
  `guardian-ctl audit-verify` detects any edit/deletion; verified e2e incl. tamper). TODO: `chattr +i`
  ops step (documented). **Watchdog shipped** (`packaging/guardian-watchdog.*`): a systemd timer
  restores missing/altered Guardian files from a backup, restarts the daemon if down, verifies the
  audit chain, and alerts the parent (`guardian-ctl alert` → ntfy) — verified e2e. TODO: AUR
  scriptlet / Intent-Binding safeguards.
- [ ] **P3 — account/session hardening** (lock root, mask spare getty, no autologin/empty pw, single account); Flatpak/PackageKit coverage.

## Content-safety features (design via agy)
- [~] **Screen-time schedule**: engine (`daemon/src/schedule.rs`) — allowed window + daily budget +
  parent-granted extra, day rollover, idle-aware — DONE & unit-tested (6 tests). TODO: daemon tick
  loop feeding local day/minute, `grant_time` over the signed channel, and Wayland enforcement
  (logind idle + `loginctl lock-session`/`systemctl freeze user-<uid>.slice`).
- [ ] **DNS filter + SafeSearch**: run `dnsmasq` on `:5353` (upstream 1.1.1.3), per-uid nftables
  redirect of the child's `:53`; guardiand generates the StevenBlack blocklist `hosts` + a SafeSearch
  `address=` map (Google/YouTube/Bing/DuckDuckGo) and reloads on update. (Config-gen = Ollama task.)
- [ ] **Wayland enforcer**: the screen-time slice above, live on Hyprland.

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
