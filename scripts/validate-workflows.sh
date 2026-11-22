#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cargo run --quiet --manifest-path "$root/scripts/check/Cargo.toml" -- "$root"
