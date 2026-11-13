#!/usr/bin/env bash
# T-0-02: fails the build if a networking crate enters the dependency tree, or if raw
# std::net usage appears in source outside the explicit allowlist. Enforced from T-0-01
# onward per the roadmap's hard constraint: no network code anywhere, ever.
#
# Usage: no_network_guard.sh [metadata_json_file] [scan_root] [allowlist_file]
#   metadata_json_file - path to a `cargo metadata --format-version 1` JSON dump.
#                         Omit to run `cargo metadata` live against the real workspace.
#                         (Lets the regression test in no_network_guard_test.sh feed
#                         synthetic JSON without touching the real dependency tree.)
#   scan_root           - directory to scan for raw std::net usage. Defaults to ".".
#   allowlist_file       - path to the allowlist file. Defaults to "scripts/network_allowlist.txt".
set -euo pipefail
cd "$(dirname "$0")/.."

# Banned outright, broader than the spec's literal list (reqwest/hyper/ureq/tokio-with-net):
# tokio is banned in full (not just its "net" feature) because this is a desktop GUI app with
# no legitimate async-runtime need; the TLS/HTTP-adjacent crates are defense-in-depth against
# a transitive dependency quietly pulling in networking. Documented as a deliberate stricter
# reading in DECISIONS.md.
BANNED_CRATES=(reqwest hyper ureq tokio async-std isahc surf attohttpc curl native-tls openssl rustls)

METADATA_FILE="${1:-}"
SCAN_ROOT="${2:-.}"
ALLOWLIST_FILE="${3:-scripts/network_allowlist.txt}"

if [ -n "$METADATA_FILE" ]; then
    names=$(jq -r '.packages[].name' "$METADATA_FILE" | sort -u)
else
    # `cargo metadata` with no --filter-platform reports the union of every cfg(target_os) this
    # workspace could ever build for, including platform-specific transitive deps (e.g. eframe's
    # Linux accessibility backend pulling in zbus -> tokio) that will never actually compile on
    # the targets this project ships for (dev host + the Windows ship target, ADR-1/roadmap §2).
    # Filtering to just those two targets is the correct check: "no network code in what we
    # actually build," not "no network crate anywhere in the full cross-platform graph."
    host_target=$(rustc -vV | sed -n 's/^host: //p')
    echo "no-network-guard: resolving dependency tree for $host_target + x86_64-pc-windows-msvc..."
    names=$(
        {
            cargo metadata --all-features --filter-platform "$host_target" --format-version 1 | jq -r '.packages[].name'
            cargo metadata --all-features --filter-platform x86_64-pc-windows-msvc --format-version 1 | jq -r '.packages[].name'
        } | sort -u
    )
fi

violations=()
for crate in "${BANNED_CRATES[@]}"; do
    if echo "$names" | grep -qx "$crate"; then
        violations+=("$crate")
    fi
done

if [ ${#violations[@]} -gt 0 ]; then
    echo "no-network-guard: FAIL — banned networking crate(s) in dependency tree: ${violations[*]}" >&2
    exit 1
fi

echo "no-network-guard: checking '$SCAN_ROOT' for raw std::net usage..."
allowlisted=()
if [ -f "$ALLOWLIST_FILE" ]; then
    while IFS= read -r line; do
        case "$line" in
            ''|'#'*) continue ;;
        esac
        allowlisted+=("$line")
    done < "$ALLOWLIST_FILE"
fi

violation_found=0
while IFS= read -r -d '' file; do
    if grep -q 'std::net' "$file"; then
        rel="${file#"$SCAN_ROOT"/}"
        skip=0
        for a in "${allowlisted[@]:-}"; do
            [ "$rel" = "$a" ] && skip=1 && break
        done
        if [ "$skip" -eq 0 ]; then
            echo "no-network-guard: FAIL — raw std::net usage in $file (not in $ALLOWLIST_FILE)" >&2
            violation_found=1
        fi
    fi
done < <(find "$SCAN_ROOT" -path "*/target" -prune -o -path "*/.git" -prune -o -name '*.rs' -print0)

if [ "$violation_found" -eq 1 ]; then
    exit 1
fi

echo "no-network-guard: PASS"
