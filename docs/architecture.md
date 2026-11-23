# Architecture

Who this is for: a developer copying this tree into a new service and needing to know which pieces to keep.

## Layout

| Path | Purpose |
|---|---|
| `crates/fw-config` | TOML config: unknown keys refused, 64 KiB cap, errors name the table, TLS key mode check |
| `crates/fw-auth` | Bearer digests, constant-time match, atomic per-address attempt throttle |
| `crates/fw-serve` | HTTP(S) listener: connection caps, header timeout, request ids, drain on shutdown, certificate expiry check |
| `crates/fw-tui` | Terminal dashboard: pure model and sanitizer, ratatui view, guard that restores the terminal; behind `tui` |
| `app/` | `fw-app` binary wiring the crates together: `check`, `serve` (default), `top` (`tui` only), `--version` |
| `fw-app.toml` | Demo config; `fw-app check --config fw-app.toml` prints one `ok:` line |
| `Makefile` | `check`, `build`, `install`, `uninstall`, `docs-check`, cleanup targets |
| `.github/workflows/` | CI, audit, CodeQL, scorecard, dependency review and release |
| `deny.toml` | cargo-deny advisory, license and source policy |
| `scripts/check` | Workflow, policy and docs-index validator |
| `scripts/docs-check.sh` | Quickstart, config-example, CLI, link and style checks |

## Flow

`fw_config::load` reads and validates the file, checks the TLS key mode and returns a `Config`. `check` also probes the state directory and loads TLS metadata, then prints the summary. `serve` does the same setup, starts the tracing subscriber, binds a `TcpListener` and calls `fw_serve::serve`.

The accept loop takes a global semaphore slot and a per-address slot before spawning a connection task. Plain HTTP uses hyper's HTTP/1.1 connection builder with a header read timeout; TLS wraps the stream with rustls after a 10-second handshake timeout. A middleware layer gives every request an id and logs method, path, status and elapsed milliseconds. The demo router exposes `/healthz` and `/v1/whoami`, and applies a 64 KiB extractor body limit.

Shutdown resolves on SIGTERM or Ctrl-C. The listener stops accepting, waits up to 10 seconds for connection slots to be released, then returns. SIGHUP reopens the log file. The `top` subcommand runs a separate blocking loop with a terminal guard; it does not read the config.

## Where each piece came from

| Piece | Reiki file it was lifted from (patterns only) |
|---|---|
| `fw-config` | `src/config.rs` |
| `fw-auth` | `src/auth.rs`, `src/peers/publisher.rs` (login throttle) |
| `fw-serve` | `src/serve.rs`, `src/peers/tls.rs` |
| `fw-tui` | `crates/reiki-top/src/ui.rs`, `crates/reiki-top/src/view.rs`, `src/top.rs` |
| `app/src/logging.rs` | `src/logging.rs` |
| `app/src/main.rs` | `src/main.rs` (CLI shape) |
| `Cargo.toml`, `deny.toml`, `Makefile`, workflows | Same-named files in Reiki |

## Starting a new project from it

1. Copy the tree and start fresh history: `cp -r rust-framework ../new-service && cd ../new-service && rm -rf .git && git init`.
2. Rename the `fw` prefix in files and directory names, then rename `fw-app.toml`. A scripted rename with `grep`, `sed` and `mv` works; review the diff before committing.
3. Delete what you do not need. Dropping `fw-tui` also means removing its line from `app/Cargo.toml`, the `tui` feature, the `top` module and the feature steps in the Makefile and workflows.
4. Replace the routes in `app/src/service.rs` and the config keys in `crates/*/src/lib.rs`. Update the docs in the same change so the drift check still passes.
5. Regenerate the lockfile with `cargo generate-lockfile` (or `cargo build`), run `make check` and `make docs-check`, then commit with a terse message.

See [limits](limits.md) before deciding what to keep.
