//! Fenêtre des réglages.

use std::path::PathBuf;

use dragontree_git::Git;
use dragontree_i18n::{Lang, tr, trn};
use egui::RichText;

use crate::app::{AppAction, Shared};
use crate::icons;
use crate::settings::{Settings, ThemeChoice, path_key};
use crate::theme::Palette;

pub struct SettingsWindow {
    draft: Settings,
    skip_dirs: String,
    git_path: String,
    git_version: Option<Result<String, String>>,
}

impl SettingsWindow {
    pub fn new(settings: &Settings) -> SettingsWindow {
        SettingsWindow {
            skip_dirs: settings.scan_skip_dirs.join(", "),
            git_path: settings.git_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            draft: settings.clone(),
            git_version: None,
        }
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
                egui::ScrollArea::vertical().max_height(ui.available_height() - 44.0).show(ui, |ui| {
                    let d = &mut self.draft;
                    section(ui, tr!("Apparence"));
                    ui.horizontal(|ui| {
                        ui.label(tr!("Thème"));
                        ui.selectable_value(&mut d.theme, ThemeChoice::System, tr!("Système"));
                        ui.selectable_value(
                            &mut d.theme,
                            ThemeChoice::Light,
                            format!("{} {}", icons::SUN, tr!("Clair")),
                        );
                        ui.selectable_value(
                            &mut d.theme,
                            ThemeChoice::Dark,
                            format!("{} {}", icons::MOON, tr!("Sombre")),
                        );
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
                            egui::Slider::new(&mut d.ui_scale, 0.8..=1.6)
                                .step_by(0.05)
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
                                ui.label(
                                    RichText::new(tr!("Utilisé : {exe}", exe = g.exe().display()))
                                        .color(p.weak)
                                        .size(12.0),
                                );
                            }
                        }
                    }

                    section(ui, tr!("Liste des dépôts (barre de droite)"));
                    ui.label(RichText::new(tr!("Dossiers scannés récursivement :")).color(p.weak));
                    let mut remove = None;
                    for (i, root) in d.scan_roots.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{} {}", icons::FOLDER_SIMPLE, root.display()))
                                    .monospace()
                                    .size(12.5),
                            );
                            if ui.small_button(icons::X).on_hover_text(tr!("Retirer")).clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if let Some(i) = remove {
                        d.scan_roots.remove(i);
                    }
                    if ui.button(format!("{} {}", icons::PLUS, tr!("Ajouter un dossier…"))).clicked()
                        && let Some(dir) =
                            rfd::FileDialog::new().set_title(tr!("Dossier contenant vos dépôts")).pick_folder()
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
                    ui.checkbox(
                        &mut d.status_sweep,
                        tr!("Afficher l'état de chaque dépôt (modifié, en avance, en retard)"),
                    );
                    if !d.hidden_repos.is_empty()
                        && ui
                            .button(trn!(
                                d.hidden_repos.len(),
                                "Réafficher le dépôt masqué",
                                "Réafficher les {n} dépôts masqués"
                            ))
                            .clicked()
                    {
                        d.hidden_repos.clear();
                    }

                    section(ui, tr!("Clonage"));
                    ui.horizontal(|ui| {
                        let label = ui.label(tr!("Dossier par défaut"));
                        let mut text =
                            d.default_clone_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                        if ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(320.0))
                            .labelled_by(label.id)
                            .changed()
                        {
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

                    section(ui, tr!("Réseau"));
                    ui.horizontal(|ui| {
                        let label = ui.label(tr!("Fetch automatique des onglets ouverts toutes les"));
                        ui.add(egui::DragValue::new(&mut d.auto_fetch_minutes).range(0..=240).suffix(" min"))
                            .labelled_by(label.id);
                    });
                    ui.label(
                        RichText::new(tr!(
                            "0 = désactivé. Le fetch automatique n'ouvre jamais de fenêtre d'authentification."
                        ))
                        .color(p.weak)
                        .size(12.0),
                    );

                    section(ui, tr!("Comptes"));
                    if ui
                        .button(format!(
                            "{} {}",
                            icons::USER_CIRCLE,
                            tr!("Gérer les comptes GitHub / GitLab / Azure DevOps…")
                        ))
                        .clicked()
                    {
                        shared.actions.push(AppAction::ShowAccounts);
                    }
                    ui.label(
                        RichText::new(tr!("Réglages : {path}", path = shared.settings_file().display()))
                            .color(p.weak)
                            .size(11.5),
                    );
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
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).strong().size(14.5));
    ui.separator();
}
