#!/usr/bin/env bash
# T-3-01 gate: fails the build if a grammar/theme file is added to the repo without a
# corresponding resolution line in THIRD_PARTY_LICENSES.md. Mirrors scripts/no_network_guard.sh's
# shape (scan the tree, fail loudly, exit nonzero).
set -euo pipefail
cd "$(dirname "$0")/.."

LICENSES_FILE="THIRD_PARTY_LICENSES.md"
violation_found=0

while IFS= read -r -d '' file; do
    echo "check_licenses: FAIL — $file was added but has no resolution line in $LICENSES_FILE" >&2
    violation_found=1
done < <(find . -path "*/target" -prune -o -path "*/.git" -prune \
    \( -iname "*.tmLanguage" -o -iname "*.tmTheme" -o -iname "*.sublime-syntax" \) -print0)

if [ "$violation_found" -eq 1 ]; then
    echo "check_licenses: add a resolution line to $LICENSES_FILE before shipping a new grammar/theme file." >&2
    exit 1
fi

echo "check_licenses: PASS (no grammar/theme files in the repo — syntax highlighting uses syntect's bundled defaults, see $LICENSES_FILE)"
