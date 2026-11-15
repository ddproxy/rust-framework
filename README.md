# rust-framework

A small, working reference workspace for Rust services. Copy it to start a new project; it is not a dependency. It builds and tests as shipped, and each piece is a generic version of something from Reiki, with the proxy-specific parts removed.

## What is here

| Path | Purpose |
|---|---|
| `crates/fw-config` | TOML config: unknown keys refused, 64 KiB cap, errors name the table, TLS key mode check |
| `crates/fw-auth` | Bearer tokens stored as SHA-256; constant-time match; per-address login throttle |
| `crates/fw-serve` | HTTP(S) listener: global and per-address connection caps, header timeout, request ids, drain on SIGTERM and Ctrl-C |
| `crates/fw-tui` | Terminal dashboard: pure model and sanitizer, ratatui view, guard that restores the terminal; behind `tui` |
| `app/` | `fw-app` binary wiring the above: `check`, `serve` (default), `top` (needs `tui`), `--version` |
| `fw-app.toml` | Demo config; `fw-app check --config fw-app.toml` should print `ok: ...` |
| `Makefile` | `build`, `install`, `uninstall`, `du`, `clean*` |
| `.github/workflows/` | `ci.yml`: fmt, clippy with and without `tui`, tests, cargo-deny. `release.yml`: musl and macOS builds, checksums, draft release |
| `deny.toml` | cargo-deny license and source policy |

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

- The login throttle has no success reset. A caller who holds one valid token cannot clear the failure count between guesses at another, so a count only ends with its window.
- A request that overruns `request_timeout_secs` gets a 408. Body limits apply to extractors (the demo sets 64 KiB); a raw `Body` has none, so a handler that reads one must bound it.
- A non-loopback `listen` needs `[tls]`; bearer tokens never travel in the clear off the host.
- Names drawn in the terminal pass through `fw_tui::sanitize::printable`: control characters, bidi overrides and zero-width characters become `?`.

## Known gaps (for review)

- The log file has no rotation and no reload; `logging` is a single-file append.
- A TLS certificate's expiry is not checked at load.
- Refused connections (over a cap) are logged at debug level, so a flood is invisible by default.
- Idle keep-alive connections hold their slot until the drain timeout (10 s) during shutdown.
- The release workflow runs only on `github.com` (its `if:` guard, copied from Reiki); a forge-hosted copy needs its own release step.
- Token length is not enforced at config time, because a hash cannot show how long its source was.
- The `top` dashboard shows synthetic data; there is no real feed.
- The CI workflow keeps Reiki's `curl | sh` rustup fallback for runners without Rust; drop it if every runner you use has Rust.

## License

MIT. See `LICENSE`.
