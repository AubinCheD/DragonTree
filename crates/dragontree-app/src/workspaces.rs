//! Couleurs d'onglets (par dépôt) et espaces de travail (listes d'onglets nommées) : le modèle, porté par
//! [`Settings`]. Les onglets ouverts de l'espace actif restent dans `Settings::open_tabs` ; les autres espaces
//! sont rangés dans `Settings::workspaces`. Changer d'espace range les onglets de l'espace quitté et reprend
//! ceux de l'espace choisi ; l'application rouvre ensuite les onglets (`DragonTreeApp::switch_workspace`).

use std::path::{Path, PathBuf};

use dragontree_i18n::tr;
use egui::Color32;
use serde::{Deserialize, Serialize};

use crate::settings::{Settings, path_key};
use crate::theme::Palette;

/// Identifiant interne (jamais affiché, jamais traduit) de l'espace qui existe toujours : la chaîne vide, qu'aucun
/// nom saisi ne peut être. Il reprend les onglets de la migration ; son nom affiché est `workspace_label`.
pub const DEFAULT_WORKSPACE: &str = "";

/// Couleur du liseré d'un onglet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
}

impl TabColor {
    pub const ALL: [TabColor; 7] = [
        TabColor::Red,
        TabColor::Orange,
        TabColor::Yellow,
        TabColor::Green,
        TabColor::Teal,
        TabColor::Blue,
        TabColor::Purple,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TabColor::Red => tr!("Rouge"),
            TabColor::Orange => tr!("Orange"),
            TabColor::Yellow => tr!("Jaune"),
            TabColor::Green => tr!("Vert"),
            TabColor::Teal => tr!("Turquoise"),
            TabColor::Blue => tr!("Bleu"),
            TabColor::Purple => tr!("Violet"),
        }
    }

    /// Teinte du thème courant : celles du graphe, choisies lisibles sur le fond clair comme sur le sombre.
    pub fn color(self, p: &Palette) -> Color32 {
        let index = match self {
            TabColor::Red => 1,
            TabColor::Orange => 3,
            TabColor::Yellow => 6,
            TabColor::Green => 2,
            TabColor::Teal => 5,
            TabColor::Blue => 0,
            TabColor::Purple => 4,
        };
        p.graph[index]
    }
}

/// Onglets d'un espace de travail rangé (qui n'est pas l'espace actif).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Workspace {
    pub tabs: Vec<PathBuf>,
    /// Position, parmi les onglets de dépôt, de l'onglet actif.
    pub active: usize,
}

/// Pourquoi un nom d'espace est refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    Taken,
    /// L'espace « Par défaut » ne se renomme pas.
    Default,
}

impl Settings {
    /// Espace de travail actif (`DEFAULT_WORKSPACE` tant qu'aucun autre n'a été choisi).
    pub fn current_workspace_name(&self) -> &str {
        &self.current_workspace
    }

    /// Tous les espaces : « Par défaut » d'abord, puis les autres par ordre alphabétique.
    pub fn workspace_names(&self) -> Vec<String> {
        let mut others: Vec<String> = self
            .workspaces
            .keys()
            .map(String::as_str)
            .chain([self.current_workspace_name()])
            .filter(|name| *name != DEFAULT_WORKSPACE)
            .map(str::to_string)
            .collect();
        others.sort_by_key(|name| name.to_lowercase());
        others.dedup();
        std::iter::once(DEFAULT_WORKSPACE.to_string()).chain(others).collect()
    }

    /// Nom affiché d'un espace : « Par défaut » est traduit, les autres sont tels que saisis.
    pub fn workspace_label(name: &str) -> String {
        if name == DEFAULT_WORKSPACE { tr!("Par défaut").to_string() } else { name.to_string() }
    }

    /// Le nom convient-il à un espace nouveau, ou à `renaming` s'il en est un renommé (dont la casse seule peut
    /// changer) ?
    pub fn check_workspace_name(&self, name: &str, renaming: Option<&str>) -> Result<(), NameError> {
        let name = name.trim();
        if renaming == Some(DEFAULT_WORKSPACE) {
            return Err(NameError::Default);
        }
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        let lower = name.to_lowercase();
        let own = renaming.map(str::to_lowercase);
        let taken =
            self.workspace_names().iter().any(|n| n.to_lowercase() == lower && own.as_ref() != Some(&n.to_lowercase()));
        if taken { Err(NameError::Taken) } else { Ok(()) }
    }

    /// Crée un espace vide ; renvoie son nom (sans blancs autour).
    pub fn create_workspace(&mut self, name: &str) -> Result<String, NameError> {
        self.check_workspace_name(name, None)?;
        let name = name.trim();
        self.workspaces.insert(name.to_string(), Workspace::default());
        Ok(name.to_string())
    }

    /// Renomme l'espace `old`. L'espace « Par défaut » garde son nom.
    pub fn rename_workspace(&mut self, old: &str, new: &str) -> Result<(), NameError> {
        self.check_workspace_name(new, Some(old))?;
        let new = new.trim();
        if self.current_workspace_name() == old {
            self.current_workspace = new.to_string();
        } else if let Some(stored) = self.workspaces.remove(old) {
            self.workspaces.insert(new.to_string(), stored);
        }
        Ok(())
    }

    /// Passe à l'espace `name` : les onglets ouverts sont rangés dans l'espace quitté, ceux de `name` deviennent
    /// les onglets ouverts. Faux si `name` n'existe pas ou est déjà actif.
    pub fn switch_workspace(&mut self, name: &str) -> bool {
        if !self.workspace_names().iter().any(|n| n == name) || self.current_workspace_name() == name {
            return false;
        }
        let left = Workspace { tabs: std::mem::take(&mut self.open_tabs), active: self.active_tab };
        self.workspaces.insert(self.current_workspace_name().to_string(), left);
        let entering = self.workspaces.remove(name).unwrap_or_default();
        self.open_tabs = entering.tabs;
        self.active_tab = entering.active;
        self.current_workspace = name.to_string();
        true
    }

    /// Supprime l'espace `name` (jamais « Par défaut ») ; s'il est actif, on repasse d'abord par « Par défaut ».
    pub fn delete_workspace(&mut self, name: &str) -> bool {
        if name == DEFAULT_WORKSPACE || !self.workspace_names().iter().any(|n| n == name) {
            return false;
        }
        if self.current_workspace_name() == name {
            self.switch_workspace(DEFAULT_WORKSPACE);
        }
        self.workspaces.remove(name);
        true
    }

    /// Couleur mémorisée de l'onglet du dépôt `repo`.
    pub fn tab_color_of(&self, repo: &Path) -> Option<TabColor> {
        self.tab_colors.get(&path_key(repo)).copied()
    }

    /// Mémorise la couleur de l'onglet du dépôt `repo` (`None` : la retire).
    pub fn set_tab_color(&mut self, repo: &Path, color: Option<TabColor>) {
        match color {
            Some(c) => self.tab_colors.insert(path_key(repo), c),
            None => self.tab_colors.remove(&path_key(repo)),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_default_workspace_always_exists_and_takes_the_current_tabs() {
        let mut s = Settings { open_tabs: tabs(&["a", "b"]), active_tab: 1, ..Settings::default() };
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE]);
        assert_eq!(s.current_workspace_name(), DEFAULT_WORKSPACE);
        // Rien n'est rangé : les onglets ouverts sont ceux de « Par défaut ».
        assert!(s.workspaces.is_empty());
        assert_eq!(s.open_tabs, tabs(&["a", "b"]));
        assert!(!s.switch_workspace(DEFAULT_WORKSPACE), "déjà actif");
        assert!(!s.delete_workspace(DEFAULT_WORKSPACE));
        assert_eq!(s.rename_workspace(DEFAULT_WORKSPACE, "Autre"), Err(NameError::Default));
    }

    #[test]
    fn switching_stores_the_tabs_left_and_restores_the_ones_chosen() {
        let mut s = Settings { open_tabs: tabs(&["a", "b"]), active_tab: 1, ..Settings::default() };
        assert_eq!(s.create_workspace("  Client Contoso "), Ok("Client Contoso".to_string()));
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE, "Client Contoso"]);

        assert!(s.switch_workspace("Client Contoso"));
        assert_eq!(s.current_workspace_name(), "Client Contoso");
        assert!(s.open_tabs.is_empty(), "un espace neuf n'a aucun onglet");
        s.open_tabs = tabs(&["c"]);
        s.active_tab = 0;

        assert!(s.switch_workspace(DEFAULT_WORKSPACE));
        assert_eq!((s.open_tabs.clone(), s.active_tab), (tabs(&["a", "b"]), 1));
        assert_eq!(s.workspaces["Client Contoso"].tabs, tabs(&["c"]));
        assert!(!s.workspaces.contains_key(DEFAULT_WORKSPACE), "l'espace actif n'est pas rangé en double");

        assert!(s.switch_workspace("Client Contoso"));
        assert_eq!(s.open_tabs, tabs(&["c"]));
        assert!(!s.switch_workspace("Inconnu"));
    }

    #[test]
    fn names_are_unique_non_empty_and_renaming_follows_the_active_workspace() {
        let mut s = Settings::default();
        assert_eq!(s.create_workspace("   "), Err(NameError::Empty));
        s.create_workspace("Northwind").unwrap();
        assert_eq!(s.create_workspace("northwind"), Err(NameError::Taken));
        assert_eq!(s.create_workspace(DEFAULT_WORKSPACE), Err(NameError::Empty));

        s.rename_workspace("Northwind", "Fabrikam").unwrap();
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE, "Fabrikam"]);
        s.switch_workspace("Fabrikam");
        s.rename_workspace("Fabrikam", "FABRIKAM").unwrap();
        assert_eq!(s.current_workspace_name(), "FABRIKAM");
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE, "FABRIKAM"]);
    }

    #[test]
    fn deleting_the_active_workspace_returns_to_the_default_one() {
        let mut s = Settings { open_tabs: tabs(&["a"]), ..Settings::default() };
        s.create_workspace("Contoso").unwrap();
        s.switch_workspace("Contoso");
        s.open_tabs = tabs(&["z"]);
        assert!(s.delete_workspace("Contoso"));
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE]);
        assert_eq!(s.open_tabs, tabs(&["a"]));
        assert!(!s.delete_workspace("Contoso"));
    }

    #[test]
    fn tab_colors_are_per_repository_and_removable() {
        let mut s = Settings::default();
        s.set_tab_color(Path::new(r"C:\DEV\A"), Some(TabColor::Teal));
        assert_eq!(s.tab_color_of(Path::new("c:/dev/a/")), Some(TabColor::Teal));
        assert_eq!(s.tab_color_of(Path::new(r"C:\DEV\B")), None);
        s.set_tab_color(Path::new(r"C:\DEV\A"), None);
        assert!(s.tab_colors.is_empty());
    }

    #[test]
    fn an_old_settings_file_loads_with_the_default_workspace() {
        let s: Settings = serde_json::from_str(r#"{"open_tabs":["a"],"active_tab":0}"#).unwrap();
        assert_eq!(s.current_workspace_name(), DEFAULT_WORKSPACE);
        assert_eq!(s.workspace_names(), [DEFAULT_WORKSPACE]);
        assert!(s.tab_colors.is_empty());
        let json = serde_json::to_string(&Settings { tab_colors: [("x".into(), TabColor::Blue)].into(), ..s }).unwrap();
        assert!(json.contains(r#""x":"blue""#), "{json}");
    }
}
