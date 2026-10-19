//! Commandes personnalisées dans l'onglet (Réglages › Commandes) : sous-menu des menus contextuels (branches,
//! commits, fichiers), valeurs des variables selon l'élément cliqué, confirmation, exécution sur la file de l'onglet
//! (bandeau de progression et annulation comme une opération git) et fenêtre de sortie. La substitution des
//! variables et leur citation sont dans `crate::custom_commands`.

use dragontree_git::shell::{ShellOutput, run_shell};
use dragontree_git::{GitError, Oid};
use dragontree_i18n::tr;
use egui::{RichText, Ui};

use super::actions::OpOptions;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::custom_commands::{self, CommandKind, CommandTarget, Prepared, Vars};
use crate::icons;
use crate::theme::Palette;
use crate::toasts::Toast;

/// Lignes de la sortie reprises dans la notification d'échec.
const FAILURE_TAIL_LINES: usize = 12;

/// Élément sur lequel une commande est lancée : il donne leur valeur aux variables.
#[derive(Debug, Clone)]
pub(crate) enum Clicked {
    Repo,
    /// Branche locale, ou distante (`origin/x`) avec son remote.
    Branch {
        name: String,
        oid: Oid,
        remote: Option<String>,
    },
    Commit(Oid),
    /// Fichier (chemin relatif à la racine) de la vue Changements, ou des détails du commit `commit`.
    File {
        path: String,
        commit: Option<Oid>,
    },
}

impl Clicked {
    fn target(&self) -> CommandTarget {
        match self {
            Clicked::Repo => CommandTarget::Repo,
            Clicked::Branch { .. } => CommandTarget::Branch,
            Clicked::Commit(_) => CommandTarget::Commit,
            Clicked::File { .. } => CommandTarget::File,
        }
    }
}

/// Fin d'un script : sa sortie, ou l'erreur de lancement.
pub struct CustomOutcome {
    prepared: Prepared,
    result: Result<ShellOutput, GitError>,
}

/// Fenêtre « Sortie de … » : la ligne exécutée, son code de sortie et sa sortie.
pub struct CustomOutputWindow {
    name: String,
    line: String,
    code: i32,
    output: String,
}

impl RepoTab {
    /// Sous-menu « Commandes personnalisées » d'un menu contextuel : les commandes dont la cible est celle de
    /// l'élément ; rien quand il n'y en a pas.
    pub(super) fn custom_commands_menu(&mut self, ui: &mut Ui, shared: &mut Shared, clicked: Clicked) {
        let entries: Vec<(usize, String)> =
            custom_commands::for_target(&shared.settings.custom_commands, clicked.target())
                .map(|(i, c)| (i, c.name.trim().to_string()))
                .collect();
        if entries.is_empty() {
            return;
        }
        ui.separator();
        ui.menu_button(format!("{} {}", icons::TERMINAL_WINDOW, tr!("Commandes personnalisées")), |ui| {
            for (i, name) in entries {
                if ui.button(name).clicked() {
                    self.run_custom_command(shared, i, clicked.clone());
                    ui.close();
                }
            }
        });
    }

    /// Lance la commande `index` des réglages sur l'élément (après confirmation si elle le demande). Une valeur
    /// qui ne peut pas entrer dans la ligne (voir `custom_commands::expand_script`) est signalée, rien n'est lancé.
    pub(crate) fn run_custom_command(&mut self, shared: &mut Shared, index: usize, clicked: Clicked) {
        let Some(command) =
            shared.settings.custom_commands.get(index).filter(|c| c.target == clicked.target()).cloned()
        else {
            return;
        };
        match command.prepare(&self.custom_vars(&clicked)) {
            Ok(prepared) if command.confirm => self.dialog = Some(Dialog::CustomCommand(Box::new(prepared))),
            Ok(prepared) => self.launch_custom(shared, prepared),
            Err(e) => shared
                .toast(Toast::error(tr!("« {name} » n'a pas été lancée", name = command.name.trim()), e.to_string())),
        }
    }

    /// URL : ouverte dans le navigateur. Script : exécuté dans la copie de travail sur la file de l'onglet ; sa fin
    /// arrive par `RepoEvent::CustomDone` (l'annulation, par la fin de l'opération).
    pub(super) fn launch_custom(&mut self, shared: &mut Shared, prepared: Prepared) {
        if prepared.kind == CommandKind::Url {
            crate::util::open_url(&shared.ctx, &prepared.text);
            return;
        }
        let refresh = if prepared.refresh { Refresh::ALL } else { Refresh::NONE };
        let (sender, tab, title) = (shared.jobs.sender(), self.id, prepared.name.clone());
        self.run_op(
            shared,
            &title,
            refresh,
            OpOptions::default(),
            Box::new(move |r, net, _| {
                let result = run_shell(&r.workdir, &prepared.text, &prepared.env, &net.cancel);
                if result == Err(GitError::Cancelled) {
                    return Err(GitError::Cancelled);
                }
                sender.send(AppEvent::Repo(tab, RepoEvent::CustomDone(Box::new(CustomOutcome { prepared, result }))));
                Ok(None)
            }),
        );
    }

    /// Fin d'un script : notification (échec avec le code de sortie et la fin de la sortie), fenêtre de sortie si
    /// elle est demandée.
    pub(super) fn on_custom_done(&mut self, shared: &mut Shared, outcome: CustomOutcome) {
        let CustomOutcome { prepared, result } = outcome;
        let name = prepared.name;
        let out = match result {
            Ok(out) => out,
            Err(e) => {
                let detail = match e {
                    GitError::Spawn(msg) => msg,
                    other => other.to_string(),
                };
                shared.toast(Toast::error(tr!("« {name} » n'a pas pu être lancée", name), detail));
                return;
            }
        };
        let output = out.output.replace("\r\n", "\n");
        if out.code != 0 {
            let lines: Vec<&str> = output.trim_end().lines().collect();
            let tail = lines[lines.len().saturating_sub(FAILURE_TAIL_LINES)..].join("\n");
            shared.toast(Toast::error(tr!("« {name} » a échoué (code {code})", name, code = out.code), tail));
        } else if !prepared.show_output {
            shared.toast(Toast::success(tr!("« {name} » terminée", name)));
        }
        if prepared.show_output {
            self.custom_output = Some(CustomOutputWindow { name, line: prepared.text, code: out.code, output });
        }
    }

    /// Valeurs des variables pour l'élément : `${branch}` est la branche cliquée (`origin/x` pour une branche
    /// distante), `${sha}` son sommet, le commit cliqué ou celui des détails d'où vient le fichier ; à défaut, ceux de
    /// HEAD (branche vide en HEAD détachée, SHA vide sans commit). `${remote_url}` : URL du remote de la branche
    /// distante cliquée, sinon celle du remote par défaut.
    pub(crate) fn custom_vars(&self, clicked: &Clicked) -> Vars {
        let refs = self.refs.as_deref();
        let url_of = |remote: &str| self.remotes.iter().find(|r| r.name == remote).map(|r| r.fetch_url.clone());
        let mut vars = Vars {
            repo: self.workdir().display().to_string(),
            branch: refs.and_then(|r| r.current_branch()).unwrap_or_default().to_string(),
            sha: refs.and_then(|r| r.head_oid).map(|o| o.to_hex()).unwrap_or_default(),
            file: String::new(),
            remote_url: self.default_remote().and_then(|r| url_of(&r)).unwrap_or_default(),
        };
        match clicked {
            Clicked::Repo => {}
            Clicked::Branch { name, oid, remote } => {
                vars.branch = name.clone();
                vars.sha = oid.to_hex();
                if let Some(url) = remote.as_deref().and_then(url_of) {
                    vars.remote_url = url;
                }
            }
            Clicked::Commit(oid) => vars.sha = oid.to_hex(),
            Clicked::File { path, commit } => {
                vars.file = path.clone();
                if let Some(oid) = commit {
                    vars.sha = oid.to_hex();
                }
            }
        }
        vars
    }

    /// Corps du dialogue de confirmation : la ligne complète (ou l'URL), variables remplacées.
    pub(super) fn custom_command_body(&self, ui: &mut Ui, prepared: &Prepared, p: &Palette) -> bool {
        let intro = match prepared.kind {
            CommandKind::Script => tr!("Ligne exécutée dans {repo} :", repo = self.workdir().display()),
            CommandKind::Url => tr!("Adresse ouverte dans le navigateur :").to_string(),
        };
        ui.label(RichText::new(intro).color(p.weak));
        ui.add(egui::Label::new(RichText::new(&prepared.text).monospace().size(12.5)).selectable(true).wrap());
        true
    }

    /// Fenêtre de sortie du dernier script qui l'a demandée.
    pub(super) fn custom_output_window(&mut self, ctx: &egui::Context, p: &Palette) {
        let Some(window) = &self.custom_output else { return };
        let (mut open, mut close) = (true, false);
        let title = format!("{} {}", icons::TERMINAL_WINDOW, tr!("Sortie de « {name} »", name = window.name));
        crate::widgets::centered_window(ctx, title)
            .id(egui::Id::unique(("custom-output", self.id)))
            .open(&mut open)
            .default_size([640.0, 380.0])
            .show(ctx, |ui| {
                ui.add(egui::Label::new(RichText::new(&window.line).monospace().size(12.0).color(p.weak)).wrap());
                let color = if window.code == 0 { p.added_fg } else { p.removed_fg };
                ui.label(RichText::new(tr!("Code de sortie : {code}", code = window.code)).color(color));
                ui.separator();
                egui::ScrollArea::vertical().max_height(ui.available_height() - 36.0).auto_shrink([false, false]).show(
                    ui,
                    |ui| {
                        let text = window.output.trim_end();
                        if text.is_empty() {
                            ui.label(RichText::new(tr!("(aucune sortie)")).color(p.weak));
                        } else {
                            ui.add(egui::Label::new(RichText::new(text).monospace().size(12.0)).selectable(true));
                        }
                    },
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(format!("{} {}", icons::COPY, tr!("Copier"))).clicked() {
                        ui.ctx().copy_text(window.output.clone());
                    }
                    if ui.button(tr!("Fermer")).clicked() {
                        close = true;
                    }
                });
            });
        if !open || close {
            self.custom_output = None;
        }
    }
}
