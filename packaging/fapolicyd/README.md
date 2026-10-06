# fapolicyd — execution allowlisting for the child

This is the layer that actually stops "download a game and just run it" (the real bypass the pacman
hook can't catch). It denies the child the execution of any file fapolicyd doesn't **trust**.

> ⚠️ **Reviewed template, not yet validated on Arch in this repo.** fapolicyd in *enforcing* mode can
> make the machine unable to run programs if the trust database is incomplete. Follow the permissive
> rollout below and keep a root shell open the whole time.

## 1. Install (Arch)

`fapolicyd` is in the AUR:

```bash
yay -S fapolicyd        # or paru -S fapolicyd
```

## 2. Seed the trust database from pacman

fapolicyd's rpmdb backend doesn't exist on Arch, so use the **file** trust backend and seed it from
everything pacman owns (that is your "known-good" set):

```bash
# add every real file owned by an installed package to fapolicyd's trust
pacman -Qlq | while read -r f; do [ -f "$f" ] && printf '%s\n' "$f"; done \
  | sudo tee /etc/fapolicyd/trust.d/pacman.list >/dev/null
sudo fapolicyd-cli --file add --from-file /etc/fapolicyd/trust.d/pacman.list 2>/dev/null \
  || while read -r f; do sudo fapolicyd-cli --file add "$f"; done < /etc/fapolicyd/trust.d/pacman.list
sudo fapolicyd-cli --update
```

**Auto-trust of approved installs (implemented):** install the PostTransaction hook
`../60-guardian-trust.hook` and its helper `guardian-trust.sh`:

```bash
sudo install -Dm755 guardian-trust.sh /usr/lib/guardian/guardian-trust
sudo cp ../60-guardian-trust.hook /etc/pacman.d/hooks/
```

Because the PreTransaction `guardian-hook` already denied everything the parent didn't approve, any
package that reaches a PostTransaction *Install* was approved — so the helper adds exactly those
files to fapolicyd trust (and runs `fapolicyd-cli --update`). The approved app runs; anything the
child downloads themselves stays non-executable. No daemon involvement; upgrades (`-Syu`) don't trigger it.

## 3. Rules

Copy `50-guardian.rules` to `/etc/fapolicyd/rules.d/50-guardian.rules`, replace the `uid=1001` with
the child's uid (`id -u <child>`), then `sudo fapolicyd-cli --update`.

## 4. Roll out safely (permissive → enforce)

```bash
# permissive: logs what WOULD be denied, blocks nothing
sudo fapolicyd --debug --permissive 2>&1 | tee /tmp/fap.log      # Ctrl-C to stop
# ...have the child use the machine normally; grep the log for legit things that would be denied,
#    and add them to trust (step 2) until the log is clean.
# only then enforce:
sudo systemctl enable --now fapolicyd
```

Roll back instantly: `sudo systemctl stop fapolicyd`.

## 5. Residual gaps

- Interpreted code (a `.py`/`.sh` the child wrote) runs under a trusted interpreter. Pair with the
  network egress policy (so it can't phone home) and, later, interpreter-level controls.
- This is one layer; it is strongest together with nftables egress (../nftables) and the hardware
  baseline (../harden-audit.sh). See ../../docs/THREAT_MODEL.md.
