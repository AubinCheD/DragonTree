//! Page d'un nouvel onglet : ouvrir, cloner, créer, dépôts récents.

use std::path::PathBuf;

use dragontree_i18n::tr;
use egui::{RichText, Ui};

use crate::icons;
use crate::settings::Settings;
use crate::theme::Palette;

pub enum WelcomeAction {
    OpenFolder,
    Clone,
    Init,
    AddScanRoot,
    OpenRecent(PathBuf),
    Accounts,
}

pub fn show(ui: &mut Ui, settings: &Settings, p: &Palette) -> Option<WelcomeAction> {
    let mut action = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(48.0);
            ui.label(RichText::new(icons::GIT_BRANCH).size(54.0).color(p.accent));
            ui.label(RichText::new("DragonTree").size(30.0).strong());
            ui.label(RichText::new(tr!("Client git natif — ouvrez un dépôt dans cet onglet")).color(p.weak));
            ui.add_space(26.0);

            let big = |ui: &mut Ui, icon: &str, text: &str, hint: &str| -> bool {
                ui.add_sized(
                    [340.0, 40.0],
                    egui::Button::new(RichText::new(format!("{icon}   {text}")).size(15.0)).right_text(RichText::new(hint).weak().size(11.5)),
                )
                .clicked()
            };
            if big(ui, icons::FOLDER_OPEN, tr!("Ouvrir un dépôt…"), "Ctrl+O") {
                action = Some(WelcomeAction::OpenFolder);
            }
            if big(ui, icons::DOWNLOAD_SIMPLE, tr!("Cloner un dépôt…"), tr!("Ctrl+Maj+C")) {
                action = Some(WelcomeAction::Clone);
            }
            if big(ui, icons::PLUS, tr!("Créer un nouveau dépôt…"), "") {
                action = Some(WelcomeAction::Init);
            }
            if big(ui, icons::TREE_STRUCTURE, tr!("Ajouter un dossier à scanner…"), "") {
                action = Some(WelcomeAction::AddScanRoot);
            }
            if big(ui, icons::USER_CIRCLE, tr!("Comptes GitHub / GitLab / Azure DevOps…"), "") {
                action = Some(WelcomeAction::Accounts);
            }

            if !settings.recent_repos.is_empty() {
                ui.add_space(28.0);
                ui.label(RichText::new(tr!("Dépôts récents")).strong().size(15.0));
                ui.add_space(6.0);
                for path in settings.recent_repos.iter().take(12) {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let text = format!("{}  {name}", icons::GIT_BRANCH);
                    let resp = ui
                        .add_sized(
                            [460.0, 28.0],
                            egui::Button::new(RichText::new(text))
                                .right_text(RichText::new(crate::widgets::ellipsize_middle(&path.display().to_string(), 46)).weak().size(11.5))
                                .frame(false),
                        )
                        .on_hover_text(path.display().to_string());
                    if resp.clicked() {
                        action = Some(WelcomeAction::OpenRecent(path.clone()));
                    }
                }
            }

            if settings.scan_roots.is_empty() {
                ui.add_space(28.0);
                ui.label(
                    RichText::new(format!(
                        "{}  {}",
                        icons::INFO,
                        tr!("Astuce : ajoutez votre dossier de travail (par ex. DEV) pour que tous vos dépôts apparaissent dans la barre de droite.")
                    ))
                    .color(p.weak),
                );
            }
        });
    });
    action
}
