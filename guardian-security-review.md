# Omarchy Child Protect Guardian security review

Reviewed on October 9, 2026. Repository: https://github.com/wasyleque/omarchy-child-protect-guardian

Commit: `bdd62cfcfddf7d5eecc7e0ad82760244e222d76f` (October 8, 2026).

**Recommendation: do not rely on this version as the protection system for a child's everyday computer.** It contains installation approval failures and bypasses, and several advertised child-safety features are absent. The project can be evaluated as an experimental prototype after the findings below are fixed.

Severity here describes impact on the project's promised parental-control boundary; it does not imply each issue grants an ordinary Linux user root privileges.

## Findings

### 1. Critical enforcement failure: denied pacman installations still proceed

Locations: `packaging/50-guardian.hook:15–19`, `packaging/99-guardian-test.hook:18–22`, and `packaging/60-guardian-trust.hook`.

Both approval hooks have `When = PreTransaction` but omit `AbortOnFail`. The hook executable returns exit status 1 for denial, timeout, or an unreachable daemon. Without `AbortOnFail`, that failure does not abort the package transaction. The subsequent trust hook can then add the denied package's files to fapolicyd's trusted files.

This affects transactions that already have permission to use pacman; the missing option does not itself give the child permission to run pacman as root.

**Reproduced:** installed a harmless marker package into a temporary filesystem and database inside an unprivileged user namespace. Used the repository's production hook with its executable replaced by `/usr/bin/false`, representing any denial exit. Without `AbortOnFail`, pacman exited 0 and installed the marker. Adding `AbortOnFail` made pacman exit 1 and left the marker absent. No host package database or installed files were changed.

Fix: add `AbortOnFail` to both PreTransaction hooks. Test denial, timeout, daemon outage, and multi-package transactions through actual pacman, and verify that denied packages never reach the trust database.

Reference: [Arch's official hook documentation](https://man.archlinux.org/man/alpm-hooks.5.en).

### 2. High: execution allowlist checks the wrong file's trust

Location: `packaging/fapolicyd/50-guardian.rules:20–31`.

The rule `allow perm=execute trust=1 : all` checks whether the calling process is trusted. It does not check whether the program being launched is trusted. A trusted shell can therefore match this allow rule when launching an untrusted downloaded program, before the later child-specific deny rule is reached. The intended dynamic-loader deny also comes after this broad allow. The final allow rule permits file opens, leaving interpreted scripts and libraries without the intended restrictions in this template.

The exact outcome also depends on the other rules installed on the machine and their order. The supplied Guardian rule cannot serve as the advertised execution backstop.

Fix: require trust on the object being executed, and design interpreter, shared-library, and dynamic-loader restrictions explicitly. Validate the complete compiled ruleset with downloaded ELF files, AppImages, scripts, and loader invocations from the actual child account.

Evidence: source review against [fapolicyd's upstream rule documentation](https://raw.githubusercontent.com/linux-application-whitelisting/fapolicyd/main/doc/fapolicyd.rules.5). No fapolicyd enforcement test was run on this host.

### 3. High: unsigned remote approvals expose the authorization secret

Locations: `daemon/src/ntfy.rs:88–98,189–209,301–330`; `packaging/install.sh:29`.

When remote approval is enabled without a parent public key, the daemon chooses token mode. The notification includes the token and response URL. Anyone who can read the request topic can submit the same token with an allow decision. The default installer also creates the policy file with mode 0644, so the supposedly secret topic becomes readable by local users under ordinary directory permissions once the parent puts it there.

Fix: require a valid paired public key for remote approval; remove token mode from production. Restrict policy and topic access. Topic secrecy alone is insufficient authorization.

Evidence: source review. No messages were sent to public topics. [ntfy's official access-control documentation](https://docs.ntfy.sh/config/#access-control) explains the default public read/write access.

### 4. High: signatures do not protect what the parent sees

Locations: `daemon/src/crypto.rs:20–27`; `daemon/src/ntfy.rs:216–224`; `parent-app/index.html:135–138,157–193`.

The signed bytes contain the request ID, decision, nonce, and timestamp. They omit the package name, source, version, artifact hash, and displayed reason. The parent app accepts unsigned descriptions from the broker. A malicious broker can change a request for unwanted software into a card describing a school calculator, preserve its ID and nonce, and obtain a valid allow signature when the parent taps Allow. Someone with topic access can also inject forged cards; whether a duplicate is shown depends on arrival order and the app's connection state.

**Reproduced locally:** extracted and executed the parent app's actual signing function with the vendored Ed25519 library. Changed the displayed package from `unwanted-software` to `School calculator`. The resulting allow signature remained valid for the original request's canonical message. This tested the signing omission, not a live Rust daemon/broker attack.

Fix: authenticate requests from the computer to the parent app, and bind the decision to a canonical digest of the complete immutable install intent. Have the daemon compare that digest with its held request. Bind execution to the approved artifact, not just a user-supplied name. Constrain the response endpoint to the paired server.

### 5. High: release approval client accepts an attacker-selected socket

Locations: `daemon/src/bin/guardian-ctl.rs:70–78`; the Flatpak, Snap, and Nix wrapper scripts.

Unlike the release `guardian-hook`, `guardian-ctl request` honors `GUARDIAN_SUBMIT_SOCKET` in every build. The wrappers use that client and proceed when it prints `ALLOW`. A child can point it at their own Unix socket and provide a fabricated allow response. The wrappers also resolve `guardian-ctl` through PATH, and the real package-manager binaries can be invoked directly.

This bypass does not confer root permissions, but it defeats approval gating for installations the child can already perform, such as user Flatpaks or Nix profiles. The broken execution rules in finding 2 undermine the advertised backstop.

Fix: remove production socket overrides, use fixed absolute client paths, and enforce authorization at an actual protected execution/install boundary. User-side wrappers alone cannot enforce this policy.

Evidence: source review; no live installation was attempted.

### 6. Medium: supplied service silently loses auditing and blocks child submissions

Locations: `packaging/guardiand.service:11–17`; `daemon/src/bin/guardiand.rs:43–58`; `daemon/src/audit.rs:107–120`.

`ProtectSystem=strict` makes the filesystem read-only except explicitly writable paths. The unit declares a writable runtime directory but no writable log directory for `/var/log/guardian/audit.log`. On an ordinary systemd deployment, opening or creating the audit log fails and the daemon continues with auditing disabled.

Separately, `/run/guardian` is root-owned mode 0750. An ordinary child account cannot traverse that directory to reach `submit.sock`, even though the socket is mode 0666. User-space wrappers consequently fail closed instead of providing the intended approval workflow.

Fix: declare an appropriate `LogsDirectory=guardian` or narrowly scoped writable path; handle loss of auditing visibly. Permit traversal to the submit socket while retaining root ownership and control-socket credential checks. Test both sockets as an independent child UID.

Evidence: source review and [systemd's upstream execution documentation](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml); the service was not installed or started.

### 7. Medium: request handling has unbounded resource consumption

Locations: `daemon/src/ipc.rs:102–148,186–188`; `daemon/src/ntfy.rs:266–278`.

Every accepted socket connection creates a task. Line length and read duration are unbounded. The five-pending-request limit is checked only after a complete JSON message has been read and parsed. Once the submit socket is reachable, a local process can hold many connections or send large unterminated lines, consuming daemon memory without hitting that limit. The broker stream buffer is also unbounded when a newline never arrives.

Fix: bound concurrent connections, message sizes, string lengths, and read time. Enforce resource limits before buffering and parsing. Cap remote stream frames as well.

Evidence: source review, without resource-exhaustion testing.

## Child-safety gaps and limits

- The firewall allows arbitrary TCP traffic to ports 80 and 443. A short list of blocked DoH IPs does not prevent another DoH service, HTTPS proxy, or tunnel. This limitation is partly acknowledged in the firewall file, but conflicts with broader bypass-proof claims. It also accepts loopback and established connections before applying the DNS restrictions.
- The repository does not implement a filtering DNS resolver, category blocklists, SafeSearch enforcement, or screen-time schedules. The firewall depends on a separately configured filtering resolver; a normal router DNS address does not automatically provide child-safe filtering. Browser policies disable DoH but do not prohibit VPN/proxy extensions.
- A package name does not establish that a package is safe. AUR build scripts execute before the final pacman hook; approved install scriptlets can run as root. Immutable flags and a root-owned watchdog cannot reliably defend against arbitrary malicious root code.
- Audit hashes are unkeyed and have no externally retained chain head. Removing the end of the log, truncating it to empty, or rewriting it with recomputed hashes can pass verification. The watchdog skips a missing audit file entirely. These mechanisms do not provide the claimed universal tamper detection.
- The parent's signing seed is extractable from IndexedDB by code running in the app's origin. It is not an OS-keystore or biometric-protected key. A hosted app update is part of the trust boundary. This is acknowledged as future hardening in the parent-app documentation.
- The installer leaves major layers manual, starts in a hook mode that gates only `sl`, and does not establish a separate unprivileged child account. The audit script checks some prerequisites superficially, for example treating any crypt device as evidence of root/home encryption. It is not proof that the machine is protected.

## Verification and scope

Reviewed the Rust daemon/client/crypto/audit code, installer and watchdog scripts, package-manager wrappers, firewall and fapolicyd templates, browser/Polkit policies, and parent app/service worker. The cloned repository remains unchanged.

Ran the isolated pacman hook reproduction and local Ed25519 signing test described above. Queried OSV for all 165 registry dependencies pinned in `daemon/Cargo.lock`; the service returned no known advisory matches. That is a dependency-database result, not assurance that the application or dependencies have no vulnerabilities. Cargo was unavailable, so the Rust build and unit tests were not run. No Guardian services, firewall rules, or parental controls were installed on the host.

Test scripts and results are in `/tmp/guardian-review-tests/`; the reviewed checkout is `/tmp/guardian-security-review/`.

## Applying this to the HP laptop

Use a separate parent administrator account and child account, with the child denied sudo/Polkit administration and other root-equivalent access such as a privileged Docker socket. Keep the OS and browser updated, retain disk encryption and the normal firewall, and configure and test filtering in the actual browser as the child. Test both ordinary safety needs and likely bypasses before relying on any controls.

Guardian's threat model additionally requires Secure Boot and a protected boot path. [Omarchy's current installation instructions](https://omarchy.org/manual/getting-started/) require disabling Secure Boot to install; its installation procedure alone does not establish Guardian's stronger boot baseline. Do not assume disk encryption prevents boot-policy tampering by someone who knows the disk password. Secure Boot can be configured separately, but the required firmware and signed-boot setup needs verification on this specific laptop.

The [HP product datasheet](https://files.bbystatic.com/rtiS/CVtoK27I8RFg/zDsQ%3D%3D/Datasheet) lists a Celeron N4120, 4 GB RAM, and 64 GB eMMC. If the machine is still in that configuration, memory and storage leave limited room for a large desktop package set, browser tabs, updates, and snapshots. This is a capacity consideration, not a security vulnerability or a claim that Omarchy cannot boot on it.
