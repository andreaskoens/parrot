#!/usr/bin/env bash
# T-12-02: every literal egui color in parrot-app must come from src/tokens.rs (ui-description
# §10.1). Fails on any Color32::from_rgb*/from_gray with a numeric literal argument outside that
# module (computed colors, e.g. syntax-highlight output in render.rs, are not literals).
set -euo pipefail
cd "$(dirname "$0")/.."

hits=$(grep -rnE 'Color32::(from_rgb|from_rgba_premultiplied|from_rgba_unmultiplied|from_gray)\(\s*(0x)?[0-9]' \
    parrot-app/src parrot-app/tests 2>/dev/null | grep -v '^parrot-app/src/tokens.rs:' || true)

if [ -n "$hits" ]; then
    echo "check_no_hardcoded_colors: literal colors outside parrot-app/src/tokens.rs:" >&2
    echo "$hits" >&2
    exit 1
fi
echo "check_no_hardcoded_colors: PASS"
