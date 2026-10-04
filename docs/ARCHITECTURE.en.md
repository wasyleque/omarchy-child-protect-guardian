# Architecture — Omarchy Guardian

Technical sketch, phase 0. Nothing is set in stone — this is a starting point for discussion.
(Polish original: [`ARCHITEKTURA.md`](ARCHITEKTURA.md).)

```
┌──────────────────────── Child's computer (Omarchy) ─────────────────────────┐
│  pacman / yay / flatpak ──┐                                                  │
│  Omarchy TUI installer ───┤                                                  │
│                           ▼                                                  │
│                 [ Interception points ]                                      │
│                 • Polkit rule  • PAM hook  • flatpak wrapper (D-Bus)         │
│                           ▼                                                  │
│                 ┌───────────────────────┐    enforcement:                    │
│                 │  guardiand (root)      │──► nftables/eBPF (network)        │
│                 │  - policies (read-only)│──► cgroups v2 + SIGSTOP (time)    │
│                 │  - request queue       │──► Hyprland IPC (windows/blur)    │
│                 │  - signature verify    │──► Bubblewrap (sandbox)           │
│                 └───────────┬───────────┘                                    │
│                             │ (out: POST request; listen: SSE/WS)            │
└─────────────────────────────┼────────────────────────────────────────────────┘
                              ▼
                   ┌─────────────────────┐   free pub/sub broker
                   │   ntfy.sh / self-   │   (push + action buttons)
                   │   hosted ntfy       │
                   └──────────┬──────────┘
                              ▼
                   ┌─────────────────────┐   Allow / Deny (Ed25519 signature)
                   │  Parent's phone     │   MVP: ntfy app; v2: PWA/native
                   └─────────────────────┘
```

## 1. Intercepting installs

Three install paths → three hook mechanisms:

| Path | Interception mechanism |
|---|---|
| `pacman`, system `flatpak` | **Polkit rule** in `/etc/polkit-1/rules.d/` — the install action's authorization request goes to `guardiand`, which instead of asking for the root password holds and waits for a remote token. |
| `sudo`/`doas pacman -U` (AUR via `yay`) | **PAM module** (`pam_exec` or custom) — the terminal hangs at the PAM stage with *"Waiting for parent's approval…"*, exactly like server-side 2FA (Duo Unix pattern). |
| `flatpak --user` | **Wrapper** at `/usr/local/bin/flatpak` (ahead in `PATH`) talking to `guardiand` over D-Bus; holds until a decision. |

Building an AUR package (as a user) is harmless — we block only the *install* into the system. For
full tightness we also consider a mode where the child has no `sudo` path at all, and installs go
exclusively through Guardian's authorized channel.

## 2. `guardiand` — the control daemon

- Root process under `systemd` (`Restart=always`), protected by cgroups (child can't kill it).
- Holds policies (YAML/TOML, read-only for the child), the pending-request queue, keys.
- Enforces:
  - **Network:** `nftables` + eBPF — domain/IP blocklists, enforced DNS (SafeSearch/restricted),
    bypass blocking (known VPN/proxy endpoints, DoH to unauthorized servers).
  - **Time:** cgroups v2 — counts active-app time, `SIGSTOP`/`SIGCONT` instead of kill.
  - **Windows:** Hyprland IPC — blur/message on the frozen window, screen-capture rules.
  - **Sandbox:** Bubblewrap — temporary, cut off from the home network and `$HOME`.

## 3. Push-approval without an expensive backend

Principle: **no custom stateful server**. We only need a message "mailman".

- **Broker: [ntfy](https://ntfy.sh)** — open-source pub/sub with ready Android/iOS apps, supports
  **action buttons** in notifications and publishing via plain HTTP POST. Use public `ntfy.sh`
  (a private, random topic) or self-host (one small VPS/RPi).
- **Flow:**
  1. `guardiand` creates a request UUID, POSTs to the private topic with *Allow/Deny* buttons.
  2. The parent taps in the notification → the action hits a light endpoint (**Cloudflare Worker**,
     free tier ~100k req/day) or a return topic on ntfy.
  3. `guardiand` listens (ntfy SSE/WebSocket) and receives the decision for that UUID.
- **Cost:** €0 to start (public ntfy + optional free Worker).

## 4. Securing the approval itself (zero-trust)

Pushing through a public broker must not mean "anyone with the link" can approve an install.

- **Pairing** phone↔PC via QR: exchange public keys (Ed25519).
- **Signed decision:** the phone signs `{uuid, decision, timestamp}` with its private key;
  `guardiand` verifies with the parent's public key. Compromising the broker/topic does **not**
  let anyone forge an "Allow".
- **Anti-replay:** one-time UUID + short TTL + timestamp.
- **MVP caveat:** ntfy action buttons are plain HTTP (no client signature). Therefore:
  - **MVP:** trust based on a secret topic + one-time token (good enough to start; a documented trade-off).
  - **v2:** a thin **PWA/app** that holds the key and truly **signs** decisions → full zero-trust.
    This is also where the parent-app principle lives: **simple, transparent, and 100% effective** —
    signed decisions guarantee the tap is both easy *and* unforgeable, with delivery receipts so the
    parent sees it was enforced on the PC.

## 5. Components & stack (proposal)

- `guardiand` — **Rust** (memory safety, small footprint, good nft/eBPF/D-Bus libraries).
- Integrations: Polkit (JS rules), PAM (C/Rust), D-Bus, Hyprland IPC (socket).
- Config/policies: TOML.
- Phone: MVP = ntfy; v2 = PWA (Web Crypto for Ed25519) or native app.
- Optional AI-TL;DR: local model (Ollama) parsing package metadata — offline, private.

## 6. Risks / to think through

- Correct, complete coverage of every install path (no "back door").
- eBPF/nftables vs. permissions and stability across Arch kernel updates.
- UX of a hung terminal (clear message, timeout, offline mode when no network).
- Push delivery assurance (retries, fallback channel).
