# Configuration

Who this is for: an operator editing `fw-app.toml` or a developer adding keys to a copied tree.

The loader reads at most 64 KiB, refuses unknown keys at every level and names the table in every validation error. Missing keys and missing tables use the defaults below. `fw-app check --config <path>` applies the same parser without starting the listener.

## Keys

| Key | Default | Meaning and bounds |
|---|---|---|
| `server.listen` | `127.0.0.1:8080` | Socket address. A non-loopback address requires `[tls]`. |
| `server.max_connections` | `256` | Global open connections, 1 to 100000. |
| `server.max_connections_per_addr` | `16` | Per-address open connections, 1 to `max_connections`. |
| `server.header_read_timeout_secs` | `30` | Seconds allowed for request headers, 1 to 300. |
| `server.request_timeout_secs` | `60` | Seconds allowed for a handler, 1 to 600. Overruns get 408. |
| `tls.cert` | required with `[tls]` | PEM certificate chain path. |
| `tls.key` | required with `[tls]` | PEM key path. On Unix it must not be readable by group or others. |
| `log.level` | `info` | One of `error`, `warn`, `info`, `debug`, `trace`. `RUST_LOG` overrides this in `serve`. |
| `log.format` | `text` | `text` or `json`. |
| `log.stderr` | `true` | Also write to stderr. Always on when no file is set. |
| `log.file` | none | Append-only path. Created with mode 0640 on Unix, opened without following symlinks. |
| `state.dir` | none | Directory created and probed with a temporary file at startup. Empty strings are refused. |
| `auth.throttle` | `true` | Reserve one attempt per address on every credential check. Successes do not reset it. |
| `auth.max_failures` | `10` | Attempts per window, 1 to 1000. |
| `auth.window_secs` | `60` | Throttle window in seconds, 1 to 86400. |
| `auth.tokens` | empty | List of `[[auth.tokens]]` tables. |
| `auth.tokens[].name` | required | 1 to 64 characters from A-Z, a-z, 0-9, dot, underscore, hyphen. Unique. |
| `auth.tokens[].token_sha256` | required | 64 lowercase hex characters. Unique. The source length is not checked. |

`RUST_LOG` is read by the tracing filter at startup. When it is unset or cannot be parsed, `serve` falls back to `log.level`, then to `info`. `check` does not start the tracing subscriber, so it ignores that variable.

## Minimal example

This parses with the real validator in the docs test:

```toml
# docs-check: parse
[server]
listen = "127.0.0.1:9090"
```

## Full example

TLS and token entries are added only when needed; both are described below. This example also parses:

```toml
# docs-check: parse
[server]
listen = "127.0.0.1:8080"
max_connections = 256
max_connections_per_addr = 16
header_read_timeout_secs = 30
request_timeout_secs = 60

[log]
level = "info"
format = "text"
stderr = true
# file = "/var/log/fw-app.log"

[state]
dir = "/var/lib/fw-app"

[auth]
throttle = true
max_failures = 10
window_secs = 60
```

A listener on a non-loopback address needs a TLS table:

```toml
# docs-check: parse
[tls]
cert = "/etc/fw-app/cert.pem"
key = "/etc/fw-app/key.pem"
```

## TLS details

`fw-serve` loads the PEM chain and key, parses the leaf certificate's expiry and uses HTTP/1.1 only with ALPN `http/1.1`. An expired leaf is refused at load. A leaf expiring within 30 days loads with a warning. The `not_after` value in unix seconds appears in `fw-app check`, `/healthz` and the serve log.

## Auth entries

Each `[[auth.tokens]]` table carries a `name` and a digest of the credential under `token_sha256`. Generate the digest with your system's sha256sum tool over the chosen value, then put only the digest in the config. The shipped demo config shows the shape and must not be reused outside a demo. Never paste a credential or a digest into a ticket or chat message; describe the entry by its `name` instead. Token length is not enforced at config time because a digest cannot reveal the source length.

## Logging and state

A log file is opened in append mode with mode 0640 and `O_NOFOLLOW` on Unix. SIGHUP reopens the path so rotation works. Failed writes increment `log_write_errors`, exposed in `/healthz`, and emit a warning at most once every 60 seconds. A failed write never fails the request that caused it. When `[state] dir` is set, it is created and probed with a temporary file that is removed before startup continues.

See [troubleshooting](troubleshooting.md) for the exact startup messages and [limits](limits.md) for the numeric ceilings.
