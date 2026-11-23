# Security

Report vulnerabilities privately through GitHub's [private vulnerability reporting](https://github.com/ddproxy/rust-framework/security/advisories/new) form. Do not open a public issue for an unfixed vulnerability.

Describe the affected file or entry by its `name`. Do not include a credential, a digest or a token value in a report.

## Supported versions

Only the latest release gets security fixes. Older tags are not patched. The current version is 0.1.0.

## Response times

We aim to acknowledge a report within 3 business days and post a status update at least every 7 days until it is resolved. Fix timing depends on severity; we do not promise a fixed date.

## Scope

Reports about token handling, the login throttle, the listener's connection limits, TLS setup, log output and terminal output are especially welcome. This is a reference scaffold, so issues in the demo routes are in scope only if they affect one of those shared components.

## Threat model

See the [security model](docs/security.md) for trust boundaries, what fails closed, and the logging and signal paths that fail open. [Limits and non-goals](docs/limits.md) lists what is deliberately out of scope, including credential lifecycle and shared rate limits.

See [Security scanning](docs/security-scanning.md) for the checks this repository runs and how to verify a release.
