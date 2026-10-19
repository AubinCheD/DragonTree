//! Patchs de l'onglet : appliquer un patch venu d'un fichier, d'un glisser-déposer ou du texte collé.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dragontree_git::{ApplyTarget, GitError, PatchFile, Repo, is_mail_patch};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::theme::Palette;
use crate::toasts::Toast;

/// Où appliquer le patch. `Am` : `git am` crée les commits (patch au format mail seulement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatchTarget {
    Worktree,
    Index,
    Am,
}

/// Patch lu, avec ce que l'aperçu affiche : fichiers touchés et résultat de la vérification pour chaque cible
/// (`None` : le patch s'applique, sinon l'explication de git).
#[derive(Debug, Clone)]
pub struct PatchPreview {
    pub name: String,
    pub patch: Arc<Vec<u8>>,
    pub files: Vec<PatchFile>,
    pub is_mail: bool,
    pub check_worktree: Option<String>,
    pub check_index: Option<String>,
    /// Vérification pour `git am` : l'index et la copie de travail doivent tous deux accepter le patch.
    pub check_am: Option<String>,
}

impl PatchPreview {
    /// Analyse et vérifie le patch (commandes git : hors du thread de l'interface).
    fn read(repo: &Repo, name: String, patch: Vec<u8>) -> Result<PatchPreview, GitError> {
        let files = repo.patch_numstat(&patch)?;
        if files.is_empty() {
            return Err(GitError::Refused(tr!("Ce texte ne contient aucun patch.").into()));
        }
        let check = |target| repo.patch_check(&patch, target).err().map(|e| e.to_string());
        Ok(PatchPreview {
            name,
            files,
            is_mail: is_mail_patch(&patch),
            check_worktree: check(ApplyTarget::Worktree),
            check_index: check(ApplyTarget::Index),
            check_am: check(ApplyTarget::IndexAndWorktree),
            patch: Arc::new(patch),
        })
    }

    /// Résultat de la vérification pour la cible choisie.
    fn check_for(&self, target: PatchTarget) -> &Option<String> {
        match target {
            PatchTarget::Worktree => &self.check_worktree,
            PatchTarget::Index => &self.check_index,
            PatchTarget::Am => &self.check_am,
        }
    }
}

/// Un fichier déposé est un patch (extension `.patch` ou `.diff`).
pub fn is_patch_file(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("patch") || e.eq_ignore_ascii_case("diff"))
}

impl RepoTab {
    /// Ouvre l'aperçu d'un patch lu dans un fichier.
    pub(crate) fn open_patch_file(&mut self, shared: &Shared, path: PathBuf) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.load_patch_preview(shared, name, move || std::fs::read(&path).map_err(GitError::from));
    }

    /// Ouvre l'aperçu d'un patch donné en texte (presse-papiers).
    pub(crate) fn open_patch_text(&mut self, shared: &Shared, name: String, text: String) {
        let mut bytes = text.into_bytes();
        // git exige que la dernière ligne se termine : un texte collé n'a pas toujours de retour à la ligne final.
        if !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        self.load_patch_preview(shared, name, move || Ok(bytes));
    }

    /// Lecture du patch et analyse en tâche de fond (un patch peut être gros) ; le résultat ouvre le dialogue.
    fn load_patch_preview(
        &mut self,
        shared: &Shared,
        name: String,
        read: impl FnOnce() -> Result<Vec<u8>, GitError> + Send + 'static,
    ) {
        let Some(repo) = self.repo.clone() else { return };
        self.gens.patch += 1;
        let (g, id) = (self.gens.patch, self.id);
        shared.jobs.spawn("patch-preview", move || {
            let res = read().and_then(|bytes| PatchPreview::read(&repo, name, bytes));
            Some(AppEvent::Repo(id, RepoEvent::PatchPreview(g, res.map(Box::new))))
        });
    }

    pub(super) fn on_patch_preview(&mut self, shared: &mut Shared, res: Result<Box<PatchPreview>, GitError>) {
        match res {
            Ok(preview) => {
                // Un dialogue déjà ouvert garde la main : l'aperçu ne le remplace pas.
                if self.dialog.is_none() {
                    self.dialog = Some(Dialog::ApplyPatch { preview, target: PatchTarget::Worktree });
                }
            }
            Err(e) => shared.toast(Toast::error(tr!("Patch illisible"), e.to_string())),
        }
    }

    /// Corps du dialogue d'application ; vrai si le patch peut être appliqué à la cible choisie.
    pub(super) fn apply_patch_body(ui: &mut Ui, preview: &PatchPreview, target: &mut PatchTarget, p: &Palette) -> bool {
        ui.label(RichText::new(tr!("Source : {name}", name = preview.name)).color(p.weak));
        ui.add_space(4.0);
        let n = preview.files.len();
        ui.label(RichText::new(trn!(n, "{n} fichier touché", "{n} fichiers touchés")).strong());
        egui::ScrollArea::vertical().id_salt("patch-files").max_height(160.0).auto_shrink([false, true]).show(
            ui,
            |ui| {
                for f in &preview.files {
                    let counts = match (f.added, f.removed) {
                        (Some(a), Some(r)) => format!("+{a}  -{r}"),
                        _ => tr!("binaire").to_string(),
                    };
                    ui.label(format!("{}    {counts}", f.path));
                }
            },
        );
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("Appliquer à")).color(p.weak));
        ui.radio_value(target, PatchTarget::Worktree, tr!("Copie de travail"));
        ui.radio_value(target, PatchTarget::Index, tr!("Index"));
        if preview.is_mail {
            ui.radio_value(target, PatchTarget::Am, tr!("Créer les commits (git am)"))
                .on_hover_text(tr!("Un commit par message du patch, avec son auteur et sa date d'origine."));
        } else if *target == PatchTarget::Am {
            *target = PatchTarget::Worktree;
        }
        ui.add_space(6.0);
        match preview.check_for(*target) {
            None => {
                ui.label(RichText::new(tr!("Le patch s'applique sans conflit.")).color(p.added_fg));
                true
            }
            Some(err) => {
                warning(ui, &tr!("Le patch ne s'applique pas : {err}", err), p);
                false
            }
        }
    }

    /// Corps du dialogue « Coller un patch » : une zone de texte où coller (Ctrl+V).
    pub(super) fn paste_patch_body(ui: &mut Ui, text: &mut String, p: &Palette) -> bool {
        ui.label(RichText::new(tr!("Collez le contenu d'un patch (diff ou format-patch).")).color(p.weak));
        egui::ScrollArea::vertical().id_salt("patch-paste").max_height(260.0).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .hint_text(tr!("Collez le patch ici (Ctrl+V)"))
                    .code_editor()
                    .desired_rows(12)
                    .desired_width(f32::INFINITY),
            )
            .request_focus();
        });
        !text.trim().is_empty()
    }

    pub(crate) fn op_apply_patch(&mut self, shared: &Shared, preview: Box<PatchPreview>, target: PatchTarget) {
        let refresh = if target == PatchTarget::Am { Refresh::ALL } else { Refresh::STATUS };
        self.run_simple(shared, tr!("Application du patch"), refresh, move |r| {
            let patch = &preview.patch;
            match target {
                PatchTarget::Worktree => r.apply_patch_to(patch, ApplyTarget::Worktree)?,
                PatchTarget::Index => r.apply_patch_to(patch, ApplyTarget::Index)?,
                PatchTarget::Am => r.am_patch(patch)?,
            }
            Ok(Some(tr!("Patch appliqué : {name}", name = preview.name)))
        });
    }
}
