#!/usr/bin/env bash
# setup-child.sh <child-username> [--parent <parent-username>]
#
# One command to create a locked-down child account (NO root, NO sudo) and wire every per-uid Guardian
# control to it, then print the phone-pairing QR. Idempotent: safe to re-run.
#
# What it does:
#   1. creates the child as a normal unprivileged user (own home) if missing, and sets a password;
#   2. strips any path to root (removes it from wheel/sudo/root/adm);
#   3. adds only the non-privileged groups a Hyprland desktop needs (video/audio/input/render/seat);
#   4. writes the child's real uid into the nftables + DNS rules (define CHILD_UID = ...);
#   5. drops a systemd override so guardiand knows which user the screen-time lock applies to;
#   6. prints the pairing QR (if [ntfy] is configured) so the parent just scans it.
#
# It does NOT apply the firewall or enable enforcement — those stay explicit (see packaging/README.md).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
POLICY="/etc/guardian/policy.toml"

[ "$(id -u)" -eq 0 ] || { echo "Run as root:  sudo $0 $*" >&2; exit 1; }

CHILD=""; PARENT="${SUDO_USER:-}"; FULLNAME=""
while [ $# -gt 0 ]; do
  case "$1" in
    --parent) PARENT="$2"; shift 2 ;;
    --name) FULLNAME="$2"; shift 2 ;;
    -*) echo "unknown option: $1" >&2; exit 2 ;;
    *) CHILD="$1"; shift ;;
  esac
done
[ -n "$CHILD" ] || {
  echo "usage: setup-child.sh <child-username> [--name \"Full Name\"] [--parent <parent-username>]" >&2
  echo "       run it once per child — each child gets their own named, unprivileged account." >&2
  exit 2
}

# 1. Create the child account (normal, unprivileged). The username is the child's name; --name sets a
#    friendly display name (GECOS) shown on the login/greeter.
if ! id "$CHILD" >/dev/null 2>&1; then
  echo "==> creating unprivileged account '$CHILD'${FULLNAME:+ ($FULLNAME)}"
  useradd --create-home --shell /bin/bash ${FULLNAME:+--comment "$FULLNAME"} "$CHILD"
  echo "==> set a password for '$CHILD' (the child uses this to log in):"
  passwd "$CHILD"
else
  echo "==> account '$CHILD' already exists — reusing it"
  [ -n "$FULLNAME" ] && usermod --comment "$FULLNAME" "$CHILD" || true
fi
CHILD_UID="$(id -u "$CHILD")"

# 2. Make sure the child has NO route to root. (A child in wheel/sudo could just approve its own installs.)
for g in wheel sudo root adm; do
  if id -nG "$CHILD" 2>/dev/null | tr ' ' '\n' | grep -qx "$g"; then
    echo "==> removing '$CHILD' from privileged group '$g'"
    gpasswd -d "$CHILD" "$g" >/dev/null 2>&1 || true
  fi
done

# 3. Add only the non-privileged groups a desktop session needs.
for g in video audio input render seat; do
  if getent group "$g" >/dev/null 2>&1; then
    gpasswd -a "$CHILD" "$g" >/dev/null 2>&1 || true
  fi
done

# 4. ADD this child's uid to the per-uid network + DNS rule sets (accumulates across children — run
#    once per child and every kid is covered). Rewrites `define CHILD_UIDS = { ... }` in place.
add_uid_to_set() {
  local f="$1" uid="$2"
  [ -f "$f" ] || return 0
  # pull the current uids, add this one if missing, rewrite sorted+unique
  local cur new
  cur="$(sed -nE 's/^define CHILD_UIDS[[:space:]]*=[[:space:]]*\{([^}]*)\}.*/\1/p' "$f" | tr ', ' '\n' | grep -E '^[0-9]+$' || true)"
  new="$(printf '%s\n%s\n' "$cur" "$uid" | grep -E '^[0-9]+$' | sort -n -u | paste -sd, -)"
  sed -i -E "s/^(define CHILD_UIDS[[:space:]]*=[[:space:]]*)\{[^}]*\}/\1{ $(printf '%s' "$new" | sed 's/,/, /g') }/" "$f"
  echo "==> CHILD_UIDS now { $(printf '%s' "$new" | sed 's/,/, /g') } in $(basename "$f")"
}
add_uid_to_set "$HERE/nftables/guardian-egress.nft" "$CHILD_UID"
add_uid_to_set "$HERE/dns/10-guardian-dns-redirect.nft" "$CHILD_UID"

# 5. Tell guardiand which users the screen-time lock applies to (used only if [schedule] sets a
#    lock_command). GUARDIAN_CHILD is a space-separated list; add this child if not already present.
mkdir -p /etc/systemd/system/guardiand.service.d
DROPIN=/etc/systemd/system/guardiand.service.d/10-child.conf
EXISTING="$(sed -nE 's/^Environment=GUARDIAN_CHILD=(.*)/\1/p' "$DROPIN" 2>/dev/null | tr -d '"' || true)"
LIST="$EXISTING"
case " $EXISTING " in *" $CHILD "*) ;; *) LIST="${EXISTING:+$EXISTING }$CHILD" ;; esac
cat > "$DROPIN" <<EOF
[Service]
Environment=GUARDIAN_CHILD=$LIST
EOF
systemctl daemon-reload 2>/dev/null || true
if systemctl is-active --quiet guardiand 2>/dev/null; then
  systemctl restart guardiand 2>/dev/null || true
fi
echo "==> guardiand screen-time lock bound to '$CHILD'"

echo
echo "Child account ready:  $CHILD  (uid $CHILD_UID)  — no sudo, no root."
echo "Verify the lockdown:  $HERE/harden-audit.sh $CHILD"
echo

# 6. Print the pairing QR for the parent's phone, if ntfy is configured.
if [ -f "$POLICY" ] && grep -q '^\s*request_topic' "$POLICY"; then
  TOPIC="$(sed -nE 's/^[[:space:]]*request_topic[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$POLICY" | head -1)"
  SERVER="$(sed -nE 's/^[[:space:]]*server[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$POLICY" | head -1)"
  if [ -n "$TOPIC" ] && command -v guardian-ctl >/dev/null 2>&1; then
    echo "==> Scan this on the parent's phone (Guardian Parent app → Pair — scan QR):"
    guardian-ctl pair --topic "$TOPIC" ${SERVER:+--server "$SERVER"} || true
    echo
    echo "After scanning, finish the one-time key exchange:"
    echo "  - in the app tap 'Copy public key'"
    echo "  - on this PC run:  sudo $HERE/set-parent-key.sh <paste-the-key>"
  fi
else
  echo "Note: [ntfy] request_topic not set in $POLICY yet — configure it, then re-run to get the QR."
fi
