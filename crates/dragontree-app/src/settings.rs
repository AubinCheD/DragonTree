//! Réglages persistants (`%APPDATA%\DragonTree\settings.json`). Aucun secret ici : les jetons
//! des comptes sont dans le Gestionnaire d'identifiants Windows.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dragontree_forges::Account;
use dragontree_i18n::{Lang, tr};
use serde::{Deserialize, Serialize};

use crate::custom_commands::CustomCommand;
use crate::diff_style::DiffFont;
use crate::external_tools::ToolChoice;
use crate::terminal::TerminalChoice;

/// Bornes et pas de la taille de l'interface (curseur des réglages, zoom au clavier).
pub const UI_SCALE_MIN: f32 = 0.8;
pub const UI_SCALE_MAX: f32 = 1.6;
pub const UI_SCALE_STEP: f32 = 0.05;

/// Taille de l'interface après `steps` pas (négatif : zoom arrière), ramenée sur la grille des pas et bornée.
pub fn step_ui_scale(current: f32, steps: i32) -> f32 {
    let index = (current / UI_SCALE_STEP).round() + steps as f32;
    // Arrondi à 0,01 : les pas répétés ne dérivent pas (0,05 n'est pas exact en flottant).
    let scale = (index * UI_SCALE_STEP * 100.0).round() / 100.0;
    scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
}

/// Disposition de l'historique : où se place le panneau de détails par rapport à la liste des commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum HistoryLayout {
    /// Détails sous la liste (la disposition d'origine).
    #[default]
    Vertical,
    /// Détails à droite de la liste.
    Horizontal,
}

/// Présentation de l'aperçu d'une image modifiée (avant / après).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ImageDiffMode {
    /// Les deux versions l'une à côté de l'autre.
    #[default]
    SideBySide,
    /// Les deux versions superposées, un curseur vertical montre l'ancienne à gauche et la nouvelle à droite.
    Swipe,
    /// La nouvelle version par-dessus l'ancienne, d'opacité réglable.
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// Nombre de messages de commit gardés par dépôt (« Messages récents »).
pub const RECENT_MESSAGES_MAX: usize = 20;

/// Message de commit déjà utilisé (résumé et description), proposé par « Messages récents ».
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecentMessage {
    pub subject: String,
    pub body: String,
}

/// Règle de lien vers un ticket : les occurrences de `pattern` (expression régulière) dans un message de commit
/// deviennent des liens vers `url`, où `$1`, `${nom}`… sont remplacés par les groupes capturés.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TicketRule {
    pub pattern: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    /// Langue de l'interface ; `None` : celle de Windows.
    pub language: Option<Lang>,
    pub ui_scale: f32,
    /// Exécutable git imposé (sinon détecté dans le PATH).
    pub git_path: Option<PathBuf>,
    /// Dossiers racines scannés pour la liste de droite.
    pub scan_roots: Vec<PathBuf>,
    pub scan_max_depth: usize,
    pub scan_skip_dirs: Vec<String>,
    /// Chercher aussi dans les dépôts (sous-modules, dépôts imbriqués).
    pub scan_deep: bool,
    /// Calculer l'état (modifié, en avance, en retard) des dépôts listés.
    pub status_sweep: bool,
    pub repo_list_visible: bool,
    /// Panneau de détails du commit visible sous l'historique.
    pub details_panel_visible: bool,
    /// Place du panneau de détails : sous la liste des commits ou à sa droite.
    pub history_layout: HistoryLayout,
    /// Aperçu des images modifiées : côte à côte, balayage ou superposition.
    pub image_diff_mode: ImageDiffMode,
    /// Historique trié topologiquement (sinon par date).
    pub topological_order: bool,
    /// Tags parcourus par le graphe et étiquetés dans l'historique.
    pub show_tags: bool,
    /// Branches distantes parcourues par le graphe et étiquetées dans l'historique.
    pub show_remote_branches: bool,
    /// Vue Changements : liste aussi les fichiers ignorés par git (atténués, sans indexation en bloc).
    pub show_ignored_files: bool,
    /// Vue Changements : masque les fichiers non suivis.
    pub hide_untracked_files: bool,
    /// Stashs affichés comme des lignes du graphe (en mode « toutes les branches »).
    pub show_stashes: bool,
    /// Dépôt (chemin) -> références masquées du graphe : noms complets ou motifs (`refs/heads/x`,
    /// `refs/heads/feature/*`, `refs/remotes/origin/*`).
    pub hidden_refs: BTreeMap<String, Vec<String>>,
    /// Dépôts (chemin) dont l'historique est limité à la branche courante ; le filtre sur des branches choisies
    /// (V3) reste propre à la session.
    pub current_branch_only_repos: BTreeSet<String>,
    /// Largeur choisie de la colonne du graphe (poignée de l'en-tête) ; `None` : calculée d'après les branches.
    pub graph_column_width: Option<f32>,
    pub repo_list_width: f32,
    pub favorites: Vec<PathBuf>,
    pub hidden_repos: Vec<PathBuf>,
    pub default_clone_dir: Option<PathBuf>,
    pub open_tabs: Vec<PathBuf>,
    pub active_tab: usize,
    pub recent_repos: Vec<PathBuf>,
    /// Dépôt (chemin) -> couleur du liseré de son onglet.
    pub tab_colors: BTreeMap<String, crate::workspaces::TabColor>,
    /// Espaces de travail rangés (hors l'espace actif, dont les onglets sont `open_tabs`) : nom -> onglets.
    pub workspaces: BTreeMap<String, crate::workspaces::Workspace>,
    /// Espace de travail actif ; vide : « Par défaut ».
    pub current_workspace: String,
    pub accounts: Vec<Account>,
    /// Dépôt (chemin) → identifiant du compte utilisé pour fetch/pull/push : rempli par un clonage depuis un compte,
    /// par Dépôt › Utiliser le compte… et par les réglages du dépôt (la fenêtre Réglages ne l'édite pas).
    pub repo_accounts: BTreeMap<String, String>,
    pub diff_context: u32,
    pub ignore_whitespace: bool,
    pub split_diff: bool,
    pub syntax_highlight: bool,
    /// Police, taille, largeur de tabulation et espaces visibles du texte des diffs, du blame et des conflits.
    pub diff_font: DiffFont,
    pub diff_font_size: f32,
    pub diff_tab_width: u8,
    pub diff_show_whitespace: bool,
    /// IA (Réglages › IA) : active les boutons « Générer avec Claude » et « Revue par Claude », chemin de la CLI Claude
    /// Code (vide : `claude` dans le PATH) et modèle facultatif. Rien n'est envoyé sans un clic.
    pub ai_enabled: bool,
    pub ai_cli_path: String,
    pub ai_model: String,
    pub push_after_commit: bool,
    /// Minutes entre deux fetch automatiques des onglets ouverts (0 = jamais).
    pub auto_fetch_minutes: u32,
    pub show_read_commands_in_log: bool,
    /// Dépôt (chemin) -> règles de liens vers les tickets des messages de commit.
    pub ticket_links: BTreeMap<String, Vec<TicketRule>>,
    /// Commandes personnalisées (Réglages › Commandes), dans l'ordre des menus.
    pub custom_commands: Vec<CustomCommand>,
    /// Dernier éditeur choisi dans « Ouvrir dans » (identifiant de `EditorKind`) : l'action du clic principal.
    pub last_editor: Option<String>,
    /// Outil de diff externe (Réglages › Outils externes) et sa commande quand le choix est « personnalisée ».
    pub diff_tool: ToolChoice,
    pub custom_diff_tool: String,
    /// Outil de merge externe des conflits et sa commande personnalisée.
    pub merge_tool: ToolChoice,
    pub custom_merge_tool: String,
    /// Le code de sortie de la commande de merge personnalisée dit si le fichier est résolu (sinon git compare le fichier
    /// avant et après). À cocher seulement si l'outil renvoie un code d'échec quand on le ferme sans résoudre.
    pub custom_merge_tool_trust_exit: bool,
    /// Terminal de « Ouvrir un terminal » (Réglages › Terminal).
    pub terminal: TerminalChoice,
    /// Longueurs du résumé d'un commit : au-delà de la première, avertissement ; au-delà de la seconde, erreur
    /// (la seconde est aussi la colonne guide de la description). Réglages › Diff et commit.
    pub commit_soft_limit: usize,
    pub commit_hard_limit: usize,
    /// Dépôt (chemin) -> derniers messages de commit, le plus récent d'abord (voir [`Settings::remember_message`]).
    pub recent_messages: BTreeMap<String, Vec<RecentMessage>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeChoice::System,
            language: None,
            ui_scale: 1.0,
            git_path: None,
            scan_roots: Vec::new(),
            scan_max_depth: 8,
            scan_skip_dirs: dragontree_git::discovery::DEFAULT_SKIP_DIRS.iter().map(|s| s.to_string()).collect(),
            scan_deep: false,
            status_sweep: true,
            repo_list_visible: true,
            details_panel_visible: true,
            history_layout: HistoryLayout::Vertical,
            image_diff_mode: ImageDiffMode::SideBySide,
            topological_order: false,
            show_tags: true,
            show_remote_branches: true,
            show_stashes: true,
            show_ignored_files: false,
            hide_untracked_files: false,
            hidden_refs: BTreeMap::new(),
            current_branch_only_repos: BTreeSet::new(),
            graph_column_width: None,
            repo_list_width: 280.0,
            favorites: Vec::new(),
            hidden_repos: Vec::new(),
            default_clone_dir: None,
            open_tabs: Vec::new(),
            active_tab: 0,
            recent_repos: Vec::new(),
            tab_colors: BTreeMap::new(),
            workspaces: BTreeMap::new(),
            current_workspace: String::new(),
            accounts: Vec::new(),
            repo_accounts: BTreeMap::new(),
            diff_context: 3,
            ignore_whitespace: false,
            split_diff: false,
            syntax_highlight: true,
            diff_font: DiffFont::Builtin,
            diff_font_size: 12.5,
            diff_tab_width: 4,
            diff_show_whitespace: false,
            ai_enabled: false,
            ai_cli_path: String::new(),
            ai_model: String::new(),
            push_after_commit: false,
            auto_fetch_minutes: 10,
            show_read_commands_in_log: false,
            ticket_links: BTreeMap::new(),
            custom_commands: Vec::new(),
            last_editor: None,
            diff_tool: ToolChoice::Git,
            custom_diff_tool: String::new(),
            merge_tool: ToolChoice::Git,
            custom_merge_tool: String::new(),
            custom_merge_tool_trust_exit: false,
            terminal: TerminalChoice::Auto,
            commit_soft_limit: 50,
            commit_hard_limit: 72,
            recent_messages: BTreeMap::new(),
        }
    }
}

/// Clé stable d'un chemin de dépôt (minuscules, séparateurs `/`, sans `/` final).
pub fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    s.trim_end_matches('/').to_lowercase()
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("DragonTree")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("DragonTree")
}

/// Fichier de réglages : il n'est jamais écrasé s'il existait mais n'a pas pu être lu.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
    writable: bool,
}

impl SettingsFile {
    /// Charge les réglages de `path` ; l'éventuel message d'erreur est à afficher.
    ///
    /// - absent : réglages par défaut ;
    /// - contenu invalide : le fichier est mis de côté (`settings.illisible-<horodatage>.json`) ;
    /// - illisible (verrouillé, droits…) : réglages par défaut et aucun enregistrement pendant la
    ///   session, pour ne jamais écraser un fichier qu'on n'a pas pu lire.
    pub fn load(path: &Path) -> (SettingsFile, Settings, Option<String>) {
        let file = |writable| SettingsFile { path: path.to_path_buf(), writable };
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (file(true), Settings::default(), None),
            Err(e) => {
                let msg = tr!(
                    "Réglages illisibles ({e}) : réglages par défaut utilisés, et rien ne sera enregistré pendant cette session pour ne pas écraser {path}.",
                    e,
                    path = path.display()
                );
                return (file(false), Settings::default(), Some(msg));
            }
        };
        match serde_json::from_slice(&bytes) {
            Ok(s) => (file(true), s, None),
            Err(e) => {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let backup = path.with_file_name(format!("settings.illisible-{stamp}.json"));
                if std::fs::rename(path, &backup).is_ok() {
                    let msg = tr!(
                        "Réglages illisibles ({e}) : fichier mis de côté dans {backup} — réglages par défaut utilisés.",
                        e,
                        backup = backup.display()
                    );
                    (file(true), Settings::default(), Some(msg))
                } else {
                    let msg = tr!(
                        "Réglages illisibles ({e}) : réglages par défaut utilisés, et rien ne sera enregistré pendant cette session pour ne pas écraser {path}.",
                        e,
                        path = path.display()
                    );
                    (file(false), Settings::default(), Some(msg))
                }
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Enregistre (sauf si le fichier d'origine n'a pas pu être lu).
    pub fn save(&self, settings: &Settings) -> std::io::Result<()> {
        if !self.writable {
            return Ok(());
        }
        settings.save_to(&self.path)
    }
}

impl Settings {
    pub fn file() -> PathBuf {
        config_dir().join("settings.json")
    }

    pub fn load_from(path: &Path) -> Settings {
        SettingsFile::load(path).1
    }

    /// Écriture atomique : fichier temporaire puis renommage.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path)
    }

    /// Mémorise un dépôt ouvert récemment (20 au plus, le plus récent en tête).
    pub fn push_recent(&mut self, path: &Path) {
        let key = path_key(path);
        self.recent_repos.retain(|p| path_key(p) != key);
        self.recent_repos.insert(0, path.to_path_buf());
        self.recent_repos.truncate(20);
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        let key = path_key(path);
        self.favorites.iter().any(|p| path_key(p) == key)
    }

    pub fn toggle_favorite(&mut self, path: &Path) {
        let key = path_key(path);
        if self.is_favorite(path) {
            self.favorites.retain(|p| path_key(p) != key);
        } else {
            self.favorites.push(path.to_path_buf());
        }
    }

    /// Compte associé au dépôt `path` (voir [`Settings::repo_accounts`]) : `None` sans association, ou si le compte
    /// n'existe plus.
    pub fn account_for_repo(&self, path: &Path) -> Option<&Account> {
        let id = self.repo_accounts.get(&path_key(path))?;
        self.accounts.iter().find(|a| &a.id == id)
    }

    /// Associe le compte `account_id` au dépôt `path` (fetch, pull et push s'authentifient avec son jeton), ou
    /// retire l'association (`None`) : git reprend alors sa configuration habituelle.
    pub fn set_account_for_repo(&mut self, path: &Path, account_id: Option<&str>) {
        match account_id {
            Some(id) => self.repo_accounts.insert(path_key(path), id.to_string()),
            None => self.repo_accounts.remove(&path_key(path)),
        };
    }

    /// Références masquées du graphe pour un dépôt (voir [`Settings::hidden_refs`]).
    pub fn hidden_refs_of(&self, repo: &Path) -> &[String] {
        self.hidden_refs.get(&path_key(repo)).map_or(&[], Vec::as_slice)
    }

    /// Masque la référence ou le motif `pattern` du graphe de `repo`, ou le réaffiche s'il l'était.
    pub fn toggle_hidden_ref(&mut self, repo: &Path, pattern: &str) {
        let list = self.hidden_refs.entry(path_key(repo)).or_default();
        match list.iter().position(|p| p == pattern) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(pattern.to_string()),
        }
        if list.is_empty() {
            self.hidden_refs.remove(&path_key(repo));
        }
    }

    /// Vrai si le choix « branche courante seulement » est mémorisé pour `repo`.
    pub fn current_branch_only_of(&self, repo: &Path) -> bool {
        self.current_branch_only_repos.contains(&path_key(repo))
    }

    /// Mémorise (ou oublie) le choix « branche courante seulement » de `repo`.
    pub fn set_current_branch_only(&mut self, repo: &Path, on: bool) {
        if on {
            self.current_branch_only_repos.insert(path_key(repo));
        } else {
            self.current_branch_only_repos.remove(&path_key(repo));
        }
    }

    pub fn clear_hidden_refs(&mut self, repo: &Path) {
        self.hidden_refs.remove(&path_key(repo));
    }

    /// Règles de liens vers les tickets d'un dépôt (voir [`Settings::ticket_links`]).
    pub fn ticket_rules_of(&self, repo: &Path) -> &[TicketRule] {
        self.ticket_links.get(&path_key(repo)).map_or(&[], Vec::as_slice)
    }

    /// Derniers messages de commit d'un dépôt, le plus récent d'abord.
    pub fn recent_messages_of(&self, repo: &Path) -> &[RecentMessage] {
        self.recent_messages.get(&path_key(repo)).map_or(&[], Vec::as_slice)
    }

    /// Retient un message de commit réussi : en tête de liste, sans doublon, au plus [`RECENT_MESSAGES_MAX`].
    /// Un message sans résumé n'est pas gardé.
    pub fn remember_message(&mut self, repo: &Path, subject: &str, body: &str) {
        let (subject, body) = (subject.trim(), body.trim_end());
        if subject.is_empty() {
            return;
        }
        let list = self.recent_messages.entry(path_key(repo)).or_default();
        let message = RecentMessage { subject: subject.to_string(), body: body.to_string() };
        list.retain(|m| *m != message);
        list.insert(0, message);
        list.truncate(RECENT_MESSAGES_MAX);
    }

    /// Remplace les règles d'un dépôt ; les règles sans motif ni URL sont écartées, et un dépôt sans règle n'a
    /// plus d'entrée.
    pub fn set_ticket_rules(&mut self, repo: &Path, rules: Vec<TicketRule>) {
        let rules: Vec<TicketRule> = rules
            .into_iter()
            .map(|r| TicketRule { pattern: r.pattern.trim().to_string(), url: r.url.trim().to_string() })
            .filter(|r| !r.pattern.is_empty() && !r.url.is_empty())
            .collect();
        if rules.is_empty() {
            self.ticket_links.remove(&path_key(repo));
        } else {
            self.ticket_links.insert(path_key(repo), rules);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_scale_steps_stay_on_the_grid_and_within_bounds() {
        assert_eq!(step_ui_scale(1.0, 1), 1.05);
        assert_eq!(step_ui_scale(1.0, -1), 0.95);
        // Une valeur hors grille (ancien réglage) revient sur la grille.
        assert_eq!(step_ui_scale(1.02, 1), 1.05);
        // Des pas répétés n'accumulent pas d'erreur d'arrondi.
        let mut scale = 1.0;
        for _ in 0..4 {
            scale = step_ui_scale(scale, 1);
        }
        assert_eq!(scale, 1.2);
        // Bornes.
        assert_eq!(step_ui_scale(UI_SCALE_MAX, 1), UI_SCALE_MAX);
        assert_eq!(step_ui_scale(UI_SCALE_MIN, -1), UI_SCALE_MIN);
        assert_eq!(step_ui_scale(3.0, -1), UI_SCALE_MAX);
    }

    #[test]
    fn settings_roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("settings.json");
        assert_eq!(Settings::load_from(&file), Settings::default());
        let mut s = Settings::default();
        s.scan_roots.push(PathBuf::from(r"C:\Users\x\DEV"));
        s.theme = ThemeChoice::Dark;
        s.save_to(&file).unwrap();
        assert_eq!(Settings::load_from(&file), s);
    }

    #[test]
    fn corrupt_settings_are_backed_up_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ contenu invalide").unwrap();
        let (_, s, err) = SettingsFile::load(&file);
        assert_eq!(s, Settings::default());
        let msg = err.expect("erreur signalée");
        assert!(!file.exists(), "le fichier illisible est déplacé, pas écrasé plus tard");
        let backups: Vec<_> = std::fs::read_dir(dir.path()).unwrap().filter_map(Result::ok).collect();
        assert_eq!(backups.len(), 1);
        let backup = backups[0].path();
        assert!(msg.contains(&backup.display().to_string()), "{msg}");
        assert!(std::fs::read_to_string(backup).unwrap().contains("invalide"));
        let (_, s, err) = SettingsFile::load(&dir.path().join("absent.json"));
        assert_eq!((s, err), (Settings::default(), None));
    }

    #[cfg(windows)]
    #[test]
    fn locked_settings_file_is_never_overwritten() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, r#"{"theme":"Dark"}"#).unwrap();
        // Fichier verrouillé par un autre programme (antivirus, synchronisation…) au démarrage.
        let lock = std::fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
        let (store, settings, error) = SettingsFile::load(&file);
        drop(lock);
        assert_eq!(settings, Settings::default());
        assert!(error.is_some(), "l'erreur de lecture est signalée");
        store.save(&Settings::default()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), r#"{"theme":"Dark"}"#, "le fichier n'est jamais écrasé");
    }

    #[test]
    fn settings_tolerate_unknown_and_missing_fields() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, r#"{"theme":"Light","champ_inconnu":42}"#).unwrap();
        let s = Settings::load_from(&file);
        assert_eq!(s.theme, ThemeChoice::Light);
        assert_eq!(s.diff_context, 3);
        // Un ancien fichier sans les réglages d'affichage garde tags et branches distantes visibles.
        assert!(s.show_tags && s.show_remote_branches && s.show_stashes);
    }

    #[test]
    fn recent_repos_are_deduplicated_case_insensitively() {
        let mut s = Settings::default();
        s.push_recent(Path::new(r"C:\DEV\A"));
        s.push_recent(Path::new(r"C:\DEV\B"));
        s.push_recent(Path::new(r"c:\dev\a\"));
        assert_eq!(s.recent_repos.len(), 2);
        assert_eq!(path_key(&s.recent_repos[0]), "c:/dev/a");
    }

    #[test]
    fn hidden_refs_are_per_repo_and_toggle() {
        let mut s = Settings::default();
        assert!(s.hidden_refs_of(Path::new(r"C:\DEV\A")).is_empty());
        s.toggle_hidden_ref(Path::new(r"C:\DEV\A"), "refs/heads/x");
        s.toggle_hidden_ref(Path::new(r"c:\dev\a\"), "refs/heads/feature/*");
        assert_eq!(s.hidden_refs_of(Path::new(r"C:\DEV\A")), ["refs/heads/x", "refs/heads/feature/*"]);
        assert!(s.hidden_refs_of(Path::new(r"C:\DEV\B")).is_empty());
        s.toggle_hidden_ref(Path::new(r"C:\DEV\A"), "refs/heads/x");
        assert_eq!(s.hidden_refs_of(Path::new(r"C:\DEV\A")), ["refs/heads/feature/*"]);
        s.clear_hidden_refs(Path::new(r"C:\DEV\A"));
        assert!(s.hidden_refs.is_empty(), "plus d'entrée pour un dépôt sans référence masquée");
    }
    #[test]
    fn ticket_rules_are_per_repo_trimmed_and_blank_ones_dropped() {
        let rule = |p: &str, u: &str| TicketRule { pattern: p.into(), url: u.into() };
        let mut s = Settings::default();
        s.set_ticket_rules(
            Path::new(r"C:\DEV\A"),
            vec![rule(r" #(\d+) ", " https://x.test/$1 "), rule("", "https://vide.test"), rule("x", "")],
        );
        assert_eq!(s.ticket_rules_of(Path::new(r"c:\dev\a\")), [rule(r"#(\d+)", "https://x.test/$1")]);
        assert!(s.ticket_rules_of(Path::new(r"C:\DEV\B")).is_empty());
        s.set_ticket_rules(Path::new(r"C:\DEV\A"), Vec::new());
        assert!(s.ticket_links.is_empty(), "plus d'entrée pour un dépôt sans règle");
    }

    #[test]
    fn recent_messages_are_per_repo_deduplicated_and_capped() {
        let mut s = Settings::default();
        let repo = Path::new(r"C:\DEV\A");
        s.remember_message(repo, "feat: a", "corps");
        s.remember_message(repo, "fix: b", "");
        s.remember_message(Path::new(r"c:\dev\a\"), "feat: a", "corps\n");
        // Même dépôt (clé normalisée) : le doublon remonte en tête.
        let subjects: Vec<_> = s.recent_messages_of(repo).iter().map(|m| m.subject.as_str()).collect();
        assert_eq!(subjects, ["feat: a", "fix: b"]);
        assert!(s.recent_messages_of(Path::new(r"C:\DEV\B")).is_empty());
        // Un résumé vide n'est pas gardé ; la liste est plafonnée.
        s.remember_message(repo, "  ", "orphelin");
        for i in 0..30 {
            s.remember_message(repo, &format!("commit {i}"), "");
        }
        assert_eq!(s.recent_messages_of(repo).len(), RECENT_MESSAGES_MAX);
        assert_eq!(s.recent_messages_of(repo)[0].subject, "commit 29");
    }

    #[test]
    fn an_old_settings_file_has_the_default_commit_limits_and_no_recent_messages() {
        let s: Settings = serde_json::from_str(r#"{"theme":"Dark"}"#).unwrap();
        assert_eq!((s.commit_soft_limit, s.commit_hard_limit), (50, 72));
        assert!(s.recent_messages.is_empty());
    }

    #[test]
    fn an_old_settings_file_without_ticket_links_still_loads() {
        let s: Settings = serde_json::from_str(r#"{"theme":"Dark","ui_scale":1.2}"#).unwrap();
        assert!(s.ticket_links.is_empty());
        assert_eq!(s.theme, ThemeChoice::Dark);
    }

    #[test]
    fn custom_commands_round_trip_and_an_old_file_has_none() {
        use crate::custom_commands::{CommandKind, CommandTarget, CustomCommand};
        let old: Settings = serde_json::from_str(r#"{"theme":"Dark"}"#).unwrap();
        assert!(old.custom_commands.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let mut s = Settings::default();
        s.custom_commands.push(CustomCommand {
            name: "Ouvrir sur la forge".into(),
            target: CommandTarget::Commit,
            kind: CommandKind::Url,
            command: "https://forge.contoso.test/commit/${sha}".into(),
            confirm: true,
            show_output: false,
            refresh: true,
        });
        s.save_to(&file).unwrap();
        assert_eq!(Settings::load_from(&file), s);
    }

    fn account(id: &str) -> Account {
        Account {
            id: id.into(),
            kind: dragontree_forges::ForgeKind::GitHub,
            host: "github.com".into(),
            username: "ada".into(),
            display_name: String::new(),
            orgs: Vec::new(),
            avatar_url: None,
        }
    }

    #[test]
    fn an_associated_account_is_returned_until_it_is_dissociated() {
        let mut s = Settings { accounts: vec![account("compte-a"), account("compte-b")], ..Settings::default() };
        let repo = Path::new(r"C:\DEV\Fabrikam");
        assert!(s.account_for_repo(repo).is_none(), "aucun compte avant l'association");
        s.set_account_for_repo(repo, Some("compte-b"));
        // Même dépôt quelle que soit l'écriture du chemin.
        assert_eq!(s.account_for_repo(Path::new(r"c:\dev\fabrikam\")).map(|a| a.id.as_str()), Some("compte-b"));
        assert!(s.account_for_repo(Path::new(r"C:\DEV\Contoso")).is_none(), "propre à ce dépôt");
        s.set_account_for_repo(repo, Some("compte-a"));
        assert_eq!(s.account_for_repo(repo).map(|a| a.id.as_str()), Some("compte-a"), "un seul compte par dépôt");
        s.set_account_for_repo(repo, None);
        assert!(s.account_for_repo(repo).is_none());
        assert!(s.repo_accounts.is_empty(), "plus d'entrée pour un dépôt dissocié");
    }

    #[test]
    fn favorites_toggle() {
        let mut s = Settings::default();
        s.toggle_favorite(Path::new(r"C:\DEV\A"));
        assert!(s.is_favorite(Path::new(r"c:\dev\A")));
        s.toggle_favorite(Path::new(r"C:\DEV\a"));
        assert!(!s.is_favorite(Path::new(r"C:\DEV\A")));
    }
}
