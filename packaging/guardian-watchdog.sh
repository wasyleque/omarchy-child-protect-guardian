#!/usr/bin/env bash
# guardian-watchdog — periodic integrity check (Threat model §4 L7 self-healing).
# Install at /usr/lib/guardian/guardian-watchdog (chmod 755), run by guardian-watchdog.timer.
#
# It restores Guardian's critical files from a read-only backup if they go missing or are altered
# (e.g. a root install scriptlet tried to disable Guardian), restarts the daemon if it's down, checks
# the audit chain, and alerts the parent (via `guardian-ctl alert`, which pushes over ntfy) whenever it
# had to act. Create the backup at deploy time:
#   sudo install -Dm755 /usr/bin/guardiand                     /usr/lib/guardian/backup/guardiand
#   sudo install -Dm755 /usr/lib/guardian/guardian-hook         /usr/lib/guardian/backup/guardian-hook
#   sudo install -Dm644 /etc/pacman.d/hooks/50-guardian.hook    /usr/lib/guardian/backup/50-guardian.hook
set -u

BACKUP=/usr/lib/guardian/backup
AUDIT=/var/log/guardian/audit.log   # keep in sync with policy.toml audit_path

CTL=/usr/bin/guardian-ctl
alert() { "$CTL" alert "$1" 2>/dev/null || logger -t guardian-watchdog -- "$1"; }

# target path -> backup filename
check_file() {
  local target="$1" src="$BACKUP/$2"
  [ -f "$src" ] || return 0            # no backup to compare against yet
  if [ ! -e "$target" ] || ! cmp -s "$target" "$src"; then
    chattr -i "$target" 2>/dev/null || true
    if install -D -m "$(stat -c '%a' "$src" 2>/dev/null || echo 755)" "$src" "$target" 2>/dev/null; then
      chattr +i "$target" 2>/dev/null || true
      alert "Integrity: restored $target (it was missing or altered)."
    else
      alert "Integrity: $target is missing/altered and could NOT be restored."
    fi
  fi
}

check_file /usr/bin/guardiand                   guardiand
check_file /usr/lib/guardian/guardian-hook      guardian-hook
check_file /etc/pacman.d/hooks/50-guardian.hook 50-guardian.hook

if ! systemctl is-active --quiet guardiand; then
  systemctl restart guardiand 2>/dev/null || true
  alert "Integrity: guardiand was not running; restarted it."
fi

if [ ! -f "$AUDIT" ]; then
  alert "Integrity: audit log is MISSING (possible tampering/deletion)."
elif ! "$CTL" audit-verify "$AUDIT" >/dev/null 2>&1; then
  alert "Integrity: audit log failed chain verification (possible tampering)."
fi

exit 0
