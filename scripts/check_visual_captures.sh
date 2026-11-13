#!/usr/bin/env bash
# T-9-01 (+ T-11-01 and T-12-12 extensions): asserts one capture (an image under tests/visual/, or a
# documented structural-audit section) exists per required screen. This sandbox has no
# offscreen rendering path (see docs/ui_structural_audit.md's header for why), so today every
# screen is covered by the audit-doc fallback; the image check is kept so a future machine that
# *can* render offscreen has somewhere to drop tests/visual/*.png and have this script notice.
set -euo pipefail
cd "$(dirname "$0")/.."

AUDIT_DOC="docs/ui_structural_audit.md"
VISUAL_DIR="tests/visual"

screens=(
    "library tree"
    "single-sheet editor"
    "filters UI"
    "search UI"
    "preferences"
    "style picker"
    "first-run flow"
    "export dialog"
    "goal setup/edit flow"
    "keyword manager"
    "container/sheet-flow view"
    "focus mode overlay"
    "statistics panel"
    "attachments UI"
    # M12 (T-12-12)
    "four-panel layout"
    "library sidebar smart filters"
    "sidebar context menu"
    "goal rings and badges"
    "sheet list rows"
    "editor text column"
    "sheet-flow dividers"
    "inspector panel"
    "toolbar"
    "quick export modal"
    "annotation styling"
)

missing=()
for screen in "${screens[@]}"; do
    slug=$(echo "$screen" | tr '[:upper:] ' '[:lower:]-')
    has_image=false
    if [ -d "$VISUAL_DIR" ]; then
        if compgen -G "$VISUAL_DIR/${slug}*.png" > /dev/null 2>&1; then
            has_image=true
        fi
    fi
    has_audit_note=false
    if [ -f "$AUDIT_DOC" ] && grep -qi "$screen" "$AUDIT_DOC"; then
        has_audit_note=true
    fi
    if ! $has_image && ! $has_audit_note; then
        missing+=("$screen")
    fi
done

if [ ${#missing[@]} -gt 0 ]; then
    echo "check_visual_captures: missing capture (image or audit note) for: ${missing[*]}" >&2
    exit 1
fi

echo "check_visual_captures: all ${#screens[@]} screens covered (image and/or audit note)."
