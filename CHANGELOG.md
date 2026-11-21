# Changelog

## [Unreleased]

- `fw-auth`: `LoginThrottle::try_attempt` checks and reserves an attempt under one lock, closing
  the concurrent-burst race. `blocked`/`fail` are replaced by this one call.
- `fw-serve`: saturating helpers for unix timestamps and remaining time; certificate `not_after`
  is parsed at load, expired leaves are refused, and expiries within 30 days warn.
- `fw-app`: the log file sink reopens on SIGHUP, counts write failures in an atomic exposed as
  `log_write_errors` in `/healthz`, and warns at most once a minute. Panics are logged through
  `tracing` with message and location.
- `fw-config`: optional `[state] dir`, created and probed for writability at startup with a clear
  error. `fw-app check` and `/healthz` expose `not_after` (unix seconds) when TLS is configured.
