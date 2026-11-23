# AGENTS.md

Instructions for AI agents working in this repository. Humans can use the same commands.

## Commands

| Command | Purpose |
|---|---|
| `make help` | List every Make target. |
| `make build` | Release build of `fw-app` with the `tui` feature. |
| `make check` | `cargo fmt --all --check`, clippy with warnings denied with and without `tui`, tests with both feature sets, warnings-denied doc build. |
| `make docs-check` | Run the quickstart blocks, parse config examples, check CLI drift, links and style. |
| `make install PREFIX=$HOME/.local` | Release build with `tui`, then install `fw-app` into `PREFIX/bin`. |
| `make uninstall` | Remove the installed binary (honours `PREFIX=`, `DESTDIR=` and `BINDIR=`). |
| `cargo test --locked --workspace` | Tests with default features. |
| `cargo test --locked --workspace --features tui` | Tests including the dashboard. |
| `bash scripts/validate-workflows.sh` | Workflow pins, permissions, policy and docs index. |

Run `make check` and `make docs-check` before proposing a change to docs or code. Both test feature sets are required because `top` and the ratatui view only build with `tui`.

## Repo map

- `app/src/main.rs`: CLI definition, `check` and `serve` setup, exit codes.
- `app/src/service.rs`: demo router, health response, protected route and throttle wiring.
- `app/src/logging.rs`: tracing layers, log file sink, SIGHUP reopen, write counter.
- `crates/fw-config/src/lib.rs`: config structs, defaults, validation, 64 KiB cap.
- `crates/fw-auth/src/`: digest comparison and per-address attempt throttle.
- `crates/fw-serve/src/`: accept loop, limits, request ids, shutdown, TLS.
- `crates/fw-tui/src/`: pure model, sanitizer, renderer, event loop, terminal guard.
- `fw-app.toml`: demo config used by the quickstart and release smoke test.
- `docs/`: one topic per page, indexed in `docs/index.md` and `llms.txt`.
- `scripts/check`: workflow and policy validator; `scripts/docs-check.sh`: docs checks.

## Conventions

- Keep docs and code in sync. A new flag, env var or config key needs a docs entry and a check that would fail if they drift.
- Use the template layout in the task description: proof first, one path then branches, mandatory limits, security model and compatibility matrix.
- No secret-shaped text in files or messages. Describe credentials and digests in prose and by their entry name. Never write a key block, a credential value, a token assignment or an authorization header with a value.
- No em dashes in docs. `docs-check.sh` rejects them.
- Commits are terse and imperative, with no job or personal names. Example style: `docs: add operations reference`.
- Run `cargo fmt --all` after code edits. Clippy warnings are denied; fix them rather than adding allow attributes.

## Things not to touch without a reason

- Pinned action SHAs and job names in `.github/workflows/`, or the policy lists in `deny.toml` and `dependency-review.yml`. `scripts/validate-workflows.sh` checks both.
- `Cargo.lock` files when dependencies did not change.
- The digest shape and demo entry name in `fw-app.toml`, because the quickstart asserts the resulting `ok:` line.
- The `# expect:` lines in the quickstart blocks unless you also update the expected output and the docs check.

## Quickstart as a smoke test

1. `make build` and put `target/release/fw-app` on PATH (or run `make install PREFIX=$HOME/.local`).
2. `make docs-check`. It builds a debug binary, copies the demo config to a throwaway directory, runs the three quickstart blocks in order and asserts every expected line. It also parses every `toml` example in `docs/configuration.md` with `fw_config::Config::parse`.
3. If a port 8080 conflict occurs, stop the other listener and retry. The quickstart expects the demo config unchanged.

Manual version: `fw-app --version`, `fw-app check --config fw-app.toml`, then `fw-app serve --config fw-app.toml` in one shell and `curl -fsS http://127.0.0.1:8080/healthz` in another. Expected check line: `ok: listen 127.0.0.1:8080, 1 token(s), tls off, log info`. Expected health body: `{"log_write_errors":0,"status":"ok"}`.

## Exit codes and HTTP statuses

Process exit 0 means success or a clean shutdown; exit 1 means a startup refusal or a serve error, printed with the `fw-app: ` prefix. HTTP 401, 429, 408 and 400 are response statuses and do not stop the listener.
