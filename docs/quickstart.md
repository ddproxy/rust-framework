# Quickstart

Who this is for: a developer with Rust 1.99 or newer, `curl` and `fw-app` on PATH who wants one checkable result in ten minutes. For build steps, see [install](install.md).

The three command blocks below are also the quickstart in the README. `scripts/docs-check.sh` runs them in a throwaway directory and asserts every `# expect:` line.

1. Validate the shipped config. This step only reads and checks; it starts nothing.

   ```sh
   fw-app --version
   fw-app check --config fw-app.toml
   # expect: fw-app 0.1.0
   # expect: ok: listen 127.0.0.1:8080, 1 token(s), tls off, log info
   ```

   Expected output:

   ```text
   fw-app 0.1.0
   ok: listen 127.0.0.1:8080, 1 token(s), tls off, log info
   ```

2. Start the listener on loopback and read its health endpoint. The trap stops it when the shell exits.

   ```sh
   fw-app serve --config fw-app.toml >fw-app.log 2>&1 &
   pid=$!
   trap 'kill "$pid" 2>/dev/null || true' EXIT
   for i in $(seq 1 50); do curl -fsS http://127.0.0.1:8080/healthz >/dev/null 2>&1 && break; sleep 0.1; done
   curl -fsS http://127.0.0.1:8080/healthz
   # expect: {"log_write_errors":0,"status":"ok"}
   ```

   Expected output:

   ```text
   {"log_write_errors":0,"status":"ok"}
   ```

3. Proof. Assert the health response and print a named result.

   ```sh
   body=$(curl -fsS http://127.0.0.1:8080/healthz)
   printf '%s\n' "$body"
   case "$body" in
     *'"status":"ok"'*) printf 'PASS: health endpoint\n' ;;
     *) printf 'FAIL: health endpoint\n' >&2; exit 1 ;;
   esac
   # expect: PASS: health endpoint
   ```

   Expected output:

   ```text
   {"log_write_errors":0,"status":"ok"}
   PASS: health endpoint
   ```

## What this proves

`check` parsed the demo config, found one token entry and confirmed TLS is off. `serve` bound the loopback listener and answered a request with a request id. `/healthz` reports `log_write_errors` as zero until a file write fails. The protected route is `/v1/whoami`: a request without an accepted credential gets 401, and repeated attempts get 429 while the throttle is on. No credential value appears in this walkthrough.

## Next

- [Configuration](configuration.md) for every key and range.
- [Operations](operations.md) for signals, health and failure messages.
- [Limits](limits.md) for what the scaffold does not do.
