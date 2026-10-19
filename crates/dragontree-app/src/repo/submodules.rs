//! Sous-modules de l'onglet : dialogues d'ajout, de changement d'URL, de déplacement et de suppression, et leurs
//! opérations (la section de la barre latérale est dans `sidebar.rs`).

use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoTab};
use crate::app::Shared;
use crate::theme::Palette;

/// Nom de dossier proposé pour un sous-module cloné depuis `url` : le dernier segment, sans `.git`
/// (`https://x.test/org/lib.git` donne `lib`).
pub fn folder_from_url(url: &str) -> String {
    let url = url.trim().trim_end_matches(['/', '\\']);
    let last = url.rsplit(['/', '\\', ':']).next().unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

/// Chemin de sous-module acceptable : relatif au dépôt, sans `..`, lettre de lecteur ni barre initiale.
pub fn valid_submodule_path(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains(':')
        && !path.split(['/', '\\']).any(|part| part == ".." || part == ".")
}

/// Chemin tel qu'enregistré par git : séparateurs `/`, sans barre finale.
fn normalized(path: &str) -> String {
    path.trim().replace('\\', "/").trim_end_matches('/').to_string()
}

impl RepoTab {
    /// Ouvre le dialogue d'ajout (Dépôt › Ajouter un sous-module…, bouton « + » de la section).
    pub(super) fn open_add_submodule(&mut self) {
        self.dialog = Some(Dialog::AddSubmodule { url: String::new(), path: String::new(), branch: String::new() });
    }

    fn submodule_taken(&self, path: &str) -> bool {
        let path = normalized(path);
        self.submodules.iter().any(|s| s.path == path)
    }

    /// Champs du dialogue d'ajout ; renvoie vrai si la saisie est valide. Un chemin vide reprend le nom du dépôt.
    pub(super) fn add_submodule_body(
        &self,
        ui: &mut Ui,
        url: &mut String,
        path: &mut String,
        branch: &mut String,
        p: &Palette,
    ) -> bool {
        let proposed = folder_from_url(url);
        let fields = egui::Grid::new("submodule-add-grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("URL");
            let url_field =
                ui.add(egui::TextEdit::singleline(url).hint_text(tr!("https://… ou git@…")).desired_width(340.0));
            ui.end_row();
            ui.label(tr!("Chemin"));
            let path_field = ui
                .add(egui::TextEdit::singleline(path).hint_text(&proposed).desired_width(340.0))
                .on_hover_text(tr!("Dossier du sous-module, relatif au dépôt. Vide : le nom du dépôt cloné."));
            ui.end_row();
            ui.label(tr!("Branche"));
            let branch_field =
                ui.add(egui::TextEdit::singleline(branch).hint_text(tr!("facultatif")).desired_width(340.0));
            ui.end_row();
            [url_field, path_field, branch_field]
        });
        // Le focus va à l'URL tant qu'aucun champ ne l'a : à l'ouverture, pas quand on passe aux autres champs.
        if !fields.inner.iter().any(|f| f.has_focus()) {
            fields.inner[0].request_focus();
        }
        let target = if path.trim().is_empty() { proposed } else { path.trim().to_string() };
        if !valid_submodule_path(&target) {
            if !path.trim().is_empty() {
                warning(ui, tr!("Le chemin doit être relatif au dépôt, sans « .. »."), p);
            }
            return false;
        }
        let taken = self.submodule_taken(&target);
        if taken {
            warning(ui, tr!("Un sous-module occupe déjà ce chemin."), p);
        }
        !url.trim().is_empty() && !taken
    }

    pub(super) fn edit_submodule_url_body(ui: &mut Ui, url: &mut String) -> bool {
        ui.add(egui::TextEdit::singleline(url).hint_text(tr!("https://… ou git@…")).desired_width(f32::INFINITY))
            .request_focus();
        !url.trim().is_empty()
    }

    pub(super) fn move_submodule_body(&self, ui: &mut Ui, old: &str, new: &mut String, p: &Palette) -> bool {
        ui.label(RichText::new(tr!("Nouveau chemin de « {old} » :", old)).color(p.weak));
        ui.add(egui::TextEdit::singleline(new).desired_width(f32::INFINITY)).request_focus();
        let target = normalized(new);
        if !new.trim().is_empty() && !valid_submodule_path(&target) {
            warning(ui, tr!("Le chemin doit être relatif au dépôt, sans « .. »."), p);
            return false;
        }
        let taken = target != normalized(old) && self.submodule_taken(&target);
        if taken {
            warning(ui, tr!("Un sous-module occupe déjà ce chemin."), p);
        }
        valid_submodule_path(&target) && target != normalized(old) && !taken
    }

    pub(super) fn remove_submodule_body(ui: &mut Ui, path: &str, p: &Palette) -> bool {
        ui.label(tr!(
            "Le sous-module « {path} » sera retiré du dépôt : dossier, entrée de .gitmodules et index.",
            path
        ));
        warning(
            ui,
            tr!(
                "Les modifications non commitées du sous-module et ses commits jamais poussés seront définitivement perdus (son clone dans .git/modules est supprimé)."
            ),
            p,
        );
        true
    }

    pub(super) fn op_add_submodule(&mut self, shared: &Shared, url: String, path: String, branch: String) {
        let path = if path.trim().is_empty() { folder_from_url(&url) } else { normalized(&path) };
        let branch = Some(branch.trim().to_string()).filter(|b| !b.is_empty());
        self.run_net(shared, tr!("Ajout du sous-module"), Refresh::ALL, false, move |r, n, on| {
            r.add_submodule(url.trim(), &path, branch.as_deref(), n, on)?;
            Ok(Some(tr!("Sous-module {path} ajouté", path)))
        });
    }

    pub(super) fn op_set_submodule_url(&mut self, shared: &Shared, path: String, url: String) {
        self.run_simple(shared, tr!("Modification de l'URL du sous-module"), Refresh::ALL, move |r| {
            r.set_submodule_url(&path, url.trim())?;
            Ok(Some(tr!("URL du sous-module {path} modifiée", path)))
        });
    }

    pub(super) fn op_move_submodule(&mut self, shared: &Shared, old: String, new: String) {
        let new = normalized(&new);
        self.run_simple(shared, tr!("Déplacement du sous-module"), Refresh::ALL, move |r| {
            r.move_submodule(&old, &new)?;
            Ok(Some(tr!("Sous-module déplacé : {old} → {new}", old, new)))
        });
    }

    pub(super) fn op_remove_submodule(&mut self, shared: &Shared, path: String) {
        self.run_simple(shared, tr!("Suppression du sous-module"), Refresh::ALL, move |r| {
            r.remove_submodule(&path)?;
            Ok(Some(tr!("Sous-module {path} supprimé", path)))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_is_the_last_segment_of_the_url_without_dot_git() {
        assert_eq!(folder_from_url("https://git.fabrikam.test/org/lib.git"), "lib");
        assert_eq!(folder_from_url("git@fabrikam.test:org/lib.git"), "lib");
        assert_eq!(folder_from_url("file:///C:/src/contoso-lib/"), "contoso-lib");
        assert_eq!(folder_from_url("lib"), "lib");
        assert_eq!(folder_from_url(""), "");
    }

    #[test]
    fn only_relative_paths_inside_the_repository_are_valid() {
        assert!(valid_submodule_path("lib"));
        assert!(valid_submodule_path("vendor/lib"));
        assert!(!valid_submodule_path(""));
        assert!(!valid_submodule_path("../lib"));
        assert!(!valid_submodule_path("a/../../lib"));
        assert!(!valid_submodule_path("/lib"));
        assert!(!valid_submodule_path("C:/lib"));
    }
}
