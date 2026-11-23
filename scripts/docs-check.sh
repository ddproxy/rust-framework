#!/usr/bin/env bash
# Checks the docs against the binary and the config parser. Runs the quickstart in a throwaway directory.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

fail() {
  printf 'docs-check: FAIL: %s\n' "$*" >&2
  exit 1
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

target_dir="${CARGO_TARGET_DIR:-$root/target}"

# The CLI reference is checked against both feature sets. Either build runs the quickstart,
# which does not use the dashboard, so the wrapper can point at the last binary built here.
cargo build --quiet --locked -p fw-app --target-dir "$target_dir"
bin="$target_dir/debug/fw-app"
[[ -x "$bin" ]] || fail "binary not found at $bin"

check_help_lines() {
  local source="$1"
  while IFS= read -r line; do
    [[ -n "$line" ]] || continue
    grep -Fqx -- "$line" docs/cli.md || fail "docs/cli.md is missing help line: $line ($source)"
  done < "$source"
}

"$bin" --help > "$tmp/help-default.txt"
check_help_lines "$tmp/help-default.txt" "default help"

cargo build --quiet --locked -p fw-app --features tui --target-dir "$target_dir"
"$bin" --help > "$tmp/help-tui.txt"
check_help_lines "$tmp/help-tui.txt" "tui help"

mkdir -p "$tmp/bin"
cat > "$tmp/bin/fw-app" <<EOF
#!/bin/sh
exec "$bin" "\$@"
EOF
chmod +x "$tmp/bin/fw-app"
cp fw-app.toml "$tmp/fw-app.toml"

extract_sh() {
  awk '
    /^[[:space:]]*```sh$/ { inside = 1; next }
    inside && /^[[:space:]]*```$/ { inside = 0; next }
    inside { sub(/^   /, ""); print }
  ' "$1"
}

readme_blocks="$(extract_sh README.md)"
quickstart_blocks="$(extract_sh docs/quickstart.md)"
[[ "$readme_blocks" == "$quickstart_blocks" ]] || fail "README quickstart differs from docs/quickstart.md"

expects="$(printf '%s\n' "$quickstart_blocks" | grep '^# expect:' | sed 's/^# expect: //' || true)"
script="$(printf '%s\n' "$quickstart_blocks" | grep -v '^# expect:' || true)"
[[ -n "$script" ]] || fail "no quickstart commands found"

out="$(cd "$tmp" && PATH="$tmp/bin:$PATH" bash -e -c "$script" 2>&1)" \
  || fail "quickstart failed: $out"
while IFS= read -r expected; do
  [[ -n "$expected" ]] || continue
  grep -Fq -- "$expected" <<<"$out" || fail "expected output missing: $expected (got: $out)"
done <<< "$expects"

# The Rust test reads docs/configuration.md and parses every TOML fence with fw_config.
cargo test --quiet --locked -p fw-config --test docs_examples

config_doc=docs/configuration.md
while IFS= read -r key; do
  grep -Fq -- "$key" "$config_doc" || fail "config key $key is not documented"
done < <(grep -hE '^    pub [a-z_]+:' crates/fw-config/src/lib.rs | sed -E 's/^    pub ([a-z_]+):.*/\1/' | sort -u)
grep -Fq 'RUST_LOG' "$config_doc" || fail "RUST_LOG is not documented"

for page in docs/*.md; do
  name="$(basename "$page")"
  [[ "$name" == "index.md" ]] && continue
  grep -Fq -- "$name" docs/index.md || fail "docs/index.md does not link $name"
done

for target in help build check docs-check install uninstall; do
  grep -Fq -- "$target" AGENTS.md || fail "make target $target is not in AGENTS.md"
done

for file in README.md AGENTS.md SECURITY.md CHANGELOG.md llms.txt docs/*.md; do
  dir="$(dirname "$file")"
  while IFS= read -r link; do
    case "$link" in
      http*|'#'*|mailto:*) continue ;;
    esac
    target="${link%%#*}"
    [[ -n "$target" ]] || continue
    [[ -f "$dir/$target" ]] || fail "$file: broken link: $link"
  done < <(grep -oE '\]\([^)]+\)' "$file" | sed 's/^](//; s/)$//')
done

if grep -n '—' README.md AGENTS.md SECURITY.md CHANGELOG.md llms.txt docs/*.md; then
  fail "em dash found in docs"
fi

printf 'docs-check: OK: quickstart, config examples, CLI, keys, links and style agree\n'
