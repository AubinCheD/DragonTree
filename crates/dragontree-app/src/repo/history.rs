//! Historique : tableau virtualisé des commits avec graphe, étiquettes de références et filtre.

use dragontree_git::{RefKind, RefLabel};
use dragontree_i18n::{tr, trn};
use egui::{Align2, FontId, Frame, Key, Rect, RichText, Sense, Ui};

use super::graph_paint::{self, GraphGeom};
use super::scope::LabelVisibility;
use super::{RepoTab, RepoView};
use crate::app::Shared;
use crate::icons;
use crate::theme::{Palette, bold_family};
use crate::widgets;

const ROW_H: f32 = 24.0;
const AUTHOR_W: f32 = 150.0;
const DATE_W: f32 = 118.0;
const SHA_W: f32 = 70.0;

impl RepoTab {
    pub(super) fn history_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let total_h = ui.available_height();
        if shared.settings.details_panel_visible {
            egui::Panel::bottom(("history-details", self.id))
                .resizable(true)
                .default_size((total_h * 0.46).max(180.0))
                .size_range(120.0..=(total_h - 120.0).max(140.0))
                .frame(Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.border)))
                .show(ui, |ui| self.details_ui(ui, shared, p));
        }
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| self.commit_table(ui, shared, p));
    }

    fn apply_filter(&mut self) {
        let f = self.history.filter.trim().to_string();
        if f == self.history.filter_applied {
            return;
        }
        self.history.filter_applied = f.clone();
        if f.is_empty() {
            self.history.filtered = None;
            return;
        }
        let needle = widgets::fold(&f);
        let filtered: Vec<usize> = self
            .history
            .commits
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                c.id.to_hex().starts_with(&needle)
                    || widgets::fold(&c.summary).contains(&needle)
                    || widgets::fold(&c.author).contains(&needle)
                    || widgets::fold(&c.author_email).contains(&needle)
            })
            .map(|(i, _)| i)
            .collect();
        self.history.filtered = Some(filtered);
    }

    fn commit_table(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        // Barre de filtre.
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.set_height(30.0);
            let total = self.history.commits.len();
            let shown = self.history.filtered.as_ref().map_or(total, Vec::len);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let text = if self.history.filtered.is_some() {
                    trn!(total, "{shown} / {n} commit", "{shown} / {n} commits", shown)
                } else if self.history.loading {
                    trn!(total, "{n} commit…", "{n} commits…")
                } else {
                    trn!(total, "{n} commit", "{n} commits")
                };
                ui.label(RichText::new(text).color(p.weak).size(12.0));
                if self.history.loading {
                    ui.add(egui::Spinner::new().size(12.0));
                }
                ui.add_space(6.0);
                let label =
                    if self.history.current_branch_only { tr!("Branche courante") } else { tr!("Toutes les branches") };
                if ui
                    .selectable_label(
                        self.history.current_branch_only,
                        RichText::new(format!("{} {label}", icons::GIT_BRANCH)).size(12.5),
                    )
                    .on_hover_text(tr!(
                        "Basculer entre l'historique de toutes les branches et celui de la branche courante"
                    ))
                    .clicked()
                {
                    self.history.current_branch_only = !self.history.current_branch_only;
                    self.load_log(shared);
                }
                ui.add_space(6.0);
                ui.allocate_ui(egui::vec2(ui.available_width().min(420.0), 26.0), |ui| {
                    widgets::search_field(
                        ui,
                        &mut self.history.filter,
                        tr!("Rechercher : message, auteur, SHA"),
                        "history-filter",
                    );
                });
            });
        });
        self.apply_filter();

        if let Some(err) = &self.history.error {
            ui.label(RichText::new(format!("{} {}", icons::WARNING, err)).color(p.removed_fg));
            return;
        }
        let commits = self.history.commits.clone();
        let graph = self.history.graph.clone();
        let filtered = self.history.filtered.clone();
        let rows = filtered.as_ref().map_or(commits.len(), Vec::len);
        if rows == 0 {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                let msg = if self.history.loading {
                    tr!("Chargement de l'historique…")
                } else if filtered.is_some() {
                    tr!("Aucun commit ne correspond à la recherche.")
                } else {
                    tr!("Aucun commit pour l'instant.")
                };
                ui.label(RichText::new(msg).color(p.weak));
            });
            return;
        }
        let show_graph = filtered.is_none();
        let graph_w = if show_graph { graph_paint::graph_width(graph.max_lane) } else { 0.0 };

        // En-têtes de colonnes.
        let header_rect = ui.allocate_exact_size(egui::vec2(ui.available_width(), 20.0), Sense::hover()).0;
        ui.painter().rect_filled(header_rect, 0.0, p.panel);
        ui.painter().hline(header_rect.x_range(), header_rect.bottom(), egui::Stroke::new(1.0, p.border));
        let cols = Columns::new(header_rect, graph_w);
        let hdr = |ui: &Ui, x: f32, text: &str| {
            ui.painter().text(
                egui::pos2(x + 6.0, header_rect.center().y),
                Align2::LEFT_CENTER,
                text,
                FontId::proportional(11.5),
                p.weak,
            );
        };
        if show_graph && graph_w >= 60.0 {
            hdr(ui, cols.graph.left(), tr!("Graphe"));
        }
        hdr(ui, cols.desc.left(), "Description");
        hdr(ui, cols.author.left(), tr!("Auteur"));
        hdr(ui, cols.date.left(), "Date");
        hdr(ui, cols.sha.left(), "Commit");

        // Navigation clavier (quand aucun champ n'a le focus).
        let selected_row = self.history.selected.and_then(|oid| {
            let i = *self.history.index.get(&oid)?;
            match &filtered {
                Some(f) => f.iter().position(|&x| x == i),
                None => Some(i),
            }
        });
        if self.view == RepoView::History && ui.ctx().memory(|m| m.focused().is_none()) && self.dialog.is_none() {
            let (down, up, pgdn, pgup, home, end) = ui.ctx().input(|i| {
                (
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::PageDown),
                    i.key_pressed(Key::PageUp),
                    i.key_pressed(Key::Home),
                    i.key_pressed(Key::End),
                )
            });
            let page = (self.history.viewport_height / ROW_H).max(1.0) as usize;
            let target = match selected_row {
                Some(r) if down => Some((r + 1).min(rows - 1)),
                Some(r) if up => Some(r.saturating_sub(1)),
                Some(r) if pgdn => Some((r + page).min(rows - 1)),
                Some(r) if pgup => Some(r.saturating_sub(page)),
                None if down || up => Some(0),
                _ if home => Some(0),
                _ if end => Some(rows - 1),
                _ => None,
            };
            if let Some(t) = target.filter(|t| Some(*t) != selected_row) {
                let idx = filtered.as_ref().map_or(t, |f| f[t]);
                let oid = commits[idx].id;
                self.history.selected = Some(oid);
                self.history.external = None;
                self.load_details(shared, oid);
                self.ensure_row_visible(t);
            }
        }

        let head_oid = self.refs.as_ref().and_then(|r| r.head_oid);
        let head_row = head_oid.and_then(|o| self.history.index.get(&o).copied());
        let labels = self.refs.as_ref().map(|r| r.labels.clone()).unwrap_or_default();
        let visibility = LabelVisibility::of(shared, &self.path);
        let now = widgets::now_ts();

        let mut area = egui::ScrollArea::vertical().id_salt(("history-rows", self.id)).auto_shrink([false, false]);
        if let Some(offset) = self.history.scroll_offset_request() {
            area = area.vertical_scroll_offset(offset);
        }
        let mut clicked: Option<usize> = None;
        ui.spacing_mut().item_spacing.y = 0.0;
        let out = area.show_rows(ui, ROW_H, rows, |ui, range| {
            let top = ui.max_rect().top();
            let full = ui.max_rect();
            let cols = Columns::new(full, graph_w);
            for (k, r) in range.clone().enumerate() {
                let idx = filtered.as_ref().map_or(r, |f| f[r]);
                let c = &commits[idx];
                let rect = Rect::from_min_size(
                    egui::pos2(full.left(), top + k as f32 * ROW_H),
                    egui::vec2(full.width(), ROW_H),
                );
                ui.advance_cursor_after_rect(rect);
                // Identifiant lié au commit (pas à la ligne) : voir `widgets::row_response`.
                let resp = ui.interact(rect, ui.scope_id().with(("commit-row", c.id)), Sense::click());
                widgets::a11y(&resp, &c.summary, true);
                let selected = self.history.selected == Some(c.id);
                if selected {
                    ui.painter().rect_filled(rect, 0.0, p.selection);
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 0.0, p.hover);
                } else if r % 2 == 1 {
                    ui.painter().rect_filled(rect, 0.0, p.row_alt);
                }

                // Description : étiquettes puis sujet.
                let painter = ui.painter_at(cols.desc.intersect(rect));
                let mut x = cols.desc.left() + 6.0;
                let ls: Vec<&RefLabel> =
                    labels.get(&c.id).map(|ls| ls.iter().filter(|l| visibility.shows(l)).collect()).unwrap_or_default();
                for (n, l) in ls.iter().enumerate() {
                    if n >= 4 {
                        let more = format!("+{}", ls.len() - 4);
                        painter.text(
                            egui::pos2(x, rect.center().y),
                            Align2::LEFT_CENTER,
                            more,
                            FontId::proportional(11.0),
                            p.weak,
                        );
                        x += 24.0;
                        break;
                    }
                    x += paint_label(ui, &painter, egui::pos2(x, rect.top() + 3.5), l, p) + 4.0;
                }
                let is_head = Some(idx) == head_row;
                let mut job = egui::text::LayoutJob::single_section(
                    c.summary.to_string(),
                    egui::TextFormat {
                        font_id: FontId::new(
                            13.0,
                            if is_head { bold_family() } else { egui::FontFamily::Proportional },
                        ),
                        color: if selected { p.selection_text } else { p.text },
                        ..Default::default()
                    },
                );
                job.wrap = egui::text::TextWrapping {
                    max_width: (cols.desc.right() - x - 6.0).max(10.0),
                    max_rows: 1,
                    break_anywhere: true,
                    overflow_character: Some('…'),
                };
                let galley = painter.layout_job(job);
                painter.galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, p.text);

                let weak = if selected { p.selection_text } else { p.weak };
                let cell = |ui: &Ui, col: Rect, text: &str, font: FontId, color| {
                    let painter = ui.painter_at(col.intersect(rect));
                    let mut job = egui::text::LayoutJob::single_section(
                        text.to_string(),
                        egui::TextFormat { font_id: font, color, ..Default::default() },
                    );
                    job.wrap = egui::text::TextWrapping {
                        max_width: col.width() - 12.0,
                        max_rows: 1,
                        break_anywhere: true,
                        overflow_character: Some('…'),
                    };
                    let galley = painter.layout_job(job);
                    painter.galley(
                        egui::pos2(col.left() + 6.0, rect.center().y - galley.size().y / 2.0),
                        galley,
                        color,
                    );
                };
                cell(ui, cols.author, &c.author, FontId::proportional(12.5), weak);
                cell(ui, cols.date, &widgets::format_time(c.author_time, now), FontId::proportional(12.0), weak);
                cell(ui, cols.sha, &c.id.short(), FontId::monospace(12.0), weak);

                if resp.clicked() || resp.secondary_clicked() {
                    clicked = Some(r);
                }
                let (oid, summary) = (c.id, c.summary.to_string());
                resp.context_menu(|ui| self.commit_menu(ui, shared, oid, &summary));
            }
            if show_graph {
                let last = range.end.saturating_sub(1);
                let geom = GraphGeom { left: cols.graph.left(), first_top: top, first_row: range.start, row_h: ROW_H };
                let painter = ui.painter_at(Rect::from_x_y_ranges(cols.graph.x_range(), full.y_range()));
                graph_paint::paint(&painter, &graph, &geom, last, p, head_row);
            }
        });
        self.history.viewport_height = out.inner_rect.height();
        if let Some(r) = clicked {
            let idx = filtered.as_ref().map_or(r, |f| f[r]);
            let oid = commits[idx].id;
            if self.history.selected != Some(oid) {
                self.history.selected = Some(oid);
                self.history.external = None;
                self.load_details(shared, oid);
            }
        }
    }

    fn ensure_row_visible(&mut self, row: usize) {
        self.history.scroll_to = Some(row);
    }
}

impl super::HistoryState {
    /// Décalage de défilement demandé (centre la ligne voulue), consommé une seule fois.
    fn scroll_offset_request(&mut self) -> Option<f32> {
        let row = self.scroll_to.take()?;
        Some((row as f32 * ROW_H - self.viewport_height / 2.0 + ROW_H).max(0.0))
    }
}

struct Columns {
    graph: Rect,
    desc: Rect,
    author: Rect,
    date: Rect,
    sha: Rect,
}

impl Columns {
    fn new(r: Rect, graph_w: f32) -> Columns {
        let x0 = r.left();
        let sha = Rect::from_x_y_ranges((r.right() - SHA_W)..=r.right(), r.y_range());
        let date = Rect::from_x_y_ranges((sha.left() - DATE_W)..=sha.left(), r.y_range());
        let author = Rect::from_x_y_ranges((date.left() - AUTHOR_W)..=date.left(), r.y_range());
        let graph = Rect::from_x_y_ranges(x0..=(x0 + graph_w), r.y_range());
        let desc = Rect::from_x_y_ranges(graph.right()..=author.left().max(graph.right() + 40.0), r.y_range());
        Columns { graph, desc, author, date, sha }
    }
}

/// Étiquette de référence (branche, remote, tag, HEAD) ; renvoie sa largeur.
fn paint_label(ui: &Ui, painter: &egui::Painter, pos: egui::Pos2, l: &RefLabel, p: &Palette) -> f32 {
    let ((bg, fg), icon) = match l.kind {
        RefKind::Local => (p.badge_local, if l.is_head { icons::CHECK } else { icons::GIT_BRANCH }),
        RefKind::Remote => (p.badge_remote, icons::CLOUD),
        RefKind::Tag => (p.badge_tag, icons::TAG),
        RefKind::Head => (p.badge_head, icons::WARNING),
    };
    let text = format!("{icon} {}", widgets::ellipsize_middle(&l.name, 34));
    let family = if l.is_head { bold_family() } else { egui::FontFamily::Proportional };
    let galley = ui.painter().layout_no_wrap(text, FontId::new(11.5, family), fg);
    let w = galley.size().x + 10.0;
    let rect = Rect::from_min_size(pos, egui::vec2(w, 17.0));
    painter.rect_filled(rect, 4.0, bg);
    if l.is_head {
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, fg), egui::StrokeKind::Inside);
    }
    painter.galley(egui::pos2(rect.left() + 5.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    w
}
