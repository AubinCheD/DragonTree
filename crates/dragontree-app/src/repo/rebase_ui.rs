//! Éditeur de rebase interactif : ordre et action de chaque commit.

use dragontree_git::rebase::{RebaseAction, RebaseItem};
use dragontree_git::{Commit, Oid};
use dragontree_i18n::tr;
use egui::{RichText, Sense};

use super::{Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;

pub struct RebaseEditor {
    base: Oid,
    items: Vec<RebaseItem>,
    loading: bool,
    editing: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Pick,
    Reword,
    Edit,
    Squash,
    Fixup,
    Drop,
}

fn kind_of(a: &RebaseAction) -> Kind {
    match a {
        RebaseAction::Pick => Kind::Pick,
        RebaseAction::Reword(_) => Kind::Reword,
        RebaseAction::Edit => Kind::Edit,
        RebaseAction::Squash => Kind::Squash,
        RebaseAction::Fixup => Kind::Fixup,
        RebaseAction::Drop => Kind::Drop,
    }
}

impl RebaseEditor {
    fn new(base: Oid) -> RebaseEditor {
        RebaseEditor { base, items: Vec::new(), loading: true, editing: None }
    }

    pub fn set_commits(&mut self, list: Vec<Commit>) {
        self.loading = false;
        self.items = list
            .into_iter()
            .map(|c| RebaseItem { id: c.id, action: RebaseAction::Pick, summary: c.summary.to_string() })
            .collect();
    }

    /// Un squash/fixup a besoin d'un commit conservé avant lui.
    fn validation_error(&self) -> Option<String> {
        let first_kept = self.items.iter().position(|i| !matches!(i.action, RebaseAction::Drop))?;
        match self.items[first_kept].action {
            RebaseAction::Squash | RebaseAction::Fixup => Some(
                tr!("Le premier commit conservé ne peut pas être fusionné (squash/fixup) : il n'y a rien avant lui.")
                    .into(),
            ),
            _ => self
                .items
                .iter()
                .find(|i| matches!(&i.action, RebaseAction::Reword(m) if m.trim().is_empty()))
                .map(|_| tr!("Un message de « reword » est vide.").to_string()),
        }
    }
}

impl RepoTab {
    pub(super) fn open_rebase_editor(&mut self, shared: &Shared, base: Oid) {
        let Some(repo) = self.repo.clone() else { return };
        self.rebase = Some(RebaseEditor::new(base));
        let id = self.id;
        shared.jobs.spawn("rebase-list", move || {
            Some(AppEvent::Repo(id, RepoEvent::RebaseList(repo.rebase_candidates(&base.to_hex()))))
        });
    }

    pub(super) fn rebase_window(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        let Some(editor) = &mut self.rebase else { return };
        let mut open = true;
        let mut start = false;
        let title = tr!("Rebase interactif sur {base}", base = editor.base.short());
        crate::widgets::centered_window(ctx, format!("{} {title}", icons::LIST))
            .id(egui::Id::unique(("rebase", self.id)))
            .open(&mut open)
            .default_size([760.0, 520.0])
            .show(ctx, |ui| {
                if editor.loading {
                    ui.spinner();
                    return;
                }
                if editor.items.is_empty() {
                    ui.label(tr!("Aucun commit à rejouer après ce commit sur la branche courante."));
                    return;
                }
                ui.label(RichText::new(tr!("Du plus ancien (en haut) au plus récent. Glissez-déposez ou utilisez les flèches pour réordonner.")).color(p.weak));
                ui.add_space(6.0);
                let mut swap: Option<(usize, usize)> = None;
                egui::ScrollArea::vertical().max_height(ui.available_height() - 90.0).show(ui, |ui| {
                    for i in 0..editor.items.len() {
                        let row = ui.horizontal(|ui| {
                            let handle = ui.add(egui::Label::new(RichText::new(icons::DOTS_THREE).color(p.weak)).sense(Sense::drag()));
                            if handle.dragged()
                                && let Some(pos) = ui.ctx().pointer_interact_pos()
                            {
                                let rect = handle.rect;
                                if pos.y < rect.top() - 6.0 && i > 0 {
                                    swap = Some((i, i - 1));
                                } else if pos.y > rect.bottom() + 6.0 && i + 1 < editor.items.len() {
                                    swap = Some((i, i + 1));
                                }
                            }
                            let item = &mut editor.items[i];
                            let mut kind = kind_of(&item.action);
                            egui::ComboBox::from_id_salt(("rb-kind", i)).width(90.0).selected_text(match kind {
                                Kind::Pick => "pick",
                                Kind::Reword => "reword",
                                Kind::Edit => "edit",
                                Kind::Squash => "squash",
                                Kind::Fixup => "fixup",
                                Kind::Drop => "drop",
                            }).show_ui(ui, |ui| {
                                ui.selectable_value(&mut kind, Kind::Pick, tr!("pick — garder"));
                                ui.selectable_value(&mut kind, Kind::Reword, tr!("reword — changer le message"));
                                ui.selectable_value(&mut kind, Kind::Edit, tr!("edit — s'arrêter pour modifier"));
                                ui.selectable_value(&mut kind, Kind::Squash, tr!("squash — fusionner avec le précédent"));
                                ui.selectable_value(&mut kind, Kind::Fixup, tr!("fixup — fusionner, garder le message précédent"));
                                ui.selectable_value(&mut kind, Kind::Drop, tr!("drop — supprimer"));
                            });
                            if kind != kind_of(&item.action) {
                                item.action = match kind {
                                    Kind::Pick => RebaseAction::Pick,
                                    Kind::Reword => RebaseAction::Reword(item.summary.clone()),
                                    Kind::Edit => RebaseAction::Edit,
                                    Kind::Squash => RebaseAction::Squash,
                                    Kind::Fixup => RebaseAction::Fixup,
                                    Kind::Drop => RebaseAction::Drop,
                                };
                                if kind == Kind::Reword {
                                    editor.editing = Some(i);
                                }
                            }
                            let dropped = matches!(item.action, RebaseAction::Drop);
                            ui.label(RichText::new(item.id.short()).monospace().color(p.weak));
                            let text = RichText::new(&item.summary);
                            ui.label(if dropped { text.strikethrough().color(p.weak) } else { text });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add_enabled(i + 1 < editor.items.len(), egui::Button::new(icons::ARROW_DOWN).small()).clicked() {
                                    swap = Some((i, i + 1));
                                }
                                if ui.add_enabled(i > 0, egui::Button::new(icons::ARROW_UP).small()).clicked() {
                                    swap = Some((i, i - 1));
                                }
                            });
                        });
                        let _ = row;
                        if editor.editing == Some(i)
                            && let RebaseAction::Reword(msg) = &mut editor.items[i].action
                        {
                            ui.add(egui::TextEdit::multiline(msg).desired_rows(3).desired_width(f32::INFINITY));
                            if ui.small_button("OK").clicked() {
                                editor.editing = None;
                            }
                        }
                    }
                });
                if let Some((a, b)) = swap {
                    editor.items.swap(a, b);
                }
                ui.separator();
                if let Some(err) = editor.validation_error() {
                    ui.label(RichText::new(format!("{} {err}", icons::WARNING)).color(p.modified_fg));
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} {}", icons::INFO, tr!("Les commits réécrits changeront de SHA."))).color(p.weak));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ok = editor.validation_error().is_none();
                        if ui.add_enabled(ok, egui::Button::new(RichText::new(tr!("Lancer le rebase")).strong().color(egui::Color32::WHITE)).fill(p.accent)).clicked() {
                            start = true;
                        }
                    });
                });
            });
        if start && let Some(editor) = self.rebase.take() {
            let (base, items) = (editor.base.to_hex(), editor.items);
            self.run_simple(shared, tr!("Rebase interactif"), Refresh::ALL, move |r| {
                r.interactive_rebase(&base, &items)?;
                Ok(Some(tr!("Rebase interactif terminé").into()))
            });
            return;
        }
        if !open {
            self.rebase = None;
        }
    }
}
