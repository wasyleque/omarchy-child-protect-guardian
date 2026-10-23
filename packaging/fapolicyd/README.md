# fapolicyd — execution allowlisting for the child

This is the layer that actually stops "download a game and just run it" (the real bypass the pacman
hook can't catch). It denies the child the execution of any file fapolicyd doesn't **trust**.

> ⚠️ **fapolicyd in *enforcing* mode can make the machine unable to run programs — or fail to boot —
> if the trust database or rules are wrong.** The 2026-10-23 on-host test hit exactly this. Use the
> setup script below (it encodes the verified fixes), roll out in **permissive** first, keep a **root
> shell open**, and **reboot-test** before relying on it.

## 1. Install (Arch)

```bash
yay -S fapolicyd        # or paru -S fapolicyd
```

## 2. Run the setup script (does everything correctly)

```bash
sudo packaging/fapolicyd/guardian-fapolicyd-setup.sh         # set up, DON'T enforce yet
```

It reads the child uids from the guardiand drop-in (or pass `--child-uid N`, repeatable) and:

- **F3 — Arch prerequisites:** creates the `fapolicyd` user (`systemd-sysusers`), forces
  `trust = file` (the `rpmdb` backend doesn't exist on Arch), disables `22-buildroot.rules` (it
  references the RPM user `mock` and aborts rule compilation), and substitutes a leftover
  `%ld_so_path%` in `43-known-elf.rules`.
- **C1 — `allow_filesystem_mark = 1`** in `fapolicyd.conf`. Without it a child runs any downloaded
  binary by bind-mounting it over a trusted path in a user namespace
  (`bwrap --dev-bind / / --bind ~/evil /usr/bin/true /usr/bin/true`, or `unshare -rm`), and Flatpak
  `--user` apps run too. This one line closes it.
- **F2 — rules as `05-guardian.rules`** (not `50-`). fapolicyd compiles `rules.d` in filename order,
  first match wins; upstream `42-trusted-elf`/`43-known-elf` **deny untrusted ELF for every uid**. At
  `50` they fire before ours and deny the *parent's* own non-pacman binaries and services (a failed
  boot in testing). At `05` our rules run first, restrict only the child, and end with "everyone else:
  allow", so the upstream all-uid denies never bite. Multi-child: one rule block per uid.
- **F1 — trust seeding in the real format.** `fapolicyd-cli --file add --from-file` does **not** exist,
  and `trust.d` files must be `path size sha256` (not bare paths). A path containing a backslash or a
  space silently **stops the load**, leaving a partial DB — under enforce systemd then can't start any
  unit. The script writes a correct seed, drops `\`/space paths, and warns if the loaded DB count is
  far below the seed.
- **F14 — trusts Guardian's own files** (`/usr/local/bin/{flatpak,snap,nix-env}`,
  `/usr/bin/guardian-{ctl,sign}`, `/usr/bin/guardiand`, `/usr/lib/guardian/*`). They aren't
  pacman-owned, so under enforce the child couldn't even run the wrapper that reaches the install gate.

**Auto-trust of approved installs (already wired):** the PostTransaction hook `../60-guardian-trust.hook`
+ `guardian-trust.sh` add exactly the files of any *approved* install to trust (the PreTransaction hook
already blocked the rest), so the approved app runs and child downloads stay non-executable. `-Syu`
upgrades don't trigger it.

## 3. Roll out safely (permissive → enforce → reboot)

```bash
# permissive: logs what WOULD be denied, blocks nothing. Keep a root shell open.
sudo fapolicyd --debug --permissive 2>&1 | tee /tmp/fap.log      # Ctrl-C to stop
# ...have the child use the machine; grep the log for legit "would deny" and add them to trust,
#    then re-run the setup script. When the log is clean:
sudo packaging/fapolicyd/guardian-fapolicyd-setup.sh --enforce
# then REBOOT and confirm `systemctl --failed` is empty and the child still can't run ~/evil.
```

Roll back instantly: `sudo systemctl stop fapolicyd`.

Verify the namespace bypass is closed (as the child):

```bash
bwrap --dev-bind / / --bind ~/evil /usr/bin/true /usr/bin/true   # → Operation not permitted
unshare -rm sh -c 'mount --bind ~/evil /usr/bin/true && /usr/bin/true'   # → Operation not permitted
```

## 4. Residual gaps

- Interpreted code (a `.py`/`.sh` the child wrote) runs under a trusted interpreter; `python3 -m <pkg>`
  still runs. Pair with the network egress policy (so it can't phone home) and interpreter-level
  controls later.
- A `--user` Flatpak *install* still writes to `~/.local/share/flatpak` unaudited (B3); with C1 fixed
  the app can't **execute**, so the impact is disk use + an unaudited install, not a runnable app.
- This is one layer; strongest together with nftables egress (../nftables), no autologin / no spare
  consoles (../README.md, setup-child.sh), and the hardware baseline. See ../../docs/THREAT_MODEL.md.
