# rust-framework

[![ci](https://github.com/ddproxy/rust-framework/actions/workflows/ci.yml/badge.svg)](https://github.com/ddproxy/rust-framework/actions/workflows/ci.yml)
[![CodeQL](https://github.com/ddproxy/rust-framework/actions/workflows/codeql.yml/badge.svg)](https://github.com/ddproxy/rust-framework/actions/workflows/codeql.yml)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/ddproxy/rust-framework/badge)](https://scorecard.dev/viewer/?uri=github.com/ddproxy/rust-framework)
[![Latest release](https://img.shields.io/github/v/release/ddproxy/rust-framework)](https://github.com/ddproxy/rust-framework/releases/latest)
[![License: MIT](https://img.shields.io/github/license/ddproxy/rust-framework)](LICENSE)

A small, working reference workspace for Rust services. Copy it to start a new project; it is not a dependency. It builds and tests as shipped, and each piece is a generic version of something from Reiki, with the proxy-specific parts removed.

## What is here

| Path | Purpose |
|---|---|
| `crates/fw-config` | TOML config: unknown keys refused, 64 KiB cap, errors name the table, TLS key mode check |
| `crates/fw-auth` | Bearer tokens stored as SHA-256; constant-time match; atomic per-address attempt throttle |
| `crates/fw-serve` | HTTP(S) listener: connection caps, header timeout, request ids, drain on SIGTERM and Ctrl-C, certificate expiry check at load |
| `crates/fw-tui` | Terminal dashboard: pure model and sanitizer, ratatui view, guard that restores the terminal; behind `tui` |
| `app/` | `fw-app` binary wiring the above: `check`, `serve` (default), `top` (needs `tui`), `--version`; log reopen on SIGHUP, panic hook, state-dir check |
| `fw-app.toml` | Demo config; `fw-app check --config fw-app.toml` should print `ok: ...` |
| `Makefile` | `check` (fmt, clippy, tests and warnings-denied docs), `build`, `install`, `uninstall`, `du`, `clean*` |
| `.github/workflows/` | `ci.yml` (fmt, clippy, tests), `audit.yml` (cargo-deny), `codeql.yml`, `scorecard.yml`, `dependency-review.yml`, `release.yml` (builds, checksums, SBOM, provenance) |
| `deny.toml` | cargo-deny advisory, license and source policy |
| `docs/` | Documentation index and [security scanning](docs/security-scanning.md) |
| `SECURITY.md` | Private reporting, supported versions, response times |

### Where each piece came from

| Piece | Reiki file it was lifted from (patterns only) |
|---|---|
| `fw-config` | `src/config.rs` |
| `fw-auth` | `src/auth.rs`, `src/peers/publisher.rs` (login throttle) |
| `fw-serve` | `src/serve.rs`, `src/peers/tls.rs` |
| `fw-tui` | `crates/reiki-top/src/ui.rs`, `crates/reiki-top/src/view.rs`, `src/top.rs` |
| `app/src/logging.rs` | `src/logging.rs` |
| `app/src/main.rs` | `src/main.rs` (CLI shape) |
| `Cargo.toml`, `deny.toml`, `Makefile`, workflows | `Cargo.toml`, `deny.toml`, `Makefile`, `.github/workflows/` |

## Commands

```sh
make help                                  # list targets
make check                                 # fmt, clippy, tests and docs (warnings denied)
cargo test --workspace                     # default feature set
cargo test --workspace --features tui      # includes the top dashboard
cargo run -p fw-app -- check --config fw-app.toml
cargo run -p fw-app -- serve --config fw-app.toml
make install PREFIX=$HOME/.local           # release build with tui, installs fw-app
```

Make a token hash for the config with `printf '%s' "$TOKEN" | sha256sum`. Then call the demo route with `curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/v1/whoami`.

## Start a new project from it

1. Copy the tree and start fresh history:
   `cp -r rust-framework ../lu && cd ../lu && rm -rf .git && git init`
2. Rename the `fw` prefix to your project's, in files and directories (run in bash):
   `grep -rlE 'fw[-_]' --exclude-dir=.git --exclude-dir=target . | xargs sed -i -E 's/fw-/lu-/g; s/fw_/lu_/g' && for d in crates/fw-*; do mv "$d" "${d/fw-/lu-}"; done && mv fw-app.toml lu-app.toml`
3. Delete what you do not need. Dropping `fw-tui` also means removing its line from `app/Cargo.toml`, the `tui` feature, the `top` module and the `--features tui` steps in the Makefile and workflows.
4. Replace the routes in `app/src/service.rs` and the config keys in `crates/*/src/lib.rs`. Keep the gates green.
5. Regenerate the lockfile after the rename with `cargo generate-lockfile` (or `cargo build`), then commit.

## Behaviour worth knowing

- Every request to a throttled route reserves one attempt under a single lock, valid token or not. There is no success reset: a caller holding a valid token cannot clear the count between guesses at another.
- `[state] dir` is optional. When set it is created and probed with a temporary file at startup; an unwritable directory is a clear startup error.
- A log file sink reopens its path on SIGHUP for rotation. Failed writes increment an atomic counter shown as `log_write_errors` in `/healthz`; a warning is emitted at most once a minute.
- TLS certificates are parsed at load: an expired leaf is refused, one expiring within 30 days logs a warning, and `not_after` (unix seconds) appears in `fw-app check`, `/healthz` and the serve logs.
- Panics are logged through `tracing` with their message and location; before logging starts the message also goes to stderr.
- A request that overruns `request_timeout_secs` gets a 408. Body limits apply to extractors (the demo sets 64 KiB); a raw `Body` has none, so a handler that reads one must bound it.
- A non-loopback `listen` needs `[tls]`; bearer tokens never travel in the clear off the host.
- Names drawn in the terminal pass through `fw_tui::sanitize::printable`: control characters, bidi overrides and zero-width characters become `?`.

## Known gaps (for review)

- Refused connections (over a cap) are logged at debug level, so a flood is invisible by default.
- Idle keep-alive connections hold their slot until the drain timeout (10 s) during shutdown.
- The release workflow runs only on `github.com` (its `if:` guard, copied from Reiki); a forge-hosted copy needs its own release step.
- Token length is not enforced at config time, because a hash cannot show how long its source was.
- The `top` dashboard shows synthetic data; there is no real feed.
- The CI workflow keeps Reiki's `curl | sh` rustup fallback for runners without Rust; drop it if every runner you use has Rust.

## Security

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md). [Security scanning](docs/security-scanning.md) lists the workflows, their required check names and where findings appear.

### Verify a release

```sh
tag=v0.1.0
target=x86_64-unknown-linux-musl
gh release download "$tag" --repo ddproxy/rust-framework --pattern '*'
gh attestation verify "fw-app-$tag-$target.tar.gz" --repo ddproxy/rust-framework
shasum -a 256 -c SHA256SUMS
```

The release also carries a CycloneDX SBOM, `rust-framework-$tag.cdx.json`, and a `.sha256` file per archive.

## License

MIT. See `LICENSE`.
