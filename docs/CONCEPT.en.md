# Concept — Omarchy Guardian

Working document, phase 0. We collect and organize ideas before writing a single line of code.
(Polish original: [`KONCEPCJA.md`](KONCEPCJA.md).)

## 1. Problem & audience

A parent wants their child to use the computer (Omarchy/Linux) **independently**, but without:
installing arbitrary games/apps without consent; sitting endlessly at any hour; stumbling onto
inappropriate content — and crucially, **with no easy way to bypass it** (kids are resourceful).

Competitors (Family Safety, Screen Time, Family Link, Qustodio) are either easy to bypass, or heavy
and spying, or require a corporate cloud account. We build something lighter, private and genuinely
tamper-proof — because Linux allows it.

## 2. Core feature: remote install approval (push-approval)

A "2FA-like" flow, but for installing apps:

1. The child launches an install (`yay -S some-game`, `flatpak install ...`, Omarchy TUI, etc.).
2. Guardian **intercepts** and holds it — the terminal/GUI shows *"Waiting for parent's approval…"*
   (with an optional field to add a reason for the request).
3. The parent's phone gets a **push** with a readable description: what the app is, its category,
   whether it's open-source, who's asking, on which device.
4. The parent taps **Allow** / **Deny** (optionally: *Allow for 1h in a sandbox*).
5. The PC receives the **signed** decision and continues or cancels the install.

Key point: the parent need not be home or at the computer. The decision takes 3 seconds from the
phone's notification.

### Parent-app design principle
The phone app must be **simple and transparent** (decide in ~3 seconds, understand exactly what is
being approved) **and at the same time totally effective** (a tapped decision always reaches the PC
and is reliably enforced — no silent failures, no bypass). Neither goal is sacrificed for the other.

## 3. Advantages unlocked by Linux/Arch

### Zero-Bypass
- The child is a normal user with no `sudo`/`doas`; the daemon and policies belong to root.
- Daemon protected: `systemd` (Restart=always), cgroups v2 (child can't kill it), Polkit blocks escalation.
- Network filtered in the kernel (`nftables` + eBPF) — VPN/proxy/Tor can't route around the policy.
- Policies read-only for the child (root-owned; optional immutable/`chattr +i`).

### Compositor-level control (Hyprland)
- Hard time enforcement: on budget overrun the app's process gets `SIGSTOP` (frozen, not closed —
  no lost progress), window dimmed with a message.
- Window rules: block screen capture/sharing in sensitive contexts.

### Privacy & light footprint
- Zero telemetry. Reports stay local; the parent views them on their side.
- Cloud is only a "mailman" for decisions — the child's activity never flows through it.

### System-wide filter, not per-app
- One DNS/network policy covers everything: browsers, games, messengers, launchers.

## 4. Non-obvious ideas (differentiators)

1. **Playtest Sandbox (temporary access, no waiting).** Instead of a frustrating "wait until the
   parent replies", the app starts immediately in a **15-minute isolated sandbox** (Bubblewrap):
   no home-network access, no private files. After 15 min Hyprland freezes the window (`SIGSTOP`) —
   continue only after approval. The child isn't stuck, the parent feels no pressure.
2. **AI-TL;DR instead of technical jargon.** The parent doesn't see `prismlauncher-bin` or
   `lib32-mangohud`. The daemon parses AUR/Flathub metadata (description, category, license,
   popularity) and optionally summarizes with a local model: *"PrismLauncher — an alternative
   Minecraft launcher. Games. Open-source, safe."* The decision becomes informed.
3. **Request with context.** The child can add a reason ("it's for a school project") — shown in
   the push. Fewer "dad, come approve this" calls.
4. **Two parents / multiple guardians.** Each guardian has their own key; any can approve (or a
   "both must approve" mode for sensitive categories).
5. **Panic/appeal log.** Denials and requests land in a weekly summary — a fact-based conversation
   with the child, not guesswork.

## 5. Use cases (sketch)

- *"Child wants a new game at 8pm"* → push to dad → *Allow for the weekend* → install proceeds.
- *"2h limit exceeded"* → game frozen, on-screen message, request +30 min from mom.
- *"Attempt to open a blocked site"* → system-level block + report entry (no full URL history).

## 6. Open questions (to decide)

- Native phone app, or start from off-the-shelf **ntfy** + (later) a PWA? (see ARCHITECTURE)
- Content-filter scope in MVP: DNS blocklists only, or eBPF per-app from the start?
- License, and whether public from day one.
- Name/brand (working: *Omarchy Guardian*).
