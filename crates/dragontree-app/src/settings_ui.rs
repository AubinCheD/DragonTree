//! Fenêtre des réglages.

use std::path::PathBuf;

use dragontree_git::Git;
use dragontree_i18n::{Lang, tr, trn};
use egui::RichText;

use crate::app::{AiCheck, AppAction, AppEvent, Shared};
use crate::custom_commands::{self, CommandKind, CommandTarget, CustomCommand};
use crate::external_tools::{self, DetectedTool, ToolChoice, ToolKind, ToolPreset};
use crate::icons;
use crate::settings::{Settings, ThemeChoice, UI_SCALE_MAX, UI_SCALE_MIN, UI_SCALE_STEP, path_key};
use crate::terminal::TerminalChoice;
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
    /// Réglages à l'ouverture de la fenêtre, avec le thème et la langue en vigueur : l'enregistrement ne reporte que
    /// les champs du brouillon qui en diffèrent.
    opened: Settings,
    draft: Settings,
    skip_dirs: String,
    git_path: String,
    git_version: Option<Result<String, String>>,
}

impl SettingsWindow {
    pub fn new(shared: &Shared) -> SettingsWindow {
        // Thème et langue en vigueur, ceux de `--theme` et `--lang` compris (voir `Shared::theme_override`).
        let settings = &Settings { theme: shared.theme(), language: shared.language(), ..shared.settings.clone() };
        SettingsWindow {
            tab: SettingsTab::General,
            skip_dirs: settings.scan_skip_dirs.join(", "),
            git_path: settings.git_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            opened: settings.clone(),
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
            d.commit_hard_limit = d.commit_hard_limit.max(d.commit_soft_limit);
            d.custom_diff_tool = d.custom_diff_tool.trim().to_string();
            d.custom_merge_tool = d.custom_merge_tool.trim().to_string();
            // Les réglages actuels, avec seulement ce que la fenêtre a changé : un réglage modifié ailleurs pendant
            // qu'elle était ouverte (menu Affichage, réglages du dépôt, barre d'onglets, vue diff…) est conservé.
            let mut merged = shared.settings.clone();
            take_edits(&mut merged, &self.opened, &d);
            // Un thème ou une langue choisis ici sont enregistrés et remplacent ceux de `--theme` ou `--lang` ; sans
            // y toucher, ceux-ci restent en vigueur et ne sont pas enregistrés (voir `Shared::theme_override`).
            if d.theme != self.opened.theme {
                shared.theme_override = None;
            }
            if d.language != self.opened.language {
                shared.lang_override = None;
            }
            shared.adopt_settings(merged);
            shared.save_settings();
            return false;
        }
        open && !cancel
    }

    /// Onglet « Général » : apparence, git, liste des dépôts, clonage, diff, réseau, comptes.
    fn general_tab(&mut self, ui: &mut egui::Ui, shared: &mut Shared, p: &Palette) {
        let d = &mut self.draft;
        section(ui, tr!("Apparence"));
        // Embarqué, le thème et la langue sont ceux de l'hôte.
        if !shared.hosted {
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
        }
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
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Police du diff"));
            let response =
                egui::ComboBox::from_id_salt("diff-font").selected_text(d.diff_font.label()).show_ui(ui, |ui| {
                    // Polices système réellement présentes (la liste est lue à l'ouverture du menu seulement).
                    for font in crate::diff_style::DiffFont::available() {
                        ui.selectable_value(&mut d.diff_font, font, font.label());
                    }
                });
            response.response.labelled_by(label.id);
            let label = ui.label(tr!("taille"));
            ui.add(
                egui::DragValue::new(&mut d.diff_font_size)
                    .range(crate::diff_style::FONT_SIZE_MIN..=crate::diff_style::FONT_SIZE_MAX)
                    .speed(0.1)
                    .max_decimals(1),
            )
            .labelled_by(label.id);
            let label = ui.label(tr!("tabulation"));
            let response = egui::ComboBox::from_id_salt("diff-tab-width")
                .selected_text(d.diff_tab_width.to_string())
                .show_ui(ui, |ui| {
                    for width in crate::diff_style::TAB_WIDTHS {
                        ui.selectable_value(&mut d.diff_tab_width, width as u8, width.to_string());
                    }
                });
            response.response.labelled_by(label.id);
        });
        ui.checkbox(&mut d.diff_show_whitespace, tr!("Afficher les espaces (· et →)"));
        ui.checkbox(&mut d.push_after_commit, tr!("Pousser après chaque commit (valeur par défaut)"));
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Résumé du commit : longueur conseillée"));
            ui.add(egui::DragValue::new(&mut d.commit_soft_limit).range(1..=200)).labelled_by(label.id);
            let label = ui.label(tr!("maximale (colonne guide)"));
            ui.add(egui::DragValue::new(&mut d.commit_hard_limit).range(1..=200)).labelled_by(label.id);
        });

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

        section(ui, tr!("Terminal"));
        ui.label(
            RichText::new(tr!(
                "Terminal utilisé par « Ouvrir un terminal ». PowerShell choisit pwsh s'il est installé ; Git Bash est déduit du chemin de git."
            ))
            .color(p.weak)
            .size(12.0),
        );
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Terminal"));
            let response =
                egui::ComboBox::from_id_salt("terminal-choice").selected_text(d.terminal.label()).show_ui(ui, |ui| {
                    for choice in TerminalChoice::ALL {
                        ui.selectable_value(&mut d.terminal, choice, choice.label());
                    }
                });
            response.response.labelled_by(label.id);
        });

        section(ui, tr!("IA (Claude Code)"));
        ui.label(
            RichText::new(tr!(
                "Rédige les messages de commit et relit les branches avec la CLI Claude Code (claude -p). Le diff n'est envoyé qu'au clic sur « Générer avec Claude » ou « Revue par Claude »."
            ))
            .color(p.weak)
            .size(12.0),
        );
        ui.checkbox(&mut d.ai_enabled, tr!("Activer les fonctions IA"));
        ui.horizontal(|ui| {
            let label = ui.label(tr!("CLI Claude Code"));
            ui.add(
                egui::TextEdit::singleline(&mut d.ai_cli_path)
                    .hint_text(tr!("claude (cherché dans le PATH)"))
                    .desired_width(320.0),
            )
            .labelled_by(label.id);
            let running = shared.ai_check == AiCheck::Running;
            if ui.add_enabled(!running, egui::Button::new(tr!("Tester"))).clicked() {
                shared.ai_check = AiCheck::Running;
                let cli = crate::ai::AiCli::from_settings(d);
                shared.jobs.spawn("ai-check", move || {
                    Some(AppEvent::AiChecked(crate::ai::check(&cli).map_err(|e| e.message())))
                });
            }
        });
        match &shared.ai_check {
            AiCheck::Idle => {}
            AiCheck::Running => {
                ui.label(RichText::new(tr!("Test en cours…")).color(p.weak));
            }
            AiCheck::Done(Ok(version)) => {
                ui.label(RichText::new(format!("{} {version}", icons::CHECK_CIRCLE)).color(p.added_fg));
            }
            AiCheck::Done(Err(error)) => {
                ui.label(RichText::new(format!("{} {error}", icons::WARNING)).color(p.removed_fg));
            }
        }
        ui.horizontal(|ui| {
            let label = ui.label(tr!("Modèle (facultatif)"));
            ui.add(egui::TextEdit::singleline(&mut d.ai_model).hint_text("sonnet").desired_width(200.0))
                .labelled_by(label.id);
        });

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

/// Reporte dans `live` (les réglages actuels) les champs que la fenêtre modifie et qui, dans le brouillon `edited`,
/// diffèrent de l'état d'ouverture `opened`. Les autres réglages, et ceux que la fenêtre montre mais qu'on n'y a pas
/// touchés, gardent leur valeur actuelle, même changée ailleurs pendant que la fenêtre était ouverte. Un champ
/// ajouté à la fenêtre doit être ajouté à cette liste.
fn take_edits(live: &mut Settings, opened: &Settings, edited: &Settings) {
    macro_rules! take {
        ($($field:ident),* $(,)?) => {
            $(
                if edited.$field != opened.$field {
                    live.$field.clone_from(&edited.$field);
                }
            )*
        };
    }
    take!(
        // Apparence, git, liste des dépôts, clonage.
        theme,
        language,
        ui_scale,
        git_path,
        scan_roots,
        scan_max_depth,
        scan_skip_dirs,
        scan_deep,
        status_sweep,
        hidden_repos,
        default_clone_dir,
        // Diff et commit.
        diff_context,
        ignore_whitespace,
        split_diff,
        syntax_highlight,
        diff_font,
        diff_font_size,
        diff_tab_width,
        diff_show_whitespace,
        push_after_commit,
        commit_soft_limit,
        commit_hard_limit,
        // Outils externes, terminal, IA, réseau.
        diff_tool,
        custom_diff_tool,
        merge_tool,
        custom_merge_tool,
        custom_merge_tool_trust_exit,
        terminal,
        ai_enabled,
        ai_cli_path,
        ai_model,
        auto_fetch_minutes,
        // Onglet « Commandes ».
        custom_commands,
    );
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::settings::TicketRule;
    use crate::workspaces::TabColor;

    #[test]
    fn take_edits_keeps_what_changed_elsewhere() {
        let opened = Settings::default();
        // Ailleurs, pendant que la fenêtre était ouverte : menu Affichage, réglages du dépôt, compte du dépôt, zoom,
        // « Espaces » dans la vue diff, barre d'onglets.
        let mut live = opened.clone();
        live.show_tags = false;
        live.topological_order = true;
        let rule = TicketRule { pattern: r"#(\d+)".into(), url: "https://forge.contoso.test/issues/$1".into() };
        live.ticket_links.insert("c:/dev/fabrikam".into(), vec![rule]);
        live.hidden_refs.insert("c:/dev/fabrikam".into(), vec!["refs/heads/old".into()]);
        live.set_account_for_repo(Path::new(r"C:\DEV\fabrikam"), Some("compte-contoso"));
        live.ui_scale = 1.25;
        live.ignore_whitespace = true;
        live.open_tabs = vec![PathBuf::from(r"C:\DEV\fabrikam")];
        live.create_workspace("Contoso").unwrap();
        live.set_tab_color(Path::new(r"C:\DEV\fabrikam"), Some(TabColor::Red));
        live.diff_context = 5;
        // Dans la fenêtre : la coloration syntaxique et les lignes de contexte (changées aussi ailleurs).
        let mut edited = opened.clone();
        edited.syntax_highlight = false;
        edited.diff_context = 8;

        let mut merged = live.clone();
        take_edits(&mut merged, &opened, &edited);
        assert!(!merged.syntax_highlight, "réglage changé dans la fenêtre");
        assert_eq!(merged.diff_context, 8, "changé dans la fenêtre et ailleurs : la fenêtre l'emporte");
        assert_eq!(Settings { syntax_highlight: true, diff_context: 5, ..merged }, live, "le reste est conservé");

        // Rien de changé dans la fenêtre : les réglages actuels restent tels quels.
        let mut untouched = live.clone();
        take_edits(&mut untouched, &opened, &opened);
        assert_eq!(untouched, live);
    }
}
