//! Détails du commit sélectionné : informations, fichiers modifiés et diff.

use dragontree_git::{ChangeKind, FileChange};
use dragontree_i18n::{tr, trn};
use egui::{Frame, RichText, Sense, Ui};

use super::RepoTab;
use super::diff_view::{self, DiffMode};
use crate::app::Shared;
use crate::icons;
use crate::theme::Palette;
use crate::widgets;

pub fn change_badge(kind: ChangeKind, p: &Palette) -> (&'static str, egui::Color32, &'static str) {
    match kind {
        ChangeKind::Added => ("A", p.added_fg, tr!("ajouté")),
        ChangeKind::Modified => ("M", p.modified_fg, tr!("modifié")),
        ChangeKind::Deleted => ("D", p.removed_fg, tr!("supprimé")),
        ChangeKind::Renamed => ("R", p.renamed_fg, tr!("renommé")),
        ChangeKind::Copied => ("C", p.renamed_fg, tr!("copié")),
        ChangeKind::TypeChanged => ("T", p.modified_fg, tr!("type modifié")),
        ChangeKind::Unmerged => ("U", p.conflict_fg, tr!("en conflit")),
    }
}

/// Ligne de fichier : lettre d'état colorée, nom en évidence, dossier en grisé.
#[allow(clippy::too_many_arguments)]
pub fn file_row(
    ui: &mut Ui,
    id_salt: impl egui::AsIdSalt,
    letter: &str,
    color: egui::Color32,
    path: &str,
    old_path: Option<&str>,
    selected: bool,
    p: &Palette,
) -> egui::Response {
    let (rect, resp) = widgets::row_response(ui, id_salt, 22.0, selected, Sense::click());
    widgets::a11y(&resp, path, true);
    let badge = egui::Rect::from_center_size(rect.left_center() + egui::vec2(12.0, 0.0), egui::vec2(16.0, 16.0));
    ui.painter().rect_filled(badge, 3.0, color.gamma_multiply(0.16));
    ui.painter().text(
        badge.center(),
        egui::Align2::CENTER_CENTER,
        letter,
        egui::FontId::new(11.0, crate::theme::bold_family()),
        color,
    );
    let (dir, name) = match path.rsplit_once('/') {
        Some((d, n)) => (format!("{d}/"), n.to_string()),
        None => (String::new(), path.to_string()),
    };
    let mut job = egui::text::LayoutJob::default();
    let fmt = |c, family| egui::TextFormat { font_id: egui::FontId::new(13.0, family), color: c, ..Default::default() };
    job.append(&name, 0.0, fmt(if selected { p.selection_text } else { p.text }, egui::FontFamily::Proportional));
    if !dir.is_empty() {
        job.append(&format!("  {dir}"), 0.0, fmt(p.weak, egui::FontFamily::Proportional));
    }
    if let Some(old) = old_path {
        job.append(&format!("  ← {old}"), 0.0, fmt(p.weak, egui::FontFamily::Proportional));
    }
    job.wrap = egui::text::TextWrapping {
        max_width: rect.width() - 30.0,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(egui::pos2(rect.left() + 26.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
    resp.on_hover_text(path)
}

impl RepoTab {
    pub(super) fn details_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let Some((info, files)) = self.history.details.clone() else {
            ui.centered_and_justified(|ui| {
                if self.history.details_loading {
                    ui.spinner();
                } else {
                    ui.label(RichText::new(tr!("Sélectionnez un commit pour voir ses modifications")).color(p.weak));
                }
            });
            return;
        };

        egui::Panel::left(("details-info", self.id))
            .resizable(true)
            .default_size(400.0)
            .size_range(240.0..=900.0)
            .frame(Frame::new().fill(p.panel).inner_margin(egui::Margin::same(8)))
            .show(ui, |ui| {
                // En haut : message et métadonnées ; en bas : fichiers (toujours visibles).
                let h = ui.available_height();
                egui::Panel::top(("details-meta", self.id))
                    .resizable(true)
                    .default_size((h * 0.45).max(90.0))
                    .size_range(60.0..=(h - 80.0).max(70.0))
                    .frame(Frame::new().fill(p.panel))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(("details-scroll", self.id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.commit_meta(ui, shared, &info, p);
                            });
                    });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(trn!(files.len(), "{n} fichier modifié", "{n} fichiers modifiés")).strong());
                    if info.parents.len() > 1 {
                        ui.label(RichText::new(tr!("(par rapport au premier parent)")).color(p.weak).size(11.5));
                    }
                });
                ui.add_space(2.0);
                egui::ScrollArea::vertical().id_salt(("details-files", self.id)).auto_shrink([false, false]).show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for (i, f) in files.iter().enumerate() {
                            let (letter, color, label) = change_badge(f.kind, p);
                            let selected = self.history.selected_file == Some(i);
                            let old = f.old_path.as_deref();
                            let resp = file_row(ui, ("commit-file", &f.path), letter, color, &f.path, old, selected, p)
                                .on_hover_text(label);
                            if resp.clicked() && !selected {
                                self.load_commit_diff(shared, i);
                            }
                            let change: FileChange = f.clone();
                            let commit = info.id;
                            resp.context_menu(|ui| self.file_menu(ui, shared, &change.path, Some(commit)));
                        }
                    },
                );
            });

        egui::CentralPanel::default().frame(Frame::new().fill(p.bg)).show(ui, |ui| {
            if let Some(diff) = &self.history.diff {
                let diff = diff.clone();
                self.diff_header(ui, shared, &diff, None, p);
                if diff.is_binary && super::image_diff::is_image(diff.display_path()) {
                    if let Some(change) = self.history.selected_file.and_then(|i| files.get(i)).cloned() {
                        let (old, new) = Self::commit_image_sources(info.id, info.parents.first().copied(), &change);
                        let key = format!("{}:{}", info.id, change.path);
                        self.image_preview_ui(ui, shared, true, key, old, new, p);
                    }
                    return;
                }
                if self.history.highlighter.is_dark() != p.dark {
                    self.history.highlighter = crate::highlight::Highlighter::new(p.dark);
                }
                let hl = shared.settings.syntax_highlight.then_some(&mut self.history.highlighter);
                diff_view::show(
                    ui,
                    &diff,
                    DiffMode::ReadOnly,
                    &mut self.history.diff_state,
                    shared.settings.split_diff,
                    hl,
                    p,
                    ("hist", self.id),
                );
            } else if self.history.diff_loading {
                ui.centered_and_justified(|ui| ui.spinner());
            } else if files.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(tr!("Ce commit ne modifie aucun fichier.")).color(p.weak))
                });
            }
        });
    }

    fn commit_meta(&mut self, ui: &mut Ui, shared: &mut Shared, info: &dragontree_git::CommitInfo, p: &Palette) {
        if let Some((_, label)) = &self.history.external {
            ui.label(RichText::new(format!("{} {label}", icons::STACK)).color(p.weak));
        }
        let (subject, body) = super::split_message(&info.message);
        ui.add(egui::Label::new(RichText::new(subject).strong().size(15.0)).wrap().selectable(true));
        if !body.is_empty() {
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(body).size(13.0)).wrap().selectable(true));
        }
        ui.add_space(8.0);
        egui::Grid::new(("details-grid", self.id)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            ui.label(RichText::new("Commit").color(p.weak));
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(info.id.to_hex()).monospace().size(12.0)).selectable(true));
                if widgets::icon_button(ui, icons::COPY, tr!("Copier le SHA")).clicked() {
                    ui.ctx().copy_text(info.id.to_hex());
                }
            });
            ui.end_row();
            ui.label(RichText::new(tr!("Auteur")).color(p.weak));
            ui.add(egui::Label::new(format!("{} <{}>", info.author, info.author_email)).selectable(true));
            ui.end_row();
            ui.label(RichText::new("Date").color(p.weak));
            ui.label(widgets::format_date(info.author_time));
            ui.end_row();
            if info.committer != info.author || info.committer_time != info.author_time {
                ui.label(RichText::new(tr!("Committeur")).color(p.weak));
                ui.label(format!("{} — {}", info.committer, widgets::format_date(info.committer_time)));
                ui.end_row();
            }
            if !info.parents.is_empty() {
                ui.label(RichText::new(if info.parents.len() > 1 { "Parents" } else { "Parent" }).color(p.weak));
                ui.horizontal(|ui| {
                    for parent in &info.parents {
                        if ui
                            .link(RichText::new(parent.short()).monospace())
                            .on_hover_text(tr!("Afficher ce commit"))
                            .clicked()
                        {
                            let oid = *parent;
                            self.view = super::RepoView::History;
                            self.select_commit(shared, oid, true);
                        }
                    }
                });
                ui.end_row();
            }
            let visibility = super::scope::LabelVisibility::of(shared, &self.path);
            let labels: Vec<_> = self
                .refs
                .as_ref()
                .and_then(|r| r.labels.get(&info.id))
                .map(|ls| ls.iter().filter(|l| visibility.shows(l)).collect())
                .unwrap_or_default();
            let is_lost = self.history.lost.contains(&info.id);
            if !labels.is_empty() || is_lost {
                ui.label(RichText::new(tr!("Références")).color(p.weak));
                ui.horizontal_wrapped(|ui| {
                    if is_lost {
                        ui.label(RichText::new(tr!("perdu")).size(12.0).background_color(p.hover).color(p.weak))
                            .on_hover_text(tr!(
                                "Aucune branche, aucun tag ni HEAD n'atteint ce commit : seul le reflog le garde. Créez une branche ou un tag ici pour le conserver."
                            ));
                    }
                    for l in labels {
                        ui.label(
                            RichText::new(&l.name).size(12.0).background_color(p.badge_local.0).color(p.badge_local.1),
                        );
                    }
                });
                ui.end_row();
            }
        });
    }

    /// En-tête d'un diff : options à droite, chemin tronqué à gauche.
    pub(super) fn diff_header(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        diff: &dragontree_git::FileDiff,
        note: Option<&str>,
        p: &Palette,
    ) {
        ui.horizontal(|ui| {
            ui.set_height(26.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                self.diff_toolbar(ui, shared);
                ui.separator();
                ui.label(
                    RichText::new(format!("+{}  -{}", diff.added_count(), diff.removed_count()))
                        .color(p.weak)
                        .size(12.0),
                );
                if let Some(n) = note {
                    ui.label(RichText::new(n).color(p.weak).size(12.0));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new(diff.display_path()).strong()).truncate())
                        .on_hover_text(diff.display_path());
                });
            });
        });
    }

    /// Options d'affichage du diff (côte à côte, espaces, coloration).
    pub(super) fn diff_toolbar(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let s = &mut shared.settings;
        let mut changed = false;
        changed |= ui.selectable_label(s.split_diff, icons::COLUMNS).on_hover_text(tr!("Côte à côte")).clicked();
        if changed {
            s.split_diff = !s.split_diff;
        }
        let mut ws_changed = false;
        if ui
            .selectable_label(s.ignore_whitespace, RichText::new(tr!("Espaces")).size(11.5))
            .on_hover_text(tr!("Ignorer les différences d'espaces"))
            .clicked()
        {
            s.ignore_whitespace = !s.ignore_whitespace;
            ws_changed = true;
        }
        if ui.selectable_label(s.syntax_highlight, icons::CODE).on_hover_text(tr!("Coloration syntaxique")).clicked() {
            s.syntax_highlight = !s.syntax_highlight;
            changed = true;
        }
        if ui.small_button("+").on_hover_text(tr!("Plus de contexte")).clicked() {
            s.diff_context = (s.diff_context + 3).min(99);
            ws_changed = true;
        }
        if ui.small_button("−").on_hover_text(tr!("Moins de contexte")).clicked() && s.diff_context > 0 {
            s.diff_context = s.diff_context.saturating_sub(3);
            ws_changed = true;
        }
        if changed || ws_changed {
            shared.settings_dirty = true;
        }
        if ws_changed {
            // Recharger les diffs affichés avec les nouvelles options.
            if let Some(i) = self.history.selected_file {
                self.load_commit_diff(shared, i);
            }
            if let Some((side, path)) = self.changes.selected.clone() {
                self.load_work_diff(shared, side, path, false);
            }
        }
    }

    /// Menu d'un fichier (historique ou changements).
    pub(super) fn file_menu(
        &mut self,
        ui: &mut Ui,
        shared: &mut Shared,
        path: &str,
        commit: Option<dragontree_git::Oid>,
    ) {
        // `path` est relatif à la racine du dépôt, pas au dossier ouvert.
        let full = crate::util::native_path(&self.workdir().join(path));
        if ui.button(format!("{} {}", icons::CLOCK_COUNTER_CLOCKWISE, tr!("Historique du fichier"))).clicked() {
            self.open_file_history(shared, path.to_string(), false, commit);
            ui.close();
        }
        if ui.button(format!("{} Blame", icons::USER)).clicked() {
            self.open_file_history(shared, path.to_string(), true, commit);
            ui.close();
        }
        ui.separator();
        if full.exists() {
            if ui.button(format!("{} {}", icons::ARROW_SQUARE_OUT, tr!("Ouvrir le fichier"))).clicked() {
                crate::util::open_with_default_app(&full);
                ui.close();
            }
            if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Afficher dans l'Explorateur"))).clicked() {
                crate::util::reveal_in_explorer(&full);
                ui.close();
            }
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le chemin relatif"))).clicked() {
            ui.ctx().copy_text(path.to_string());
            ui.close();
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le chemin complet"))).clicked() {
            ui.ctx().copy_text(full.display().to_string());
            ui.close();
        }
    }
}
