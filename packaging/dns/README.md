# DNS filtering + SafeSearch

A dedicated `dnsmasq` on `127.0.0.1:5353` filters the **child's** name resolution: a category
blocklist (sinkholed to `0.0.0.0`) and SafeSearch overrides for Google/YouTube/Bing/DuckDuckGo. The
child's `:53` is redirected here by `10-guardian-dns-redirect.nft`; root/parent keep the normal
resolver. This pairs with `../nftables/guardian-egress.nft` (which blocks DoH/DoT/VPN escapes) and the
browser DoH policies in `../network/`.

> Design per the agy consult: use `dnsmasq` (don't hand-write a DNS server), force the child to it
> per-uid with nftables, and manage the blocklist/SafeSearch from config.

## Install

```bash
sudo pacman -S dnsmasq
sudo install -Dm644 dnsmasq-guardian.conf /etc/dnsmasq-guardian.conf
sudo install -Dm644 safesearch.conf       /etc/guardian/dns/safesearch.conf
sudo install -Dm755 guardian-blocklist-update.sh /usr/lib/guardian/guardian-blocklist-update
sudo /usr/lib/guardian/guardian-blocklist-update        # fetch the first blocklist
sudo install -Dm644 guardian-dnsmasq.service            /etc/systemd/system/guardian-dnsmasq.service
sudo install -Dm644 guardian-blocklist-update.service   /etc/systemd/system/guardian-blocklist-update.service
sudo install -Dm644 guardian-blocklist-update.timer     /etc/systemd/system/guardian-blocklist-update.timer
sudo systemctl enable --now guardian-dnsmasq.service guardian-blocklist-update.timer

# redirect the child's DNS to it (edit CHILD_UID first), then make persistent
sudo sed -i "s/define CHILD_UID = 1001/define CHILD_UID = $(id -u <child>)/" 10-guardian-dns-redirect.nft
sudo nft -f 10-guardian-dns-redirect.nft
```

## Test (on host)

```bash
# resolver answers and filters:
dig @127.0.0.1 -p 5353 www.google.com +short     # → 216.239.38.120 (forcesafesearch)
dig @127.0.0.1 -p 5353 www.bing.com   +short     # → 204.79.197.220 (strict)
dig @127.0.0.1 -p 5353 <a-blocked-domain> +short # → 0.0.0.0
# as the CHILD, a direct query to another resolver is transparently redirected here:
dig @8.8.8.8 www.google.com +short               # (run as child) → still 216.239.38.120
```

## Roll back
```bash
sudo systemctl disable --now guardian-dnsmasq.service guardian-blocklist-update.timer
sudo nft delete table ip guardian_dns; sudo nft delete table ip6 guardian_dns
```

## Limits (honest)
- SafeSearch maps specific search hostnames; add localized `www.google.<cctld>` lines for your locale.
- Blocklist is category-based (StevenBlack porn variant by default) — not a guarantee; tune the source.
- An arbitrary HTTPS proxy/tunnel on :443 still needs SNI/DPI allowlisting (planned). See THREAT_MODEL.
