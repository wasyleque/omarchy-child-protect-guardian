# Flatpak coverage

Flatpak doesn't go through `libalpm`, so the pacman hook can't see it. Two complementary pieces close
the Flatpak path:

## 1. CLI wrapper — `guardian-flatpak-wrapper.sh`

Intercepts `flatpak install` from the command line (incl. `flatpak --user install`), submits it to
guardiand, and only runs the real `flatpak` on approval. Everything else passes straight through.

```bash
sudo install -Dm755 guardian-flatpak-wrapper.sh /usr/local/bin/flatpak   # must precede /usr/bin on PATH
# guardian-ctl must be on PATH (installed in step 1 of ../README.md)
```
Verified: `install` is held → parent approves → the real flatpak runs with the original args; a denial
aborts with exit 1 and the real flatpak never runs; non-install subcommands are untouched.

## 2. Polkit rule — `49-guardian-flatpak.rules`

GUI stores (GNOME Software, Discover, Pamac) install via Flatpak's **polkit** actions, not the CLI.
This rule forces admin auth for system installs/updates, which the child can't provide:

```bash
sudo install -Dm644 49-guardian-flatpak.rules /etc/polkit-1/rules.d/49-guardian-flatpak.rules
```

## Residual gaps (honest)

- Calling `/usr/bin/flatpak` by full path, or a `--user` install via a GUI portal, bypasses the CLI
  wrapper. That's why this is layered: the polkit rule covers GUI **system** installs, and **fapolicyd**
  (../fapolicyd) is the backstop — a `--user` app the child sneaks in still can't **execute** unless it
  was approved (and approvals add files to trust). 
- Trusting approved Flatpak app files in fapolicyd automatically (like the pacman PostTransaction hook)
  is a follow-up — Flatpak apps live under `/var/lib/flatpak` / `~/.local/share/flatpak` and run via
  bubblewrap, so their trust integration needs its own step.
