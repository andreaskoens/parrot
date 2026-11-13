#!/usr/bin/env bash
# T-4-01: asserts the real ULSS fixture corpus is present and complete. The expected set is 10
# files, not the roadmap's original "17" — corrected after direct inspection of the real Parrot
# bundle found HTML/ePub "styles" are plain CSS, not .ulss; see tests/fixtures/FIXTURES_SOURCE.md.
set -euo pipefail
cd "$(dirname "$0")/.."

FIXTURES_DIR="tests/fixtures/ulss"
EXPECTED=(
    academica.ulss
    business.ulss
    columns.ulss
    defaults.ulss
    manuscript.ulss
    papers.ulss
    readingmode.ulss
    revision.ulss
    rtf_default.ulss
    swissknife.ulss
)

missing=0
for f in "${EXPECTED[@]}"; do
    if [ ! -f "$FIXTURES_DIR/$f" ]; then
        echo "check_fixtures: FAIL — missing $FIXTURES_DIR/$f" >&2
        missing=1
    fi
done

actual_count=$(find "$FIXTURES_DIR" -maxdepth 1 -iname "*.ulss" | wc -l | tr -d ' ')
if [ "$actual_count" -ne "${#EXPECTED[@]}" ]; then
    echo "check_fixtures: FAIL — expected exactly ${#EXPECTED[@]} .ulss files in $FIXTURES_DIR, found $actual_count" >&2
    missing=1
fi

if [ "$missing" -eq 1 ]; then
    exit 1
fi

echo "check_fixtures: PASS (${#EXPECTED[@]}/${#EXPECTED[@]} real ULSS fixtures present)"
