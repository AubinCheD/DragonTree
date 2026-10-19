//! Dessin du graphe des commits (lignes verticales, courbes de fusion, nœuds).

use dragontree_git::graph::{GraphLayout, UNKNOWN_ROW};
use egui::epaint::CubicBezierShape;
use egui::{Color32, Painter, Pos2, Stroke};

use crate::theme::Palette;

/// Largeur d'une voie.
pub const LANE_W: f32 = 13.0;

/// Géométrie des lignes visibles.
pub struct GraphGeom {
    pub left: f32,
    /// Ordonnée du haut de la ligne `first_row`.
    pub first_top: f32,
    pub first_row: usize,
    pub row_h: f32,
}

impl GraphGeom {
    pub fn x(&self, lane: u16) -> f32 {
        self.left + LANE_W / 2.0 + f32::from(lane) * LANE_W
    }

    fn top(&self, row: u32) -> f32 {
        self.first_top + (row as f32 - self.first_row as f32) * self.row_h
    }

    fn mid(&self, row: u32) -> f32 {
        self.top(row) + self.row_h / 2.0
    }

    fn bottom(&self, row: u32) -> f32 {
        self.top(row) + self.row_h
    }
}

/// Graphe largeur en pixels pour `max_lane`, borné pour les dépôts à très nombreuses branches.
pub fn graph_width(max_lane: u16) -> f32 {
    ((f32::from(max_lane) + 1.0) * LANE_W + 6.0).clamp(22.0, 220.0)
}

fn link(painter: &Painter, from: Pos2, to: Pos2, stroke: Stroke) {
    if (from.x - to.x).abs() < 0.5 {
        painter.line_segment([from, to], stroke);
    } else {
        let dy = to.y - from.y;
        let c1 = Pos2::new(from.x, from.y + dy * 0.6);
        let c2 = Pos2::new(to.x, to.y - dy * 0.6);
        painter.add(CubicBezierShape::from_points_stroke([from, c1, c2, to], false, Color32::TRANSPARENT, stroke));
    }
}

/// Dessine les lignes `first_row..=last_row`. `head_row` reçoit un anneau.
pub fn paint(
    painter: &Painter,
    layout: &GraphLayout,
    g: &GraphGeom,
    last_row: usize,
    p: &Palette,
    head_row: Option<usize>,
) {
    if layout.rows.is_empty() {
        return;
    }
    let last_row = last_row.min(layout.rows.len() - 1);
    let (first, last) = (g.first_row as u32, last_row as u32);
    let view_bottom = g.bottom(last) + g.row_h;

    for e in layout.edges_in(first, last) {
        let stroke = Stroke::new(1.7, p.lane(e.color));
        let (xc, xl) = (g.x(e.child_lane), g.x(e.lane));
        if e.child_row >= first {
            link(painter, Pos2::new(xc, g.mid(e.child_row)), Pos2::new(xl, g.bottom(e.child_row)), stroke);
        }
        let y_start = g.bottom(e.child_row).max(g.top(first) - g.row_h);
        if e.parent_row == UNKNOWN_ROW {
            painter.vline(xl, y_start..=view_bottom, stroke);
            continue;
        }
        if e.parent_row > e.child_row + 1 {
            painter.vline(xl, y_start..=g.top(e.parent_row).min(view_bottom), stroke);
        }
        if e.parent_row <= last {
            link(
                painter,
                Pos2::new(xl, g.top(e.parent_row)),
                Pos2::new(g.x(e.parent_lane), g.mid(e.parent_row)),
                stroke,
            );
        }
    }

    for r in g.first_row..=last_row {
        let row = layout.rows[r];
        let center = Pos2::new(g.x(row.lane), g.mid(r as u32));
        let color = p.lane(row.color);
        if Some(r) == head_row {
            painter.circle_filled(center, 6.0, p.bg);
            painter.circle_stroke(center, 5.5, Stroke::new(2.0, color));
            painter.circle_filled(center, 2.5, color);
        } else if row.is_merge {
            painter.circle_filled(center, 3.6, color);
            painter.circle_stroke(center, 3.6, Stroke::new(1.0, p.bg));
        } else {
            painter.circle_filled(center, 4.2, color);
            painter.circle_stroke(center, 4.2, Stroke::new(1.2, p.bg));
        }
    }
}
