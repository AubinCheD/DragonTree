//! Barre de menus Fichier / Affichage / Dépôt / Configuration, construite depuis le registre de commandes
//! (`commands::MENUS`) : libellés, raccourcis, coches et entrées grisées viennent du registre.

use dragontree_i18n::{tr, tr_str};
use egui::class::HasClasses as _;
use egui::menu::SubMenuButton;
use egui::{Align2, Atom, Atoms, Button, FontId, Response, Ui, Vec2};

use crate::commands::{self, Command, CommandContext, MENUS, MenuItem, submenu_disabled_hint};
use crate::icons;

/// Affiche la barre ; renvoie la commande choisie.
pub fn show(ui: &mut Ui, cx: &CommandContext) -> Option<Command> {
    let mut chosen = None;
    egui::MenuBar::new().ui(ui, |ui| {
        for (title, items) in MENUS {
            ui.menu_button(tr_str(title), |ui| entries(ui, items, cx, &mut chosen));
        }
    });
    chosen
}

/// Entrées d'un menu ou d'un sous-menu.
fn entries(ui: &mut Ui, items: &[MenuItem], cx: &CommandContext, chosen: &mut Option<Command>) {
    // Colonne des coches, réservée quand une entrée de ce menu peut être cochée (libellés alignés).
    let column = items.iter().any(|item| {
        matches!(item, MenuItem::OpenInEditors) || matches!(item, MenuItem::Cmd(cmd) if cmd.checked(cx).is_some())
    });
    for item in items {
        match item {
            MenuItem::Cmd(cmd) => {
                if entry(ui, *cmd, cx, column).clicked() {
                    *chosen = Some(*cmd);
                    ui.close();
                }
            }
            MenuItem::Separator => {
                ui.separator();
            }
            MenuItem::Submenu(title, sub) => {
                let label = with_check_column(tr_str(title), column.then_some(false));
                if let Some(hint) = submenu_disabled_hint(sub, cx) {
                    // Grisé avec la raison en infobulle (Git LFS absent) : le sous-menu ne s'ouvre pas.
                    let button = Button::new(label).right_text(SubMenuButton::arrow_atom(None));
                    ui.add_enabled(false, button).on_disabled_hover_text(hint);
                } else {
                    SubMenuButton::new(label).ui(ui, |ui| entries(ui, sub, cx, chosen));
                }
            }
            MenuItem::CustomCommands => custom_command_entries(ui, cx, column, chosen),
            MenuItem::OpenInEditors => open_in_entries(ui, cx, column, chosen),
        }
    }
}

/// Commandes personnalisées de cible « dépôt », sous le nom donné dans les réglages ; une entrée grisée le dit
/// quand il n'y en a aucune.
fn custom_command_entries(ui: &mut Ui, cx: &CommandContext, column: bool, chosen: &mut Option<Command>) {
    let list = commands::custom_entries(cx.settings);
    if list.is_empty() {
        ui.add_enabled(false, Button::new(with_check_column(tr!("Aucune commande"), column.then_some(false))));
    }
    for (cmd, name) in list {
        let button = Button::new(with_check_column(name, column.then_some(false)));
        if ui.add_enabled(cmd.enabled(cx), button).clicked() {
            *chosen = Some(cmd);
            ui.close();
        }
    }
}

/// Éditeurs détectés, un par entrée ; le dernier utilisé est coché.
fn open_in_entries(ui: &mut Ui, cx: &CommandContext, column: bool, chosen: &mut Option<Command>) {
    let main = crate::editors::main_editor(cx.editors, cx.settings.last_editor.as_deref());
    for (i, editor) in cx.editors.iter().enumerate() {
        let cmd = Command::OpenIn(i);
        let label = with_check_column(editor.kind.label(), column.then_some(i == main));
        if ui.add_enabled(cmd.enabled(cx), Button::new(label)).clicked() {
            *chosen = Some(cmd);
            ui.close();
        }
    }
}

/// Entrée d'une commande : coche, libellé et raccourci ; grisée si la commande n'est pas utilisable.
fn entry(ui: &mut Ui, cmd: Command, cx: &CommandContext, column: bool) -> Response {
    let checked = cmd.checked(cx);
    let mut button = Button::new(with_check_column(cmd.label(), column.then_some(checked == Some(true))));
    if let Some(text) = cmd.shortcut_text() {
        button = button.shortcut_text(text);
    }
    if let Some(on) = checked {
        // État exposé à l'accessibilité (lecteurs d'écran, tests) ; la coche remplace le fond de sélection.
        button = button.selected(on);
        button.remove_class(Button::CLASS_SELECTED);
    }
    ui.add_enabled(cmd.enabled(cx), button)
}

/// Libellé précédé de la colonne des coches (`None` : pas de colonne).
fn with_check_column(label: &str, checked: Option<bool>) -> Atoms<'_> {
    let mut atoms = Atoms::new(label);
    if let Some(on) = checked {
        atoms.push_left(Atom::paint(Vec2::splat(14.0), move |ui, args| {
            if on {
                let font = FontId::proportional(13.0);
                ui.painter().text(
                    args.rect.center(),
                    Align2::CENTER_CENTER,
                    icons::CHECK,
                    font,
                    args.fallback_text_color,
                );
            }
        }));
    }
    atoms
}
