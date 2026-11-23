# Limits and non-goals

Who this is for: anyone deciding whether to copy this scaffold, and a reviewer checking a claim made elsewhere in the docs.

## Numeric limits

These values come from the code. Configuration bounds are rejected at parse time; runtime caps are enforced by the listener or throttle.

| Limit | Value | Where |
|---|---|---|
| Config file size | 65536 bytes (64 KiB), inclusive | `fw-config::load` |
| Global open connections | 1 to 100000, default 256 | `server.max_connections` |
| Open connections per address | 1 to global cap, default 16 | `server.max_connections_per_addr` |
| Header read timeout | 1 to 300 seconds, default 30 | `server.header_read_timeout_secs` |
| Handler timeout | 1 to 600 seconds, default 60; overrun gets 408 | `server.request_timeout_secs` |
| TLS handshake timeout | 10 seconds | `fw-serve` |
| Shutdown drain timeout | 10 seconds | `fw-serve` |
| Certificate expiry warning | 30 days before `not_after` | `fw-serve::tls` |
| Throttle attempts per window | 1 to 1000, default 10 | `auth.max_failures` |
| Throttle window | 1 to 86400 seconds, default 60 | `auth.window_secs` |
| Tracked throttle addresses | 4096; IPv6 keyed by /64 | `fw-auth` |
| Token name | 1 to 64 ASCII characters from A-Z, a-z, 0-9, dot, underscore, hyphen | `auth.tokens` |
| Token digest | 64 lowercase hex characters | `auth.tokens` |
| Extractor body limit | 64 KiB | demo router |
| Log warning interval | 60 seconds | `fw-app::logging` |
| Dashboard tick and key poll | 1 second and 100 ms | `fw-tui` |

A raw `Body` has no built-in limit; a handler that reads one must bound it. A successful credential check does not reset the throttle count.

## Non-goals

- Not a library, dependency or published package. Copy the tree and rename the prefix.
- Not a production service. The two HTTP routes are demos and the dashboard uses synthetic data.
- No real metrics endpoint. `/healthz` carries a status, a log-write counter and a certificate expiry.
- No shared or persistent rate limit, no cluster coordination and no horizontal scaling story.
- No credential lifecycle: no rotation, expiry, revocation list or source-length rule.
- No client-certificate auth, no HTTP/2 and no per-route authorization.
- No key-permission check off Unix, where mode bits have no comparable meaning.
- No audit trail or signed logs.

## Known gaps

- Refused connections are logged at debug level, so a flood is invisible at the default `info` level.
- Idle keep-alive connections hold their slot until the 10-second drain timeout during shutdown.
- The release workflow runs only on github.com because of an `if:` guard. A forge-hosted copy needs its own release step.
- Token length is not enforced at config time because a digest cannot show how long its source was.
- The CI workflow keeps a rustup fallback that pipes a download to a shell for runners without Rust. Drop it if every runner you use has Rust.
- If a shutdown signal handler cannot be installed, the shutdown future never resolves and the server keeps running until stopped by other means.

## Fail-open and fail-closed in plain words

Config, TLS, auth and state checks fail closed: a problem stops startup or rejects the request. Logging fails open: a write error is counted and the request still succeeds. The signal path fails open: a missing handler does not stop the server. Connection caps fail closed by dropping the connection without a response.

See [security model](security.md) for the full boundary list and [troubleshooting](troubleshooting.md) for fixes.
