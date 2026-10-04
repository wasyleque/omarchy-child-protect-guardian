# Concept — Omarchy Child Protect Guardian

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
- ~~Name/brand~~ → decided: **Omarchy Child Protect Guardian**.

## 7. Approval = scoped remote-root (Intent Binding)

The parent's "Allow" is **not** a root shell for the child — it is a **one-time, scoped privilege
elevation** that authorizes `guardiand` (already root) to perform **exactly one** pre-described
action (e.g. "install flatpak X"). The phone decides *whether to grant remote root for that one action*.

- **Intent Binding:** the phone signs (Ed25519) a canonical JSON: `action` id, exact package id,
  timestamp, one-time nonce, and the **SHA-256 of the exact description the parent saw**. The PC
  executes only what matches that signed intent — nothing else.
- **No TOCTOU:** before asking, `guardiand` pre-fetches the package + metadata into a frozen root
  cache (`/var/cache/guardian/`) and hashes it; the parent authorizes *that exact hash*; the install
  runs only from the frozen cache, with no re-fetch from the network.
- **Least privilege:** no `bash -c`; a direct `execve` of a dedicated binary in an isolated
  namespace, dropping every capability except the one the action needs.
- **Short TTL + replay protection:** ~5-minute validity; used nonces are stored.
- **Non-repudiable audit:** every grant + the parent's signature is appended to a protected
  append-only log.

Message anyone understands — not "grant CAP_SYS_ADMIN", but:
*"Zuzia wants to install 'PrismLauncher' (a Minecraft launcher). Allow this one install?"*

## 8. AI risk assistant in the parent app (on demand)

A **"Help me decide"** button gives a plain-language risk assessment of this exact app/action.

- **Privacy (zero-knowledge about the child):** only **public app metadata** is analyzed — the
  Flatpak manifest, `PKGBUILD`, requested system permissions, network domains, and the LACS entry.
  No child data (IP, hostname, login, browsing history) ever leaves the device.
- **Hybrid model:** by default an **on-device small model** (e.g. Gemma 2B / Phi-3) for instant,
  offline categorization of known permissions; an **optional on-demand call** to an external API
  (parent's own key, zero-retention policy) for "explain this unknown AUR package in depth".
- **Readable output:** a **traffic light** (🟢 safe / 🟡 network or payments / 🔴 high permissions or
  adult content), a **3-point TL;DR**, and a **permission translator**
  (`filesystem=home` → *"the app can see your private photos and documents"*).

## 9. Community open-source age rating — LACS

A **new open scale beyond PEGI/ESRB** (which don't cover Linux/AUR/Flathub apps at all). Working name
**LACS — Linux App Content Standard** (a.k.a. *OpenAge Matrix*). Multi-dimensional, not one number:

1. **Sensitive content** — violence, profanity, nudity/horror (0–3).
2. **Communication & network** — text/voice chat, P2P, open multiplayer, telemetry (none/moderated/open).
3. **Monetization & dark patterns** — ads, microtransactions, lootboxes, FOMO.
4. **System permissions** — camera/mic, home files, network, root.
5. **Cognitive age** — 3+ / 7+ / 12+ / 16+ / 18+.

- **Curation (Web of Trust):** changes via PRs to an open registry; a merge needs cryptographic
  signatures from **verified curators** (distro maintainers, trusted educational orgs); ordinary
  users can only *flag* inconsistencies (reputation/history gate to stop trolling & manipulation).
- **Distribution:** a public Git repo + a **signed metadata feed** (TUF / Sigstore-style), cached
  locally and queryable **offline**.

This dataset powers both the parent's decision card and the AI assistant — and, being open, it can
become a community standard the competition (closed, store-bound) can't match.

## 10. Parent statistics + gentle AI parenting digest

A clear dashboard for the parent: **how many hours** the child spent at the computer, **which apps**,
and **which sites/categories** they visited — per day/week, with trends.

- **Privacy-balanced, not surveillance.** We show **categories and time**, not a keystroke-level log;
  aggregates rather than a raw URL history. The child can see **their own** stats too — transparency
  builds trust instead of fear.
- **Optional AI parenting digest (on demand).** The AI summarizes the week and **proactively surfaces
  topics worth a parent's attention** — things to *gently worry about* (e.g. a spike in late-night
  use, searches around a mature or sensitive topic) and things to *be glad about* (a new creative
  hobby, learning) — then **suggests how to open a conversation** with the child so as to protect them
  **without making them feel over-watched**. The tone coaches the *parent*; it never polices the child.
- **Local & private.** The digest is generated from local stats; the child's raw data never leaves the
  machine for the cloud.

This is the opposite of competitors' creepy "spy reports": the goal is a better *conversation*, not a
longer *dossier*.
