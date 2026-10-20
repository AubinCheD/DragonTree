//! Fenêtre des réglages.

use std::path::PathBuf;

use dragontree_git::Git;
use dragontree_i18n::{Lang, tr, trn};
use egui::RichText;

use crate::app::{AppAction, Shared};
use crate::custom_commands::{self, CommandKind, CommandTarget, CustomCommand};
use crate::external_tools::{self, DetectedTool, ToolChoice, ToolKind, ToolPreset};
use crate::icons;
use crate::settings::{Settings, ThemeChoice, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP, path_key};
use crate::theme::Palette;

/// Onglet de la fenêtre des réglages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    /// Commandes personnalisées.
    Commands,
}

pub struct SettingsWindow {
    tab: SettingsTab,
    draft: Settings,
    skip_dirs: String,
    git_path: String,
    git_version: Option<Result<String, String>>,
}

impl SettingsWindow {
    pub fn new(settings: &Settings) -> SettingsWindow {
        SettingsWindow {
            tab: SettingsTab::General,
            skip_dirs: settings.scan_skip_dirs.join(", "),
            git_path: settings.git_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            draft: settings.clone(),
            git_version: None,
        }
    }

    pub fn show_tab(&mut self, tab: SettingsTab) {
        self.tab = tab;
    }

    /// Renvoie faux quand la fenêtre est fermée.
    pub fn ui(&mut self, ctx: &egui::Context, shared: &mut Shared, p: &Palette) -> bool {
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::GEAR_SIX, tr!("Réglages")))
            .id(egui::Id::unique("settings-window"))
            .open(&mut open)
            .default_size([640.0, 620.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, SettingsTab::General, tr!("Général"));
                    ui.selectable_value(&mut self.tab, SettingsTab::Commands, tr!("Commandes"));
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(ui.available_height() - 44.0).show(ui, |ui| match self.tab {
                    SettingsTab::General => self.general_tab(ui, shared, p),
                    SettingsTab::Commands => self.commands_tab(ui, p),
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(tr!("Enregistrer")).strong().color(egui::Color32::WHITE),
                                )
                                .fill(p.accent),
                            )
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button(tr!("Annuler")).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if save {
            let mut d = self.draft.clone();
            d.scan_skip_dirs =
                self.skip_dirs.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
            d.git_path = (!self.git_path.trim().is_empty()).then(|| PathBuf::from(self.git_path.trim()));
            // Une commande laissée vide est retirée ; les autres sont gardées telles que saisies, sans blancs autour.
            d.custom_commands.retain(|c| !c.name.trim().is_empty() || !c.command.trim().is_empty());
            for c in &mut d.custom_commands {
                c.name = c.name.trim().to_string();
                c.command = c.command.trim().to_string();
            }
            d.custom_diff_tool = d.custom_diff_tool.trim().to_string();
            d.custom_merge_tool = d.custom_merge_tool.trim().to_string();
            // Les listes modifiées ailleurs pendant l'édition (onglets, comptes) sont conservées.
            d.open_tabs = shared.settings.open_tabs.clone();
            d.active_tab = shared.settings.active_tab;
            d.recent_repos = shared.settings.recent_repos.clone();
            d.accounts = shared.settings.accounts.clone();
            d.repo_accounts = shared.settings.repo_accounts.clone();
            d.favorites = shared.settings.favorites.clone();
            let rescan = d.scan_roots != shared.settings.scan_roots
                || d.scan_max_depth != shared.settings.scan_max_depth
                || d.scan_skip_dirs != shared.settings.scan_skip_dirs
                || d.scan_deep != shared.settings.scan_deep
                || d.hidden_repos != shared.settings.hidden_repos;
            if d.git_path != shared.settings.git_path {
                shared.git = match &d.git_path {
                    Some(p) => Some(Git::with_exe(p.clone())),
                    None => Git::discover().ok(),
                };
            }
            shared.settings = d;
            shared.save_settings();
            if rescan {
                shared.actions.push(AppAction::Rescan);
            }
            return false;
        }
        open && !cancel
    }

    /// Onglet « Général » : apparence, git, liste des dépôts, clonage, diff, réseau, comptes.
    fn general_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared, p: &Palette) {
        let d = &mut self.draft;
        section(ui, tr!("Apparence"));
        ui.horizontal(|ui| {
            ui.label(tr!("Thème"));
            ui.selectable_value(&mut d.theme, ThemeChoice::System, tr!("Système"));
            ui.selectable_value(&mut d.theme, ThemeChoice::Light, format!("{} {}", icons::SUN, tr!("Clair")));
            ui.selectable_value(&mut d.theme, ThemeChoice::Dark, format!("{} {}", icons::MOON, tr!("Sombre")));
        });
        ui.horizontal(|ui| {
            ui.label(tr!("Langue"));
            ui.selectable_value(&mut d.language, None, tr!("Système"));
            for lang in Lang::ALL {
                ui.selectable_value(&mut d.language, Some(lang), lang.native_name());
            }
        });
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Taille de l'interface"));
            ui.add(
                egui::Slider::new(&mut d.ui_scale, UI_SCALE_MIN..=UI_SCALE_MAX)
                    .step_by(f64::from(UI_SCALE_STEP))
                    .custom_formatter(|v, _| tr!("{percent} %", percent = format!("{:.0}", v * 100.0))),
            )
            .labelled_by(label.id);
        });

        section(ui, "Git");
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Exécutable git"));
            ui.add(
                egui::TextEdit::singleline(&mut self.git_path)
                    .hint_text(tr!("(détecté automatiquement)"))
                    .desired_width(320.0),
            )
            .labelled_by(label.id);
            if ui.button(tr!("Tester")).clicked() {
                let git = if self.git_path.trim().is_empty() {
                    Git::discover()
                } else {
                    Ok(Git::with_exe(PathBuf::from(self.git_path.trim())))
                };
                self.git_version = Some(git.and_then(|g| g.version()).map_err(|e| e.to_string()));
            }
        });
        match &self.git_version {
            Some(Ok(v)) => {
                ui.label(RichText::new(format!("{} {v}", icons::CHECK_CIRCLE)).color(p.added_fg));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("{} {e}", icons::WARNING)).color(p.removed_fg));
            }
            None => {
                if let Some(g) = &shared.git {
                    ui.label(RichText::new(tr!("Utilisé : {exe}", exe = g.exe().display())).color(p.weak).size(12.0));
                }
            }
        }

        section(ui, tr!("Liste des dépôts (barre de droite)"));
        ui.label(RichText::new(tr!("Dossiers scannés récursivement :")).color(p.weak));
        let mut remove = None;
        for (i, root) in d.scan_roots.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{} {}", icons::FOLDER_SIMPLE, root.display())).monospace().size(12.5));
                if ui.small_button(icons::X).on_hover_text(tr!("Retirer")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            d.scan_roots.remove(i);
        }
        if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter un dossier…"))).clicked()
            && let Some(dir) = rfd::FileDialog::new().set_title(tr!("Dossier contenant vos dépôts")).pick_folder()
            && !d.scan_roots.iter().any(|r| path_key(r) == path_key(&dir))
        {
            d.scan_roots.push(dir);
        }
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Profondeur maximale"));
            ui.add(egui::DragValue::new(&mut d.scan_max_depth).range(1..=30)).labelled_by(label.id);
        });
        let label = ui.label(tr!("Dossiers ignorés (séparés par des virgules)"));
        ui.add(egui::TextEdit::multiline(&mut self.skip_dirs).desired_rows(2).desired_width(f32::INFINITY))
            .labelled_by(label.id);
        ui.checkbox(
            &mut d.scan_deep,
            tr!("Chercher aussi à l'intérieur des dépôts (sous-modules, dépôts imbriqués — plus lent)"),
        );
        ui.checkbox(&mut d.status_sweep, tr!("Afficher l'état de chaque dépôt (modifié, en avance, en retard)"));
        if !d.hidden_repos.is_empty()
            && ui
                .button(trn!(d.hidden_repos.len(), "Réafficher le dépôt masqué", "Réafficher les {n} dépôts masqués"))
                .clicked()
        {
            d.hidden_repos.clear();
        }

        section(ui, tr!("Clonage"));
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Dossier par défaut"));
            let mut text = d.default_clone_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
            if ui.add(egui::TextEdit::singleline(&mut text).desired_width(320.0)).labelled_by(label.id).changed() {
                d.default_clone_dir = (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
            }
        });

        section(ui, tr!("Diff et commit"));
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Lignes de contexte"));
            ui.add(egui::DragValue::new(&mut d.diff_context).range(0..=99)).labelled_by(label.id);
        });
        ui.checkbox(&mut d.ignore_whitespace, tr!("Ignorer les différences d'espaces"));
        ui.checkbox(&mut d.split_diff, tr!("Diff côte à côte par défaut"));
        ui.checkbox(&mut d.syntax_highlight, tr!("Coloration syntaxique"));
        ui.checkbox(&mut d.push_after_commit, tr!("Pousser après chaque commit (valeur par défaut)"));

        section(ui, tr!("Outils externes"));
        ui.label(
            RichText::new(tr!(
                "Outil de diff : « Ouvrir dans l'outil de diff externe » (menu d'un fichier). Outil de merge : bouton « Outil de merge externe » d'un conflit."
            ))
            .color(p.weak)
            .size(12.0),
        );
        tool_picker(ui, ToolKind::Diff, &mut d.diff_tool, &mut d.custom_diff_tool, None, &shared.tools, p);
        tool_picker(
            ui,
            ToolKind::Merge,
            &mut d.merge_tool,
            &mut d.custom_merge_tool,
            Some(&mut d.custom_merge_tool_trust_exit),
            &shared.tools,
            p,
        );

        section(ui, tr!("Réseau"));
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Fetch automatique des onglets ouverts toutes les"));
            ui.add(egui::DragValue::new(&mut d.auto_fetch_minutes).range(0..=240).suffix(" min")).labelled_by(label.id);
        });
        ui.label(
            RichText::new(tr!("0 = désactivé. Le fetch automatique n'ouvre jamais de fenêtre d'authentification."))
                .color(p.weak)
                .size(12.0),
        );

        section(ui, tr!("Comptes"));
        if ui
            .button(format!("{} {}", icons::USER_CIRCLE, tr!("Gérer les comptes GitHub / GitLab / Azure DevOps…")))
            .clicked()
        {
            shared.actions.push(AppAction::ShowAccounts);
        }
        ui.label(
            RichText::new(tr!("Réglages : {path}", path = shared.settings_file().display())).color(p.weak).size(11.5),
        );
    }

    /// Onglet « Commandes » : les commandes personnalisées, dans l'ordre des menus.
    fn commands_tab(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let help = |ui: &mut egui::Ui, text: &str| {
            ui.add(egui::Label::new(RichText::new(text).color(p.weak).size(12.0)).wrap());
        };
        help(
            ui,
            tr!(
                "Elles apparaissent dans Dépôt › Commandes personnalisées (cible « dépôt ») ou dans le menu contextuel des branches, des commits ou des fichiers."
            ),
        );
        help(
            ui,
            tr!(
                "Variables : ${repo} (dossier du dépôt), ${branch}, ${sha}, ${file} (chemin dans le dépôt), ${remote_url}."
            ),
        );
        help(
            ui,
            tr!(
                "Script : ligne exécutée par cmd.exe dans le dossier du dépôt. Chaque valeur y est citée pour cmd (inutile de l'entourer de guillemets) et reste un seul argument pour le programme lancé ; placez « -- » devant un nom de fichier qui pourrait commencer par « - ». Sont refusés : une valeur avec un guillemet, un « % », un retour à la ligne ou un caractère que Windows changerait en guillemet ; un « % » collé à une variable, un « % » seul avant elle ou des « % » de part et d'autre ; un guillemet échappé (^\" ou \\\") dans la ligne ; une ligne qui lit CMDCMDLINE (cmd y recopierait sa propre ligne, valeurs comprises)."
            ),
        );
        help(
            ui,
            tr!(
                "Les valeurs acceptées sont aussi dans les variables d'environnement DT_REPO, DT_BRANCH, DT_SHA, DT_FILE et DT_REMOTE_URL : à lire dans un script .cmd ou .ps1 appelé par la ligne, jamais en %DT_…% dans la ligne elle-même (cmd y interpréterait la valeur : la commande est refusée). Dans un .cmd, écrivez \"%DT_FILE%\" entre guillemets, sans expansion différée."
            ),
        );
        help(
            ui,
            tr!(
                "URL : les valeurs sont encodées (« / » gardé seulement dans ${file}) ; ${remote_url} en tête donne l'adresse entière. Seules les adresses http, https et mailto s'ouvrent."
            ),
        );
        ui.add_space(6.0);
        let mut remove = None;
        for (i, c) in self.draft.custom_commands.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut c.name)
                            .hint_text(tr!("Nom dans les menus"))
                            .desired_width(200.0),
                    );
                    egui::ComboBox::from_id_salt(("custom-command-target", i))
                        .selected_text(target_label(c.target))
                        .show_ui(ui, |ui| {
                            for target in
                                [CommandTarget::Repo, CommandTarget::Branch, CommandTarget::Commit, CommandTarget::File]
                            {
                                ui.selectable_value(&mut c.target, target, target_label(target));
                            }
                        })
                        .response
                        .on_hover_text(tr!("Élément sur lequel porte la commande"));
                    ui.selectable_value(&mut c.kind, CommandKind::Script, "Script");
                    ui.selectable_value(&mut c.kind, CommandKind::Url, "URL");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::widgets::icon_button(ui, icons::TRASH, tr!("Retirer la commande")).clicked() {
                            remove = Some(i);
                        }
                    });
                });
                let hint = match c.kind {
                    CommandKind::Script => tr!("Ligne de commande, ex. notepad ${file}"),
                    CommandKind::Url => "https://forge.contoso.test/commit/${sha}",
                };
                ui.add(
                    egui::TextEdit::singleline(&mut c.command)
                        .hint_text(hint)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
                let warning = match c.kind {
                    CommandKind::Script if custom_commands::reads_dt_env(&c.command) => Some(tr!(
                        "Ne lisez pas %DT_…% ni !DT_…! dans cette ligne : cmd interpréterait la valeur, la commande sera refusée. Écrivez ${…}, ou lisez DT_… dans un script .cmd ou .ps1."
                    )),
                    CommandKind::Script if custom_commands::reads_cmd_line(&c.command) => Some(tr!(
                        "Ne lisez pas CMDCMDLINE dans cette ligne : cmd y recopierait sa propre ligne, valeurs et guillemets compris, la commande sera refusée."
                    )),
                    _ => None,
                };
                if let Some(text) = warning {
                    ui.add(
                        egui::Label::new(RichText::new(format!("{} {text}", icons::WARNING)).color(p.modified_fg))
                            .wrap(),
                    );
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut c.confirm, tr!("Demander confirmation"));
                    let script = c.kind == CommandKind::Script;
                    ui.add_enabled(script, egui::Checkbox::new(&mut c.show_output, tr!("Afficher la sortie")));
                    ui.add_enabled(script, egui::Checkbox::new(&mut c.refresh, tr!("Actualiser le dépôt après")));
                });
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove {
            self.draft.custom_commands.remove(i);
        }
        if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter une commande"))).clicked() {
            self.draft.custom_commands.push(CustomCommand::default());
        }
    }
}

/// Choix d'un outil externe : « Configuration de git », un préréglage (grisé tant qu'il n'est pas détecté) ou une
/// commande personnalisée avec la syntaxe de ses variables.
fn tool_picker(
    ui: &mut egui::Ui,
    kind: ToolKind,
    choice: &mut ToolChoice,
    custom: &mut String,
    trust_exit: Option<&mut bool>,
    detected: &[DetectedTool],
    p: &Palette,
) {
    let (title, salt, hint) = match kind {
        ToolKind::Diff => (tr!("Outil de diff"), "tool-diff", r#""C:\Outils\meld.exe" $LOCAL $REMOTE"#),
        ToolKind::Merge => {
            (tr!("Outil de merge"), "tool-merge", r#""C:\Outils\meld.exe" $LOCAL $BASE $REMOTE -o $MERGED"#)
        }
    };
    let label_of = |choice: &ToolChoice| match choice {
        ToolChoice::Git => tr!("Configuration de git").to_string(),
        ToolChoice::Preset(preset) => preset.label().to_string(),
        ToolChoice::Custom => tr!("Commande personnalisée").to_string(),
    };
    ui.horizontal(|ui| {
        let label = ui.label(title);
        let response = egui::ComboBox::from_id_salt(salt).selected_text(label_of(choice)).show_ui(ui, |ui| {
            ui.selectable_value(choice, ToolChoice::Git, label_of(&ToolChoice::Git)).on_hover_text(match kind {
                ToolKind::Diff => tr!("git difftool, tel que configuré dans git (diff.tool)"),
                ToolKind::Merge => tr!("git mergetool, tel que configuré dans git (merge.tool)"),
            });
            for preset in ToolPreset::ALL {
                let found = detected.iter().any(|d| d.preset == preset);
                let text =
                    if found { preset.label().to_string() } else { tr!("{tool} (introuvable)", tool = preset.label()) };
                // Un préréglage absent ne se choisit pas, mais celui qui l'est déjà reste visible.
                let selected = *choice == ToolChoice::Preset(preset);
                ui.add_enabled_ui(found || selected, |ui| {
                    ui.selectable_value(choice, ToolChoice::Preset(preset), text);
                });
            }
            ui.selectable_value(choice, ToolChoice::Custom, label_of(&ToolChoice::Custom));
        });
        response.response.labelled_by(label.id);
    });
    if *choice != ToolChoice::Custom {
        return;
    }
    ui.add(
        egui::TextEdit::singleline(custom)
            .hint_text(hint)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    let help = match kind {
        ToolKind::Diff => tr!("Variables : $LOCAL (version de gauche) et $REMOTE (version de droite)."),
        ToolKind::Merge => tr!(
            "Variables : $LOCAL (notre version), $REMOTE (la leur), $BASE (ancêtre commun) et $MERGED (fichier à résoudre)."
        ),
    };
    ui.add(egui::Label::new(RichText::new(help).color(p.weak).size(12.0)).wrap());
    ui.add(
        egui::Label::new(
            RichText::new(tr!(
                "Syntaxe : le programme puis ses arguments, séparés par des espaces ; des guillemets doubles regroupent un argument qui contient des espaces. Aucun autre échappement (l'antislash et le ^ sont du texte), aucun guillemet dans un argument, et aucun interpréteur : chaque variable reste un seul argument."
            ))
            .color(p.weak)
            .size(12.0),
        )
        .wrap(),
    );
    if let Some(trust) = trust_exit {
        ui.checkbox(trust, tr!("Le code de sortie de l'outil dit si le conflit est résolu"));
        ui.add(
            egui::Label::new(
                RichText::new(tr!(
                    "À cocher seulement si l'outil renvoie un code d'échec quand on le ferme sans résoudre. Sinon, git compare le fichier avant et après : un fichier inchangé n'est pas indexé. Pour le merge, un texte qui contient une suite d'espaces est refusé : git la réduirait à un seul espace."
                ))
                .color(p.weak)
                .size(12.0),
            )
            .wrap(),
        );
    }
    if !custom.trim().is_empty()
        && let Err(e) = external_tools::resolve(kind, &ToolChoice::Custom, custom, &[])
    {
        ui.label(RichText::new(format!("{} {e}", icons::WARNING)).color(p.modified_fg));
    }
}

/// Libellé de la cible d'une commande personnalisée.
fn target_label(target: CommandTarget) -> &'static str {
    match target {
        CommandTarget::Repo => tr!("Dépôt"),
        CommandTarget::Branch => tr!("Branche"),
        CommandTarget::Commit => "Commit",
        CommandTarget::File => tr!("Fichier"),
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong().size(14.5));
    ui.separator();
}
