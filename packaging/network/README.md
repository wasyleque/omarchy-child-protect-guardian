# Network lockdown (close the DNS/VPN escapes)

These stop the child from routing around the filtering resolver. They pair with
`../nftables/guardian-egress.nft` (kernel-level block of DoH IPs, DoT, UDP VPNs) — browsers and
NetworkManager are the user-space escapes this layer shuts.

## NetworkManager — block the child from changing the network

```bash
sudo install -Dm644 49-guardian-nm.rules /etc/polkit-1/rules.d/49-guardian-nm.rules
```
The child can no longer add/edit connections (VPN profiles, USB/Wi-Fi tethering), create a hotspot, or
activate arbitrary connections — all need admin auth. Connecting to a Wi-Fi the admin already saved
still works. Set up the child's needed connections as admin beforehand. Rollback: delete the file.

## Browsers — force "Secure DNS / DoH" off (and locked)

A browser's own DoH would tunnel DNS past the system resolver, so lock it off.

**Firefox** (enterprise policies — the child can't re-enable; the lock shows greyed-out in settings):
```bash
# Arch package path:
sudo install -Dm644 firefox-policies.json /etc/firefox/policies/policies.json
# (fallback used by some builds: /usr/lib/firefox/distribution/policies.json)
```

**Chromium / Chrome** (managed policy):
```bash
sudo install -Dm644 chromium-dns-policy.json /etc/chromium/policies/managed/guardian-dns.json
# Google Chrome: /etc/opt/chrome/policies/managed/guardian-dns.json
```

## Residual

- Other browsers / apps with their own DoH: the nftables layer still blocks the **known** DoH IP sets
  on 443 and forces port-53 DNS to the approved resolver, so even an un-policied browser can't use the
  common public DoH providers. A fully exhaustive block needs SNI/DPI allowlisting (a later step).
