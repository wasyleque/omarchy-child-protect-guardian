# guardiand — Stage 1 (hold & local decision)

The Rust daemon that *holds* an app-install request and blocks until a decision (allow/deny) is made. Stage 1 proves the hold->decision loop locally; the real pacman/yay/flatpak interception (Polkit/PAM/flatpak wrapper) and the remote phone push-approval come in later stages.

## Components

Two binaries:
- `guardiand` (the daemon)
- `guardian-ctl` (local control client)

## Build

From the `daemon/` directory run `cargo build --release`; binaries land in `target/release/`.

## Config

TOML policy file, default path `/etc/guardian/policy.toml`, pass an alternate path as the first CLI arg to `guardiand`. Keys:
- `socket_path` (default `/run/guardian/guardian.sock`)
- `decision_timeout_secs` (default 300)
- `default_on_timeout` ('deny' = fail-closed, the default)

A missing file uses all defaults; a malformed file warns and uses defaults. See `../packaging/policy.example.toml`.

## Run (dev)

`guardiand ./policy.toml`. It prints the socket it listens on.

## guardian-ctl

Reads socket path from env `GUARDIAN_SOCKET`, else the default. Subcommands:
- `list` (show held requests)
- `allow <id>` 
- `deny <id>` (resolve a held request by its UUID)
- `request <source> <package> [reason...]` (submit a request and block until the decision prints as ALLOW or DENY)

`<source>` is one of: pacman, flatpak, aur.

## Protocol

Newline-delimited JSON over the Unix socket (one request line, one response line).

## Security Note

On timeout the daemon fails closed (denies). Socket is created mode 0660.

## Systemd

Install `guardiand` to `/usr/bin/`, the policy to `/etc/guardian/policy.toml`, and use
`../packaging/guardiand.service` (its `RuntimeDirectory` creates `/run/guardian`), then:

```bash
sudo systemctl enable --now guardiand
```

## Example

```bash
# build
cd daemon && cargo build --release

# terminal 1 — run the daemon with a local policy
cp ../packaging/policy.example.toml ./policy.toml
./target/release/guardiand ./policy.toml

# terminal 2 — drive it via the control client
export GUARDIAN_SOCKET=/run/guardian/guardian.sock   # or whatever policy.toml sets
./target/release/guardian-ctl request pacman firefox "for a school project"   # blocks...
./target/release/guardian-ctl list                                           # shows the held request + its id
./target/release/guardian-ctl allow <id>                                     # terminal 1's request now prints ALLOW
```

## Remote approval (ntfy)

Add an `[ntfy]` section to the policy (see `../packaging/policy.example.toml`) to push each
held request to the parent's phone with **Allow / Deny** buttons — no own server needed:

```toml
[ntfy]
enabled = true
server = "https://ntfy.sh"
request_topic = "guardian-<long-random-string>"   # the phone subscribes to this; keep it secret
```

The parent installs the [ntfy app](https://ntfy.sh) (Android/iOS) and subscribes to `request_topic`.
Tapping a button sends a one-time token back over a random response channel; the daemon verifies it
and resolves the request. `guardian-ctl` still works as a local override. On a timeout, or if a
decision can't be trusted, the daemon **fails closed (denies)**.

### Zero-trust (signed) mode

Add a paired `parent_pubkey` (base64 Ed25519) to `[ntfy]` and the daemon switches to **signed mode**:
the push carries only `{id, package, nonce}` — nothing that grants approval — and the daemon accepts
*only* a decision signed by the parent's private key. An attacker who controls the network and the
broker and sees the nonce still cannot forge an approval. The raw token path is disabled.

```bash
# generate a demo keypair (the real parent app does this on the phone; the private key never leaves it)
cargo run --bin guardian-sign -- keygen        # prints privkey (secret) + pubkey (→ policy.toml)
# sign a decision the way the app will (dev/testing):
cargo run --bin guardian-sign -- sign --privkey <b64> --id <uuid> --decision allow --nonce <hex>
```

> The token mode is the MVP for the raw ntfy app; signed mode is the real zero-trust and is what the
> future parent app (PWA/native) will use.

---

See [../docs/ROADMAP.md](../docs/ROADMAP.md) for how the stages fit the bigger plan.

