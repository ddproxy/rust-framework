# Troubleshooting

Who this is for: an operator with a failing command or an unexpected HTTP response who wants the exact message and the fix.

## The config check fails

Run `fw-app check --config fw-app.toml`. Every startup error is printed as `fw-app: <message>` on stderr and exits 1. The message names its table.

| Symptom | Meaning | Fix |
|---|---|---|
| `cannot read fw-app.toml: ...` | The file is missing or unreadable. | Fix the path passed to `--config` or its permissions. |
| `fw-app.toml is larger than 65536 bytes` | The 64 KiB cap was hit. | Shorten the file. |
| `invalid TOML: ...` | Syntax error. | Edit the reported line. |
| `[serevr]: ...` | Unknown table. | Use `server`; unknown keys are refused everywhere. |
| `[server]: listen must be a socket address like 127.0.0.1:8080 (got ...)` | Not `host:port`. | Use a parseable address. |
| `[server]: listen 0.0.0.0:8080 is not a loopback address, so [tls] is required` | Cleartext off host is refused. | Add a TLS table or bind loopback. |
| `[server]: max_connections must be 1 to 100000 (got 0)` | Out of range. | Set a value in range. |
| `[server]: max_connections_per_addr must be 1 to max_connections (256) (got 300)` | Per-address cap above global cap. | Lower the per-address value. |
| `[log]: level must be one of error, warn, info, debug, trace (got "loud")` | Unknown level. | Use a listed level. |
| `[tls]: cert and key must both be set` | Partial TLS table. | Set both paths or remove the table. |
| `[tls] key <path> has mode 0644; it must not be accessible to group or others (chmod 600)` | Key readable by others on Unix. | Tighten the mode to 0600. |
| `tls cert <path> expired at unix time ...` | Leaf certificate is expired. | Replace the certificate. |
| `state directory <path> is not writable: ...` | The state probe failed. | Fix the mount or directory permissions. |
| `[[auth.tokens]] #1 (peer-a): token_sha256 must be 64 lowercase hex characters` | Malformed digest. | Store a lowercase SHA-256 digest. Do not put the credential itself here. |
| `name is used by an earlier token` or `token_sha256 is the same as an earlier token` | Duplicate entry. | Remove or rename the duplicate. |

A valid check prints one line:

```text
ok: listen 127.0.0.1:8080, 1 token(s), tls off, log info
```

The counts and `tls` state in that line come from the config you passed, so a surprising value means you checked a different file than you edited.

## The listener will not start

- `cannot listen on <addr>: ...` means the bind failed. Free the port or change `server.listen`.
- `cannot open log file <path>: ...` means the file sink could not be opened. Check the path and remove a symlink at that path.
- An expired certificate is refused during the same setup as `check`, before the port is bound.

## Requests fail

| Response | Cause | Fix |
|---|---|---|
| 401 with body `unauthorized` | No accepted credential reached the protected route. | Configure a digest from sha256sum and send a matching credential from the client. |
| 429 with body `too many attempts` | The address used all attempts in the window. | Wait for `Retry-After`, or lower the attempt count only if the policy should change. |
| 408 with body `request timed out` | The handler exceeded `server.request_timeout_secs`. | Speed up the handler or raise the bounded timeout. |
| 400 | The HTTP parser rejected malformed headers. | Fix the client, especially control bytes in header values. |
| Empty response | A connection cap was reached and the connection was dropped. | Raise the cap or reduce clients. These drops are logged at debug level. |

`GET /healthz` needs no credential. If it fails while `check` passes, the problem is in the listener, not the config.

## Logs and shutdown

- SIGHUP on a build with no `log.file` prints `no log file configured`; it is a warning, not a failure.
- Failed file writes increment `log_write_errors` in `/healthz` and warn `log file write failed` at most once every 60 seconds. Requests still succeed.
- SIGTERM or Ctrl-C logs `shutdown requested`, then `stopped`. The warning `shutdown: connections still open after the drain timeout` means a connection outlived the 10-second drain and was abandoned.
- If the dashboard leaves a terminal in raw mode, run `reset` in the shell. The guard normally restores it on exit and on panic.

## Build and docs checks

| Failure | Fix |
|---|---|
| `cargo fmt --all --check` fails | Run `cargo fmt --all` and re-run `make check`. |
| Clippy fails with warnings denied | Fix the warning; do not add an allow attribute without a reason. |
| `docs-check` reports an expected line missing | Re-run the quickstart by hand and compare with `# expect:` lines in [quickstart](quickstart.md). |
| `docs-check` reports a CLI drift | Regenerate the help text in [cli](cli.md) from `fw-app --help`. |
| `docs-check` reports a broken link | Fix the path relative to the file that contains it. |

Both test feature sets are required: `cargo test --locked --workspace` and `cargo test --locked --workspace --features tui`. See [operations](operations.md) for routine procedures and [security scanning](security-scanning.md) for release verification.
