#!/usr/bin/env bash
# Guided installer for Omarchy Child Protect Guardian. Run as root from the repo AFTER building:
#   cd daemon && cargo build --release && cd .. && sudo packaging/install.sh [--enforce]
#
# Safe by default: installs the daemon, the interception wrappers, the watchdog, and the SCOPED TEST
# pacman hook (only `pacman -S sl` is gated) so you can verify without risk. Pass --enforce to switch
# in the real system-wide pacman hook. It never auto-applies the host-specific / lock-you-out layers
# (nftables egress, fapolicyd, browser/NM policies, hardware baseline) — it prints the next steps.
# Pairs with uninstall.sh for a full rollback.
set -euo pipefail

[ "$(id -u)" -eq 0 ] || { echo "run as root (sudo)"; exit 1; }
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
BIN="$REPO/daemon/target/release"
ENFORCE=0; [ "${1:-}" = "--enforce" ] && ENFORCE=1

for f in guardiand guardian-ctl guardian-hook guardian-sign; do
  [ -x "$BIN/$f" ] || { echo "missing $BIN/$f — run 'cd daemon && cargo build --release' first"; exit 1; }
done

# Clear immutability on managed files FIRST. The watchdog re-applies `chattr +i` whenever it restores
# a file, so a later `install.sh` (e.g. --enforce) would otherwise die at the first overwrite with
# "cannot remove '/usr/bin/guardiand': Operation not permitted" and leave the host in TEST mode with
# all installs ungated (review F4). Mirrors uninstall.sh's list.
echo "==> clearing immutability on managed files (so re-runs don't fail — review F4)"
for f in /usr/bin/guardiand /usr/bin/guardian-ctl /usr/bin/guardian-sign \
         /usr/lib/guardian/guardian-hook \
         /usr/lib/guardian/backup/guardiand /usr/lib/guardian/backup/guardian-hook \
         /usr/lib/guardian/backup/50-guardian.hook \
         /etc/pacman.d/hooks/50-guardian.hook /etc/pacman.d/hooks/60-guardian-trust.hook \
         /etc/pacman.d/hooks/99-guardian-test.hook; do
  chattr -i "$f" 2>/dev/null || true
done

echo "==> binaries"
install -Dm755 "$BIN/guardiand"     /usr/bin/guardiand
install -Dm755 "$BIN/guardian-ctl"  /usr/bin/guardian-ctl
install -Dm755 "$BIN/guardian-sign" /usr/bin/guardian-sign
install -Dm755 "$BIN/guardian-hook" /usr/lib/guardian/guardian-hook

echo "==> policy + unit"
[ -f /etc/guardian/policy.toml ] || install -Dm600 "$HERE/policy.example.toml" /etc/guardian/policy.toml
chmod 600 /etc/guardian/policy.toml   # may hold the (secret) ntfy request_topic; root-only
install -Dm644 "$HERE/guardiand.service" /etc/systemd/system/guardiand.service
systemctl daemon-reload
systemctl enable --now guardiand

echo "==> interception wrappers (flatpak/snap/nix) + flatpak polkit"
install -Dm755 "$HERE/flatpak/guardian-flatpak-wrapper.sh" /usr/local/bin/flatpak
install -Dm644 "$HERE/flatpak/49-guardian-flatpak.rules"   /etc/polkit-1/rules.d/49-guardian-flatpak.rules
install -Dm755 "$HERE/snap/guardian-snap-wrapper.sh" /usr/local/bin/snap
install -Dm755 "$HERE/nix/guardian-nix-wrapper.sh"   /usr/local/bin/nix-env
ln -sf /usr/local/bin/nix-env /usr/local/bin/nix

echo "==> fapolicyd auto-trust hook helper (active only once fapolicyd is set up)"
install -Dm755 "$HERE/fapolicyd/guardian-trust.sh" /usr/lib/guardian/guardian-trust

echo "==> screen-time lock/unlock hooks (used only if [schedule] sets lock_command/unlock_command)"
install -Dm755 "$HERE/schedule/guardian-lock.sh"   /usr/lib/guardian/guardian-lock
install -Dm755 "$HERE/schedule/guardian-unlock.sh" /usr/lib/guardian/guardian-unlock

echo "==> browser Secure-DNS(DoH) off policies (review B4: Firefox's built-in DoH bypasses filtering)"
install -Dm644 "$HERE/network/firefox-policies.json"     /etc/firefox/policies/policies.json
install -Dm644 "$HERE/network/chromium-dns-policy.json"  /etc/chromium/policies/managed/guardian-dns.json
# (Chrome uses /etc/opt/chrome/policies/managed/ — copy there too if Chrome is used.)

echo "==> pacman interception hook"
if [ "$ENFORCE" -eq 1 ]; then
  rm -f /etc/pacman.d/hooks/99-guardian-test.hook
  install -Dm644 "$HERE/50-guardian.hook"       /etc/pacman.d/hooks/50-guardian.hook
  install -Dm644 "$HERE/60-guardian-trust.hook" /etc/pacman.d/hooks/60-guardian-trust.hook
  echo "    ENFORCING: all new installs are gated."
else
  rm -f /etc/pacman.d/hooks/50-guardian.hook
  install -Dm644 "$HERE/99-guardian-test.hook" /etc/pacman.d/hooks/99-guardian-test.hook
  echo "    TEST MODE: only 'pacman -S sl' is gated. Re-run with --enforce when satisfied."
fi

echo "==> watchdog (backup of known-good files + timer)"
install -Dm755 /usr/bin/guardiand                   /usr/lib/guardian/backup/guardiand
install -Dm755 /usr/lib/guardian/guardian-hook       /usr/lib/guardian/backup/guardian-hook
[ -f /etc/pacman.d/hooks/50-guardian.hook ] && install -Dm644 /etc/pacman.d/hooks/50-guardian.hook /usr/lib/guardian/backup/50-guardian.hook || true
install -Dm755 "$HERE/guardian-watchdog.sh"      /usr/lib/guardian/guardian-watchdog
install -Dm644 "$HERE/guardian-watchdog.service" /etc/systemd/system/guardian-watchdog.service
install -Dm644 "$HERE/guardian-watchdog.timer"   /etc/systemd/system/guardian-watchdog.timer
systemctl daemon-reload
systemctl enable --now guardian-watchdog.timer

cat <<'NEXT'

==> Installed. NEXT STEPS you must do by hand (host-specific; can lock you out if wrong):
  1. Edit /etc/guardian/policy.toml — set [ntfy] request_topic + parent_pubkey (pair the phone app:
     `guardian-ctl pair --topic <topic>`), then: systemctl restart guardiand
  2. Network egress:   edit CHILD_UID+DNS in packaging/nftables/guardian-egress.nft, test with
     `nft -f ...`, then make persistent. Browser/NM lockdown: see packaging/network/README.md
  3. Execution allowlist (fapolicyd):  sudo packaging/fapolicyd/guardian-fapolicyd-setup.sh
     then test PERMISSIVE, then re-run with --enforce, then REBOOT-TEST. See packaging/fapolicyd/README.md.
     *** REQUIRED, not optional *** — without fapolicyd enforcing, a child can bypass the install
     controls entirely: `/usr/bin/flatpak --user install …` (direct path, skips the wrapper — review
     B3), any downloaded AppImage/portable binary from \$HOME, and bind-mount laundering in a user
     namespace (needs allow_filesystem_mark=1 — the script sets it). Until it enforces, treat
     install-gating as advisory.
  4. Hardware baseline (UEFI pw, Secure Boot, UKI/locked GRUB, LUKS) — then: packaging/harden-audit.sh
  5. Tamper-proof the files:  chattr +i /usr/lib/guardian/guardian-hook /usr/bin/guardiand /etc/pacman.d/hooks/*guardian*.hook
  6. Child account(s): one command per child creates a named, unprivileged account (no sudo/root),
     wires the firewall/DNS/screen-time to that child, and prints the pairing QR:
         sudo packaging/setup-child.sh <child-name> [--name "Full Name"]
     After the phone scans the QR, finish pairing:  sudo packaging/set-parent-key.sh <key-from-app>
     Also: lock root; mask spare getty.
Verify anytime:  packaging/harden-audit.sh <child-username>
Roll everything back:  sudo packaging/uninstall.sh
NEXT
