//! Fichier › Espaces de travail › Nouvel espace de travail… / Renommer… / Supprimer… : petite fenêtre de saisie
//! ou de confirmation. Elle ne modifie rien elle-même : elle renvoie l'action à appliquer.

use dragontree_i18n::tr;
use egui::RichText;

use crate::icons;
use crate::settings::Settings;
use crate::theme::Palette;
use crate::workspaces::NameError;

/// Ce que la fenêtre demande à l'application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceAction {
    Create(String),
    Rename { old: String, new: String },
    Delete(String),
}

enum Kind {
    New { name: String },
    Rename { old: String, name: String },
    Delete { name: String },
}

#[derive(Default)]
pub struct WorkspaceDialog {
    kind: Option<Kind>,
}

impl WorkspaceDialog {
    pub fn show_new(&mut self) {
        self.kind = Some(Kind::New { name: String::new() });
    }

    pub fn show_rename(&mut self, current: &str) {
        self.kind = Some(Kind::Rename { old: current.to_string(), name: current.to_string() });
    }

    pub fn show_delete(&mut self, current: &str) {
        self.kind = Some(Kind::Delete { name: current.to_string() });
    }

    pub fn ui(&mut self, ctx: &egui::Context, settings: &Settings, p: &Palette) -> Option<WorkspaceAction> {
        let kind = self.kind.as_mut()?;
        let (title, ok_label, destructive) = match kind {
            Kind::New { .. } => (tr!("Nouvel espace de travail"), tr!("Créer"), false),
            Kind::Rename { .. } => (tr!("Renommer l'espace de travail"), tr!("Renommer"), false),
            Kind::Delete { .. } => (tr!("Supprimer l'espace de travail"), tr!("Supprimer"), true),
        };
        let mut confirmed = false;
        let mut cancelled = false;
        let response = egui::Modal::new(egui::Id::unique("workspace-dialog")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(format!("{} {title}", icons::SQUARES_FOUR)).strong().size(16.0));
            ui.add_space(10.0);
            let (name, renaming) = match kind {
                Kind::New { name } => (Some(name), None),
                Kind::Rename { old, name } => (Some(name), Some(old.clone())),
                Kind::Delete { name } => {
                    ui.label(tr!(
                        "Supprimer l'espace de travail « {name} » ? Ses onglets ouverts sont oubliés (les dépôts ne sont pas touchés).",
                        name = Settings::workspace_label(name)
                    ));
                    (None, None)
                }
            };
            let mut valid = true;
            if let Some(name) = name {
                let field = ui.add(egui::TextEdit::singleline(name).hint_text(tr!("Nom")).desired_width(f32::INFINITY));
                field.request_focus();
                let problem = settings.check_workspace_name(name, renaming.as_deref()).err();
                valid = problem.is_none();
                let text = match problem {
                    Some(NameError::Taken) => Some(tr!("Un espace de travail porte déjà ce nom.")),
                    Some(NameError::Default) => Some(tr!("L'espace « Par défaut » ne se renomme pas.")),
                    _ => None,
                };
                if let Some(text) = text {
                    ui.label(RichText::new(format!("{} {text}", icons::WARNING)).color(p.removed_fg));
                }
                // Entrée valide un nom ; jamais une suppression (action destructrice).
                confirmed = valid && field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            }
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let fill = if destructive { p.removed_fg } else { p.accent };
                let ok = egui::Button::new(RichText::new(ok_label).strong().color(egui::Color32::WHITE))
                    .fill(fill)
                    .min_size(egui::vec2(110.0, 28.0));
                if ui.add_enabled(valid, ok).clicked() {
                    confirmed = true;
                }
                if ui.add(egui::Button::new(tr!("Annuler")).min_size(egui::vec2(90.0, 28.0))).clicked() {
                    cancelled = true;
                }
            });
        });
        if confirmed {
            return match self.kind.take()? {
                Kind::New { name } => Some(WorkspaceAction::Create(name.trim().to_string())),
                Kind::Rename { old, name } => Some(WorkspaceAction::Rename { old, new: name.trim().to_string() }),
                Kind::Delete { name } => Some(WorkspaceAction::Delete(name)),
            };
        }
        if cancelled || response.should_close() {
            self.kind = None;
        }
        None
    }
}
