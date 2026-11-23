# Operations

Who this is for: an operator running `fw-app serve` who needs health checks, signals and recovery steps.

## Running and stopping

`fw-app serve --config fw-app.toml` is also the default when no subcommand is given. Startup validates the config, probes the optional state directory, opens the log file, loads TLS and binds the listener. The serve log records the address, TLS state, certificate expiry (or `none`) and the config path.

| Signal | Effect |
|---|---|
| SIGTERM or Ctrl-C | Stop accepting, drain in-flight requests for up to 10 seconds, log `stopped`, exit 0. |
| SIGHUP | Reopen the configured log file. Without a file sink the warning is `no log file configured`. |

If a connection is still open when the drain timeout expires, the warn message is `shutdown: connections still open after the drain timeout` and the connection is abandoned. If a signal handler cannot be installed, that shutdown path never resolves; the process keeps serving until stopped another way.

## Health and metrics

`fw-app check --config fw-app.toml` exits 0 after printing one `ok:` line. It exits 1 and prints `fw-app: <message>` on stderr for any startup-time refusal. `serve` uses the same exit codes when it fails.

`GET /healthz` returns JSON with `status` and `log_write_errors`. When TLS is configured it also returns `not_after` in unix seconds. `log_write_errors` counts failed writes to the log file since startup; it starts at zero. There is no Prometheus endpoint and no other metric.

## Upgrade, rollback, backup, restore

| Task | Step |
|---|---|
| Upgrade | Build or install the new binary, then run `fw-app check` before replacing the running process. |
| Rollback | Restore the previous binary and re-run `fw-app check`. Config and state are not versioned. |
| Backup | Copy `fw-app.toml`, the TLS files and the configured state directory. Keep the config with its binary. |
| Restore | Put the copies back, run `fw-app check`, then serve. |
| Uninstall | `make uninstall` (honours `PREFIX=`, `DESTDIR=` and `BINDIR=`). |

## Logs

With `log.stderr = true` (the default), events go to stderr; setting `log.file` adds an append-only sink. Format is `text` or `json`. `RUST_LOG` overrides `log.level` in `serve`. The file is created with mode 0640 and refused if it is a symlink on Unix. Request spans record an id, method and path but never the query string. An incoming request id is kept only when it is a short plain token.

A log write failure increments `log_write_errors` and emits `log file write failed` at most once every 60 seconds. The request that caused it still succeeds.

## Common failures

Startup errors are printed as `fw-app: <message>` on stderr and exit 1.

| Message | Fix |
|---|---|
| `cannot read <path>: ...` | Fix the config path or its permissions. |
| `<path> is larger than 65536 bytes` | Shrink the file below the 64 KiB cap. |
| `invalid TOML: ...` | Fix the syntax at the reported location. |
| `[server]: listen must be a socket address like 127.0.0.1:8080 (got ...)` | Use a `host:port` address. |
| `[server]: listen <addr> is not a loopback address, so [tls] is required` | Add a TLS table or bind to loopback. |
| `[server]: max_connections must be 1 to 100000 (got ...)` | Set a value in range. |
| `[server]: max_connections_per_addr must be 1 to max_connections (...) (got ...)` | Lower the per-address cap. |
| `[server]: header_read_timeout_secs must be 1 to 300 (got ...)` | Set a value in range. |
| `[server]: request_timeout_secs must be 1 to 600 (got ...)` | Set a value in range. |
| `[log]: level must be one of error, warn, info, debug, trace (got ...)` | Use one of the listed levels. |
| `[tls]: cert and key must both be set` | Set both paths or remove the table. |
| `[tls] key <path> has mode 0644; it must not be accessible to group or others (chmod 600)` | Tighten the file mode. The check runs on Unix only. |
| `tls cert <path> expired at unix time ...` | Install a current certificate. Expiry within 30 days only warns. |
| `state directory <path> is not writable: ...` | Fix the mount or permissions for the state directory. |
| `cannot open log file <path>: ...` | Fix the log path or permissions, or remove a symlink at that path. |
| `[[auth.tokens]] #1 (peer-a): token_sha256 must be 64 lowercase hex characters` | Store a lowercase SHA-256 digest, not the credential. |
| `name is used by an earlier token` | Rename the duplicate entry. |
| `token_sha256 is the same as an earlier token` | Remove the duplicate digest. |
| `cannot listen on <addr>: ...` | Free the port or change `server.listen`. |

Request-time failures, not startup errors:

| Response | Body | Cause and fix |
|---|---|---|
| 401 | `unauthorized` | No accepted credential on the protected route. Add a valid entry under `auth.tokens`. |
| 429 | `too many attempts` | The per-address throttle is full. Wait for the `Retry-After` interval. |
| 408 | `request timed out` | The handler exceeded `server.request_timeout_secs`. |
| 400 | set by the HTTP parser | Malformed headers. Fix the client. |
| none | empty response | A connection cap was reached. Refused connections get no body and are logged at debug level. |

For validation messages that name a different table, the same wording applies: the table name comes first, then the reason. See [troubleshooting](troubleshooting.md) for symptom-first steps and [security scanning](security-scanning.md) for release verification.
