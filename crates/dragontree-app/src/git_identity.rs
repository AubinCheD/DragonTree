//! Configuration › Identité git… : nom et e-mail globaux (`git config --global`). Les valeurs actuelles sont lues en
//! tâche de fond à l'ouverture ; tant que la lecture n'a pas réussi, les champs restent grisés (un champ vide par
//! défaut ferait retirer la clé à l'enregistrement). Un dépôt qui a sa propre identité (Dépôt › Réglages du dépôt)
//! n'utilise pas celle-ci.

use dragontree_git::GitError;
use dragontree_git::identity::{self, Identity};
use dragontree_i18n::tr;
use egui::RichText;

use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;
use crate::toasts::Toast;

pub enum IdentityEvent {
    Loaded(u64, Result<Identity, GitError>),
    Saved(Result<Identity, GitError>),
}

pub struct GitIdentityWindow {
    open: bool,
    id: egui::Id,
    name: String,
    email: String,
    /// Valeurs lues avec succès : les champs ne sont modifiables qu'ensuite.
    loaded: bool,
    saving: bool,
    error: Option<String>,
    /// Numéro de la lecture en cours (une réponse périmée est ignorée).
    req: u64,
}

impl Default for GitIdentityWindow {
    fn default() -> Self {
        GitIdentityWindow {
            open: false,
            id: egui::Id::unique("git-identity-window"),
            name: String::new(),
            email: String::new(),
            loaded: false,
            saving: false,
            error: None,
            req: 0,
        }
    }
}

impl GitIdentityWindow {
    /// Ouvre la fenêtre et lit l'identité globale actuelle (hors du thread de l'interface).
    pub fn show(&mut self, shared: &Shared) {
        self.open = true;
        self.loaded = false;
        self.saving = false;
        self.error = None;
        self.req += 1;
        let Some(git) = shared.git.clone() else {
            self.error = Some(tr!("git est introuvable").into());
            return;
        };
        let req = self.req;
        shared.jobs.spawn("git-identity-read", move || {
            let result = identity::global_identity(&git, &std::env::temp_dir());
            Some(AppEvent::GitIdentity(IdentityEvent::Loaded(req, result)))
        });
    }

    pub fn handle(&mut self, ev: IdentityEvent, shared: &mut Shared) {
        match ev {
            IdentityEvent::Loaded(req, _) if req != self.req || !self.open => {}
            IdentityEvent::Loaded(_, Ok(id)) => {
                (self.name, self.email) = (id.name, id.email);
                self.loaded = true;
            }
            IdentityEvent::Loaded(_, Err(e)) => self.error = Some(e.to_string()),
            IdentityEvent::Saved(Ok(_)) => {
                self.open = false;
                self.saving = false;
                shared.toast(Toast::success(tr!("Identité git enregistrée")));
            }
            IdentityEvent::Saved(Err(e)) => {
                self.saving = false;
                self.error = Some(e.to_string());
            }
        }
    }

    fn save(&mut self, shared: &Shared) {
        let Some(git) = shared.git.clone() else { return };
        self.saving = true;
        self.error = None;
        let id = Identity { name: self.name.trim().to_string(), email: self.email.trim().to_string() };
        shared.jobs.spawn("git-identity-write", move || {
            let result = identity::set_global_identity(&git, &std::env::temp_dir(), &id).map(|()| id);
            Some(AppEvent::GitIdentity(IdentityEvent::Saved(result)))
        });
    }

    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) {
        if !self.open {
            return;
        }
        let mut open = true;
        let mut save = false;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::USER, tr!("Identité git")))
            .id(self.id)
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(tr!(
                        "Nom et e-mail de l'auteur des commits (git config --global). Un dépôt qui a sa propre identité (Dépôt › Réglages du dépôt) ne l'utilise pas."
                    ))
                    .color(p.weak)
                    .size(12.0),
                );
                ui.add_space(6.0);
                let editable = self.loaded && !self.saving;
                egui::Grid::new("git-identity-fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    let label = ui.label(tr!("Nom"));
                    ui.add_enabled(
                        editable,
                        egui::TextEdit::singleline(&mut self.name).hint_text("Alice Contoso").desired_width(320.0),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                    let label = ui.label("E-mail");
                    ui.add_enabled(
                        editable,
                        egui::TextEdit::singleline(&mut self.email)
                            .hint_text("alice@contoso.test")
                            .desired_width(320.0),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                });
                if let Some(error) = &self.error {
                    ui.label(RichText::new(format!("{} {error}", icons::WARNING)).color(p.removed_fg));
                } else if !self.loaded {
                    ui.label(RichText::new(tr!("Lecture de la configuration…")).color(p.weak));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(editable, egui::Button::new(tr!("Enregistrer"))).clicked() {
                        save = true;
                    }
                });
            });
        if save {
            self.save(shared);
        }
        if !open {
            self.open = false;
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }
}
