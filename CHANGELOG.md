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
- `fw-tui`: dashboard counters and tick numbers saturate at `u64::MAX` instead of overflowing.
- `fw-config`: malformed listen addresses, empty input and very large numeric fields are reported as
  validation or parse errors; the 64 KiB file cap is checked with an exact byte conversion.
- `fw-auth`, `fw-serve`: throttle waits and per-address connection counts saturate at extreme
  settings instead of relying on subtraction or increment invariants. Refused connections no longer
  leave empty entries in the per-address table.
- `fw-app`: the serve log includes the TLS leaf's `not_after` (unix seconds) when TLS is configured,
  as `fw-app check` and `/healthz` already did.
- Docs and build: public API docs now carry runnable examples; `make check` runs fmt, clippy, tests
  (with and without `tui`) and a warnings-denied doc build. The README and security-scanning docs
  describe the new target.
