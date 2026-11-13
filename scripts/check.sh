#!/usr/bin/env bash
# Full triple-check + Windows cross-check, run before considering any task done (roadmap §0 rule 2).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all --check
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo test --workspace
cargo check --target x86_64-pc-windows-msvc --workspace

if [ -x scripts/no_network_guard.sh ]; then
    scripts/no_network_guard.sh
fi

if [ -x scripts/check_licenses.sh ]; then
    scripts/check_licenses.sh
fi

if [ -x scripts/check_fixtures.sh ]; then
    scripts/check_fixtures.sh
fi

if [ -x scripts/check_visual_captures.sh ]; then
    scripts/check_visual_captures.sh
fi

if [ -x scripts/check_no_hardcoded_colors.sh ]; then
    scripts/check_no_hardcoded_colors.sh
fi

if [ -x scripts/check_icons.sh ]; then
    scripts/check_icons.sh
fi
