# Security model

Who this is for: a reviewer deciding where this scaffold can be trusted, and an operator configuring auth and TLS.

## What it protects

- Config integrity: files over 64 KiB, unknown keys, out-of-range values, duplicate token names and duplicate digests are refused before the listener starts.
- Credentials at rest: only a lowercase SHA-256 digest is stored. A presented value is hashed and compared in constant time against every configured entry, so the matching entry is not revealed by timing.
- The listener: global and per-address connection caps, a header read timeout and a handler timeout bound resource use. IPv6 addresses share a count by /64. At most 4096 addresses are tracked, and expired entries are evicted first.
- Transport: a non-loopback listener requires TLS. Expired certificates are refused at load; leaves expiring within 30 days warn. ALPN is HTTP/1.1 only.
- Logs and terminal output: request logs use the path without the query string. Incoming request ids are kept only when they are short plain tokens. Names drawn in the dashboard are sanitized so control and bidi characters cannot reorder or hide text.

## Trust boundaries

| Boundary | Assumption | Failure behaviour |
|---|---|---|
| Config file and TLS files | Written by the operator, mode 0600 for a key on Unix | Fail closed: startup refuses the file. |
| Network client | Untrusted, may send malformed headers or hostile header values | Fail closed: 400, 401 or 429, or the connection is dropped. |
| State directory | Local filesystem, writable only by the service | Fail closed: startup refuses an unwritable path. |
| Log file | Local filesystem, operator-controlled rotation | Fail open for the request: a failed write is counted and warned about, never propagated. |
| Signals | Local process supervisor | Fail open: a handler that cannot be installed leaves the server running. |
| Dashboard data | Local demo generator today | No trust decision is made; it is synthetic and must be replaced before real use. |

The throttle has no success reset. A caller holding one valid credential cannot clear the count between guesses at another. Every attempt reserves a slot, valid credential or not, and the check plus reservation share one lock.

## What it does not do

- It does not store credentials, rotate them, expire them or enforce a minimum source length. A digest cannot reveal the source length.
- It does not authenticate clients with certificates, support HTTP/2, or provide per-route authorization beyond the two demo routes.
- It does not share throttle state between processes or hosts. Each binary has its own in-memory table.
- It does not verify TLS key permissions off Unix, and it does not audit log access or sign logs.
- It is a scaffold, not a hardened production service. Replace the demo routes and review the [limits](limits.md) before deployment.

## Reporting

Report vulnerabilities privately as described in [SECURITY.md](../SECURITY.md). Describe an affected entry by its `name`; do not include a credential, a digest or a token value in a report. See [security scanning](security-scanning.md) for the checks this repository runs.
