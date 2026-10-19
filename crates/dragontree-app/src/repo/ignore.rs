//! Dialogue « Ignorer des fichiers » : motif (proposé ou libre), fichier qui le reçoit, et aperçu en direct des
//! fichiers concernés, calculé en tâche de fond.

use dragontree_git::{GitError, IgnorePreview, IgnoreTarget};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;

/// Fichiers suivis nommés dans l'avertissement (le total est donné en chiffre).
const TRACKED_SHOWN: usize = 5;

/// Fichier cible choisi dans le dialogue ; `Dir` désigne le dossier du fichier sur lequel on a ouvert le dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreWhere {
    Root,
    Dir,
    Exclude,
}

/// État du dialogue. `asked` : le motif et la cible dont l'aperçu a été demandé (le dernier calcul lancé) ;
/// `preview` : son résultat, ou `error` si git a échoué.
#[derive(Debug, Clone)]
pub struct IgnoreForm {
    /// Fichier cliqué (chemin relatif au dépôt), qui fournit les motifs proposés et le dossier de `.gitignore`.
    pub file: Option<String>,
    pub pattern: String,
    pub target: IgnoreWhere,
    pub asked: Option<(String, IgnoreTarget)>,
    pub preview: Option<IgnorePreview>,
    pub error: Option<String>,
}

impl IgnoreForm {
    pub fn new(file: Option<String>) -> IgnoreForm {
        IgnoreForm { file, pattern: String::new(), target: IgnoreWhere::Root, asked: None, preview: None, error: None }
    }

    /// Dossier du fichier (relatif au dépôt), s'il n'est pas à la racine.
    fn folder(&self) -> Option<&str> {
        self.file.as_deref().and_then(|f| f.rsplit_once('/')).map(|(dir, _)| dir)
    }

    /// Cible demandée ; `Dir` sans dossier retombe sur la racine.
    pub fn target(&self) -> IgnoreTarget {
        match (self.target, self.folder()) {
            (IgnoreWhere::Dir, Some(dir)) => IgnoreTarget::Dir(dir.to_string()),
            (IgnoreWhere::Exclude, _) => IgnoreTarget::InfoExclude,
            _ => IgnoreTarget::Root,
        }
    }

    /// Un motif peut être ajouté.
    pub fn is_valid(&self) -> bool {
        !self.pattern.trim().is_empty()
    }
}

/// Motifs proposés pour `file` : le fichier, `*.ext`, le dossier. Pour un `.gitignore` de dossier, les chemins se
/// lisent à partir de ce dossier (le fichier seul, pas le dossier lui-même).
pub fn suggestions(file: &str, target: &IgnoreTarget) -> Vec<String> {
    let local = match target {
        IgnoreTarget::Dir(dir) => file.strip_prefix(&format!("{dir}/")).unwrap_or(file),
        _ => file,
    };
    let mut list = vec![local.to_string()];
    let name = local.rsplit('/').next().unwrap_or(local);
    if let Some((stem, ext)) = name.rsplit_once('.')
        && !stem.is_empty()
    {
        list.push(format!("*.{ext}"));
    }
    if !matches!(target, IgnoreTarget::Dir(_))
        && let Some((dir, _)) = file.rsplit_once('/')
    {
        list.push(format!("{dir}/"));
    }
    list
}

fn target_file_label(target: &IgnoreTarget) -> String {
    match target {
        IgnoreTarget::Root => ".gitignore".into(),
        IgnoreTarget::Dir(dir) => format!("{dir}/.gitignore"),
        IgnoreTarget::InfoExclude => ".git/info/exclude".into(),
    }
}

impl RepoTab {
    /// Ouvre le dialogue, sur un fichier de la vue Changements ou sans (Dépôt › Ignorer des fichiers…).
    pub(super) fn open_ignore(&mut self, file: Option<String>) {
        self.dialog = Some(Dialog::Ignore(Box::new(IgnoreForm::new(file))));
    }

    /// Lance le calcul de l'aperçu quand le motif ou la cible a changé depuis le dernier calcul ; une réponse
    /// d'un calcul périmé est ignorée (génération).
    pub(super) fn request_ignore_preview(&mut self, shared: &Shared, form: &mut IgnoreForm) {
        let wanted = (form.pattern.trim().to_string(), form.target());
        if form.asked.as_ref() == Some(&wanted) {
            return;
        }
        form.asked = Some(wanted.clone());
        form.preview = None;
        form.error = None;
        self.gens.ignore += 1;
        let (g, id) = (self.gens.ignore, self.id);
        let (Some(repo), (pattern, target)) = (self.repo.clone(), wanted) else { return };
        if pattern.is_empty() {
            form.preview = Some(IgnorePreview::default());
            return;
        }
        shared.jobs.spawn("ignore-preview", move || {
            let result = repo.ignore_preview(&pattern, &target);
            Some(AppEvent::Repo(id, RepoEvent::IgnorePreview(g, pattern, target, result)))
        });
    }

    /// Aperçu calculé : il est gardé seulement s'il correspond toujours à ce qui est saisi dans le dialogue.
    pub(super) fn on_ignore_preview(
        &mut self,
        pattern: String,
        target: IgnoreTarget,
        result: Result<IgnorePreview, GitError>,
    ) {
        let Some(Dialog::Ignore(form)) = &mut self.dialog else { return };
        if form.asked.as_ref() != Some(&(pattern, target)) {
            return;
        }
        match result {
            Ok(preview) => form.preview = Some(preview),
            Err(e) => form.error = Some(e.stderr_summary()),
        }
    }

    /// Champs du dialogue ; renvoie vrai si la saisie est valide.
    pub(super) fn ignore_body(ui: &mut Ui, form: &mut IgnoreForm, p: &Palette) -> bool {
        if let Some(file) = form.file.clone() {
            ui.label(RichText::new(tr!("Motifs proposés pour {file} :", file)).color(p.weak));
            ui.horizontal_wrapped(|ui| {
                for suggestion in suggestions(&file, &form.target()) {
                    if ui.selectable_label(form.pattern.trim() == suggestion, &suggestion).clicked() {
                        form.pattern = suggestion;
                    }
                }
            });
        }
        let field = ui.add(
            egui::TextEdit::singleline(&mut form.pattern)
                .hint_text(tr!("Motif (ex. *.log, build/, /config.local)"))
                .desired_width(f32::INFINITY),
        );
        // Le focus n'est pris qu'à l'ouverture (aucun aperçu encore demandé) : pas de blocage de Tab ensuite.
        if form.asked.is_none() {
            field.request_focus();
        }
        ui.add_space(4.0);
        ui.label(tr!("Ajouter à :"));
        ui.radio_value(&mut form.target, IgnoreWhere::Root, ".gitignore");
        let folder = form.folder().map(str::to_string);
        if let Some(dir) = folder {
            let label = tr!("{dir}/.gitignore (propre à ce dossier)", dir);
            ui.radio_value(&mut form.target, IgnoreWhere::Dir, label);
        } else if form.target == IgnoreWhere::Dir {
            form.target = IgnoreWhere::Root;
        }
        ui.radio_value(&mut form.target, IgnoreWhere::Exclude, tr!(".git/info/exclude (local, jamais commité)"));
        ui.add_space(6.0);
        Self::ignore_preview_ui(ui, form, p);
        form.is_valid()
    }

    fn ignore_preview_ui(ui: &mut Ui, form: &IgnoreForm, p: &Palette) {
        if !form.is_valid() {
            return;
        }
        if let Some(error) = &form.error {
            warning(ui, &tr!("Aperçu impossible : {error}", error), p);
            return;
        }
        let Some(preview) = &form.preview else {
            ui.label(RichText::new(tr!("Calcul de l'aperçu…")).color(p.weak));
            return;
        };
        if preview.untracked_total == 0 {
            ui.label(RichText::new(tr!("Aucun fichier non suivi n'est concerné.")).color(p.weak));
        } else {
            let n = preview.untracked_total;
            ui.label(
                RichText::new(trn!(
                    n,
                    "{n} fichier non suivi sera ignoré :",
                    "{n} fichiers non suivis seront ignorés :"
                ))
                .strong(),
            );
            egui::ScrollArea::vertical().max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                for path in &preview.untracked {
                    ui.label(format!("{} {path}", icons::FILE));
                }
                if n > preview.untracked.len() {
                    let more = n - preview.untracked.len();
                    ui.label(RichText::new(tr!("… et {more} autres", more)).color(p.weak));
                }
            });
        }
        if preview.tracked_total > 0 {
            let n = preview.tracked_total;
            let names = preview.tracked.iter().take(TRACKED_SHOWN).cloned().collect::<Vec<_>>().join(", ");
            warning(
                ui,
                &trn!(
                    n,
                    "{n} fichier déjà suivi correspond ({names}) : il restera suivi (git rm --cached pour l'ignorer vraiment).",
                    "{n} fichiers déjà suivis correspondent ({names}) : ils resteront suivis (git rm --cached pour les ignorer vraiment).",
                    names
                ),
                p,
            );
        }
    }

    pub(super) fn op_add_ignore(&mut self, shared: &Shared, pattern: String, target: IgnoreTarget) {
        self.run_simple(shared, tr!("Ignorer"), Refresh::STATUS, move |r| {
            r.add_ignore_pattern(&pattern, &target)?;
            let file = target_file_label(&target);
            Ok(Some(tr!("« {pattern} » ajouté à {file}", pattern = pattern.trim(), file)))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggestions_are_the_file_its_extension_and_its_folder() {
        assert_eq!(suggestions("sub/dir/a.log", &IgnoreTarget::Root), ["sub/dir/a.log", "*.log", "sub/dir/"]);
        assert_eq!(suggestions("a.log", &IgnoreTarget::Root), ["a.log", "*.log"]);
        assert_eq!(suggestions("Makefile", &IgnoreTarget::Root), ["Makefile"]);
        assert_eq!(suggestions(".env", &IgnoreTarget::Root), [".env"]);
    }

    #[test]
    fn for_a_folder_gitignore_paths_start_at_that_folder() {
        let dir = IgnoreTarget::Dir("sub/dir".into());
        assert_eq!(suggestions("sub/dir/a.log", &dir), ["a.log", "*.log"]);
    }

    #[test]
    fn the_folder_target_falls_back_to_the_root_without_a_folder() {
        let mut form = IgnoreForm::new(Some("a.log".into()));
        form.target = IgnoreWhere::Dir;
        assert_eq!(form.target(), IgnoreTarget::Root);
        let mut form = IgnoreForm::new(Some("sub/a.log".into()));
        form.target = IgnoreWhere::Dir;
        assert_eq!(form.target(), IgnoreTarget::Dir("sub".into()));
    }
}
