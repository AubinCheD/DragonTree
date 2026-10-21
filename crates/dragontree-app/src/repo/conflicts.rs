//! Éditeur de conflits : choix de « notre » version, de « leur » version ou des deux, par région.

use dragontree_git::conflict::{ConflictRegion, Resolution, parse_conflicts, resolve, resolve_partial};
use dragontree_i18n::{tr, trn};
use egui::{Frame, RichText};

use super::{Refresh, RepoTab};
use crate::app::Shared;
use crate::icons;
use crate::theme::Palette;

pub struct ConflictEditor {
    path: String,
    text: String,
    regions: Vec<ConflictRegion>,
    choices: Vec<Option<Resolution>>,
    error: Option<String>,
    /// Aperçu du résultat pour `preview_choices` : recalculé seulement quand les choix changent.
    preview: String,
    preview_choices: Vec<Option<Resolution>>,
    /// Étendue (en octets de `preview`) de chaque ligne de l'aperçu, pour n'afficher que les lignes visibles.
    preview_lines: Vec<std::ops::Range<usize>>,
}

impl ConflictEditor {
    fn new(path: String, text: String, error: Option<String>) -> ConflictEditor {
        let regions = parse_conflicts(&text);
        let error = error.or_else(|| {
            regions.is_empty().then(|| tr!("Aucun marqueur de conflit trouvé dans le fichier.").to_string())
        });
        let choices = vec![None; regions.len()];
        let mut ed = ConflictEditor {
            path,
            text,
            regions,
            choices,
            error,
            preview: String::new(),
            preview_choices: vec![],
            preview_lines: vec![],
        };
        ed.set_preview();
        ed
    }

    /// Texte du fichier tel qu'il sera enregistré avec les choix actuels ; une région sans choix garde ses marqueurs.
    fn result_preview(&self) -> String {
        resolve_partial(&self.text, &self.choices)
    }

    /// Recalcule l'aperçu si les choix ont changé depuis son dernier calcul.
    fn refresh_preview(&mut self) {
        if self.preview_choices != self.choices {
            self.set_preview();
        }
    }

    fn set_preview(&mut self) {
        self.preview = self.result_preview();
        self.preview_choices.clone_from(&self.choices);
        let mut start = 0;
        self.preview_lines = self
            .preview
            .split_inclusive('\n')
            .map(|l| {
                let range = start..start + l.trim_end_matches(['\n', '\r']).len();
                start += l.len();
                range
            })
            .collect();
    }
}

impl RepoTab {
    pub(super) fn open_conflict_editor(&mut self, path: String) {
        let Some(repo) = &self.repo else { return };
        let full = repo.workdir.join(&path);
        let (text, error) = match std::fs::read(&full) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(t) => (t, None),
                Err(_) => (
                    String::new(),
                    Some(
                        tr!("Fichier binaire ou non UTF-8 : utilisez « notre / leur version » ou un outil externe.")
                            .to_string(),
                    ),
                ),
            },
            Err(e) => (String::new(), Some(e.to_string())),
        };
        self.conflict_editor = Some(ConflictEditor::new(path, text, error));
    }

    pub(super) fn conflict_window(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        let Some(ed) = &mut self.conflict_editor else { return };
        let style = crate::diff_style::DiffStyle::from_settings(&shared.settings);
        let mut open = true;
        let mut save = false;
        let (ours_label, theirs_label) = match self.op_state {
            dragontree_git::OpState::Rebase { .. } => (tr!("Branche cible (HEAD)"), tr!("Commit rejoué")),
            _ => (tr!("Notre version (HEAD)"), tr!("Leur version")),
        };
        let tab_id = self.id;
        let title = tr!("Conflits — {path}", path = ed.path);
        crate::widgets::centered_window(ctx, format!("{} {title}", icons::GIT_DIFF))
            .id(egui::Id::unique(("conflicts", self.id)))
            .open(&mut open)
            .default_size([1100.0, 700.0])
            .show(ctx, |ui| {
                if let Some(err) = &ed.error {
                    ui.label(RichText::new(format!("{} {err}", icons::WARNING)).color(p.modified_fg));
                    return;
                }
                let done = ed.choices.iter().filter(|c| c.is_some()).count();
                ui.horizontal(|ui| {
                    let summary =
                        trn!(ed.regions.len(), "{done} / {n} conflit résolu", "{done} / {n} conflits résolus", done);
                    ui.label(RichText::new(summary).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ok = done == ed.regions.len();
                        if ui
                            .add_enabled(
                                ok,
                                egui::Button::new(
                                    RichText::new(format!("{} {}", icons::CHECK, tr!("Enregistrer et marquer résolu")))
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(p.accent),
                            )
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button(tr!("Tout : notre version")).clicked() {
                            ed.choices.iter_mut().for_each(|c| *c = Some(Resolution::Ours));
                        }
                        if ui.button(tr!("Tout : leur version")).clicked() {
                            ed.choices.iter_mut().for_each(|c| *c = Some(Resolution::Theirs));
                        }
                    });
                });
                ui.separator();
                // Aperçu du résultat, sous les régions : à jour avec les choix de cette image (un choix fait plus bas
                // dans l'image est pris en compte à la suivante).
                ed.refresh_preview();
                egui::Panel::bottom(("conflict-preview", tab_id))
                    .resizable(true)
                    .default_size(200.0)
                    .size_range(80.0..=420.0)
                    .frame(Frame::new())
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new(tr!("Aperçu du résultat")).strong());
                        if ed.choices.iter().any(|c| c.is_none()) {
                            ui.label(
                                RichText::new(tr!("Les conflits sans choix gardent leurs marqueurs."))
                                    .color(p.weak)
                                    .size(11.5),
                            );
                        }
                        let font = style.font_id();
                        let row_h = ui.fonts_mut(|f| f.row_height(&font));
                        Frame::new().fill(p.panel).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                            egui::ScrollArea::both()
                                .id_salt(("conflict-preview-rows", tab_id))
                                .auto_shrink([false, false])
                                .show_rows(ui, row_h, ed.preview_lines.len(), |ui, range| {
                                    for line in &ed.preview_lines[range] {
                                        let line = &ed.preview[line.clone()];
                                        ui.add(
                                            egui::Label::new(RichText::new(style.render(line)).font(font.clone()))
                                                .extend(),
                                        );
                                    }
                                });
                        });
                    });
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (i, region) in ed.regions.iter().enumerate() {
                        Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(tr!(
                                        "Conflit {num} — ligne {line}",
                                        num = i + 1,
                                        line = region.start_line + 1
                                    ))
                                    .strong(),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let choice = &mut ed.choices[i];
                                    ui.selectable_value(
                                        choice,
                                        Some(Resolution::BothReversed),
                                        tr!("Les deux (leur d'abord)"),
                                    );
                                    ui.selectable_value(choice, Some(Resolution::Both), tr!("Les deux"));
                                    ui.selectable_value(choice, Some(Resolution::Theirs), tr!("Leur version"));
                                    ui.selectable_value(choice, Some(Resolution::Ours), tr!("Notre version"));
                                });
                            });
                            ui.columns(2, |cols| {
                                let block = |ui: &mut egui::Ui,
                                             title: &str,
                                             lines: &[String],
                                             bg: egui::Color32,
                                             chosen: bool| {
                                    ui.label(
                                        RichText::new(title).color(if chosen { p.accent } else { p.weak }).strong(),
                                    );
                                    Frame::new().fill(bg).inner_margin(egui::Margin::same(6)).corner_radius(4).show(
                                        ui,
                                        |ui| {
                                            ui.set_width(ui.available_width());
                                            if lines.is_empty() {
                                                ui.label(RichText::new(tr!("(vide)")).italics().color(p.weak));
                                            }
                                            for l in lines {
                                                ui.label(RichText::new(style.render(l)).font(style.font_id()));
                                            }
                                        },
                                    );
                                };
                                let c = ed.choices[i];
                                let ours_chosen =
                                    matches!(c, Some(Resolution::Ours | Resolution::Both | Resolution::BothReversed));
                                let theirs_chosen =
                                    matches!(c, Some(Resolution::Theirs | Resolution::Both | Resolution::BothReversed));
                                block(
                                    &mut cols[0],
                                    &format!("{ours_label} — {}", region.ours_label),
                                    &region.ours,
                                    p.added_bg,
                                    ours_chosen,
                                );
                                block(
                                    &mut cols[1],
                                    &format!("{theirs_label} — {}", region.theirs_label),
                                    &region.theirs,
                                    p.hunk_bg,
                                    theirs_chosen,
                                );
                            });
                        });
                        ui.add_space(6.0);
                    }
                });
            });
        if let Some(ed) = &self.conflict_editor
            && ed.preview_choices != ed.choices
        {
            ctx.request_repaint();
        }
        if save && let Some(ed) = self.conflict_editor.take() {
            let choices: Vec<Resolution> = ed.choices.iter().map(|c| c.unwrap_or(Resolution::Ours)).collect();
            let resolved = resolve(&ed.text, &choices);
            let path = ed.path.clone();
            self.run_simple(shared, tr!("Résolution du conflit"), Refresh::STATUS, move |r| {
                std::fs::write(r.workdir.join(&path), resolved)?;
                r.stage(std::slice::from_ref(&path))?;
                Ok(Some(tr!("{path} résolu", path)))
            });
            return;
        }
        if !open {
            self.conflict_editor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "début\n<<<<<<< HEAD\nnotre ligne\n=======\nleur ligne\n>>>>>>> autre\nfin\n";

    fn editor_with(choice: Option<Resolution>) -> ConflictEditor {
        let mut ed = ConflictEditor::new("f.txt".into(), TEXT.into(), None);
        ed.choices[0] = choice;
        ed.refresh_preview();
        ed
    }

    #[test]
    fn the_preview_shows_our_side_when_ours_is_chosen() {
        assert_eq!(editor_with(Some(Resolution::Ours)).preview, "début\nnotre ligne\nfin\n");
    }

    #[test]
    fn the_preview_shows_their_side_when_theirs_is_chosen() {
        assert_eq!(editor_with(Some(Resolution::Theirs)).preview, "début\nleur ligne\nfin\n");
    }

    #[test]
    fn the_preview_shows_both_sides_in_the_chosen_order() {
        assert_eq!(editor_with(Some(Resolution::Both)).preview, "début\nnotre ligne\nleur ligne\nfin\n");
        assert_eq!(editor_with(Some(Resolution::BothReversed)).preview, "début\nleur ligne\nnotre ligne\nfin\n");
    }

    #[test]
    fn the_preview_line_ranges_follow_the_preview_text() {
        let mut ed = editor_with(Some(Resolution::Both));
        let lines: Vec<&str> = ed.preview_lines.iter().map(|r| &ed.preview[r.clone()]).collect();
        assert_eq!(lines, ["début", "notre ligne", "leur ligne", "fin"]);
        ed.choices[0] = Some(Resolution::Ours);
        ed.refresh_preview();
        let lines: Vec<&str> = ed.preview_lines.iter().map(|r| &ed.preview[r.clone()]).collect();
        assert_eq!(lines, ["début", "notre ligne", "fin"]);
    }

    #[test]
    fn the_preview_keeps_the_markers_of_an_unresolved_conflict_and_follows_the_choices() {
        let mut ed = editor_with(None);
        assert_eq!(ed.preview, TEXT);
        ed.choices[0] = Some(Resolution::Theirs);
        assert_eq!(ed.preview, TEXT, "recalculé seulement par refresh_preview");
        ed.refresh_preview();
        assert_eq!(ed.preview, "début\nleur ligne\nfin\n");
    }
}
