//! Configuration › Raccourcis clavier et À propos.
//!
//! La fenêtre des raccourcis liste ceux du registre (`crate::commands`, groupés par menu) puis les raccourcis fixes,
//! codés dans les vues (liste de modifications, historique, onglets). « À propos » donne la version de
//! DragonTree, celle de git (lue hors du thread de l'interface : elle lance git), la licence et les crédits.

use dragontree_i18n::{tr, tr_str};
use egui::RichText;

use crate::app::{AppEvent, Shared};
use crate::commands::{Command, MENUS, MenuItem, NOT_IN_MENUS};
use crate::icons;
use crate::theme::Palette;

/// Un raccourci affiché : touches et action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    pub keys: String,
    pub action: String,
}

/// Raccourcis d'un menu (ou « Navigation et listes » pour les raccourcis fixes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub title: String,
    pub rows: Vec<ShortcutRow>,
}

fn menu_commands(items: &[MenuItem], out: &mut Vec<Command>) {
    for item in items {
        match item {
            MenuItem::Cmd(cmd) => out.push(*cmd),
            MenuItem::Submenu(_, sub) => menu_commands(sub, out),
            MenuItem::Separator | MenuItem::CustomCommands | MenuItem::OpenInEditors | MenuItem::Workspaces => {}
        }
    }
}

fn row(cmd: Command) -> Option<ShortcutRow> {
    Some(ShortcutRow { keys: cmd.shortcut_text()?, action: cmd.label().to_string() })
}

/// Raccourcis fixes : ceux que les vues lisent directement, hors du registre.
fn fixed_rows() -> Vec<ShortcutRow> {
    let (shift, enter, space, del) = (tr!("Maj"), tr!("Entrée"), tr!("Espace"), tr!("Suppr"));
    let (page_up, page_down, home, end) = (tr!("Page ↑"), tr!("Page ↓"), tr!("Début"), tr!("Fin"));
    let row = |keys: String, action: &str| ShortcutRow { keys, action: action.to_string() };
    vec![
        row(format!("Ctrl+Tab / Ctrl+{shift}+Tab"), tr!("Onglet suivant / précédent")),
        row("Ctrl+1 … Ctrl+9".to_string(), tr!("Aller à l'onglet 1 … 9 (Ctrl+9 : le dernier)")),
        row(format!("Ctrl+{enter}"), tr!("Valider le commit (depuis le message)")),
        row(format!("{space} / {enter}"), tr!("Indexer ou désindexer les fichiers sélectionnés")),
        row(del.to_string(), tr!("Annuler les modifications des fichiers sélectionnés (avec confirmation)")),
        row("↑ / ↓".to_string(), tr!("Fichier précédent / suivant dans la liste des modifications")),
        row(format!("↑ ↓ {page_up} {page_down} {home} {end}"), tr!("Parcourir l'historique")),
        row("← / →".to_string(), tr!("Replier / déplier la fusion sélectionnée")),
        row(tr!("Ctrl+clic sur Pull ou Push").to_string(), tr!("Pull ou push rapide, sans dialogue")),
    ]
}

/// Tous les raccourcis, groupés par menu (dans l'ordre de la barre), puis les commandes hors menus, puis les
/// raccourcis fixes. Une commande présente dans deux menus n'est listée qu'une fois.
pub fn shortcut_groups() -> Vec<ShortcutGroup> {
    let mut seen: Vec<Command> = Vec::new();
    let mut groups = Vec::new();
    let mut push = |title: String, commands: Vec<Command>, groups: &mut Vec<ShortcutGroup>| {
        let mut rows = Vec::new();
        for cmd in commands {
            if let Some(r) = row(cmd).filter(|_| !seen.contains(&cmd)) {
                seen.push(cmd);
                rows.push(r);
            }
        }
        if !rows.is_empty() {
            groups.push(ShortcutGroup { title, rows });
        }
    };
    for (title, items) in MENUS {
        let mut commands = Vec::new();
        menu_commands(items, &mut commands);
        push(tr_str(title).to_string(), commands, &mut groups);
    }
    push(tr!("Autres").to_string(), NOT_IN_MENUS.to_vec(), &mut groups);
    groups.push(ShortcutGroup { title: tr!("Navigation et listes").to_string(), rows: fixed_rows() });
    groups
}

/// Groupes restreints aux lignes dont les touches ou l'action contiennent `query` (sans tenir compte de la casse) ;
/// un groupe sans ligne disparaît.
pub fn filter_groups(groups: &[ShortcutGroup], query: &str) -> Vec<ShortcutGroup> {
    let query = query.trim().to_lowercase();
    groups
        .iter()
        .filter_map(|g| {
            let rows: Vec<ShortcutRow> = g
                .rows
                .iter()
                .filter(|r| {
                    query.is_empty()
                        || r.keys.to_lowercase().contains(&query)
                        || r.action.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
            (!rows.is_empty()).then(|| ShortcutGroup { title: g.title.clone(), rows })
        })
        .collect()
}

/// Fenêtre « Raccourcis clavier ».
pub struct ShortcutsWindow {
    open: bool,
    id: egui::Id,
    query: String,
    /// Liste construite à l'ouverture (la langue peut changer entre deux ouvertures).
    groups: Vec<ShortcutGroup>,
}

impl Default for ShortcutsWindow {
    fn default() -> Self {
        ShortcutsWindow {
            open: false,
            id: egui::Id::unique("shortcuts-window"),
            query: String::new(),
            groups: Vec::new(),
        }
    }
}

impl ShortcutsWindow {
    pub fn show(&mut self) {
        self.open = true;
        self.query.clear();
        self.groups = shortcut_groups();
    }

    pub fn ui(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.open {
            return;
        }
        let mut open = true;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::KEYBOARD, tr!("Raccourcis clavier")))
            .id(self.id)
            .open(&mut open)
            .default_size([560.0, 600.0])
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(tr!("Rechercher un raccourci ou une action"))
                        .desired_width(f32::INFINITY),
                );
                let shown = filter_groups(&self.groups, &self.query);
                ui.add_space(4.0);
                if shown.is_empty() {
                    ui.label(RichText::new(tr!("Aucun raccourci ne correspond.")).color(p.weak));
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for group in &shown {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&group.title).strong());
                        egui::Grid::new(("shortcut-group", &group.title)).num_columns(2).spacing([16.0, 4.0]).show(
                            ui,
                            |ui| {
                                for r in &group.rows {
                                    ui.label(RichText::new(&r.keys).monospace().color(p.accent));
                                    ui.label(&r.action);
                                    ui.end_row();
                                }
                            },
                        );
                    }
                });
            });
        self.open = open;
    }
}

/// Licence de la police d'icônes (texte complet, fourni avec la police).
const PHOSPHOR_LICENSE: &str = include_str!("../../../assets/fonts/LICENSE-Phosphor.txt");

/// Fenêtre « À propos de DragonTree ».
pub struct AboutWindow {
    open: bool,
    id: egui::Id,
    /// Version de git : `None` tant que la lecture n'a pas répondu.
    git_version: Option<Result<String, String>>,
    /// Numéro de la lecture demandée (une réponse périmée est ignorée).
    req: u64,
}

impl Default for AboutWindow {
    fn default() -> Self {
        AboutWindow { open: false, id: egui::Id::unique("about-window"), git_version: None, req: 0 }
    }
}

impl AboutWindow {
    /// Ouvre la fenêtre et lit la version de git hors du thread de l'interface.
    pub fn show(&mut self, shared: &Shared) {
        self.open = true;
        self.git_version = None;
        self.req += 1;
        let req = self.req;
        match shared.git.clone() {
            Some(git) => shared.jobs.spawn("about-git-version", move || {
                Some(AppEvent::GitVersion(req, git.version().map_err(|e| e.to_string())))
            }),
            None => self.git_version = Some(Err(tr!("git est introuvable").to_string())),
        }
    }

    pub fn handle(&mut self, req: u64, version: Result<String, String>) {
        if req == self.req {
            self.git_version = Some(version);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, p: &Palette) {
        if !self.open {
            return;
        }
        let mut open = true;
        crate::widgets::centered_window(ctx, format!("{} {}", icons::INFO, tr!("À propos de DragonTree")))
            .id(self.id)
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(RichText::new("DragonTree").heading());
                ui.label(tr!("Client git pour Windows."));
                ui.add_space(6.0);
                ui.label(tr!("Version {version}", version = env!("CARGO_PKG_VERSION")));
                match &self.git_version {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new(tr!("Détection de git…")).color(p.weak));
                        });
                    }
                    Some(Ok(version)) => {
                        ui.label(tr!("git détecté : {version}", version));
                    }
                    Some(Err(error)) => {
                        ui.label(RichText::new(format!("{} {error}", icons::WARNING)).color(p.removed_fg));
                    }
                }
                ui.separator();
                ui.label(RichText::new(tr!("Licence")).strong());
                ui.label(tr!("DragonTree est distribué sous licence MIT."));
                ui.add_space(4.0);
                ui.label(RichText::new(tr!("Crédits")).strong());
                ui.label(tr!("egui et eframe — interface graphique (MIT ou Apache-2.0)"));
                ui.label(tr!("Phosphor Icons — police d'icônes (MIT)"));
                ui.label(tr!("syntect — coloration syntaxique (MIT)"));
                ui.add_space(4.0);
                ui.collapsing(tr!("Licence de la police Phosphor"), |ui| {
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        ui.label(RichText::new(PHOSPHOR_LICENSE).size(11.5).color(p.weak));
                    });
                });
            });
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(groups: &'a [ShortcutGroup], keys: &str) -> Option<(&'a ShortcutGroup, &'a ShortcutRow)> {
        groups.iter().find_map(|g| g.rows.iter().find(|r| r.keys == keys).map(|r| (g, r)))
    }

    #[test]
    fn every_registry_shortcut_is_listed_once_under_its_menu() {
        let groups = shortcut_groups();
        for cmd in Command::ALL {
            let Some(keys) = cmd.shortcut_text() else { continue };
            let all: Vec<_> = groups.iter().flat_map(|g| &g.rows).filter(|r| r.keys == keys).collect();
            assert_eq!(all.len(), 1, "{cmd:?} ({keys}) doit figurer une seule fois : {all:?}");
        }
        let (group, row) = find(&groups, "Ctrl+Maj+T").expect("Rouvrir l'onglet fermé");
        assert_eq!((group.title.as_str(), row.action.as_str()), ("Fichier", "Rouvrir l'onglet fermé"));
        assert_eq!(find(&groups, "F5").map(|(g, _)| g.title.as_str()), Some("Dépôt"));
        assert_eq!(find(&groups, "Ctrl+,").map(|(g, _)| g.title.as_str()), Some("Configuration"));
    }

    #[test]
    fn the_fixed_shortcuts_come_last() {
        let groups = shortcut_groups();
        let fixed = groups.last().unwrap();
        assert_eq!(fixed.title, "Navigation et listes");
        for keys in ["Ctrl+Tab / Ctrl+Maj+Tab", "Ctrl+1 … Ctrl+9", "Ctrl+Entrée", "Espace / Entrée", "Suppr", "← / →"]
        {
            assert!(fixed.rows.iter().any(|r| r.keys == keys), "{keys} absent : {:?}", fixed.rows);
        }
    }

    #[test]
    fn the_search_matches_keys_and_actions_and_drops_empty_groups() {
        let groups = shortcut_groups();
        let by_action = filter_groups(&groups, " ONGLET ");
        assert!(by_action.iter().flat_map(|g| &g.rows).any(|r| r.keys == "Ctrl+Maj+T"));
        assert!(by_action.iter().flat_map(|g| &g.rows).all(|r| r.action.to_lowercase().contains("onglet")));
        let by_keys = filter_groups(&groups, "ctrl+maj+f");
        assert_eq!(by_keys.len(), 1);
        assert_eq!(by_keys[0].rows.iter().map(|r| r.keys.as_str()).collect::<Vec<_>>(), ["Ctrl+Maj+F"]);
        assert!(filter_groups(&groups, "zzzz").is_empty());
        assert_eq!(filter_groups(&groups, "").len(), groups.len());
    }
}
