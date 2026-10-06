# Threat model & anti-bypass design — Omarchy Child Protect Guardian

> **TL;DR (PL):** Samo przechwytywanie instalatora (pacman) to za mało — bystre dziecko pobierze
> AppImage/binarkę/`curl|sh`/flatpak --user i uruchomi z domu. Szczelność wymaga **warstw**:
> (1) przechwyt instalacji, (2) **allowlista wykonywania** (fapolicyd), (3) **egress sieci domyślnie
> deny** (nftables), (4) hardening konta/sesji, (5) **podpis Ed25519** dla zdalnej zgody, (6) **baza
> sprzętowa** (hasło UEFI + Secure Boot + zablokowany boot/UKI + LUKS), (7) wykrywanie manipulacji
> + audyt. Bez pkt (6) reszta jest do obejścia przez GRUB/live-USB. Nie obiecujemy „zero luk" bez tej
> bazy — obiecujemy maksymalnie podniesioną poprzeczkę i uczciwie wypisane granice.

This document is the security contract for the project. It is written to be read adversarially:
assume the attacker is a motivated, intelligent child who will google, and a security researcher
probing the code. Where we cannot prevent, we **detect and alert**; where even that fails, we say so.

## 1. Assets to protect

- **A1** — the policy decision: *no new software runs without a parent's approval*.
- **A2** — time/schedule limits actually being enforced.
- **A3** — content/network policy actually being enforced.
- **A4** — integrity of Guardian itself (daemon, hook, policy, keys, audit log).
- **A5** — confidentiality/authenticity of the parent's approval channel (no forged "Allow").

## 2. Adversary tiers

| Tier | Who | Capability |
|---|---|---|
| **T0** | Curious child | GUI, app stores, obvious settings. |
| **T1** | Skilled teen | Terminal, googling bypasses, portable apps, browser VPN/DoH, USB tether. |
| **T2** | Determined teen / researcher | Reads this repo, scripts attacks, races the approval channel, abuses env/IPC, AUR scriptlets. |
| **T3** | Physical attacker | Reboots, GRUB edit, live USB, pulls the disk, resets CMOS. |

We fully defend T0–T2 **only on top of the hardware baseline** (§6). T3 is partly out of scope and
stated honestly in §9.

## 3. Trust boundaries & assumptions

- The **child account is unprivileged**: no `sudo`/`doas`, not in `wheel`, no polkit admin, cannot
  write `/etc`, `/usr`, `/boot`, or Guardian's files.
- **guardiand runs as root** under systemd `Restart=always`; the child cannot stop, replace, or
  starve it (cgroups), and cannot edit its unit/policy/hook/binaries.
- The **parent's phone and its private key are trusted**; the public ntfy broker is **untrusted**
  (read- and write-able by anyone who learns a topic).
- The **firmware/bootloader/disk** are only trustworthy once §6 is applied; until then they are an
  open bypass and the rest of the model does not hold.

## 4. Core insight — gate *execution and network*, not just the installer

The flagship "approve installs" feature intercepts `libalpm` (pacman/yay/paru/`pacman -U`/GUI). That
is necessary but **not sufficient**: the vast majority of "install a thing" on Linux never touches
pacman. Therefore the enforcement posture is **default-deny at the execution and network layers**,
with the installer hook as one input among several.

### Layered model

- **L1 — Install interception** *(done: pacman ALPM hook; Flatpak CLI wrapper + polkit rule for GUI
  Flatpak installs; remaining: other GUI/PackageKit paths).*
- **L2 — Execution allowlisting** *(planned, top priority).* Only trusted binaries may `execve`.
  Everything in user-writable paths (`/home`, `/tmp`, removable media) is **deny-by-default**.
  Mechanism: **fapolicyd** with a trust database seeded from pacman's local DB **plus** Guardian's
  approvals; a parent-approved install adds its files to the trust DB, nothing else runs. Backstop:
  `noexec` on user-writable mounts where compatible, and blocking the dynamic loader
  (`ld-linux.so`) as an interpreter trick to run non-trusted ELF.
- **L3 — Network egress, default-deny** *(planned).* Kernel `nftables` per the child's uid:
  - force all DNS (UDP/TCP 53) to the local filtering resolver; **drop DoT (853)**; block known
    **DoH** resolver IP sets on 443; block unexpected outbound UDP (kills userspace WireGuard/QUIC
    smuggling); default-deny egress with an allowlist.
  - lock down **NetworkManager via polkit** (`org.freedesktop.NetworkManager.*`) so the child can't
    add a VPN profile, tether, or randomize MAC; deny new connections/USB-tether interfaces.
  - **browser managed policies**: Firefox `DNSOverHTTPS {Enabled:false, Locked:true}`, Chromium
    `DnsOverHttpsMode:"off"`, and disable installing VPN extensions.
- **L4 — Account & session hardening** *(planned).* Single child account; lock `root` login; no
  autologin; `systemctl mask getty@tty2..ttyN` + disable VT switch to a shell; no empty passwords;
  no second/guest account; polkit `allow_any/allow_active` for install/network actions set to auth-admin.
- **L5 — Remote-approval cryptography** *(MVP token now; Ed25519 required next).* See §7.
- **L6 — Boot/firmware/disk baseline** *(hardware/setup, mandatory).* See §6.
- **L7 — Tamper-evidence & self-healing** *(planned).* `chattr +i` on hook/binary/policy; a watchdog
  that re-asserts the hook/unit; **append-only audit log**; and a **tamper alert pushed to the
  parent** (via the same channel) whenever the daemon restarts unexpectedly, the hook goes missing,
  config changes, or an unauthorized IPC/approval attempt is seen. Prevention where possible,
  detection everywhere else.

## 5. Bypass register (vector → mitigation → status)

### 5.1 Installing/running software around pacman
| Vector | Mitigation | Status |
|---|---|---|
| AppImage / static ELF / tarball into `$HOME`; run from `/tmp` or USB | L2 execution allowlist (fapolicyd), noexec backstop | planned (P1) |
| `curl \| sh`, `pip/npm/cargo/gem install --user`, language scripts | L2 (deny non-trusted ELF) + L3 (egress allowlist throttles fetching) | planned (P1) |
| `flatpak install` / `--user` (CLI) | **wrapper** at `/usr/local/bin/flatpak` submits + waits for approval (done, verified) | **done (CLI)** |
| Flatpak via GUI stores (polkit) | **polkit rule** forces admin auth for system Flatpak installs (done); `--user` GUI falls back to L2 | **done (system)** |
| snap, nix, conda | per-manager wrapper + L2 covers the eventual exec | planned |
| GUI stores via PackageKit/D-Bus (Discover, Pamac, GNOME Software) | they still hit `libalpm` → L1 hook fires; lock their polkit actions | partly done |
| Web apps / PWAs in the browser | content/DNS policy (L3) + browser managed policy | planned |

### 5.2 Network
| Vector | Mitigation | Status |
|---|---|---|
| Browser DoH / system DoT | L3: drop 853, block DoH IP sets, browser policy locks DoH off | planned (P1) |
| Tor / obfs4, userspace VPN (Shadowsocks, v2ray, `ssh -D`, wireguard-go/slirp4netns) | L3 default-deny egress + block unexpected UDP; L2 stops the binary running at all | planned (P1) |
| USB tethering / second NIC / MAC randomization | L4 NetworkManager polkit lockdown; nftables bound to known uplinks | planned |
| `/etc/hosts` edit | already root-only (child can't) | done (by perms) |

### 5.3 Boot / physical (T3)
| Vector | Mitigation | Status |
|---|---|---|
| GRUB `e` → `init=/bin/sh` / `single` | lock GRUB password, or prefer **signed UKI + systemd-boot timeout 0** (no editable cmdline) | hardware/setup (P1) |
| Live USB / external boot | UEFI admin password + **Secure Boot with own keys** | hardware/setup (P1) |
| Pull the disk / adapter | **LUKS2 full-disk encryption** (`/`, `/home`, swap), TPM2+PIN or passphrase | hardware/setup (P1) |
| CMOS reset (desktop) | case lock / business laptop with NVRAM supervisor password | **out of scope** (§9) |

### 5.4 Attacks on Guardian itself
| Vector | Mitigation | Status |
|---|---|---|
| Child connects to IPC socket and sends `Resolve{allow}` for their own request | **SO_PEERCRED**: only the daemon-owner uid (root in prod) may send control msgs; split hook vs admin sockets | **done (SO_PEERCRED)**, split planned |
| `GUARDIAN_SOCKET=/tmp/fake.sock` → fake daemon that always allows | hook ignores the env var in release builds; path is hard-coded | **done** |
| Malicious AUR/pkg `post_install` scriptlet runs as root and disables Guardian | Intent Binding (SHA-256 of approved artifact), flag packages carrying install scriptlets for explicit parent consent, L7 immutability + watchdog + tamper alert | planned (P2) |
| TTY switch `Ctrl+Alt+F2` to a login | L4: mask extra getty, no empty/autologin | planned |
| Starve the daemon so the hook fails | hook **fails closed** (deny) on no-daemon; watchdog + tamper alert | done (fail-closed) |

### 5.5 Remote approval channel
| Vector | Mitigation | Status |
|---|---|---|
| Child reads the public ntfy topic, grabs the one-time token, self-approves before the parent (preemption race) | **signed mode**: notification carries only `{id, package, nonce}`, no approval secret; decision must be Ed25519-signed | **done (daemon)**; parent app pending |
| Broker flips `deny`→`allow` | Ed25519 signature over `OCPG-v1‖id‖decision‖nonce‖ts` (broker can't forge); verified e2e (flip + bogus-sig rejected → fail-closed DENY) | **done (daemon)** |
| Replay an old approval | one-time nonce + short TTL + request-id binding | done (token) / hardened by Ed25519 |

## 6. Mandatory hardware/setup baseline (the foundation)

Software cannot defend a machine an attacker can reboot. The installer/setup MUST apply:

1. **UEFI administrator (supervisor) password** + disable the boot menu / external boot.
2. **Secure Boot** enrolled with the owner's keys.
3. **No editable kernel cmdline**: ship a **signed Unified Kernel Image** booted by `systemd-boot`
   with `timeout 0` (preferred), or a **password-locked GRUB**. This kills `init=/bin/sh`.
4. **LUKS2 full-disk encryption** of `/`, `/home`, and swap (TPM2+PIN or a parent passphrase), so a
   pulled disk or live USB sees only ciphertext.

Guardian ships a checklist/automation for these and **refuses to claim "protected" until they pass**.

## 7. Why Ed25519 (not just a token) closes the remote channel

MVP sends the approval secret (`token`) through a **public** broker, so a child who learns the topic
can read the token and self-approve in ~50 ms — before the phone even buzzes. Fix:

- Phone generates an Ed25519 keypair; **private key never leaves** the device (Android Keystore /
  iOS Secure Enclave, biometric-gated). Guardian stores only the **public key** (paired via QR).
- On a held request the daemon sends only `{request_id, package, nonce}` — **nothing that grants
  approval**. The phone signs `nonce ‖ "ALLOW" ‖ request_id ‖ timestamp`; only the signature returns.
- The daemon verifies with the public key. Even an attacker who controls the network and the broker
  and sees the nonce **cannot forge the signature** without the private key. This is the real
  zero-trust; the token MVP is a stopgap and the daemon stays fail-closed until it ships.

## 8. Immediate code hardenings applied (this change)

- **guardian-hook ignores `GUARDIAN_SOCKET` in release builds** and uses the fixed, root-owned path
  `/run/guardian/guardian.sock`; the env override is honored only in debug builds (for tests). Closes
  the fake-socket bypass.
- **IPC peer-credential check (SO_PEERCRED)**: the daemon accepts connections only from the
  daemon-owner uid (root in production) — the child's uid is refused — regardless of socket file
  permissions. Closes the "connect and self-approve" vector; a split hook/admin socket is the next step.
- **Ed25519 signed approvals (signed mode)**: when a `parent_pubkey` is paired, the daemon only
  accepts decisions signed by the parent's key over `OCPG-v1‖id‖decision‖nonce‖ts`; the push carries
  no approval secret, nonces are one-time, and timestamps must be fresh. Verified end-to-end over
  real ntfy.sh: a legitimate signed approval succeeds; a broker decision-flip and a fabricated
  signature are both rejected → fail-closed DENY. The parent app (`parent-app/`) now holds the private
  key and signs on-device; its signing is proven byte-compatible with the daemon.
- **Parent alerts (L7 tamper-evidence)**: the daemon pushes a high-priority alert to the parent when a
  control attempt from an unauthorized uid is blocked, or a forged/stale approval is rejected
  (rate-limited per kind so it can't be used to flood). Verified e2e: a forged approval produced a
  priority-5 "FORGED approval blocked" push. Ops hardening to pair with it (apply on the host):
  `chattr +i` on the hook/daemon/policy so even a root install scriptlet can't remove them, plus the
  unit's `Restart=always`. A watchdog that re-asserts the hook/unit and an append-only audit log are follow-ups.

## 9. Honestly out of scope (no false promises)

- **CMOS reset on a desktop** (battery/jumper) clears the UEFI password → mitigate physically (case
  lock) or use a business laptop whose supervisor password survives in NVRAM.
- **A different device entirely** (old phone, a friend's laptop, a console). Guardian protects *this*
  machine, not the child's access to all computing.
- **A compromised parent phone / shoulder-surfed approval.** Out of band.
- **Nation-state / hardware implants.** Not our threat model.

## 10. Hardening priorities

- **P1:** execution allowlisting (fapolicyd) · network egress default-deny (nftables + DoH/DoT/VPN
  block) · hardware baseline (UEFI/SecureBoot/UKI/LUKS). *(SO_PEERCRED, the env-var fix, and
  **Ed25519 signed approvals (daemon side)** are already done; signed mode awaits only the parent app.)*
- **P2:** scriptlet/Intent-Binding safeguards · L7 tamper-evidence, watchdog, audit log & parent alert.
- **P3:** account/session hardening polish · Flatpak/PackageKit coverage · browser managed policies.
