# Deploying Omarchy Child Protect Guardian

**Quick start (guided):**
```bash
cd daemon && cargo build --release && cd ..
sudo packaging/install.sh            # safe: daemon + wrappers + watchdog + SCOPED test hook (pacman -S sl)
# verify the test hook, pair the phone, then:
sudo packaging/install.sh --enforce  # switch in the real system-wide pacman hook
sudo packaging/uninstall.sh          # full rollback
```
`install.sh` auto-applies only the safe layers and prints the host-specific steps (nftables, fapolicyd,
browser/NM policies, hardware baseline) to do by hand. The manual walkthrough below explains each.

---


Everything here changes a real system. Deploy **in this order**, test each layer, and keep a root
shell open. Each layer has an instant rollback. Read [`../docs/THREAT_MODEL.md`](../docs/THREAT_MODEL.md)
first — the layers only add up to "no bypass" together, on top of the hardware baseline.

Check where you stand at any time (read-only, safe):

```bash
./harden-audit.sh <child-username>
```

## 0. Hardware baseline (MANDATORY — software can't defend a machine you can reboot)
Do these in firmware / at install time. Without them, the rest is bypassable in ~10 s via GRUB or a
live USB (threat model §6):
- UEFI **administrator password** + disable the boot menu / external boot.
- **Secure Boot** with your keys.
- No editable kernel cmdline: **signed UKI + systemd-boot `timeout 0`** (preferred) or a
  password-locked GRUB.
- **LUKS2 full-disk encryption** of `/`, `/home`, swap.

## 1. The daemon + pacman interception
```bash
cd ../daemon && cargo build --release
sudo install -Dm755 target/release/guardiand      /usr/bin/guardiand
sudo install -Dm755 target/release/guardian-ctl   /usr/bin/guardian-ctl
sudo install -Dm755 target/release/guardian-hook   /usr/lib/guardian/guardian-hook
sudo install -Dm644 ../packaging/policy.example.toml /etc/guardian/policy.toml   # then edit it
sudo install -Dm644 ../packaging/guardiand.service  /etc/systemd/system/guardiand.service
sudo systemctl enable --now guardiand
```
Interception — **test with the scoped hook first** (only `pacman -S sl` is held):
```bash
sudo cp 99-guardian-test.hook /etc/pacman.d/hooks/        # safe trial
sudo pacman -S sl           # hangs → guardian-ctl list → guardian-ctl allow <id>
# happy? switch to the real hook:
sudo rm /etc/pacman.d/hooks/99-guardian-test.hook
sudo cp 50-guardian.hook /etc/pacman.d/hooks/
```
Rollback: `sudo rm /etc/pacman.d/hooks/{50-guardian,99-guardian-test}.hook`.

**Flatpak** (not libalpm — covered separately, see [`flatpak/README.md`](flatpak/README.md)):
```bash
sudo install -Dm755 flatpak/guardian-flatpak-wrapper.sh /usr/local/bin/flatpak   # must precede /usr/bin on PATH
sudo install -Dm644 flatpak/49-guardian-flatpak.rules /etc/polkit-1/rules.d/49-guardian-flatpak.rules
```
Rollback: `sudo rm /usr/local/bin/flatpak /etc/polkit-1/rules.d/49-guardian-flatpak.rules`.

**Snap / Nix** (optional — only if those managers are in use; see `snap/README.md`, `nix/README.md`):
```bash
sudo install -Dm755 snap/guardian-snap-wrapper.sh /usr/local/bin/snap
sudo install -Dm755 nix/guardian-nix-wrapper.sh /usr/local/bin/nix-env && sudo ln -sf /usr/local/bin/nix-env /usr/local/bin/nix
```

## 2. Remote approval (optional)
Add an `[ntfy]` section to `/etc/guardian/policy.toml` (see `policy.example.toml`). Prefer **signed
mode** (`parent_pubkey`) over the token MVP — it's immune to broker eavesdropping. `guardian-ctl`
remains a local override.

## 3. Network egress policy (nftables)
```bash
# edit CHILD_UID + DNS_SERVERS at the top first
sudo nft -f nftables/guardian-egress.nft     # non-persistent: test now
# verify the child can browse but DoH/DoT/VPN are blocked, then make it persistent
sudo nft delete table inet guardian          # <-- instant rollback
```

## 3b. Browser & NetworkManager lockdown
Close the user-space DNS/VPN escapes (see [`network/README.md`](network/README.md)):
```bash
sudo install -Dm644 network/49-guardian-nm.rules /etc/polkit-1/rules.d/49-guardian-nm.rules
sudo install -Dm644 network/firefox-policies.json /etc/firefox/policies/policies.json
sudo install -Dm644 network/chromium-dns-policy.json /etc/chromium/policies/managed/guardian-dns.json
```

## 3c. Filtering DNS + SafeSearch
A dnsmasq instance on `:5353` filters the child's DNS (category blocklist + SafeSearch), with the
child's `:53` redirected to it. Full steps in [`dns/README.md`](dns/README.md).

## 4. Execution allowlist (fapolicyd) — the big one
See [`fapolicyd/README.md`](fapolicyd/README.md). Seed trust from pacman, run **permissive** until the
log is clean, only then enforce. Rollback: `sudo systemctl stop fapolicyd`. Also install the
auto-trust hook (`60-guardian-trust.hook` + `guardian-trust.sh`) so parent-approved installs become
executable automatically while everything the child fetches stays non-executable.

## 5. Tamper-resistance (recommended)
The daemon already pushes a **priority alert to the parent's phone** when it blocks an unauthorized
control attempt or rejects a forged approval. Make its files hard to remove even by a root install
scriptlet, and ensure it auto-restarts:
```bash
sudo chattr +i /usr/lib/guardian/guardian-hook /usr/bin/guardiand /etc/pacman.d/hooks/50-guardian.hook
# (the systemd unit already has Restart=always). To update Guardian later: chattr -i first, then re-install.
```
Add the **watchdog** so missing/altered files are auto-restored and you get an alert:
```bash
# keep known-good copies for the watchdog to restore from
sudo install -Dm755 /usr/bin/guardiand                  /usr/lib/guardian/backup/guardiand
sudo install -Dm755 /usr/lib/guardian/guardian-hook      /usr/lib/guardian/backup/guardian-hook
sudo install -Dm644 /etc/pacman.d/hooks/50-guardian.hook /usr/lib/guardian/backup/50-guardian.hook
sudo install -Dm755 guardian-watchdog.sh /usr/lib/guardian/guardian-watchdog
sudo install -Dm644 guardian-watchdog.service /etc/systemd/system/guardian-watchdog.service
sudo install -Dm644 guardian-watchdog.timer   /etc/systemd/system/guardian-watchdog.timer
sudo systemctl enable --now guardian-watchdog.timer
```
Rollback: `sudo systemctl disable --now guardian-watchdog.timer`.

## 6. Account & session hygiene
Single unprivileged child account; not in `wheel`/`sudo`; lock `root` login; no autologin/empty
passwords; `sudo systemctl mask getty@tty2.service …` to cut spare-VT logins. Re-run `harden-audit.sh`
until it is clean.
