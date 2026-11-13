#!/usr/bin/env bash
# T-0-02 Tests: regression-tests the guard itself, both the pass case and the fail cases,
# using synthetic metadata/source fixtures in a scratch temp dir — never mutates the real
# workspace's dependency tree.
set -euo pipefail
cd "$(dirname "$0")/.."

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

echo "=== pass case: real workspace dependency tree (live path, no metadata-file override) ==="
./scripts/no_network_guard.sh

echo "=== building a filtered clean snapshot for the injection fixtures below ==="
host_target=$(rustc -vV | sed -n 's/^host: //p')
cargo metadata --all-features --filter-platform "$host_target" --format-version 1 > "$tmpdir/clean.json"

echo "=== fail case: synthetic reqwest dependency injected ==="
jq '.packages += [{"name": "reqwest", "version": "0.11.0", "id": "reqwest 0.11.0 ()", "dependencies": [], "targets": [], "features": {}, "manifest_path": ""}]' \
    "$tmpdir/clean.json" > "$tmpdir/dirty.json"
if ./scripts/no_network_guard.sh "$tmpdir/dirty.json" >/dev/null 2>&1; then
    echo "REGRESSION: guard passed despite an injected reqwest dependency" >&2
    exit 1
fi
echo "guard correctly rejected the injected reqwest dependency"

echo "=== fail case: un-allowlisted std::net usage in source ==="
mkdir -p "$tmpdir/src_check"
printf 'pub fn x() { let _ = std::net::TcpStream::connect("example.com:80"); }\n' \
    > "$tmpdir/src_check/offender.rs"
: > "$tmpdir/empty_allowlist.txt"
if ./scripts/no_network_guard.sh "$tmpdir/clean.json" "$tmpdir/src_check" "$tmpdir/empty_allowlist.txt" >/dev/null 2>&1; then
    echo "REGRESSION: guard passed despite un-allowlisted std::net usage" >&2
    exit 1
fi
echo "guard correctly rejected un-allowlisted std::net usage"

echo "=== pass case: same std::net usage, now allowlisted ==="
printf 'offender.rs\n' > "$tmpdir/filled_allowlist.txt"
./scripts/no_network_guard.sh "$tmpdir/clean.json" "$tmpdir/src_check" "$tmpdir/filled_allowlist.txt"
echo "guard correctly allowed std::net usage once allowlisted"

echo "=== all no_network_guard regression cases passed ==="
