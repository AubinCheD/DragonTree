//! Historique : tableau virtualisé des commits avec graphe, étiquettes de références et filtre.

use dragontree_git::{RefKind, RefLabel};
use dragontree_i18n::{tr, trn};
use egui::{Align2, FontId, Frame, Key, Rect, RichText, Sense, Ui};

use super::graph_paint::{self, GraphGeom};
use super::history_search::SearchMode;
use super::scope::{LabelVisibility, display_name};
use super::{RepoTab, RepoView};
use crate::app::Shared;
use crate::icons;
use crate::settings::HistoryLayout;
use crate::theme::{Palette, bold_family};
use crate::widgets;

const ROW_H: f32 = 24.0;
const AUTHOR_W: f32 = 150.0;
const DATE_W: f32 = 118.0;
const SHA_W: f32 = 70.0;
/// Largeur minimale laissée à la description : la colonne du graphe ne peut pas la réduire davantage.
const MIN_DESC_W: f32 = 120.0;

/// Largeur maximale de la colonne du graphe dans une zone de `total` px : auteur, date et SHA gardent leur place et la
/// poignée reste dans la zone. La préférence enregistrée n'est pas touchée, seule la largeur affichée est bornée.
fn max_graph_width(total: f32) -> f32 {
    (total - AUTHOR_W - DATE_W - SHA_W - MIN_DESC_W).max(graph_paint::MIN_CHOSEN_WIDTH)
}

impl RepoTab {
    pub(super) fn history_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        if shared.settings.details_panel_visible {
            let frame = Frame::new().fill(p.bg).stroke(egui::Stroke::new(1.0, p.border));
            // Deux identifiants de panneau distincts : chaque disposition garde sa taille redimensionnée.
            match shared.settings.history_layout {
                HistoryLayout::Vertical => {
                    let total_h = ui.available_height();
                    egui::Panel::bottom(("history-details", self.id))
                        .resizable(true)
                        .default_size((total_h * 0.46).max(180.0))
                        .size_range(120.0..=(total_h - 120.0).max(140.0))
                        .frame(frame)
                        .show(ui, |ui| self.details_ui(ui, shared, p));
                }
                HistoryLayout::Horizontal => {
                    let total_w = ui.available_width();
                    egui::Panel::right(("history-details-right", self.id))
                        .resizable(true)
                        .default_size((total_w * 0.42).max(320.0))
                        .size_range(280.0..=(total_w - 360.0).max(300.0))
                        .frame(frame)
                        .show(ui, |ui| self.details_ui(ui, shared, p));
                }
            }
        }
        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| self.commit_table(ui, shared, p));
    }

    /// Sélecteur de ce que cherche le champ de recherche (message, contenu des modifications, chemin).
    fn search_mode_selector(&mut self, ui: &mut Ui) {
        let mode = &mut self.history.search.mode;
        egui::ComboBox::from_id_salt(("history-search-mode", self.id))
            .selected_text(RichText::new(mode.label()).size(12.5))
            .show_ui(ui, |ui| {
                for m in SearchMode::ALL {
                    ui.selectable_value(mode, m, m.label());
                }
            });
    }

    /// Bandeau « Filtré par : … » au-dessus de l'historique quand il est filtré sur des branches choisies :
    /// chaque élément se retire par son ✕, « Tout afficher » retire tout le filtre.
    fn filter_banner(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        if self.history.filter_refs.is_empty() {
            return;
        }
        let mut remove = None;
        let mut clear = false;
        Frame::new().fill(p.panel).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{} {}", icons::FUNNEL, tr!("Filtré par :"))).color(p.weak).size(12.5));
                for pattern in &self.history.filter_refs {
                    let name = display_name(pattern);
                    let chip = RichText::new(format!("{name}  {}", icons::X)).size(12.5);
                    let resp = ui.add(egui::Button::new(chip).small());
                    widgets::a11y(&resp, &tr!("Retirer du filtre : {name}", name), true);
                    if resp.on_hover_text(tr!("Retirer du filtre : {name}", name)).clicked() {
                        remove = Some(pattern.clone());
                    }
                }
                if ui.add(egui::Button::new(RichText::new(tr!("Tout afficher")).size(12.5)).small()).clicked() {
                    clear = true;
                }
            });
        });
        if clear {
            self.clear_filter_refs(shared);
        } else if let Some(pattern) = remove {
            self.toggle_filter_ref(shared, &pattern);
        }
    }

    fn commit_table(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        // Barre de filtre.
        ui.horizontal(|ui| {
            ui.add_space(6.0);
            ui.set_height(30.0);
            let total = self.history.commits.len();
            let searching = self.history.search.is_searching(self.history.search.mode, self.history.filter.trim());
            let shown = self.history.filtered.as_ref().map_or(total, |f| f.len());
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
                if self.history.loading || searching || self.history.filter_pending {
                    ui.add(egui::Spinner::new().size(12.0));
                }
                ui.add_space(6.0);
                let label = if self.history.current_branch_only {
                    tr!("Branche courante")
                } else if !self.history.filter_refs.is_empty() {
                    tr!("Branches filtrées")
                } else {
                    tr!("Toutes les branches")
                };
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
                    self.set_current_branch_only(shared, !self.history.current_branch_only);
                }
                ui.add_space(6.0);
                ui.allocate_ui(egui::vec2(ui.available_width().min(420.0), 26.0), |ui| {
                    widgets::search_field(
                        ui,
                        &mut self.history.filter,
                        self.history.search.mode.hint(),
                        "history-filter",
                    );
                });
                ui.add_space(4.0);
                self.search_mode_selector(ui);
            });
        });
        self.filter_banner(ui, shared, p);
        self.drive_search(ui.ctx(), shared);
        self.apply_filter(shared);

        if let Some(err) = &self.history.error {
            ui.label(RichText::new(format!("{} {}", icons::WARNING, err)).color(p.removed_fg));
            return;
        }
        let commits = self.history.commits.clone();
        let graph = self.history.graph.clone();
        let filtered = self.history.filtered.clone();
        let rows = filtered.as_ref().map_or(commits.len(), |f| f.len());
        if rows == 0 {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                let mode = self.history.search.mode;
                let query = self.history.filter.trim().to_string();
                let failed = self.history.search.error_for(mode, &query);
                let (msg, color) = if self.history.loading {
                    (tr!("Chargement de l'historique…").to_string(), p.weak)
                } else if let Some(e) = failed {
                    (format!("{} {e}", icons::WARNING), p.removed_fg)
                } else if self.history.search.is_searching(mode, &query) || self.history.filter_pending {
                    (tr!("Recherche en cours…").to_string(), p.weak)
                } else if filtered.is_some() {
                    (tr!("Aucun commit ne correspond à la recherche.").to_string(), p.weak)
                } else {
                    (tr!("Aucun commit pour l'instant.").to_string(), p.weak)
                };
                ui.label(RichText::new(msg).color(color));
            });
            return;
        }
        let show_graph = filtered.is_none();
        let max_graph_w = max_graph_width(ui.available_width());
        let graph_w = if show_graph {
            graph_paint::column_width(graph.max_lane, shared.settings.graph_column_width).min(max_graph_w)
        } else {
            0.0
        };

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
        if show_graph {
            // Poignée au bord droit de la colonne : glisser la redimensionne, double-clic revient à la largeur calculée.
            let grip = Rect::from_center_size(
                egui::pos2(cols.graph.right(), header_rect.center().y),
                egui::vec2(8.0, header_rect.height()),
            );
            let resp = ui
                .interact(grip, ui.scope_id().with(("graph-grip", self.id)), Sense::click_and_drag())
                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            widgets::a11y(&resp, tr!("Redimensionner la colonne du graphe"), true);
            if resp.dragged()
                && let Some(pos) = resp.interact_pointer_pos()
            {
                let w = (pos.x - header_rect.left())
                    .clamp(graph_paint::MIN_CHOSEN_WIDTH, max_graph_w.min(graph_paint::MAX_CHOSEN_WIDTH));
                shared.settings.graph_column_width = Some(w);
            }
            if resp.drag_stopped() {
                shared.settings_dirty = true;
            }
            if resp.double_clicked() {
                shared.settings.graph_column_width = None;
                shared.settings_dirty = true;
            }
            let color = if resp.hovered() || resp.dragged() { p.weak } else { p.border };
            let line = header_rect.shrink2(egui::vec2(0.0, 4.0)).y_range();
            ui.painter().vline(cols.graph.right(), line, egui::Stroke::new(1.0, color));
        }
        hdr(ui, cols.desc.left(), "Description");
        hdr(ui, cols.author.left(), tr!("Auteur"));
        hdr(ui, cols.date.left(), "Date");
        hdr(ui, cols.sha.left(), "Commit");

        // Fusion à replier (`true`) ou déplier, demandé par ← / →, son triangle ou son menu contextuel : appliqué après
        // le dessin, qui utilise la liste et le graphe de cette image.
        let mut fold: Option<(dragontree_git::Oid, bool)> = None;
        // Navigation clavier (quand aucun champ n'a le focus).
        let selected_row = self.history.selected.and_then(|oid| self.shown_position(*self.history.index.get(&oid)?));
        if self.view == RepoView::History && ui.ctx().memory(|m| m.focused().is_none()) && self.dialog.is_none() {
            let (down, up, pgdn, pgup, home, end, left, right) = ui.ctx().input(|i| {
                (
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::PageDown),
                    i.key_pressed(Key::PageUp),
                    i.key_pressed(Key::Home),
                    i.key_pressed(Key::End),
                    i.key_pressed(Key::ArrowLeft),
                    i.key_pressed(Key::ArrowRight),
                )
            });
            // ← replie, → déplie la fusion sélectionnée.
            if let Some(r) = selected_row
                && left != right
            {
                let c = &commits[filtered.as_ref().map_or(r, |f| f[r])];
                if c.parents.len() > 1 {
                    fold = Some((c.id, left));
                }
            }
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
                self.history.external = self.stash_selection(oid);
                self.load_details(shared, oid);
                self.ensure_row_visible(t);
            }
        }

        let head_oid = self.refs.as_ref().and_then(|r| r.head_oid);
        let head_row = head_oid.and_then(|o| self.history.index.get(&o).copied());
        // Partagées, pas copiées : l'instantané des références (`Arc`) suffit à les lire pendant le dessin.
        let refs = self.refs.clone();
        let labels = |oid: &dragontree_git::Oid| refs.as_ref().and_then(|r| r.labels.get(oid));
        let visibility = LabelVisibility::of(shared, &self.path);
        let lost = self.history.lost.clone();
        let bisect = self.bisect.clone();
        let folded = self.history.folded.clone();
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
                    labels(&c.id).map(|ls| ls.iter().filter(|l| visibility.shows(l)).collect()).unwrap_or_default();
                let is_lost = lost.contains(&c.id);
                let stash = self.stashes.iter().find(|s| s.oid == c.id);
                let collapsed = (c.parents.len() > 1).then(|| self.history.folding.is_collapsed(c.id));
                if let Some(collapsed) = collapsed {
                    let toggle = FoldToggle { oid: c.id, summary: &c.summary, collapsed, selected };
                    let (w, toggled) = paint_fold_toggle(ui, &painter, rect, x, &toggle, folded.get(&c.id), p);
                    if toggled {
                        fold = Some((c.id, !collapsed));
                    }
                    x += w;
                }
                if let Some(s) = stash {
                    let text = format!("{} stash@{{{}}}", icons::STACK, s.index);
                    let family = egui::FontFamily::Proportional;
                    x += paint_badge(ui, &painter, egui::pos2(x, rect.top() + 3.5), text, p.badge_tag, family, false)
                        + 4.0;
                }
                if is_lost {
                    x += paint_lost_badge(ui, &painter, egui::pos2(x, rect.top() + 3.5), p) + 4.0;
                }
                if let Some(text) = bisect.as_ref().and_then(|b| super::bisect::badge_text(b, c.id)) {
                    let bad = bisect.as_ref().is_some_and(|b| b.bad == Some(c.id));
                    x += super::bisect::paint_badge(ui, &painter, egui::pos2(x, rect.top() + 3.5), text, bad, p) + 4.0;
                }
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
                        color: if selected {
                            p.selection_text
                        } else if is_lost {
                            p.weak
                        } else {
                            p.text
                        },
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

                let weak =
                    if selected { p.selection_text } else { p.weak.gamma_multiply(if is_lost { 0.7 } else { 1.0 }) };
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
                let stash_menu = stash.map(|s| super::scope::stash_label(s.index, &s.message));
                resp.context_menu(|ui| match &stash_menu {
                    Some(label) => self.stash_menu(ui, shared, oid, label),
                    None => {
                        if let Some(collapsed) = collapsed {
                            let (icon, text) = if collapsed {
                                (icons::ARROWS_OUT_LINE_VERTICAL, tr!("Déplier la fusion"))
                            } else {
                                (icons::ARROWS_IN_LINE_VERTICAL, tr!("Replier la fusion"))
                            };
                            if ui.button(format!("{icon} {text}")).clicked() {
                                fold = Some((oid, !collapsed));
                                ui.close();
                            }
                            ui.separator();
                        }
                        self.commit_menu(ui, shared, oid, &summary)
                    }
                });
            }
            if show_graph {
                let last = range.end.saturating_sub(1);
                let geom = GraphGeom { left: cols.graph.left(), first_top: top, first_row: range.start, row_h: ROW_H };
                let painter = ui.painter_at(Rect::from_x_y_ranges(cols.graph.x_range(), full.y_range()));
                graph_paint::paint(&painter, &graph, &geom, last, p, head_row);
            }
        });
        self.history.viewport_height = out.inner_rect.height();
        if let Some((merge, collapse)) = fold {
            self.set_merge_collapsed(shared, merge, collapse);
        }
        if let Some(r) = clicked {
            let idx = filtered.as_ref().map_or(r, |f| f[r]);
            let oid = commits[idx].id;
            if self.history.selected != Some(oid) {
                self.history.selected = Some(oid);
                self.history.external = self.stash_selection(oid);
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

/// Ligne de fusion dont on dessine le triangle.
struct FoldToggle<'a> {
    oid: dragontree_git::Oid,
    summary: &'a str,
    collapsed: bool,
    selected: bool,
}

/// Triangle qui replie ou déplie une fusion, suivi, si elle est repliée, du nombre de commits qu'elle masque ;
/// renvoie la largeur occupée et si le triangle a été cliqué.
fn paint_fold_toggle(
    ui: &Ui,
    painter: &egui::Painter,
    row: Rect,
    x: f32,
    t: &FoldToggle,
    hidden: Option<&usize>,
    p: &Palette,
) -> (f32, bool) {
    let caret = Rect::from_min_size(egui::pos2(x - 4.0, row.top() + 2.0), egui::vec2(16.0, ROW_H - 4.0));
    // Dessiné par-dessus la ligne : un clic sur le triangle ne sélectionne pas le commit.
    let resp = ui.interact(caret, ui.scope_id().with(("fold-merge", t.oid)), Sense::click());
    let summary = t.summary;
    let label = if t.collapsed { tr!("Déplier : {summary}", summary) } else { tr!("Replier : {summary}", summary) };
    widgets::a11y(&resp, &label, true);
    let color = if resp.hovered() {
        p.text
    } else if t.selected {
        p.selection_text
    } else {
        p.weak
    };
    let icon = if t.collapsed { icons::CARET_RIGHT } else { icons::CARET_DOWN };
    painter.text(caret.center(), Align2::CENTER_CENTER, icon, FontId::proportional(11.0), color);
    let tip = if t.collapsed { tr!("Déplier la fusion (→)") } else { tr!("Replier la fusion (←)") };
    let clicked = resp.on_hover_text(tip).clicked();
    let mut w = caret.width();
    if let Some(&n) = hidden.filter(|&&n| t.collapsed && n > 0) {
        let text = trn!(n, "{n} commit replié", "{n} commits repliés");
        let pos = egui::pos2(x + w, row.top() + 3.5);
        let badge =
            paint_badge(ui, painter, pos, text.clone(), (p.hover, p.weak), egui::FontFamily::Proportional, false);
        // Lisible par l'accessibilité, comme un libellé.
        let id = ui.scope_id().with(("folded-count", t.oid));
        let info = ui.interact(Rect::from_min_size(pos, egui::vec2(badge, 17.0)), id, Sense::hover());
        info.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Label, true, text.clone()));
        w += badge + 4.0;
    }
    (w, clicked)
}

/// Étiquette « perdu » d'un commit que seul le reflog atteint ; renvoie sa largeur.
fn paint_lost_badge(ui: &Ui, painter: &egui::Painter, pos: egui::Pos2, p: &Palette) -> f32 {
    let text = format!("{} {}", icons::GHOST, tr!("perdu"));
    paint_badge(ui, painter, pos, text, (p.hover, p.weak), egui::FontFamily::Proportional, false)
}

/// Pastille de texte (fond, texte) ; `outline` la cerne ; renvoie sa largeur.
pub(super) fn paint_badge(
    ui: &Ui,
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: String,
    (bg, fg): (egui::Color32, egui::Color32),
    family: egui::FontFamily,
    outline: bool,
) -> f32 {
    let galley = ui.painter().layout_no_wrap(text, FontId::new(11.5, family), fg);
    let w = galley.size().x + 10.0;
    let rect = Rect::from_min_size(pos, egui::vec2(w, 17.0));
    painter.rect_filled(rect, 4.0, bg);
    if outline {
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, fg), egui::StrokeKind::Inside);
    }
    painter.galley(egui::pos2(rect.left() + 5.0, rect.center().y - galley.size().y / 2.0), galley, fg);
    w
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
    paint_badge(ui, painter, pos, text, (bg, fg), family, l.is_head)
}

#[cfg(test)]
mod width_tests {
    use super::*;

    #[test]
    fn the_graph_column_leaves_room_for_the_other_columns() {
        let total = 1000.0;
        let max = max_graph_width(total);
        assert_eq!(total - max - AUTHOR_W - DATE_W - SHA_W, MIN_DESC_W);
        assert_eq!(max_graph_width(300.0), graph_paint::MIN_CHOSEN_WIDTH, "zone trop étroite : largeur minimale");
    }
}
