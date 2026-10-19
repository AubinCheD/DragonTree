//! Fenêtre « Clés SSH » : les paires du dossier `.ssh`, génération d'une clé, copie de la clé publique et choix
//! de la clé utilisée par git (`core.sshCommand`, global ou propre au dépôt actif). Tout accès au disque et à
//! git se fait hors du thread de l'interface.

use std::path::{Path, PathBuf};

use dragontree_git::ssh::{self, ConfigScope, KeyType, SshKey};
use dragontree_git::{Git, GitError};
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use crate::app::{AppEvent, Shared};
use crate::icons;
use crate::theme::Palette;
use crate::toasts::Toast;

/// Résultat d'une lecture du dossier `.ssh` et de la configuration git.
pub struct SshListing {
    keys: Vec<SshKey>,
    /// `core.sshCommand` global.
    global: Option<String>,
    /// `core.sshCommand` du dépôt actif (si le dossier de travail en est un).
    local: Option<String>,
    /// E-mail git global : commentaire proposé pour une nouvelle clé.
    email: Option<String>,
}

pub enum SshEvent {
    Listed {
        req: u64,
        repo: Option<PathBuf>,
        result: Result<SshListing, GitError>,
    },
    Generated(Result<SshKey, GitError>),
    /// Fin d'une écriture de `core.sshCommand` ; le texte est celui de la notification de succès.
    Configured {
        done: String,
        result: Result<(), GitError>,
    },
}

/// Changement de `core.sshCommand` qui écraserait une commande étrangère.
struct Pending {
    /// `Some` : utiliser cette clé ; `None` : revenir à la configuration par défaut.
    key: Option<SshKey>,
    scope: ConfigScope,
    current: String,
}

struct GenerateForm {
    key_type: KeyType,
    name: String,
    comment: String,
    passphrase: String,
    confirm: String,
    running: bool,
    error: Option<String>,
}

impl GenerateForm {
    fn new(comment: &str) -> GenerateForm {
        GenerateForm {
            key_type: KeyType::Ed25519,
            name: KeyType::Ed25519.default_file_name().to_string(),
            comment: comment.to_string(),
            passphrase: String::new(),
            confirm: String::new(),
            running: false,
            error: None,
        }
    }
}

pub struct SshKeysWindow {
    pub open: bool,
    id: egui::Id,
    /// Dossier `.ssh` lu et écrit (`~/.ssh`, sauf réglage de test dans `StartOptions`).
    ssh_dir: PathBuf,
    listing: Option<SshListing>,
    /// Dépôt pour lequel `listing` a été lu : une autre valeur déclenche une relecture.
    listed_for: Option<PathBuf>,
    loading: bool,
    next_req: u64,
    last_req: u64,
    error: Option<String>,
    form: Option<GenerateForm>,
    scope: ConfigScope,
    /// Dépôt actif (portée « ce dépôt »), mis à jour à chaque affichage.
    active_repo: Option<PathBuf>,
    started: bool,
    /// Remplacement ou retrait d'un `core.sshCommand` écrit à la main, en attente de confirmation.
    pending: Option<Pending>,
}

impl SshKeysWindow {
    pub fn new(ssh_dir: PathBuf) -> SshKeysWindow {
        SshKeysWindow {
            open: false,
            id: egui::Id::unique("ssh-keys-window"),
            ssh_dir,
            listing: None,
            listed_for: None,
            loading: false,
            next_req: 0,
            last_req: 0,
            error: None,
            form: None,
            scope: ConfigScope::Global,
            active_repo: None,
            started: false,
            pending: None,
        }
    }

    /// Ouvre la fenêtre : la liste est relue à l'affichage suivant.
    pub fn show(&mut self) {
        self.open = true;
        self.started = false;
    }

    /// Dossier de travail des commandes `git config` : le dépôt actif, sinon un dossier neutre.
    fn cwd(&self) -> PathBuf {
        self.active_repo.clone().unwrap_or_else(std::env::temp_dir)
    }

    fn refresh(&mut self, shared: &Shared) {
        let Some(git) = shared.git.clone() else {
            self.error = Some(tr!("git est introuvable").into());
            return;
        };
        self.next_req += 1;
        let req = self.next_req;
        self.last_req = req;
        self.loading = true;
        let (ssh_dir, repo, cwd) = (self.ssh_dir.clone(), self.active_repo.clone(), self.cwd());
        shared.jobs.spawn("ssh-list", move || {
            let result = read_listing(&git, &ssh_dir, repo.as_deref(), &cwd);
            Some(AppEvent::Ssh(SshEvent::Listed { req, repo, result }))
        });
    }

    pub fn handle(&mut self, ev: SshEvent, shared: &mut Shared) {
        match ev {
            SshEvent::Listed { req, repo, result } => {
                if req != self.last_req {
                    return;
                }
                self.loading = false;
                self.listed_for = repo;
                match result {
                    Ok(listing) => {
                        self.error = None;
                        self.listing = Some(listing);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            SshEvent::Generated(result) => {
                let Some(form) = &mut self.form else { return };
                form.running = false;
                match result {
                    Ok(key) => {
                        shared.toast(Toast::success(tr!("Clé créée : {name}", name = key.name)));
                        self.form = None;
                        self.refresh(shared);
                    }
                    Err(e) => form.error = Some(e.to_string()),
                }
            }
            SshEvent::Configured { done, result } => match result {
                Ok(()) => {
                    shared.toast(Toast::success(done));
                    self.refresh(shared);
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                }
            },
        }
    }

    /// Demande d'écrire (`Some(clé)`) ou de retirer (`None`) `core.sshCommand` dans la portée choisie : une
    /// valeur que DragonTree n'a pas écrite n'est touchée qu'après confirmation.
    fn request(&mut self, shared: &Shared, key: Option<SshKey>) {
        let scope = self.scope;
        let current = self.listing.as_ref().and_then(|l| match scope {
            ConfigScope::Global => l.global.clone(),
            ConfigScope::Local => l.local.clone(),
        });
        match current.filter(|value| ssh::key_from_ssh_command(value).is_none()) {
            Some(current) => self.pending = Some(Pending { key, scope, current }),
            None => self.configure(shared, key.as_ref(), scope, false),
        }
    }

    /// Confirmation (dialogue destructeur : Entrée ne valide pas, il faut cliquer).
    fn confirm_ui(&mut self, ctx: &egui::Context, shared: &Shared, p: &Palette) {
        let Some(pending) = &self.pending else { return };
        let (mut confirmed, mut cancelled) = (false, false);
        let (title, ok_label) = match &pending.key {
            Some(_) => (tr!("Remplacer la commande SSH existante ?"), tr!("Remplacer")),
            None => (tr!("Supprimer la commande SSH existante ?"), tr!("Supprimer")),
        };
        let response = egui::Modal::new(egui::Id::unique("ssh-confirm")).show(ctx, |ui| {
            ui.set_width(470.0);
            ui.label(RichText::new(title).strong().size(16.0));
            ui.add_space(10.0);
            ui.add(
                egui::Label::new(
                    RichText::new(format!(
                        "{} {}",
                        icons::WARNING,
                        tr!("core.sshCommand contient une commande qui n'a pas été écrite par DragonTree :")
                    ))
                    .color(p.modified_fg),
                )
                .wrap(),
            );
            ui.add(egui::Label::new(RichText::new(&pending.current).monospace()).wrap());
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = egui::Button::new(RichText::new(ok_label).strong().color(egui::Color32::WHITE))
                    .fill(p.removed_fg)
                    .min_size(egui::vec2(110.0, 28.0));
                if ui.add(ok).clicked() {
                    confirmed = true;
                }
                if ui.add(egui::Button::new(tr!("Annuler")).min_size(egui::vec2(90.0, 28.0))).clicked() {
                    cancelled = true;
                }
            });
        });
        if confirmed {
            if let Some(pending) = self.pending.take() {
                self.configure(shared, pending.key.as_ref(), pending.scope, true);
            }
        } else if cancelled || response.should_close() {
            self.pending = None;
        }
    }
    /// Écrit (`Some(clé)`) ou retire (`None`) `core.sshCommand` dans la portée choisie.
    fn configure(&mut self, shared: &Shared, key: Option<&SshKey>, scope: ConfigScope, confirmed: bool) {
        let Some(git) = shared.git.clone() else { return };
        let cwd = self.cwd();
        let key_path = key.map(|k| k.private_path.clone());
        let done = match (&key, scope) {
            (Some(k), ConfigScope::Global) => tr!("Clé « {name} » utilisée par git", name = k.name),
            (Some(k), ConfigScope::Local) => tr!("Clé « {name} » utilisée pour ce dépôt", name = k.name),
            (None, _) => tr!("Configuration SSH par défaut rétablie").to_string(),
        };
        self.error = None;
        shared.jobs.spawn("ssh-config", move || {
            let result = match &key_path {
                Some(path) => ssh::use_key(&git, scope, &cwd, path, confirmed),
                None => ssh::reset_key(&git, scope, &cwd, confirmed),
            };
            Some(AppEvent::Ssh(SshEvent::Configured { done, result }))
        });
    }

    fn generate(&mut self, shared: &Shared) {
        let (Some(form), Some(git)) = (&mut self.form, shared.git.clone()) else { return };
        form.running = true;
        form.error = None;
        let ssh_dir = self.ssh_dir.clone();
        let (key_type, name, comment, passphrase) =
            (form.key_type, form.name.clone(), form.comment.clone(), form.passphrase.clone());
        shared.jobs.spawn("ssh-generate", move || {
            let keygen = ssh::find_ssh_keygen(git.exe());
            let result = ssh::generate_key(&ssh_dir, &keygen, &name, key_type, &comment, &passphrase);
            Some(AppEvent::Ssh(SshEvent::Generated(result)))
        });
    }

    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, active_repo: Option<&Path>, p: &Palette) {
        if !self.open {
            return;
        }
        let repo_changed = self.active_repo.as_deref() != active_repo;
        self.active_repo = active_repo.map(Path::to_path_buf);
        if self.active_repo.is_none() {
            self.scope = ConfigScope::Global;
        }
        if !self.started || (repo_changed && self.listed_for != self.active_repo) {
            self.started = true;
            self.refresh(shared);
        }
        let mut open = true;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::KEY, tr!("Clés SSH")))
            .id(self.id)
            .open(&mut open)
            .default_size([680.0, 480.0])
            .show(ctx, |ui| self.contents(ui, shared, p));
        self.confirm_ui(ctx, shared, p);
        if !open {
            self.pending = None;
            self.open = false;
            self.form = None;
        }
    }

    fn contents(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        ui.label(
            RichText::new(tr!(
                "Les clés du dossier .ssh de votre profil. « Utiliser pour git » indique à git quelle clé présenter aux serveurs SSH."
            ))
            .color(p.weak),
        );
        ui.add_space(6.0);
        if let Some(err) = &self.error {
            ui.add(egui::Label::new(RichText::new(format!("{} {err}", icons::WARNING)).color(p.removed_fg)).wrap());
        }

        let mut copy = None;
        let mut use_key = None;
        if let Some(listing) = &self.listing {
            if listing.keys.is_empty() && !self.loading {
                ui.label(RichText::new(tr!("Aucune clé dans le dossier .ssh pour l'instant.")).color(p.weak));
            }
            let is_used = |command: &Option<String>, key: &SshKey| {
                command
                    .as_deref()
                    .and_then(ssh::key_from_ssh_command)
                    .is_some_and(|path| ssh::same_key_path(&key.private_path, &path))
            };
            for key in &listing.keys {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::KEY).size(20.0).color(p.accent));
                        ui.vertical(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(&key.name).strong());
                                ui.label(RichText::new(format!("{} {}", key.key_type, key.bits)).color(p.weak));
                                if is_used(&listing.global, key) {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} {}",
                                            icons::CHECK_CIRCLE,
                                            tr!("utilisée pour git (global)")
                                        ))
                                        .color(p.added_fg),
                                    );
                                }
                                if is_used(&listing.local, key) {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} {}",
                                            icons::CHECK_CIRCLE,
                                            tr!("utilisée pour git (ce dépôt)")
                                        ))
                                        .color(p.added_fg),
                                    );
                                }
                            });
                            if !key.comment.is_empty() {
                                ui.label(RichText::new(&key.comment).color(p.weak).size(12.0));
                            }
                            ui.label(RichText::new(&key.fingerprint).monospace().size(12.0));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr!("Utiliser pour git")).clicked() {
                                use_key = Some(key.clone());
                            }
                            if ui.button(tr!("Copier la clé publique")).clicked() {
                                copy = Some(key.clone());
                            }
                        });
                    });
                });
            }
        } else if self.loading {
            ui.spinner();
        }
        if let Some(key) = copy {
            ui.ctx().copy_text(key.public_key.clone());
            shared.toast(Toast::success(tr!("Clé publique de « {name} » copiée", name = key.name)));
        }
        if let Some(key) = use_key {
            self.request(shared, Some(key));
        }

        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(tr!("Utiliser pour :"));
            ui.radio_value(&mut self.scope, ConfigScope::Global, tr!("tous les dépôts (global)"));
            ui.add_enabled_ui(self.active_repo.is_some(), |ui| {
                ui.radio_value(&mut self.scope, ConfigScope::Local, tr!("ce dépôt seulement"));
            });
        });
        ui.add_space(4.0);
        let mut reset = false;
        let mut start_form = false;
        let mut reload = false;
        ui.horizontal(|ui| {
            if self.form.is_none() && ui.button(format!("{} {}", icons::PLUS, tr!("Générer une clé…"))).clicked() {
                start_form = true;
            }
            if ui.button(tr!("Revenir à la configuration par défaut")).clicked() {
                reset = true;
            }
            if ui.button(format!("{} {}", icons::ARROWS_CLOCKWISE, tr!("Actualiser"))).clicked() {
                reload = true;
            }
        });
        if start_form {
            let email = self.listing.as_ref().and_then(|l| l.email.clone()).unwrap_or_default();
            self.form = Some(GenerateForm::new(&email));
        }
        if reset {
            self.request(shared, None);
        }
        if reload {
            self.refresh(shared);
        }
        if self.form.is_some() {
            ui.add_space(8.0);
            self.form_ui(ui, shared, p);
        }
    }

    fn form_ui(&mut self, ui: &mut Ui, shared: &mut Shared, p: &Palette) {
        let (mut cancel, mut generate) = (false, false);
        let Some(form) = &mut self.form else { return };
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(tr!("Nouvelle clé")).strong().size(15.0));
            ui.add_space(6.0);
            egui::Grid::new("ssh-key-form").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                ui.label(tr!("Type"));
                ui.horizontal(|ui| {
                    let before = form.key_type;
                    ui.radio_value(&mut form.key_type, KeyType::Ed25519, "ed25519");
                    ui.radio_value(&mut form.key_type, KeyType::Rsa4096, "RSA 4096");
                    // Le nom proposé suit le type tant qu'il n'a pas été modifié à la main.
                    if form.key_type != before && form.name == before.default_file_name() {
                        form.name = form.key_type.default_file_name().to_string();
                    }
                });
                ui.end_row();
                ui.label(tr!("Nom du fichier"));
                ui.add(egui::TextEdit::singleline(&mut form.name).desired_width(320.0));
                ui.end_row();
                ui.label(tr!("Commentaire"));
                ui.add(egui::TextEdit::singleline(&mut form.comment).desired_width(320.0))
                    .on_hover_text(tr!("Repère affiché avec la clé publique, par exemple votre e-mail"));
                ui.end_row();
                ui.label(tr!("Phrase secrète"));
                ui.add(
                    egui::TextEdit::singleline(&mut form.passphrase)
                        .password(true)
                        .hint_text(tr!("facultative"))
                        .desired_width(320.0),
                );
                ui.end_row();
                ui.label(tr!("Confirmation"));
                ui.add(egui::TextEdit::singleline(&mut form.confirm).password(true).desired_width(320.0));
                ui.end_row();
            });
            let mismatch = form.passphrase != form.confirm;
            if mismatch {
                ui.label(RichText::new(tr!("Les deux phrases secrètes sont différentes.")).color(p.removed_fg));
            }
            if let Some(err) = &form.error {
                ui.add(egui::Label::new(RichText::new(format!("{} {err}", icons::WARNING)).color(p.removed_fg)).wrap());
            }
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = !form.name.trim().is_empty() && !mismatch && !form.running;
                let label = RichText::new(format!("{} {}", icons::CHECK, tr!("Générer"))).color(egui::Color32::WHITE);
                if ui.add_enabled(ok, egui::Button::new(label).fill(p.accent)).clicked() {
                    generate = true;
                }
                if form.running {
                    ui.spinner();
                }
                if ui.button(tr!("Annuler")).clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.form = None;
        } else if generate {
            self.generate(shared);
        }
    }
}

/// Lit les clés du dossier et la configuration git (hors thread de l'interface).
fn read_listing(git: &Git, ssh_dir: &Path, repo: Option<&Path>, cwd: &Path) -> Result<SshListing, GitError> {
    let keygen = ssh::find_ssh_keygen(git.exe());
    let keys = ssh::list_keys(ssh_dir, &keygen)?;
    let global = ssh::configured_ssh_command(git, ConfigScope::Global, cwd)?;
    let local = match repo {
        Some(_) => ssh::configured_ssh_command(git, ConfigScope::Local, cwd)?,
        None => None,
    };
    Ok(SshListing { keys, global, local, email: ssh::global_email(git, cwd) })
}
