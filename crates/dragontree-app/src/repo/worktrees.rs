//! Worktrees de l'onglet : section de la barre latérale, dialogues de création et de suppression, opérations.

use std::path::{Path, PathBuf};

use dragontree_git::{Worktree, WorktreeStart};
use dragontree_i18n::tr;
use egui::{Align2, FontId, RichText, Sense, Ui};

use super::dialogs::{ConfirmAction, valid_ref_name, warning};
use super::sidebar::ROW_H;
use super::{Dialog, Refresh, RepoTab};
use crate::app::{AppAction, Shared};
use crate::icons;
use crate::theme::{Palette, bold_family};
use crate::widgets;

/// Dossier proposé pour un nouveau worktree : à côté du worktree principal `main`, nommé
/// `<dépôt>-<suffixe>` (les `/` d'un nom de branche deviennent des `-`).
pub fn worktree_folder(main: &Path, suffix: &str) -> PathBuf {
    let repo = main.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let suffix = suffix.replace(['/', '\\'], "-");
    let name = if repo.is_empty() { suffix } else { format!("{repo}-{suffix}") };
    main.parent().unwrap_or(main).join(name)
}

/// Branche (ou HEAD détachée) d'un worktree, telle qu'affichée.
fn worktree_name(wt: &Worktree) -> String {
    match (&wt.branch, wt.head) {
        (Some(branch), _) => branch.clone(),
        (None, Some(oid)) => tr!("HEAD détachée ({sha})", sha = oid.short()),
        (None, None) => "—".into(),
    }
}

impl RepoTab {
    /// Worktree, autre que celui de l'onglet, où la branche locale `branch` est sortie.
    pub(super) fn checked_out_elsewhere(&self, branch: &str) -> Option<&Worktree> {
        self.worktrees.iter().find(|w| !w.is_current && w.branch.as_deref() == Some(branch))
    }

    /// Le checkout a été refusé : la branche est sortie dans le worktree `path`. On propose de l'ouvrir.
    pub(super) fn offer_worktree(&mut self, branch: String, path: PathBuf) {
        let shown = crate::util::native_path(&path).display().to_string();
        self.dialog = Some(Dialog::Confirm {
            title: tr!("Branche déjà sortie").into(),
            text: tr!(
                "La branche « {branch} » est sortie dans le worktree {path}. Ouvrir ce worktree ?",
                branch,
                path = shown
            ),
            action: ConfirmAction::OpenWorktree(path),
        });
    }

    /// Section « Worktrees », en tête des sections de la barre latérale : affichée dès qu'il existe un worktree lié.
    pub(super) fn worktrees_section(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let list = self.worktrees.clone();
        if list.len() < 2
            || !self.section(ui, "worktrees", &format!("{} Worktrees", icons::TREE_STRUCTURE), list.len(), p)
        {
            return;
        }
        for wt in list.iter() {
            self.worktree_row(ui, shared, wt, p);
        }
        let add = RichText::new(format!("{}  {}", icons::PLUS, tr!("Créer un worktree…"))).size(12.0).color(p.weak);
        if ui.add(egui::Button::new(add).frame(false)).clicked() {
            self.open_create_worktree(None);
        }
    }

    /// Ligne d'un worktree : branche puis chemin ; un clic l'ouvre dans un onglet (ou active l'onglet existant).
    fn worktree_row(&mut self, ui: &mut Ui, shared: &mut Shared, wt: &Worktree, p: &Palette) {
        let name = worktree_name(wt);
        let path = crate::util::native_path(&wt.path).display().to_string();
        let label = format!("{name} — {path}");
        let (rect, resp) = widgets::row_response(ui, ("worktree", &path), ROW_H, false, Sense::click());
        widgets::a11y(&resp, &label, true);
        let (icon, icon_color) = if wt.prunable {
            (icons::WARNING, p.modified_fg)
        } else if wt.is_current {
            (icons::CHECK, p.accent)
        } else if wt.locked {
            (icons::LOCK_SIMPLE, p.weak)
        } else {
            (icons::TREE_STRUCTURE, p.weak)
        };
        let x = rect.left() + 18.0;
        ui.painter().text(
            egui::pos2(x + 5.0, rect.center().y),
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(13.0),
            icon_color,
        );
        let family = if wt.is_current { bold_family() } else { egui::FontFamily::Proportional };
        let text_color = if wt.prunable {
            p.weak
        } else if wt.is_current {
            p.accent
        } else {
            p.text
        };
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &name,
            0.0,
            egui::TextFormat { font_id: FontId::new(13.0, family), color: text_color, ..Default::default() },
        );
        job.append(
            &path,
            8.0,
            egui::TextFormat { font_id: FontId::proportional(11.5), color: p.weak, ..Default::default() },
        );
        job.wrap = egui::text::TextWrapping {
            max_width: (rect.right() - x - 22.0).max(10.0),
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(x + 16.0, rect.center().y - galley.size().y / 2.0), galley, text_color);

        let state = if wt.prunable {
            tr!("Dossier introuvable : à nettoyer (prune)")
        } else if wt.is_current {
            tr!("Worktree de cet onglet")
        } else if wt.locked {
            tr!("Verrouillé — clic : ouvrir dans un onglet")
        } else {
            tr!("Clic : ouvrir dans un onglet")
        };
        let resp = resp.on_hover_text(format!("{path}\n{state}"));
        if resp.clicked() && !wt.prunable && !wt.is_current {
            shared.actions.push(AppAction::OpenRepo(wt.path.clone()));
        }
        resp.context_menu(|ui| self.worktree_menu(ui, shared, wt, &path));
    }

    fn worktree_menu(&mut self, ui: &mut Ui, shared: &mut Shared, wt: &Worktree, path: &str) {
        if !wt.prunable {
            if !wt.is_current && ui.button(format!("{} {}", icons::ARROW_SQUARE_OUT, tr!("Ouvrir"))).clicked() {
                shared.actions.push(AppAction::OpenRepo(wt.path.clone()));
                ui.close();
            }
            if ui.button(format!("{} {}", icons::FOLDER_OPEN, tr!("Ouvrir dans l'Explorateur"))).clicked() {
                crate::util::open_in_explorer(&wt.path);
                ui.close();
            }
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier le chemin"))).clicked() {
            ui.ctx().copy_text(path.to_string());
            ui.close();
        }
        if wt.prunable
            && ui.button(format!("{} {}", icons::BROOM, tr!("Nettoyer les worktrees disparus (prune)"))).clicked()
        {
            self.run_simple(shared, tr!("Nettoyage des worktrees"), Refresh::REFS.with_extras(), |r| {
                r.prune_worktrees()?;
                Ok(Some(tr!("Worktrees nettoyés").into()))
            });
            ui.close();
        }
        // Ni le principal ni celui de l'onglet : git ne supprime pas le premier, et l'onglet perdrait son dossier.
        if !wt.is_main && !wt.is_current && !wt.prunable {
            ui.separator();
            if ui.button(format!("{} {}", icons::TRASH, tr!("Supprimer le worktree…"))).clicked() {
                self.dialog = Some(Dialog::RemoveWorktree { path: wt.path.clone(), force: false });
                ui.close();
            }
        }
    }

    /// Branches locales qu'un nouveau worktree peut sortir : ni HEAD, ni déjà sorties dans un autre worktree.
    fn free_branches(&self) -> Vec<String> {
        let Some(refs) = &self.refs else { return Vec::new() };
        refs.locals
            .iter()
            .filter(|b| !b.is_head && self.checked_out_elsewhere(&b.name).is_none())
            .map(|b| b.name.clone())
            .collect()
    }

    /// Dossier proposé pour `start`, à côté du worktree principal.
    fn proposed_folder(&self, start: &WorktreeStart) -> String {
        let main = self.worktrees.iter().find(|w| w.is_main).map(|w| w.path.clone()).unwrap_or_else(|| self.workdir());
        let suffix = match start {
            WorktreeStart::Branch(name) => name.clone(),
            WorktreeStart::Track { local, .. } => local.clone(),
            WorktreeStart::Detached(rev) => rev.chars().take(7).collect(),
        };
        crate::util::native_path(&worktree_folder(&main, &suffix)).display().to_string()
    }

    /// Ouvre le dialogue « Créer un worktree » ; `None` (menu Dépôt, palette) : la branche se choisit dans le
    /// dialogue, une branche libre d'abord, sinon le commit de HEAD.
    pub(super) fn open_create_worktree(&mut self, start: Option<WorktreeStart>) {
        let pick = start.is_none();
        let start = start.unwrap_or_else(|| match self.free_branches().into_iter().next() {
            Some(branch) => WorktreeStart::Branch(branch),
            None => WorktreeStart::Detached(self.head_rev()),
        });
        let path = self.proposed_folder(&start);
        self.dialog = Some(Dialog::CreateWorktree { start, path, pick });
    }

    /// Commit de HEAD (SHA complet), ou `HEAD` s'il n'est pas encore lu.
    fn head_rev(&self) -> String {
        self.refs.as_ref().and_then(|r| r.head_oid).map_or_else(|| "HEAD".into(), |o| o.to_hex())
    }

    /// Champs du dialogue « Créer un worktree » ; renvoie vrai si la saisie est valide.
    pub(super) fn create_worktree_body(
        &self,
        ui: &mut Ui,
        start: &mut WorktreeStart,
        path: &mut String,
        pick: bool,
        p: &Palette,
    ) -> bool {
        let detached_label =
            |rev: &str| tr!("Commit {sha} (HEAD détachée)", sha = rev.chars().take(7).collect::<String>());
        let mut valid = true;
        if pick {
            let selected = match &*start {
                WorktreeStart::Branch(name) => name.clone(),
                WorktreeStart::Detached(rev) => detached_label(rev),
                WorktreeStart::Track { remote_branch, .. } => remote_branch.clone(),
            };
            let mut chosen = None;
            ui.horizontal(|ui| {
                ui.label(tr!("Branche"));
                egui::ComboBox::from_id_salt(("worktree-start", self.id)).selected_text(selected).show_ui(ui, |ui| {
                    for name in self.free_branches() {
                        let on = *start == WorktreeStart::Branch(name.clone());
                        if ui.selectable_label(on, &name).clicked() {
                            chosen = Some(WorktreeStart::Branch(name));
                        }
                    }
                    let head = self.head_rev();
                    let on = matches!(&*start, WorktreeStart::Detached(_));
                    if ui.selectable_label(on, detached_label(&head)).clicked() {
                        chosen = Some(WorktreeStart::Detached(head));
                    }
                });
            });
            if let Some(new) = chosen.filter(|new| new != start) {
                *path = self.proposed_folder(&new);
                *start = new;
            }
        } else {
            match start {
                WorktreeStart::Branch(name) => {
                    ui.label(RichText::new(tr!("Branche : {name}", name = name.as_str())).color(p.weak));
                }
                WorktreeStart::Detached(rev) => {
                    ui.label(RichText::new(detached_label(rev)).color(p.weak));
                }
                WorktreeStart::Track { remote_branch, local } => {
                    ui.label(
                        RichText::new(tr!(
                            "Nouvelle branche locale suivant {remote_full} :",
                            remote_full = remote_branch.as_str()
                        ))
                        .color(p.weak),
                    );
                    ui.add(egui::TextEdit::singleline(local).desired_width(f32::INFINITY));
                    let exists = self.refs.as_ref().is_some_and(|r| r.locals.iter().any(|b| b.name == local.trim()));
                    if exists {
                        warning(ui, tr!("Une branche locale porte déjà ce nom."), p);
                    }
                    valid &= valid_ref_name(local.trim()) && !exists;
                }
            }
        }
        if let WorktreeStart::Branch(name) = &*start
            && (self.refs.as_ref().is_some_and(|r| r.current_branch() == Some(name.as_str()))
                || self.checked_out_elsewhere(name).is_some())
        {
            warning(ui, tr!("Cette branche est déjà sortie dans un worktree."), p);
            valid = false;
        }
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("Dossier du worktree")).color(p.weak));
        ui.add(egui::TextEdit::singleline(path).desired_width(f32::INFINITY));
        let folder = Path::new(path.trim());
        // Git crée le dossier, ou accepte un dossier vide.
        let occupied = folder.is_dir() && std::fs::read_dir(folder).is_ok_and(|mut d| d.next().is_some());
        if occupied || folder.is_file() {
            warning(ui, tr!("Ce dossier existe déjà et n'est pas vide."), p);
        }
        valid && folder.is_absolute() && !occupied && !folder.is_file()
    }

    /// Champs du dialogue de suppression d'un worktree (toujours valide).
    pub(super) fn remove_worktree_body(ui: &mut Ui, path: &Path, force: &mut bool, p: &Palette) -> bool {
        let shown = crate::util::native_path(path).display().to_string();
        warning(ui, &tr!("Le dossier {path} sera supprimé.", path = shown), p);
        ui.label(
            RichText::new(tr!(
                "Sans « Forcer », git refuse de supprimer un worktree qui a des modifications ou des fichiers non suivis."
            ))
            .color(p.weak),
        );
        ui.checkbox(
            force,
            tr!("Forcer : supprimer aussi les modifications non commitées et les fichiers non suivis (--force)"),
        );
        if *force {
            warning(ui, tr!("Les modifications non commitées de ce worktree seront définitivement perdues."), p);
        }
        true
    }

    /// Crée le worktree, puis l'ouvre dans un onglet s'il a été créé.
    pub(super) fn op_add_worktree(&mut self, shared: &Shared, path: PathBuf, start: WorktreeStart) {
        let before = self.next_op_id;
        let target = path.clone();
        self.run_simple(shared, tr!("Création du worktree"), Refresh::REFS.with_extras(), move |r| {
            r.add_worktree(&target, &start)?;
            Ok(Some(tr!("Worktree créé : {path}", path = target.display())))
        });
        if self.next_op_id != before {
            self.open_after_op = Some((self.next_op_id, path));
        }
    }

    /// Supprime le worktree ; `force` vient seulement de la case « Forcer » du dialogue de confirmation.
    pub(crate) fn op_remove_worktree(&mut self, shared: &Shared, path: PathBuf, force: bool) {
        self.run_simple(shared, tr!("Suppression du worktree"), Refresh::REFS.with_extras(), move |r| {
            r.remove_worktree(&path, force)?;
            Ok(Some(tr!("Worktree supprimé : {path}", path = path.display())))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_proposed_folder_sits_next_to_the_main_worktree() {
        let main = Path::new(r"C:\DEV\fabrikam");
        assert_eq!(worktree_folder(main, "fonction"), Path::new(r"C:\DEV\fabrikam-fonction"));
        assert_eq!(worktree_folder(main, "feature/x"), Path::new(r"C:\DEV\fabrikam-feature-x"));
    }
}
