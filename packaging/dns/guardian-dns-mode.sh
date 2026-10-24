#!/usr/bin/env bash
# Guardian DNS mode chooser — give every parent the same A/B choice, applied correctly and reversibly.
#
# The filtering resolver (guardian-dnsmasq on 127.0.0.1:5354) only matters once the machine actually
# USES it. On Omarchy, glibc resolves through systemd-resolved (nss-resolve varlink + the 127.0.0.53
# stub), which BYPASSES the per-child nft redirect — so out of the box nothing is filtered (M5).
# This script resolves that, two ways:
#
#   child-only    (A, recommended, Guardian's design): the child's :53 is redirected to the filter by
#                 the per-uid nft rule; the parent keeps a plain upstream and is NOT filtered.
#   whole-machine (B, simplest): the WHOLE machine (parent included) resolves through the filter.
#
# Both disable+mask systemd-resolved and its varlink/monitor sockets (review D11: disabling the
# service alone isn't enough — nss-resolve re-activates it via the sockets). `revert` undoes everything.
#
# Usage:  sudo guardian-dns-mode.sh [child-only|whole-machine|status|revert]   (no arg → interactive)
# SSH-safe: connections by IP survive a bad DNS; `revert` restores the previous resolver.
set -euo pipefail
[ "$(id -u)" -eq 0 ] || { echo "run as root (sudo)"; exit 1; }

FILTER_PORT=5354
BACKUP=/etc/guardian/resolv.conf.orig
NM_DROPIN=/etc/NetworkManager/conf.d/90-guardian-dns.conf
ALL_REDIRECT=/etc/guardian/nftables/guardian-dns-all.nft
RESOLVED_UNITS=(systemd-resolved.service systemd-resolved-varlink.socket systemd-resolved-monitor.socket)
# Upstream the PARENT (and, in child-only mode, everyone except the child) resolves through. Override
# with extra args: guardian-dns-mode.sh child-only 192.168.0.1 1.1.1.1
UPSTREAMS=("${@:2}"); [ "${#UPSTREAMS[@]}" -gt 0 ] || UPSTREAMS=(192.168.0.1 1.1.1.1)

mask_resolved() {
  echo "==> disabling + masking systemd-resolved (service + varlink + monitor sockets — review D11)"
  systemctl disable --now "${RESOLVED_UNITS[@]}" 2>/dev/null || true
  systemctl mask "${RESOLVED_UNITS[@]}" 2>/dev/null || true
}
unmask_resolved() {
  systemctl unmask "${RESOLVED_UNITS[@]}" 2>/dev/null || true
  systemctl enable --now systemd-resolved.service 2>/dev/null || true
}
write_resolv() {  # $@ = nameservers
  [ -e "$BACKUP" ] || cp -a /etc/resolv.conf "$BACKUP" 2>/dev/null || true
  rm -f /etc/resolv.conf
  { echo "# Written by guardian-dns-mode.sh — do not let a resolver manager overwrite this."
    for ns in "$@"; do echo "nameserver $ns"; done; } > /etc/resolv.conf
  # Stop NetworkManager from rewriting it.
  install -Dm644 /dev/stdin "$NM_DROPIN" <<<"[main]"$'\n'"dns=none"
  systemctl reload NetworkManager 2>/dev/null || true
}
restore_resolv() {
  rm -f "$NM_DROPIN"
  if [ -e "$BACKUP" ]; then mv -f "$BACKUP" /etc/resolv.conf; else
    ln -sf /run/systemd/resolve/stub-resolv.conf /etc/resolv.conf 2>/dev/null || true
  fi
  systemctl reload NetworkManager 2>/dev/null || true
}
ensure_filter() {
  command -v dnsmasq >/dev/null || { echo "!!! dnsmasq not installed (pacman -S dnsmasq)"; exit 1; }
  systemctl enable --now guardian-dnsmasq.service guardian-nftables.service 2>/dev/null || true
}

case "${1:-}" in
  child-only)
    ensure_filter; mask_resolved; write_resolv "${UPSTREAMS[@]}"
    rm -f "$ALL_REDIRECT"; nft delete table ip guardian_dns_all 2>/dev/null || true
    echo "==> MODE: child-only. The child's :53 → the filter (per-uid nft redirect); parent uses ${UPSTREAMS[*]}."
    ;;
  whole-machine)
    ensure_filter; mask_resolved
    # Redirect EVERYONE's :53 to the filter, and point the system resolver at localhost.
    cat > "$ALL_REDIRECT" <<EOF
table ip guardian_dns_all
delete table ip guardian_dns_all
table ip guardian_dns_all {
    chain out { type nat hook output priority -100; policy accept;
        udp dport 53 redirect to :$FILTER_PORT
        tcp dport 53 redirect to :$FILTER_PORT
    }
}
EOF
    nft -f "$ALL_REDIRECT"
    write_resolv 127.0.0.1
    echo "==> MODE: whole-machine. EVERY account (parent too) resolves through the filter."
    ;;
  revert)
    echo "==> reverting to systemd-resolved"
    rm -f "$ALL_REDIRECT"; nft delete table ip guardian_dns_all 2>/dev/null || true
    restore_resolv; unmask_resolved
    echo "    done. (guardian-dnsmasq/nftables left running; stop them with uninstall.sh if desired.)"
    ;;
  status|"")
    echo "systemd-resolved: $(systemctl is-active systemd-resolved 2>/dev/null) ($(systemctl is-enabled systemd-resolved 2>/dev/null))"
    echo "guardian-dnsmasq: $(systemctl is-active guardian-dnsmasq 2>/dev/null)   guardian-nftables: $(systemctl is-active guardian-nftables 2>/dev/null)"
    echo "resolv.conf -> "; sed -n 's/^nameserver/  nameserver/p' /etc/resolv.conf 2>/dev/null
    [ -f "$ALL_REDIRECT" ] && echo "whole-machine redirect: present" || echo "whole-machine redirect: absent"
    if [ "${1:-}" = "" ]; then
      echo
      echo "Choose:  sudo $0 child-only      (A — only the child is filtered; recommended)"
      echo "         sudo $0 whole-machine   (B — the whole computer is filtered, parent included)"
      echo "         sudo $0 revert          (back to systemd-resolved, unfiltered)"
    fi
    ;;
  *) echo "usage: $0 [child-only|whole-machine|status|revert] [upstream...]"; exit 2 ;;
esac
