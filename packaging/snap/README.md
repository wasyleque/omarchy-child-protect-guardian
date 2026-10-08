# Snap coverage

Snap doesn't use libalpm, so the pacman hook can't see it. This CLI wrapper gates `snap install`
through guardiand; other `snap` subcommands pass straight through.

```bash
sudo install -Dm755 guardian-snap-wrapper.sh /usr/local/bin/snap   # must precede /usr/bin on PATH
# guardian-ctl must be on PATH (installed in step 1 of ../README.md)
```

Verified (with a stub): `snap install --classic code` is held → approve → the real snap runs with the
original args; a denial aborts with exit 1. Residual: a direct `/usr/bin/snap` call or a GUI store is
covered by the fapolicyd execution allowlist (../fapolicyd). snapd is rarely present on Arch/Omarchy;
install this only if snap is in use.
