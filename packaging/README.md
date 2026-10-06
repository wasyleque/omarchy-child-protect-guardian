# Deploying Omarchy Child Protect Guardian

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

## 4. Execution allowlist (fapolicyd) — the big one
See [`fapolicyd/README.md`](fapolicyd/README.md). Seed trust from pacman, run **permissive** until the
log is clean, only then enforce. Rollback: `sudo systemctl stop fapolicyd`. Also install the
auto-trust hook (`60-guardian-trust.hook` + `guardian-trust.sh`) so parent-approved installs become
executable automatically while everything the child fetches stays non-executable.

## 5. Account & session hygiene
Single unprivileged child account; not in `wheel`/`sudo`; lock `root` login; no autologin/empty
passwords; `sudo systemctl mask getty@tty2.service …` to cut spare-VT logins. Re-run `harden-audit.sh`
until it is clean.
