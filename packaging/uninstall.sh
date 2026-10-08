#!/usr/bin/env bash
# Roll back everything install.sh did. Run as root: sudo packaging/uninstall.sh
# Does NOT touch host-specific layers you applied by hand (nftables table, fapolicyd, browser/NM
# policies, chattr flags on your own files, hardware settings) — those have their own rollbacks in
# packaging/README.md. It DOES clear chattr +i on Guardian's own files so they can be removed.
set -u
[ "$(id -u)" -eq 0 ] || { echo "run as root (sudo)"; exit 1; }

echo "==> stopping services"
systemctl disable --now guardian-watchdog.timer 2>/dev/null || true
systemctl disable --now guardiand 2>/dev/null || true

echo "==> removing pacman hooks"
rm -f /etc/pacman.d/hooks/50-guardian.hook /etc/pacman.d/hooks/60-guardian-trust.hook /etc/pacman.d/hooks/99-guardian-test.hook

echo "==> removing wrappers + polkit rules"
rm -f /usr/local/bin/flatpak /usr/local/bin/snap /usr/local/bin/nix /usr/local/bin/nix-env
rm -f /etc/polkit-1/rules.d/49-guardian-flatpak.rules /etc/polkit-1/rules.d/49-guardian-nm.rules

echo "==> clearing immutability + removing files"
for f in /usr/lib/guardian/guardian-hook /usr/bin/guardiand /etc/pacman.d/hooks/50-guardian.hook; do
  chattr -i "$f" 2>/dev/null || true
done
rm -f /usr/bin/guardiand /usr/bin/guardian-ctl /usr/bin/guardian-sign
rm -rf /usr/lib/guardian
rm -f /etc/systemd/system/guardiand.service /etc/systemd/system/guardian-watchdog.service /etc/systemd/system/guardian-watchdog.timer
systemctl daemon-reload 2>/dev/null || true

echo "==> kept (remove by hand if wanted): /etc/guardian/policy.toml, /var/log/guardian/audit.log,"
echo "    browser/NM policies, nftables 'guardian' table, fapolicyd config."
echo "Done."
