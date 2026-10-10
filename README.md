# Omarchy Child Protect Guardian 🛡️

**🌐 Languages:** **English** · [Polski](README.pl.md) · [Español](README.es.md) · [Deutsch](README.de.md) · [Français](README.fr.md) · [中文](README.zh.md)

> Parental controls & online child safety for **Omarchy Linux** (Arch + Hyprland),
> with **remote approval of app installs** from the parent's phone — a push-approval
> flow just like confirming a 2FA login.

**Status:** 🧪 **experimental prototype.** The core is built and unit/e2e-tested, but an external
security review (2026-10-09) found enforcement bugs (now fixed) and real limitations. **Do not yet
rely on it as the sole protection for a child's everyday computer.** See the review
[`guardian-security-review.md`](guardian-security-review.md) and our
[`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md). Several advertised pillars (content/DNS
filtering, SafeSearch, time schedules) are **planned, not yet implemented** — see [`docs/ROADMAP.md`](docs/ROADMAP.md).

Ambitious goal: be **more convenient and more effective** than Microsoft Family Safety,
Apple Screen Time, Google Family Link and Qustodio — while staying **private** (no telemetry,
no shipping your child's data to a corporate cloud).

---

## Why Linux/Omarchy wins here

Things competitors on Windows/macOS/Android **can't** do, but we can:

1. **Zero-Bypass (genuinely tamper-proof).** On Windows/macOS kids kill the process in the task
   manager or reset permissions. Here the daemon is protected by Polkit, the child has no `sudo`,
   and cgroups v2 make it unkillable. Network filtering happens in the kernel (`nftables`/eBPF),
   so free VPNs, proxies and Tor don't slip past the policy.
2. **Compositor-level control (Hyprland/Wayland).** We can *freeze* a process (`SIGSTOP`),
   blur an unauthorized window, block screen capture/sharing — with no invasive browser extensions.
3. **Zero overhead, 100% privacy.** No bloatware, no telemetry; policies run locally and offline.
   The cloud is used **only** to relay the parent's decision.
4. **System-wide filtering, not per-browser.** One DNS/network policy covers every app
   (games, launchers, messengers), not just Chrome.

## Product pillars

- **Remote install approval** — a `pacman`/`yay`/`flatpak` attempt is held; the parent gets a push
  with a human-readable description of the app and taps *Allow / Deny*.
- **Time budget & schedule** — enforced hard via cgroups freeze + Hyprland idle.
- **System-wide content filter** — DNS + eBPF, enforced SafeSearch / restricted mode.
- **Readable reports for the parent** — what the child did and asked for (no creepy corporate spying).
- **Cryptographically signed approvals** — even a compromised push broker can't forge an "Allow".
- **Scoped remote-root (Intent Binding)** — an approval grants root for *exactly one* described
  action, never a shell; cryptographically bound to what the parent actually saw.
- **AI risk assistant** — on-demand, plain-language risk check (traffic light + permission
  translator); public app metadata only, zero child data leaves the device.
- **LACS open age ratings** — a community, multi-dimensional age scale beyond PEGI/ESRB, covering
  Linux/AUR/Flathub apps.
- **Gentle stats & AI parenting digest** — hours, apps and site categories, plus a weekly summary
  that coaches the parent toward a conversation — not a surveillance dossier.

## Documentation

| Document | Contents |
|---|---|
| [`docs/CONCEPT.en.md`](docs/CONCEPT.en.md) / [`docs/KONCEPCJA.md`](docs/KONCEPCJA.md) | Full concept, advantages, use cases, non-obvious ideas |
| [`docs/ARCHITECTURE.en.md`](docs/ARCHITECTURE.en.md) / [`docs/ARCHITEKTURA.md`](docs/ARCHITEKTURA.md) | Architecture: daemon, interception points, push-approval, security |
| [`docs/ROADMAP.md`](docs/ROADMAP.md) | Build stages (MVP → v1), split into small verifiable steps |
| [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md) | Anti-bypass design: adversary tiers, every bypass vector → its mitigation, honest limits |
| [`docs/REVIEW-RESPONSE.md`](docs/REVIEW-RESPONSE.md) | Response to the external security review (each finding → fix/status) |
| [`parent-app/`](parent-app/) | Cross-platform parent approval app (installable PWA) — on-device Ed25519 signing |
| [`packaging/`](packaging/) | Deploy: installer, systemd unit, pacman hook, nftables egress, fapolicyd allowlist, hardening audit |
| [`docs/TESTING.md`](docs/TESTING.md) | Step-by-step test plan for a clean system (layer-by-layer + red-team bypass attempts) |

## How it's built (workflow)

Multi-agent workflow on this machine: **logic/architecture/integrations → `agy`**,
**simple implementation tasks → local Ollama (`qwen3-coder:30b`) via `aider`**.
`AGENTS.md` acts as the status file.

---

## 👤 Author

**Created by [wasyleque](https://github.com/wasyleque).**

## ❤️ Support the project

If Omarchy Child Protect Guardian is useful to you, you can support its development via **PayPal**:
**[wasyl@o2.pl](https://www.paypal.com/donate/?business=wasyl@o2.pl&item_name=Omarchy+Child+Protect+Guardian)**

Every bit helps keep this free, private and open.

## 🤝 Contribute & share ideas

This project grows on community ideas. **We warmly welcome you to:**
- 💡 **share ideas** — open an [Issue](../../issues) with the `idea` label (we have a template),
- 🛠️ **co-create features** — pick something from the [Roadmap](docs/ROADMAP.md), open a PR,
- 🌍 **translate the docs** into more languages.

See [`CONTRIBUTING.md`](CONTRIBUTING.md). No idea is too small — let's build the best parental-safety
tool on any platform, together.

## License

TBD (proposal: GPL-3.0 — a security tool whose value comes from being open and auditable).
