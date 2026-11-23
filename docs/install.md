# Install

Who this is for: an operator or developer placing the `fw-app` binary on a host.

## One line

`make install PREFIX=$HOME/.local`

The binary lands in `$HOME/.local/bin/fw-app`. As root the default prefix is `/usr/local`, so `make install` alone installs to `/usr/local/bin/fw-app`. This template has no installer component and no published package; a local source build is the supported path.

## Verify first

1. Read `Makefile` and `Cargo.toml`. The install target runs `cargo build --release --locked -p fw-app --features tui`, then copies the binary with `install -m 0755`.
2. Run `make check` before installing. It runs fmt, clippy with warnings denied, tests with and without `tui`, and a warnings-denied doc build.
3. After installing, run `fw-app --version` and `fw-app check --config fw-app.toml`. The check prints one `ok:` line or exits 1 with an error.

Download-and-run is not part of this repository's install flow. If a downstream repo publishes archives, use the release verification steps in [security scanning](security-scanning.md) before running anything downloaded.

## Manual alternative

```text
cargo build --release --locked -p fw-app --features tui
install -d "$HOME/.local/bin"
install -m 0755 target/release/fw-app "$HOME/.local/bin/fw-app"
```

`PREFIX=`, `DESTDIR=` and `BINDIR=` override the Makefile paths. `make uninstall` removes the installed binary. `make du`, `make clean-debug`, `make clean-release` and `make clean` manage build artifacts; `make clean-stale` needs `cargo-sweep` installed.

## Upgrade and rollback

- Upgrade: replace the binary with a newer build, then run `fw-app check` before starting the service.
- Rollback: put the previous binary back and re-run `fw-app check`. Config and state formats are not versioned; keep the config that matches the binary.
- Backup before either operation: copy the config, TLS files and any configured state directory.

## Build targets

The declared minimum is Rust 1.99, edition 2021. The release workflow builds `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `aarch64-apple-darwin` and `x86_64-apple-darwin` for version 0.1.0. See [configuration](configuration.md) for the config file and [operations](operations.md) for running the service.
