# Security scanning

The workflows in `.github/workflows/` run these checks. Required status-check names are `test`, `Audit`, `CodeQL` and `dependency-review`.

| Workflow | Runs on | Check | Where findings appear |
|---|---|---|---|
| `ci.yml` (`test`) | Pushes, pull requests, manual | `cargo fmt --check`, `cargo clippy -D warnings` with and without `tui`, `cargo test`; all Cargo commands use `--locked` | Actions tab, pull-request check `test` |
| `audit.yml` (`Audit`) | Pushes, pull requests, daily, manual | `cargo deny check advisories bans licenses sources` from `deny.toml` | Actions tab, pull-request check `Audit` |
| `dependency-review.yml` (`dependency-review`) | Pull requests | New dependencies with high or critical advisories in any scope, or licenses outside the `deny.toml` allow list | Pull-request check `dependency-review`, Actions tab |
| `codeql.yml` (`CodeQL`) | Pushes to `main`, pull requests, weekly, manual | CodeQL for Rust, `build-mode: none`; results uploaded as SARIF | Security tab → Code scanning, Actions tab |
| `scorecard.yml` | Pushes to `main`, weekly | OpenSSF Scorecard, published to the Scorecard API and uploaded as SARIF | Security tab → Code scanning, Scorecard viewer, Actions tab (results artifact, 5 days) |
| `release.yml` | `v*` tags | Builds the archives, attaches checksums and a CycloneDX JSON SBOM, and creates build provenance attestations | Releases page (assets), Actions tab and `gh attestation verify` (provenance) |

The license allow list is defined in `deny.toml` and repeated as `allow-licenses` in `dependency-review.yml`; change both together. `scripts/validate-workflows.sh` checks that they match, that all workflow YAML parses, that every `uses:` is pinned to a 40-character SHA with a version comment, and that job permissions stay within the sets the script declares. `make check` runs the local gates: fmt, clippy and tests with and without `tui`, and a doc build with warnings denied.

## Verify a release

Archives are named `fw-app-<tag>-<target>.tar.gz`; the SBOM is `rust-framework-<tag>.cdx.json`.

```sh
tag=v0.1.0
target=x86_64-unknown-linux-musl
gh release download "$tag" --repo ddproxy/rust-framework --pattern '*'
gh attestation verify "fw-app-$tag-$target.tar.gz" --repo ddproxy/rust-framework
shasum -a 256 -c "fw-app-$tag-$target.tar.gz.sha256"
shasum -a 256 -c SHA256SUMS
```

`gh attestation verify` checks the build provenance stored in this repository. `SHA256SUMS` covers every archive and the SBOM; the per-archive `.sha256` files are also attached. Open the `.cdx.json` file to inspect the CycloneDX components.

## For the repository owner

These settings cannot be set from files in the repository:

1. Enable secret scanning and push protection under Settings → Code security and analysis.
2. Enable Dependabot alerts under Settings → Code security and analysis. The update schedule itself is in `.github/dependabot.yml`.
3. Enable private vulnerability reporting so the link in `SECURITY.md` works.
4. Protect `main`: require a pull request and require the status checks `test`, `Audit`, `CodeQL` and `dependency-review`.
5. Leave code scanning default setup off; `codeql.yml` and the SARIF upload in `scorecard.yml` supply the results.
6. Register the project for the OpenSSF Best Practices badge. No badge is shown until registration is complete.
7. Do not add a crates.io badge: the workspace sets `publish = false` and no crate is published.
