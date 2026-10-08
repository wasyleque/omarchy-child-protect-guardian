# Nix coverage

Nix installs to a user profile without libalpm, so the pacman hook can't see it. This wrapper gates
`nix-env -i…` and `nix profile install|add` through guardiand; everything else passes through. It
picks the real binary from how it was invoked (`nix` vs `nix-env`).

```bash
sudo install -Dm755 guardian-nix-wrapper.sh /usr/local/bin/nix-env   # must precede /usr/bin on PATH
sudo ln -sf /usr/local/bin/nix-env /usr/local/bin/nix
# guardian-ctl must be on PATH (installed in step 1 of ../README.md)
```

Verified (with a stub): `nix-env -iA nixpkgs.hello` and `nix profile install nixpkgs#hello` are held →
approve runs the real binary, deny aborts with exit 1. Residual: a direct `/usr/bin/nix…` call is
covered by the fapolicyd execution allowlist (../fapolicyd). Install only if Nix is in use.
