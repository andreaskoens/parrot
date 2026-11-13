#!/usr/bin/env bash
# Windows packaging (T-7-06): produces a portable-zip artifact for parrot-app, not a WiX/MSI
# installer — a portable zip has far lower setup friction for an unsupervised agent (no WiX
# toolset install/config needed) and the roadmap's own task text explicitly allows picking
# "whichever has lower setup friction."
#
# This machine (macOS, no Visual Studio / Build Tools) cannot link an actual
# x86_64-pc-windows-msvc .exe — `link.exe` doesn't exist here, confirmed by a real
# `cargo build --release --target x86_64-pc-windows-msvc` attempt before writing this script
# (see DECISIONS.md). So this script tries the real cross-compile first — on a machine that
# *can* link (CI, or a future environment with a Windows SDK cross-linker such as cargo-xwin),
# it packages the real built .exe. On this sandbox, where that build fails only at the final
# link step, it falls back to producing a "packaging kit" artifact instead: everything needed to
# finish packaging on a capable machine, honestly labeled as such — never a fake or renamed
# non-Windows binary.
set -euo pipefail

cd "$(dirname "$0")/.."
VERSION="$(grep -A5 '^\[workspace.package\]' Cargo.toml | grep '^version' | head -1 | cut -d'"' -f2)"
DIST_DIR="dist"
STAGE_DIR="$DIST_DIR/parrot-windows-portable"
ZIP_PATH="$DIST_DIR/parrot-windows-portable-v${VERSION}.zip"

rm -rf "$DIST_DIR"
mkdir -p "$STAGE_DIR"

echo "package_windows: attempting a real x86_64-pc-windows-msvc release build..."
if cargo build --release --target x86_64-pc-windows-msvc -p parrot-app 2>build_output.log; then
    echo "package_windows: cross-compile succeeded — packaging the real binary."
    cp "target/x86_64-pc-windows-msvc/release/parrot-app.exe" "$STAGE_DIR/"
    cp "parrot-app/assets/icon.ico" "$STAGE_DIR/"
    cat > "$STAGE_DIR/README.txt" <<EOF
Parrot (Rust rewrite) v${VERSION} — portable Windows build.
Run parrot-app.exe directly; no installation required.
EOF
    ARTIFACT_KIND="full-binary"
else
    echo "package_windows: cross-compile could not complete on this machine (expected — see" \
         "below and DECISIONS.md). Producing a packaging kit instead of a fake binary."
    if grep -qi "link.exe" build_output.log || grep -qi "linker" build_output.log; then
        echo "package_windows: confirmed failure is the missing MSVC linker, not a code error."
    fi
    cp "parrot-app/assets/icon.ico" "$STAGE_DIR/"
    cp build_output.log "$STAGE_DIR/cross_compile_attempt.log"
    cat > "$STAGE_DIR/README.txt" <<EOF
Parrot (Rust rewrite) v${VERSION} — Windows packaging kit.

This machine has no MSVC linker (link.exe) available, so it cannot produce a linked
x86_64-pc-windows-msvc .exe (see cross_compile_attempt.log for the real build attempt's
output). To finish packaging on a machine that can:

    cargo build --release --target x86_64-pc-windows-msvc -p parrot-app
    cp target/x86_64-pc-windows-msvc/release/parrot-app.exe <this folder>

Then zip this folder's contents (parrot-app.exe, icon.ico) as the portable distribution.
icon.ico is already the real app icon (scripts/generate_icon.py) — no placeholder needed there.
EOF
    ARTIFACT_KIND="packaging-kit"
fi
rm -f build_output.log

(cd "$DIST_DIR" && zip -r -q "$(basename "$ZIP_PATH")" "$(basename "$STAGE_DIR")")

echo "package_windows: wrote $ZIP_PATH (kind: $ARTIFACT_KIND)"
