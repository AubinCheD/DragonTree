//! Patchs de l'onglet : appliquer un patch venu d'un fichier, d'un glisser-déposer ou du texte collé.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dragontree_git::{ApplyTarget, GitError, Oid, PatchFile, Repo, is_mail_patch};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
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

/// Texte d'un patch pour le presse-papiers : refusé s'il n'est pas de l'UTF-8 valide (fichier binaire, autre
/// encodage), car la copie en altérerait les octets et le patch ne s'appliquerait plus.
fn patch_text(bytes: Vec<u8>) -> Result<String, GitError> {
    String::from_utf8(bytes).map_err(|_| {
        GitError::Refused(
            tr!("Ce patch contient des octets qui ne sont pas du texte UTF-8 : utilisez « Enregistrer comme patch… ».")
                .into(),
        )
    })
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

/// Ce dont on tire un patch (Enregistrer comme patch, Copier comme patch).
#[derive(Debug, Clone)]
pub enum PatchSource {
    /// Un commit (au format mail) ; le sujet nomme le fichier proposé.
    Commit(Oid, String),
    /// Les commits de celui-ci à HEAD, un fichier par commit dans un dossier.
    CommitsToHead(Oid),
    Stash(Oid),
    /// Fichiers de la vue Changements : suivis (indexés si `staged`) et non suivis.
    Files {
        tracked: Vec<String>,
        untracked: Vec<String>,
        staged: bool,
    },
}

impl PatchSource {
    /// Nom de fichier proposé.
    fn default_name(&self) -> String {
        match self {
            PatchSource::Commit(_, subject) => dragontree_git::export::patch_file_name(subject),
            PatchSource::Stash(_) => "stash.patch".into(),
            PatchSource::Files { .. } | PatchSource::CommitsToHead(_) => "modifications.patch".into(),
        }
    }

    /// Contenu du patch (commandes git : hors du thread de l'interface).
    fn read(&self, repo: &Repo) -> Result<Vec<u8>, GitError> {
        match self {
            PatchSource::Commit(id, _) => repo.commit_patch(*id),
            PatchSource::Stash(id) => repo.stash_patch(*id),
            PatchSource::Files { tracked, untracked, staged } => repo.files_patch(tracked, untracked, *staged),
            PatchSource::CommitsToHead(_) => Err(GitError::Internal("une plage s'écrit en plusieurs fichiers".into())),
        }
    }
}

impl RepoTab {
    /// Entrées « Enregistrer comme patch… » et « Copier comme patch » d'un menu contextuel.
    pub(super) fn patch_menu_entries(&mut self, ui: &mut Ui, shared: &mut Shared, source: PatchSource) {
        if ui.button(format!("{} {}", icons::FLOPPY_DISK, tr!("Enregistrer comme patch…"))).clicked() {
            self.save_patch(shared, source.clone());
            ui.close();
        }
        if ui.button(format!("{} {}", icons::COPY, tr!("Copier comme patch"))).clicked() {
            self.copy_patch(shared, source);
            ui.close();
        }
    }

    /// Choisit où enregistrer (fichier, ou dossier pour une plage de commits) puis écrit le patch.
    pub(super) fn save_patch(&mut self, shared: &Shared, source: PatchSource) {
        let dialog = rfd::FileDialog::new();
        let dest = match &source {
            PatchSource::CommitsToHead(_) => dialog.set_title(tr!("Dossier des patchs")).pick_folder(),
            _ => dialog
                .set_title(tr!("Enregistrer comme patch"))
                .set_file_name(source.default_name())
                .add_filter(tr!("Patchs"), &["patch", "diff"])
                .save_file(),
        };
        if let Some(dest) = dest {
            self.op_write_patch(shared, source, dest);
        }
    }

    /// Écrit le patch dans `dest` (un fichier, ou un dossier pour une plage) ; lecture et écriture en arrière-plan.
    pub(crate) fn op_write_patch(&mut self, shared: &Shared, source: PatchSource, dest: PathBuf) {
        self.run_simple(shared, tr!("Enregistrement du patch"), Refresh::NONE, move |r| {
            if let PatchSource::CommitsToHead(id) = &source {
                let commits = r.commits_to_head(*id)?;
                let files = r.commit_patches_to_dir(&commits, &dest)?;
                if files.is_empty() {
                    return Err(GitError::Refused(tr!("Aucun patch : ces commits sont des fusions.").into()));
                }
                let (n, dir) = (files.len(), dest.display().to_string());
                return Ok(Some(trn!(
                    n,
                    "{n} patch enregistré dans {dir} (les fusions ne sont pas exportées)",
                    "{n} patchs enregistrés dans {dir} (les fusions ne sont pas exportées)",
                    dir
                )));
            }
            std::fs::write(&dest, source.read(r)?)?;
            Ok(Some(tr!("Patch enregistré : {path}", path = dest.display())))
        });
    }

    /// Copie le patch dans le presse-papiers une fois lu (en arrière-plan : il peut être gros).
    pub(super) fn copy_patch(&mut self, shared: &Shared, source: PatchSource) {
        let Some(repo) = self.repo.clone() else { return };
        let id = self.id;
        shared.jobs.spawn("patch-copy", move || {
            let text = source.read(&repo).and_then(patch_text);
            Some(AppEvent::Repo(id, RepoEvent::PatchText(text)))
        });
    }

    pub(super) fn on_patch_text(&mut self, shared: &mut Shared, res: Result<String, GitError>) {
        match res {
            Ok(text) => {
                shared.ctx.copy_text(text);
                shared.toast(Toast::success(tr!("Patch copié dans le presse-papiers")));
            }
            Err(e) => shared.toast(Toast::error(tr!("Patch impossible"), e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn dropped_files_are_patches_by_extension() {
        assert!(is_patch_file(Path::new("a/ajout.patch")));
        assert!(is_patch_file(Path::new("CORRECTIF.DIFF")));
        assert!(!is_patch_file(Path::new("notes.txt")));
        assert!(!is_patch_file(Path::new("patch")));
    }

    #[test]
    fn a_patch_that_is_not_utf8_is_not_copied() {
        assert_eq!(patch_text(b"+ok\n".to_vec()).unwrap(), "+ok\n");
        assert!(patch_text(vec![b'+', 0x9f, 0x92, b'\n']).is_err());
    }

    #[test]
    fn the_proposed_file_name_follows_the_source() {
        let oid = Oid::from_hex(&"a".repeat(40)).unwrap();
        assert_eq!(PatchSource::Commit(oid, "Corrige l'accueil".into()).default_name(), "corrige-l-accueil.patch");
        assert_eq!(PatchSource::Stash(oid).default_name(), "stash.patch");
    }
}
