//! Multi-selection and drag-and-drop rules shared by the library tree (Panel 1) and the sheet
//! list (Panel 2): the standard list behaviour for plain / Ctrl / Shift / Ctrl+Shift clicks,
//! and where a dragged row lands for a pointer position over a target row.

use crate::state::NodePath;

/// The selection after clicking `clicked`, with `order` the rows in display order:
///
/// - plain click: just `clicked`;
/// - Ctrl: toggles `clicked` in or out of the selection;
/// - Shift: the range from the anchor to `clicked`, replacing the selection;
/// - Ctrl+Shift: that range added to the selection.
///
/// The result is in display order. The anchor moves to `clicked` on plain and Ctrl clicks and
/// stays put on Shift clicks (see [`next_anchor`]); with no anchor a Shift click acts plain.
pub fn click(
    current: &[NodePath],
    anchor: Option<&NodePath>,
    order: &[NodePath],
    clicked: &NodePath,
    modifiers: egui::Modifiers,
) -> Vec<NodePath> {
    let range = || -> Option<Vec<NodePath>> {
        let a = order.iter().position(|p| Some(p) == anchor)?;
        let b = order.iter().position(|p| p == clicked)?;
        Some(order[a.min(b)..=a.max(b)].to_vec())
    };
    let chosen: Vec<NodePath> = match (modifiers.command, modifiers.shift) {
        (_, true) if range().is_some() => {
            let range = range().unwrap_or_default();
            if modifiers.command {
                current.iter().cloned().chain(range).collect()
            } else {
                range
            }
        }
        (true, _) => {
            if current.contains(clicked) {
                current.iter().filter(|p| *p != clicked).cloned().collect()
            } else {
                current.iter().cloned().chain([clicked.clone()]).collect()
            }
        }
        _ => vec![clicked.clone()],
    };
    order
        .iter()
        .filter(|p| chosen.contains(p))
        .cloned()
        .collect()
}

/// The range anchor after [`click`]: unchanged by Shift clicks that had an anchor to extend.
pub fn next_anchor(
    anchor: Option<&NodePath>,
    order: &[NodePath],
    clicked: &NodePath,
    modifiers: egui::Modifiers,
) -> NodePath {
    match anchor {
        Some(a) if modifiers.shift && order.contains(a) => a.clone(),
        _ => clicked.clone(),
    }
}

/// What a drag carries: the dragged rows' node paths, in tree order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedNodes(pub Vec<NodePath>);

/// The rows a drag starting on `row` moves: the whole selection when `row` is part of it,
/// otherwise just `row`.
pub fn dragged_rows(selection: &[NodePath], row: &NodePath) -> Vec<NodePath> {
    if selection.contains(row) {
        selection.to_vec()
    } else {
        vec![row.clone()]
    }
}

/// Where over a target row the pointer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropZone {
    Before,
    Into,
    After,
}

/// `fraction` is the pointer's height within the row (0 = top). Containers take drops into
/// their middle half; other rows split into before/after halves.
pub fn drop_zone(fraction: f32, container: bool) -> DropZone {
    match (container, fraction) {
        (true, f) if f < 0.25 => DropZone::Before,
        (true, f) if f > 0.75 => DropZone::After,
        (true, _) => DropZone::Into,
        (false, f) if f < 0.5 => DropZone::Before,
        (false, _) => DropZone::After,
    }
}

/// The `(parent, index)` a drop on `target` in `zone` moves to, the index counted before the
/// move (as [`crate::state::AppState::move_nodes`] takes it). `child_count` is the target's
/// own child count, used by [`DropZone::Into`] to append.
pub fn drop_destination(target: &[usize], zone: DropZone, child_count: usize) -> (NodePath, usize) {
    match (zone, target.split_last()) {
        (DropZone::Into, _) | (_, None) => (target.to_vec(), child_count),
        (DropZone::Before, Some((&i, parent))) => (parent.to_vec(), i),
        (DropZone::After, Some((&i, parent))) => (parent.to_vec(), i + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Modifiers;

    fn order() -> Vec<NodePath> {
        (0..5).map(|i| vec![i]).collect()
    }

    const CTRL_SHIFT: Modifiers = Modifiers {
        alt: false,
        ctrl: true,
        shift: true,
        mac_cmd: false,
        command: true,
    };

    #[test]
    fn plain_click_selects_only_the_row() {
        let got = click(
            &[vec![0], vec![1]],
            Some(&vec![0]),
            &order(),
            &vec![3],
            Modifiers::NONE,
        );
        assert_eq!(got, vec![vec![3]]);
        assert_eq!(
            next_anchor(Some(&vec![0]), &order(), &vec![3], Modifiers::NONE),
            vec![3]
        );
    }

    #[test]
    fn ctrl_click_toggles_and_keeps_display_order() {
        let got = click(
            &[vec![3]],
            Some(&vec![3]),
            &order(),
            &vec![1],
            Modifiers::COMMAND,
        );
        assert_eq!(got, vec![vec![1], vec![3]]);
        let got = click(&got, Some(&vec![1]), &order(), &vec![3], Modifiers::COMMAND);
        assert_eq!(got, vec![vec![1]]);
        assert_eq!(
            next_anchor(Some(&vec![1]), &order(), &vec![3], Modifiers::COMMAND),
            vec![3]
        );
    }

    #[test]
    fn shift_click_selects_the_range_from_the_anchor() {
        let got = click(
            &[vec![4]],
            Some(&vec![1]),
            &order(),
            &vec![3],
            Modifiers::SHIFT,
        );
        assert_eq!(got, vec![vec![1], vec![2], vec![3]]);
        // Backwards ranges work too, and the anchor stays.
        let got = click(&got, Some(&vec![3]), &order(), &vec![0], Modifiers::SHIFT);
        assert_eq!(got, (0..=3).map(|i| vec![i]).collect::<Vec<_>>());
        assert_eq!(
            next_anchor(Some(&vec![3]), &order(), &vec![0], Modifiers::SHIFT),
            vec![3]
        );
        // No anchor: a plain click.
        assert_eq!(
            click(&[], None, &order(), &vec![2], Modifiers::SHIFT),
            vec![vec![2]]
        );
    }

    #[test]
    fn ctrl_shift_click_adds_the_range() {
        let got = click(&[vec![0]], Some(&vec![2]), &order(), &vec![4], CTRL_SHIFT);
        assert_eq!(got, vec![vec![0], vec![2], vec![3], vec![4]]);
    }

    #[test]
    fn dragging_a_selected_row_drags_the_selection() {
        let selection = vec![vec![1], vec![3]];
        assert_eq!(dragged_rows(&selection, &vec![3]), selection);
        assert_eq!(dragged_rows(&selection, &vec![2]), vec![vec![2]]);
    }

    #[test]
    fn drop_zones_and_destinations() {
        assert_eq!(drop_zone(0.1, true), DropZone::Before);
        assert_eq!(drop_zone(0.5, true), DropZone::Into);
        assert_eq!(drop_zone(0.9, true), DropZone::After);
        assert_eq!(drop_zone(0.4, false), DropZone::Before);
        assert_eq!(drop_zone(0.6, false), DropZone::After);
        assert_eq!(drop_destination(&[2, 1], DropZone::Before, 0), (vec![2], 1));
        assert_eq!(drop_destination(&[2, 1], DropZone::After, 0), (vec![2], 2));
        assert_eq!(drop_destination(&[2], DropZone::Into, 3), (vec![2], 3));
    }
}
