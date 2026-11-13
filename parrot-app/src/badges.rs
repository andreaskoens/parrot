//! Row badges (T-12-04, ui-description §3.2/§3.3/§4.2): goal-progress rings, attachment and
//! Draft Sheet badges, and the sheet list's text-checker error dot. [`badges_for`] decides *which* badges a node
//! gets (headlessly testable); [`paint_badges`] is the only code that draws them, so a row
//! shows exactly what `badges_for` returned.

use crate::container_view::NodePath;
use crate::goal_ui::count_units;
use crate::tokens;
use parrot_core::{Library, Node};

/// §3.3 color bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalBand {
    Incomplete,
    Almost,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Badge {
    /// `fraction` is goal progress clamped to 0..=1 (the ring is full once reached).
    GoalRing {
        fraction: f32,
        band: GoalBand,
    },
    Attachment,
    /// A Draft Sheet.
    Draft,
    /// §4.2 red dot: the sheet has spelling/grammar findings.
    Error,
}

/// Goal ring + attachment badges for `node`, in left-to-right paint order. Only nodes that
/// actually carry a Goal / an Attachment get the corresponding badge.
pub fn badges_for(node: &Node) -> Vec<Badge> {
    let mut badges = Vec::new();
    let goal = match node {
        Node::Sheet(s) => s.goal.as_ref(),
        Node::Group(g) => g.goal.as_ref(),
        Node::Project(p) => p.goal.as_ref(),
        Node::Filter(_) => None,
    };
    if let Some(goal) = goal {
        let report = parrot_core::goal::progress(count_units(node, goal.counter), goal.target);
        let band = if report.reached {
            GoalBand::Complete
        } else if report.almost_reached {
            GoalBand::Almost
        } else {
            GoalBand::Incomplete
        };
        badges.push(Badge::GoalRing {
            fraction: (report.percent as f32 / 100.0).clamp(0.0, 1.0),
            band,
        });
    }
    if matches!(node, Node::Sheet(s) if !s.attachments.is_empty()) {
        badges.push(Badge::Attachment);
    }
    if matches!(node, Node::Sheet(s) if s.draft) {
        badges.push(Badge::Draft);
    }
    badges
}

/// Every node in `library` (depth-first, tree order) that has at least one badge.
pub fn plan_tree(library: &Library) -> Vec<(NodePath, Vec<Badge>)> {
    fn walk(nodes: &[Node], prefix: &mut NodePath, out: &mut Vec<(NodePath, Vec<Badge>)>) {
        for (i, node) in nodes.iter().enumerate() {
            prefix.push(i);
            let badges = badges_for(node);
            if !badges.is_empty() {
                out.push((prefix.clone(), badges));
            }
            match node {
                Node::Group(g) => walk(&g.children, prefix, out),
                Node::Project(p) => walk(&p.children, prefix, out),
                _ => {}
            }
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(&library.children, &mut Vec::new(), &mut out);
    out
}

/// Width one badge occupies, including its gap.
pub const BADGE_SLOT: f32 = tokens::GOAL_RING_DIAMETER + tokens::GRID;

/// Paints `badges` right-aligned against `right_edge`, vertically centered on `center_y`.
/// Returns the total width used, so callers can shorten the row label to fit.
pub fn paint_badges(
    painter: &egui::Painter,
    right_edge: f32,
    center_y: f32,
    badges: &[Badge],
) -> f32 {
    let mut x = right_edge;
    for badge in badges.iter().rev() {
        let center = egui::pos2(x - tokens::GOAL_RING_DIAMETER / 2.0, center_y);
        match *badge {
            Badge::GoalRing { fraction, band } => paint_goal_ring(painter, center, fraction, band),
            Badge::Attachment | Badge::Draft => {
                let glyph = if *badge == Badge::Draft {
                    crate::icons::DRAFT
                } else {
                    crate::icons::ATTACHMENT
                };
                painter.text(
                    center,
                    egui::Align2::CENTER_CENTER,
                    glyph,
                    tokens::UI_CAPTION.font_id(),
                    tokens::TEXT_TERTIARY,
                );
            }
            Badge::Error => {
                painter.circle_filled(center, 3.0, tokens::ERROR);
            }
        }
        x -= BADGE_SLOT;
    }
    right_edge - x
}

/// §12.8: background ring plus a clockwise progress arc starting at 12 o'clock.
pub fn paint_goal_ring(painter: &egui::Painter, center: egui::Pos2, fraction: f32, band: GoalBand) {
    let radius = tokens::GOAL_RING_DIAMETER / 2.0 - 1.0;
    painter.circle_stroke(
        center,
        radius,
        egui::Stroke::new(1.5_f32, tokens::GOAL_INCOMPLETE),
    );
    let color = match band {
        GoalBand::Incomplete => tokens::TEXT_TERTIARY,
        GoalBand::Almost => tokens::GOAL_ALMOST,
        GoalBand::Complete => tokens::GOAL_COMPLETE,
    };
    if fraction <= 0.0 {
        return;
    }
    let steps = ((fraction * 32.0).ceil() as usize).max(2);
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|i| {
            let angle = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * fraction * i as f32 / steps as f32;
            center + radius * egui::vec2(angle.cos(), angle.sin())
        })
        .collect();
    painter.add(egui::Shape::line(points, egui::Stroke::new(2.0_f32, color)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use parrot_core::{Attachment, Goal, GoalCounter, GoalKind, Group, Sheet, SheetContent};

    fn sheet(title: &str) -> Sheet {
        Sheet {
            title: title.to_string(),
            content: SheetContent::new(Vec::new()),
            goal: None,
            keywords: Vec::new(),
            attachments: Vec::new(),
            glued: false,
            draft: false,
        }
    }

    fn fixture() -> Library {
        let goaled = Sheet {
            goal: Some(Goal {
                kind: GoalKind::Simple,
                counter: GoalCounter::WordCount,
                target: 100,
                deadline: None,
                write_on_deadline: false,
            }),
            ..sheet("Goaled")
        };
        let attached = Sheet {
            attachments: vec![Attachment::Note {
                text: "remember".to_string(),
            }],
            ..sheet("Attached")
        };
        Library {
            children: vec![
                Node::Sheet(sheet("Plain")),
                Node::Group(Group {
                    title: "Drafts".to_string(),
                    goal: None,
                    children: vec![Node::Sheet(goaled), Node::Sheet(sheet("Other"))],
                }),
                Node::Sheet(attached),
            ],
        }
    }

    /// Paints each planned row through a real headless `egui::Context` and counts the shapes
    /// produced — proving the painter issues draw calls only for the planned nodes.
    fn painted_shape_count(badges: &[Badge]) -> usize {
        let run = |badges: &[Badge]| {
            let ctx = egui::Context::default();
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    paint_badges(ui.painter(), 200.0, 10.0, badges);
                });
            });
            output.shapes.len()
        };
        run(badges) - run(&[])
    }

    #[test]
    fn goal_ring_only_on_goaled_nodes() {
        let plan = plan_tree(&fixture());
        let ringed: Vec<&NodePath> = plan
            .iter()
            .filter(|(_, b)| b.iter().any(|b| matches!(b, Badge::GoalRing { .. })))
            .map(|(p, _)| p)
            .collect();
        assert_eq!(ringed, vec![&vec![1, 0]]);
        let (_, badges) = &plan[0];
        assert_eq!(
            badges[0],
            Badge::GoalRing {
                fraction: 0.02,
                band: GoalBand::Incomplete
            }
        );
        assert_eq!(painted_shape_count(&[]), 0);
        assert!(painted_shape_count(badges) >= 1);
    }

    #[test]
    fn attachment_badge_only_on_attached_nodes() {
        let plan = plan_tree(&fixture());
        let attached: Vec<&NodePath> = plan
            .iter()
            .filter(|(_, b)| b.contains(&Badge::Attachment))
            .map(|(p, _)| p)
            .collect();
        assert_eq!(attached, vec![&vec![2]]);
        assert_eq!(plan.len(), 2, "no other node may carry a badge: {plan:?}");
    }
}
