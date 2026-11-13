#!/usr/bin/env bash
# Every UI icon must come from parrot-app/src/icons.rs (Phosphor). Fails on any string literal
# outside that module containing an emoji or symbol/dingbat/geometric-shape character — the
# glyph classes egui's bundled fonts and Segoe UI only partly cover. Prose punctuation (…, ·, →)
# sits outside these ranges and is allowed.
set -euo pipefail
cd "$(dirname "$0")/.."

hits=$(grep -rnP '"[^"\n]*[\x{2300}-\x{2BFF}\x{1F000}-\x{1FFFF}][^"\n]*"' \
    parrot-app/src parrot-app/tests 2>/dev/null | grep -v '^parrot-app/src/icons.rs:' | grep -vP '^[^:]+:[0-9]+:\s*//' || true)

if [ -n "$hits" ]; then
    echo "check_icons: icon glyphs outside parrot-app/src/icons.rs (use a Phosphor icon from there):" >&2
    echo "$hits" >&2
    exit 1
fi
echo "check_icons: PASS"
