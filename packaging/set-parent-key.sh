#!/usr/bin/env bash
# set-parent-key.sh <parent-public-key-base64>
#
# Finishes pairing: stores the parent phone's Ed25519 public key in the daemon policy so guardiand
# accepts ONLY decisions signed by that phone, enables ntfy signed mode, and restarts the daemon.
# Run once after the phone has scanned the pairing QR (the app shows the key under "Copy public key").
set -euo pipefail

POLICY="/etc/guardian/policy.toml"

[ "$(id -u)" -eq 0 ] || { echo "Run as root:  sudo $0 $*" >&2; exit 1; }
KEY="${1:-}"
[ -n "$KEY" ] || { echo "usage: set-parent-key.sh <parent-public-key-base64>" >&2; exit 2; }
[ -f "$POLICY" ] || { echo "no $POLICY — install/configure Guardian first" >&2; exit 1; }

# Basic sanity: an Ed25519 public key is 32 bytes → 44 base64 chars ending in '='.
if ! printf '%s' "$KEY" | grep -qE '^[A-Za-z0-9+/]{43}=$'; then
  echo "that does not look like an Ed25519 public key (expected 44 base64 chars). Copy it from the app exactly." >&2
  exit 2
fi

cp -a "$POLICY" "$POLICY.bak.$(date +%s)"

# Ensure an [ntfy] section exists, enabled, with the parent key. We edit conservatively with awk so the
# rest of the file is untouched.
awk -v key="$KEY" '
  BEGIN { in_ntfy=0; set_key=0; set_en=0 }
  /^\[/ {
    # leaving the [ntfy] section without having written the key/enabled → write them now
    if (in_ntfy && (!set_key || !set_en)) {
      if (!set_en) print "enabled = true"
      if (!set_key) print "parent_pubkey = \"" key "\""
    }
    in_ntfy = ($0 ~ /^\[ntfy\]/) ? 1 : 0
    print; next
  }
  in_ntfy && /^[[:space:]]*parent_pubkey[[:space:]]*=/ { print "parent_pubkey = \"" key "\""; set_key=1; next }
  in_ntfy && /^[[:space:]]*enabled[[:space:]]*=/        { print "enabled = true"; set_en=1; next }
  { print }
  END {
    if (in_ntfy && (!set_key || !set_en)) {
      if (!set_en) print "enabled = true"
      if (!set_key) print "parent_pubkey = \"" key "\""
    }
  }
' "$POLICY" > "$POLICY.tmp"

# If there was no [ntfy] section at all, append a minimal one.
if ! grep -q '^\[ntfy\]' "$POLICY.tmp"; then
  {
    echo
    echo "[ntfy]"
    echo "enabled = true"
    echo "parent_pubkey = \"$KEY\""
  } >> "$POLICY.tmp"
fi

install -m 0600 "$POLICY.tmp" "$POLICY"
rm -f "$POLICY.tmp"
echo "==> stored parent public key and enabled signed mode in $POLICY"

if systemctl is-enabled --quiet guardiand 2>/dev/null || systemctl is-active --quiet guardiand 2>/dev/null; then
  systemctl restart guardiand && echo "==> guardiand restarted — pairing complete."
else
  echo "==> restart guardiand to apply:  sudo systemctl restart guardiand"
fi
