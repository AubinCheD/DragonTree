//! Réglages du dépôt (Dépôt › Réglages du dépôt…) : identité propre au dépôt, modèle de message de commit et
//! liens vers les tickets. L'identité et le modèle sont dans la configuration git du dépôt (`--local`), les
//! règles de liens dans les réglages de l'application, par dépôt.

use dragontree_git::ssh::ConfigScope;
use dragontree_git::{GitError, Repo};
use dragontree_i18n::tr;
use egui::{RichText, Ui};
use regex::Regex;

use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::settings::TicketRule;
use crate::theme::Palette;

/// Clés git du dialogue (identité et modèle de message), toutes en `--local`.
const KEY_NAME: &str = "user.name";
const KEY_EMAIL: &str = "user.email";
const KEY_TEMPLATE: &str = "commit.template";

/// Contenu du dialogue : les valeurs saisies, et les valeurs globales de git affichées en indication.
#[derive(Debug, Clone, Default)]
pub struct RepoSettingsForm {
    pub name: String,
    pub email: String,
    pub global_name: Option<String>,
    pub global_email: Option<String>,
    /// Chemin du fichier modèle (`commit.template`).
    pub template: String,
    pub rules: Vec<TicketRule>,
    /// Clés dont la lecture a échoué, avec le message de git : leur champ est affiché grisé et n'est jamais
    /// écrit (un champ vide par défaut ferait retirer la clé à l'enregistrement).
    pub read_errors: Vec<(&'static str, String)>,
}

impl RepoSettingsForm {
    /// Lit la configuration git du dépôt (commandes git : hors du thread de l'interface).
    fn read(repo: &Repo) -> RepoSettingsForm {
        Self::read_with(|scope, key| repo.config_in(scope, key))
    }

    /// Construit le formulaire avec la fonction de lecture donnée (`git config --get` en vrai).
    fn read_with(get: impl Fn(ConfigScope, &str) -> Result<Option<String>, GitError>) -> RepoSettingsForm {
        let mut form = RepoSettingsForm::default();
        let mut local = |key: &'static str| match get(ConfigScope::Local, key) {
            Ok(value) => value.unwrap_or_default(),
            Err(e) => {
                form.read_errors.push((key, e.to_string()));
                String::new()
            }
        };
        let (name, email, template) = (local(KEY_NAME), local(KEY_EMAIL), local(KEY_TEMPLATE));
        // Les valeurs globales ne sont qu'une indication : une lecture manquée la laisse simplement absente.
        let global = |key| get(ConfigScope::Global, key).ok().flatten();
        RepoSettingsForm {
            name,
            email,
            template,
            global_name: global(KEY_NAME),
            global_email: global(KEY_EMAIL),
            read_errors: form.read_errors,
            rules: Vec::new(),
        }
    }

    pub fn read_error(&self, key: &str) -> Option<&str> {
        self.read_errors.iter().find(|(k, _)| *k == key).map(|(_, e)| e.as_str())
    }

    /// Valeurs à écrire dans la configuration du dépôt : toutes les clés dont la lecture a réussi.
    fn writes(&self) -> Vec<(&'static str, &str)> {
        [(KEY_NAME, &self.name), (KEY_EMAIL, &self.email), (KEY_TEMPLATE, &self.template)]
            .into_iter()
            .filter(|(key, _)| self.read_error(key).is_none())
            .map(|(key, value)| (key, value.as_str()))
            .collect()
    }

    /// Un motif non vide ne se compile pas : la règle serait ignorée à l'affichage, on refuse de l'enregistrer.
    pub fn invalid_pattern(&self) -> Option<&str> {
        self.rules.iter().map(|r| r.pattern.trim()).find(|p| !p.is_empty() && Regex::new(p).is_err())
    }
}

/// Modèle sans ses lignes de commentaire (`#`) : `git commit -F -` ne les retire pas, la zone de commit montre
/// donc ce qui sera réellement enregistré (comme pour MERGE_MSG).
pub fn clean_template(template: &str) -> String {
    template.lines().filter(|l| !l.starts_with('#')).collect::<Vec<_>>().join("\n")
}

/// Place le modèle dans la zone de commit quand elle est vide ou ne contient que le modèle précédent, intact
/// (`prefill` : ce qu'on y a mis) ; un message que l'utilisateur a commencé n'est jamais touché. Sans modèle,
/// un ancien modèle intact est retiré. Rend vrai si la zone a changé.
pub fn apply_template(
    subject: &mut String,
    body: &mut String,
    prefill: &mut Option<(String, String)>,
    template: Option<&str>,
) -> bool {
    let untouched = prefill.as_ref().is_some_and(|(s, b)| s == subject && b == body);
    if !(subject.is_empty() && body.is_empty() || untouched) {
        return false;
    }
    let cleaned = template.map(clean_template).filter(|t| !t.trim().is_empty());
    let (new_subject, new_body) = cleaned.as_deref().map(super::split_message).unwrap_or_default();
    let changed = *subject != new_subject || *body != new_body;
    *prefill = cleaned.map(|_| (new_subject.clone(), new_body.clone()));
    (*subject, *body) = (new_subject, new_body);
    changed
}

/// Écrit ou retire (valeur vide) une clé `--local`.
fn write_local(repo: &Repo, key: &str, value: &str) -> Result<(), GitError> {
    let value = value.trim();
    if value.is_empty() { repo.unset_config_local(key) } else { repo.set_config_local(key, value) }
}

impl RepoTab {
    /// Lit la configuration du dépôt en tâche de fond ; le résultat ouvre le dialogue.
    pub(crate) fn open_repo_settings(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        self.gens.repo_settings += 1;
        let (g, id) = (self.gens.repo_settings, self.id);
        shared.jobs.spawn("repo-settings", move || {
            Some(AppEvent::Repo(id, RepoEvent::RepoSettings(g, Box::new(RepoSettingsForm::read(&repo)))))
        });
    }

    pub(super) fn on_repo_settings(&mut self, shared: &Shared, mut form: Box<RepoSettingsForm>) {
        // Un dialogue déjà ouvert garde la main.
        if self.dialog.is_none() {
            form.rules = shared.settings.ticket_rules_of(&self.path).to_vec();
            self.dialog = Some(Dialog::RepoSettings(form));
        }
    }

    pub(crate) fn save_repo_settings(&mut self, shared: &mut Shared, form: Box<RepoSettingsForm>) {
        shared.settings.set_ticket_rules(&self.path, form.rules.clone());
        shared.settings_dirty = true;
        self.run_simple(shared, tr!("Réglages du dépôt"), Refresh::NONE, move |r| {
            for (key, value) in form.writes() {
                write_local(r, key, value)?;
            }
            Ok(Some(tr!("Réglages du dépôt enregistrés").into()))
        });
        // Le modèle a pu changer : relu derrière l'écriture sur la même file, la zone de commit vide le reprend.
        let Some(repo) = self.repo.clone() else { return };
        self.gens.template += 1;
        let (g, id, sender) = (self.gens.template, self.id, shared.jobs.sender());
        self.worker.submit(move || {
            sender.send(AppEvent::Repo(id, RepoEvent::CommitTemplate(g, repo.commit_template())));
        });
    }

    /// Lit le modèle de message en tâche de fond pour préremplir la zone de commit (voir [`apply_template`]).
    pub(crate) fn load_commit_template(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        self.gens.template += 1;
        let (g, id) = (self.gens.template, self.id);
        shared.jobs.spawn("commit-template", move || {
            Some(AppEvent::Repo(id, RepoEvent::CommitTemplate(g, repo.commit_template())))
        });
    }

    pub(super) fn on_commit_template(&mut self, template: Option<String>) {
        let c = &mut self.changes;
        if !c.amend && self.op_state == dragontree_git::OpState::None {
            apply_template(&mut c.subject, &mut c.body, &mut c.template_prefill, template.as_deref());
        }
    }

    /// Champs du dialogue ; renvoie vrai si la saisie est valide.
    pub(super) fn repo_settings_body(ui: &mut Ui, form: &mut RepoSettingsForm, p: &Palette) -> bool {
        let (name_locked, email_locked, template_locked) = (
            form.read_error(KEY_NAME).is_some(),
            form.read_error(KEY_EMAIL).is_some(),
            form.read_error(KEY_TEMPLATE).is_some(),
        );
        for (key, error) in &form.read_errors {
            super::dialogs::warning(
                ui,
                &tr!("Lecture de {key} impossible, champ non modifiable : {error}", key, error),
                p,
            );
        }
        ui.label(RichText::new(tr!("Identité de ce dépôt")).strong());
        ui.label(RichText::new(tr!("Vide : le dépôt utilise l'identité globale de git.")).color(p.weak).size(12.0));
        egui::Grid::new("repo-settings-identity").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            let unset = tr!("(non défini)");
            ui.label(tr!("Nom"));
            let hint = form.global_name.as_deref().unwrap_or(unset);
            ui.add_enabled(
                !name_locked,
                egui::TextEdit::singleline(&mut form.name).hint_text(hint).desired_width(360.0),
            );
            ui.end_row();
            ui.label("E-mail");
            let hint = form.global_email.as_deref().unwrap_or(unset);
            ui.add_enabled(
                !email_locked,
                egui::TextEdit::singleline(&mut form.email).hint_text(hint).desired_width(360.0),
            );
            ui.end_row();
        });

        ui.add_space(10.0);
        ui.label(RichText::new(tr!("Modèle de message de commit")).strong());
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut form.template)
                .hint_text(tr!("Fichier modèle (commit.template)"))
                .desired_width(360.0);
            ui.add_enabled(!template_locked, edit);
            if ui.add_enabled(!template_locked, egui::Button::new(tr!("Parcourir…"))).clicked()
                && let Some(file) = rfd::FileDialog::new().set_title(tr!("Choisir un modèle de message")).pick_file()
            {
                form.template = file.display().to_string().replace('\\', "/");
            }
        });
        ui.label(RichText::new(tr!("Il préremplit la zone de commit quand elle est vide.")).color(p.weak).size(12.0));

        ui.add_space(10.0);
        ui.label(RichText::new(tr!("Liens vers les tickets")).strong());
        ui.label(
            RichText::new(tr!("Motif (expression régulière) → URL ; $1 reprend le premier groupe du motif."))
                .color(p.weak)
                .size(12.0),
        );
        let mut remove = None;
        egui::Grid::new("repo-settings-rules").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
            for (i, rule) in form.rules.iter_mut().enumerate() {
                let bad = !rule.pattern.trim().is_empty() && Regex::new(rule.pattern.trim()).is_err();
                let mut pattern =
                    egui::TextEdit::singleline(&mut rule.pattern).hint_text(r"#(\d+)").desired_width(140.0);
                if bad {
                    pattern = pattern.text_color(p.removed_fg);
                }
                ui.add(pattern);
                ui.add(
                    egui::TextEdit::singleline(&mut rule.url)
                        .hint_text("https://tickets.contoso.test/$1")
                        .desired_width(300.0),
                );
                if crate::widgets::icon_button(ui, icons::TRASH, tr!("Retirer la règle")).clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            form.rules.remove(i);
        }
        if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter une règle"))).clicked() {
            form.rules.push(TicketRule::default());
        }
        match form.invalid_pattern() {
            Some(pattern) => {
                super::dialogs::warning(ui, &tr!("Motif invalide : {pattern}", pattern), p);
                false
            }
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(subject: &str, body: &str) -> (String, String) {
        (subject.to_string(), body.to_string())
    }

    #[test]
    fn the_template_only_fills_an_empty_message() {
        let ((mut subject, mut body), mut prefill) = (strings("", ""), None);
        assert!(apply_template(&mut subject, &mut body, &mut prefill, Some("feat: \n\nRefs #\n")));
        assert_eq!((subject.as_str(), body.as_str()), ("feat:", "Refs #"));
        assert_eq!(prefill, Some(strings("feat:", "Refs #")));

        // Un message commencé n'est jamais écrasé, ni par le sujet ni par le corps, même si le modèle change.
        let ((mut subject, mut body), mut prefill) = (strings("Mon sujet", ""), None);
        assert!(!apply_template(&mut subject, &mut body, &mut prefill, Some("feat: \n")));
        assert_eq!(subject, "Mon sujet");
        let ((mut subject, mut body), mut prefill) = (strings("", "Corps seul"), None);
        assert!(!apply_template(&mut subject, &mut body, &mut prefill, Some("feat: \n")));
        assert_eq!((subject.as_str(), body.as_str()), ("", "Corps seul"));
    }

    #[test]
    fn an_untouched_template_follows_the_template_but_an_edited_one_does_not() {
        let ((mut subject, mut body), mut prefill) = (strings("", ""), None);
        apply_template(&mut subject, &mut body, &mut prefill, Some("feat: a\n\nun"));
        // Nouveau modèle : la zone, intacte, le reprend.
        assert!(apply_template(&mut subject, &mut body, &mut prefill, Some("fix: b\n\ndeux")));
        assert_eq!((subject.as_str(), body.as_str()), ("fix: b", "deux"));
        // Modèle retiré : la zone intacte est vidée.
        assert!(apply_template(&mut subject, &mut body, &mut prefill, None));
        assert!(subject.is_empty() && body.is_empty() && prefill.is_none());
        // Modifiée par l'utilisateur : plus jamais touchée.
        apply_template(&mut subject, &mut body, &mut prefill, Some("feat: a"));
        subject.push_str(" (édité)");
        assert!(!apply_template(&mut subject, &mut body, &mut prefill, Some("fix: b")));
        assert!(!apply_template(&mut subject, &mut body, &mut prefill, None));
        assert_eq!(subject, "feat: a (édité)");
    }

    #[test]
    fn comment_lines_are_stripped_from_the_template() {
        assert_eq!(clean_template("feat: \n# Expliquez le changement\n\nCorps\n#fin"), "feat: \n\nCorps");
        let ((mut subject, mut body), mut prefill) = (strings("", ""), None);
        apply_template(&mut subject, &mut body, &mut prefill, Some("# Message\nfeat: x\n# aide\n\nCorps"));
        // La première ligne conservée devient le sujet ; plus aucun « # » n'est enregistré.
        assert_eq!((subject.as_str(), body.as_str()), ("feat: x", "Corps"));
        // Un modèle fait seulement de commentaires est un modèle vide.
        let ((mut subject, mut body), mut prefill) = (strings("", ""), None);
        assert!(!apply_template(&mut subject, &mut body, &mut prefill, Some("# rien\n# du tout\n")));
        assert!(subject.is_empty() && prefill.is_none());
    }

    #[test]
    fn an_unreadable_key_locks_its_field_and_is_never_written() {
        let failing = |scope: ConfigScope, key: &str| match (scope, key) {
            (ConfigScope::Local, KEY_EMAIL) => {
                Err(GitError::Failed { cmd: "git config".into(), code: 128, stderr: "bad config".into() })
            }
            (ConfigScope::Local, KEY_NAME) => Ok(Some("Alice".to_string())),
            _ => Ok(None),
        };
        let form = RepoSettingsForm::read_with(failing);
        assert_eq!(form.name, "Alice");
        assert!(form.read_error(KEY_EMAIL).is_some_and(|e| e.contains("bad config")));
        assert!(form.read_error(KEY_NAME).is_none());
        // Le champ e-mail resté vide ne retirera pas la clé : il n'est pas dans les écritures.
        assert_eq!(form.writes(), vec![(KEY_NAME, "Alice"), (KEY_TEMPLATE, "")]);
    }

    #[test]
    fn an_invalid_pattern_is_reported_but_blank_rules_are_not() {
        let rule = |pattern: &str| TicketRule { pattern: pattern.into(), url: "https://x.test".into() };
        let mut form = RepoSettingsForm { rules: vec![rule(""), rule(r"#(\d+)")], ..Default::default() };
        assert_eq!(form.invalid_pattern(), None);
        form.rules.push(rule("(oups"));
        assert_eq!(form.invalid_pattern(), Some("(oups"));
    }
}
