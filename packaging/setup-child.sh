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
# Also update the INSTALLED copies the boot-time guardian-nftables.service loads (if install.sh has
# run), and reload them live so this child is covered now and after a reboot — not just in the repo.
add_uid_to_set "/etc/guardian/nftables/guardian-egress.nft" "$CHILD_UID"
add_uid_to_set "/etc/guardian/nftables/10-guardian-dns-redirect.nft" "$CHILD_UID"
if systemctl list-unit-files guardian-nftables.service >/dev/null 2>&1; then
  systemctl reload guardian-nftables.service 2>/dev/null \
    || systemctl restart guardian-nftables.service 2>/dev/null || true
  echo "==> reloaded guardian-nftables.service (child uid $CHILD_UID now in the live + boot rules)"
fi

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

# 5b. Session hardening — close the LOGIN-level escapes the on-host test found (F12/F13/F18). These
#     matter most: without them a child bypasses every other layer at the greeter or a text console.
echo
echo "==> session hardening (login-level escapes — review F12/F13/F18)"

# F13: disable on-demand text consoles. A tty login (Ctrl+Alt+F3..F6) runs in getty@ttyN.service,
#      OUTSIDE user-<uid>.slice, so it escapes the screen-time freeze (F18, confirmed live).
LOGIND_DROPIN=/etc/systemd/logind.conf.d/10-guardian-noconsoles.conf
if [ ! -f "$LOGIND_DROPIN" ]; then
  read -r -p "    Disable text consoles tty2–tty6 (recommended; closes a lock-bypass)? [Y/n] " a
  case "$a" in
    n|N) echo "    skipped — WARNING: a child can escape the screen-time lock via Ctrl+Alt+F3..F6" ;;
    *)
      mkdir -p /etc/systemd/logind.conf.d
      cat > "$LOGIND_DROPIN" <<'EOF'
# Guardian: no on-demand VTs / spare consoles (review F13/F18). Your display manager keeps its own VT.
[Login]
NAutoVTs=0
ReserveVT=0
EOF
      systemctl mask getty@tty2.service getty@tty3.service getty@tty4.service \
                     getty@tty5.service getty@tty6.service autovt@.service >/dev/null 2>&1 || true
      echo "    text consoles disabled (REBOOT to apply). Undo: rm $LOGIND_DROPIN &&"
      echo "        systemctl unmask getty@tty{2..6}.service autovt@.service   (recover via SSH if needed)"
      ;;
  esac
else
  echo "    text consoles already hardened ($LOGIND_DROPIN)"
fi

# F12: a display-manager autologin into a NON-child account (Omarchy ships SDDM autologin for the
#      parent) hands a child that session on power-on — every other layer is moot.
for conf in /etc/sddm.conf.d/*.conf /etc/sddm.conf; do
  [ -f "$conf" ] || continue
  au="$(sed -nE 's/^[[:space:]]*User[[:space:]]*=[[:space:]]*(.+)/\1/p' "$conf" | head -1)"
  if [ -n "$au" ] && grep -qiE '^\[Autologin\]|^[[:space:]]*Session[[:space:]]*=' "$conf" && [ "$au" != "$CHILD" ]; then
    echo "    !! $conf autologins '$au' — a child who powers on lands in that session with its privileges."
    read -r -p "       Remove this autologin? [Y/n] " a
    case "$a" in
      n|N) echo "       left as-is — WARNING: this bypasses Guardian entirely." ;;
      *) cp -a "$conf" "$conf.guardian.bak"
         sed -i -E '/^[[:space:]]*(User|Session)[[:space:]]*=/d' "$conf"
         echo "       autologin removed (backup: $conf.guardian.bak); a login prompt will appear on boot." ;;
    esac
  fi
done

# F12 greeter: Omarchy's SDDM theme shows only the last user (no picker), so a freshly-created child
#      can't pick their account. Recommend an alternative ONLY if it is installed AND is a Qt6 theme:
#      a Qt5 theme on a Qt6 greeter gives a BLANK login screen (libQt5Quick.so.5: cannot open shared
#      object file) — confirmed on-host. If no Qt6 alternative exists, leave 'omarchy' and just warn.
THEMES_DIR=/usr/share/sddm/themes
THEME="$(sed -nE 's/^[[:space:]]*Current[[:space:]]*=[[:space:]]*(.+)/\1/p' /etc/sddm.conf /etc/sddm.conf.d/*.conf 2>/dev/null | head -1)"
if [ "$THEME" = "omarchy" ]; then
  PICK=""
  for t in $(ls -1 "$THEMES_DIR" 2>/dev/null | grep -vix omarchy); do
    md="$THEMES_DIR/$t/metadata.desktop"
    [ -f "$md" ] && grep -qiE '^[[:space:]]*QtVersion[[:space:]]*=[[:space:]]*6' "$md" && { PICK="$t"; break; }
  done
  echo "    NOTE: the SDDM theme 'omarchy' shows only the last user, so a new child can't pick their account."
  if [ -n "$PICK" ]; then
    echo "          A Qt6 theme is installed — switch to it (then REBOOT and confirm the child can log in):"
    echo "              sudo sed -i 's/^Current=.*/Current=$PICK/' /etc/sddm.conf.d/*.conf"
  else
    echo "          WARNING: the other installed themes are Qt5 but the greeter is Qt6 — switching to them"
    echo "          gives a BLANK login screen (libQt5Quick.so.5 missing). Leaving 'omarchy' unchanged."
    echo "          For multi-user login, install a Qt6 SDDM theme that has a user list, switch to it and"
    echo "          reboot-test. Do NOT set a Qt5 theme."
  fi
fi

echo
echo "Child account ready:  $CHILD  (uid $CHILD_UID)  — no sudo, no root."
echo "Verify the lockdown:  $HERE/harden-audit.sh $CHILD"
echo

# 6. Print the pairing QR for the parent's phone, if ntfy is configured.
if [ -f "$POLICY" ] && grep -q '^\s*request_topic' "$POLICY"; then
  TOPIC="$(sed -nE 's/^[[:space:]]*request_topic[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$POLICY" | head -1)"
  SERVER="$(sed -nE 's/^[[:space:]]*server[[:space:]]*=[[:space:]]*"([^"]+)".*/\1/p' "$POLICY" | head -1)"
  if [ -n "$TOPIC" ] && command -v guardian-ctl >/dev/null 2>&1; then
    echo "==> Pair the parent's phone now (shows the QR and auto-captures the key):"
    echo "       sudo $HERE/pair.sh $TOPIC${SERVER:+ $SERVER}"
    echo "    (scan the QR in the app → it sends its key back → confirm the fingerprint here.)"
  fi
else
  echo "Note: [ntfy] request_topic not set in $POLICY yet — configure it, then re-run to get the QR."
fi
