#!/usr/bin/env bash
# Refresh the Guardian DNS category blocklist, then reload the filtering dnsmasq. Keeps the old list
# on any failure (fail-safe). Run by guardian-blocklist-update.timer. Install at
# /usr/lib/guardian/guardian-blocklist-update.
set -euo pipefail

SRC="${1:-https://raw.githubusercontent.com/StevenBlack/hosts/master/alternates/porn/hosts}"
DEST="/etc/guardian/dns/blocked.hosts"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

if ! curl -fsSL --max-time 60 "$SRC" -o "$tmp"; then
  echo "guardian-blocklist-update: download failed from $SRC — keeping existing list" >&2
  exit 1
fi

count="$(grep -c '^0\.0\.0\.0 ' "$tmp" || true)"
if [ "${count:-0}" -lt 100 ]; then
  echo "guardian-blocklist-update: downloaded file looks wrong (only ${count:-0} sinkhole lines) — keeping existing list" >&2
  exit 1
fi

mkdir -p "$(dirname "$DEST")"
chmod 644 "$tmp"
mv -f "$tmp" "$DEST"
trap - EXIT

# Reload the guardian dnsmasq so it picks up the new list.
pkill -HUP -x dnsmasq 2>/dev/null || systemctl reload guardian-dnsmasq 2>/dev/null || true

echo "guardian-blocklist-update: installed $count blocked domains to $DEST"
