//! Remettre un fichier dans son état à un commit : entrées des menus d'un fichier (détails d'un commit) et d'un
//! commit de l'historique d'un fichier. Si le fichier a des modifications locales, un dialogue destructeur demande
//! confirmation ; sinon le fichier est remis tout de suite. La logique est `Repo::restore_file` dans le moteur.

use dragontree_git::{ChangeKind, GitError, Oid};
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;

/// Quelle version du fichier remettre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreFrom {
    /// Celle du commit.
    Commit(Oid),
    /// Celle d'avant le commit (son premier parent) : la seule possible pour un fichier que le commit supprime.
    BeforeCommit(Oid),
}

impl RestoreFrom {
    /// Révision à passer à git.
    fn rev(self) -> String {
        match self {
            RestoreFrom::Commit(id) => id.to_hex(),
            RestoreFrom::BeforeCommit(id) => format!("{}^", id.to_hex()),
        }
    }

    fn label(self) -> String {
        match self {
            RestoreFrom::Commit(id) => tr!("du commit {sha}", sha = id.short()),
            RestoreFrom::BeforeCommit(id) => tr!("d'avant le commit {sha}", sha = id.short()),
        }
    }
}

/// Ce que le menu sait du fichier dans le commit : sa nature de changement (inconnue dans l'historique d'un fichier)
/// et l'existence d'un parent.
#[derive(Debug, Clone, Copy)]
pub struct RestoreTarget {
    pub commit: Oid,
    pub kind: Option<ChangeKind>,
    pub has_parent: bool,
}

impl RestoreTarget {
    /// Le fichier existe dans le commit (sauf s'il y est supprimé).
    fn at_commit(&self) -> bool {
        self.kind != Some(ChangeKind::Deleted)
    }

    /// Il existait avant le commit sous le même chemin (ni ajouté, ni renommé ou copié depuis un autre chemin).
    fn before_commit(&self) -> bool {
        self.has_parent && !matches!(self.kind, Some(ChangeKind::Added | ChangeKind::Renamed | ChangeKind::Copied))
    }
}

impl RepoTab {
    /// Entrées « Remettre ce fichier… » d'un menu ; renvoie la version choisie.
    pub(super) fn restore_menu_entries(ui: &mut Ui, target: RestoreTarget) -> Option<RestoreFrom> {
        let mut picked = None;
        if target.at_commit()
            && ui
                .button(format!("{} {}", icons::ARROW_COUNTER_CLOCKWISE, tr!("Remettre ce fichier à cette version…")))
                .clicked()
        {
            picked = Some(RestoreFrom::Commit(target.commit));
            ui.close();
        }
        if target.before_commit()
            && ui
                .button(format!(
                    "{} {}",
                    icons::ARROW_COUNTER_CLOCKWISE,
                    tr!("Remettre ce fichier à la version d'avant ce commit…")
                ))
                .clicked()
        {
            picked = Some(RestoreFrom::BeforeCommit(target.commit));
            ui.close();
        }
        picked
    }

    /// Remet `path` à la version choisie. Les modifications locales (modifié, indexé, non suivi, ignoré) sont cherchées
    /// en tâche de fond : si le fichier en a, ou si la lecture échoue, le dialogue demande confirmation avant d'écraser.
    pub(super) fn request_restore(&mut self, shared: &Shared, path: String, from: RestoreFrom) {
        let Some(repo) = self.repo.clone() else { return };
        let (id, rev, label) = (self.id, from.rev(), from.label());
        shared.jobs.spawn("restore-check", move || {
            // Lecture impossible : on suppose des modifications (jamais d'écrasement sans confirmation).
            let dirty = repo.file_has_local_changes(&path).unwrap_or(true);
            Some(AppEvent::Repo(id, RepoEvent::RestoreChecked { path, rev, label, dirty }))
        });
    }

    /// Le fichier a été examiné : confirmation s'il a des modifications locales, sinon remise tout de suite.
    pub(super) fn on_restore_checked(
        &mut self,
        shared: &Shared,
        path: String,
        rev: String,
        label: String,
        dirty: bool,
    ) {
        if self.dialog.is_some() {
            return;
        }
        if dirty {
            self.dialog = Some(Dialog::RestoreFile { path, rev, label });
        } else {
            self.op_restore_file(shared, path, rev, label, false);
        }
    }

    pub(super) fn restore_file_body(ui: &mut Ui, path: &str, rev_label: &str, p: &Palette) -> bool {
        ui.label(tr!("Remettre « {path} » dans son état {label}.", path, label = rev_label));
        ui.add_space(4.0);
        warning(ui, tr!("Les modifications locales de ce fichier seront définitivement perdues."), p);
        ui.label(
            RichText::new(tr!(
                "Le fichier remis est aussi ajouté à l'index, comme le fait git checkout <commit> -- <chemin>."
            ))
            .color(p.weak)
            .size(12.0),
        );
        true
    }

    /// Remet le fichier. Sans `confirmed`, le worker revérifie juste avant d'écrire : un fichier devenu modifié depuis
    /// l'examen n'est pas écrasé.
    pub(super) fn op_restore_file(
        &mut self,
        shared: &Shared,
        path: String,
        rev: String,
        label: String,
        confirmed: bool,
    ) {
        self.run_simple(shared, tr!("Restauration du fichier"), Refresh::STATUS, move |r| {
            if !confirmed && r.file_has_local_changes(&path).unwrap_or(true) {
                return Err(GitError::Refused(tr!(
                    "« {path} » a des modifications locales : rien n'a été écrasé, recommencez pour confirmer.",
                    path
                )));
            }
            r.restore_file(&rev, &path)?;
            Ok(Some(tr!("« {path} » remis dans son état {label}", path, label)))
        });
    }
}
