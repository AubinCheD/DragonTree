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

/// Fichier de réglages : il n'est jamais écrasé s'il existait mais n'a pas pu être lu (L3).
///
/// Illisible au démarrage (verrouillé par un autre programme, invalide sans pouvoir être mis de côté), la session part
/// des réglages par défaut et le fichier est relu à chaque enregistrement (`save`). Tant qu'il reste illisible, rien
/// n'est écrit ; une fois relu, les réglages du fichier et ceux de la session sont fusionnés (voir [`merge_session`]),
/// puis écrits.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
    /// `Some` tant que le fichier existant n'a pas pu être lu.
    unread: Option<Unread>,
}

/// Fichier pas encore lu : réglages de départ de la session et dernière erreur de lecture.
#[derive(Debug, Clone)]
struct Unread {
    base: Settings,
    error: String,
}

/// Résultat de [`SettingsFile::save`].
#[derive(Debug, Clone, PartialEq)]
pub enum Saved {
    /// Réglages écrits.
    Written,
    /// Fichier illisible au démarrage enfin relu : réglages fusionnés (voir [`merge_session`]) et écrits, à reprendre
    /// dans la session.
    Recovered(Box<Settings>),
    /// Fichier illisible au démarrage, relu invalide et mis de côté : réglages de la session écrits ; message à afficher.
    SetAside(String),
    /// Fichier toujours illisible : rien n'est écrit.
    NotWritten,
}

/// Lecture du fichier de réglages.
enum Read {
    Absent,
    Valid(Box<Settings>),
    /// Contenu invalide, fichier renommé en `backup` : message d'erreur et nom de la copie.
    SetAside(String, PathBuf),
    /// Illisible (verrouillé, droits…) ou invalide sans pouvoir être mis de côté : message d'erreur.
    Unreadable(String),
}

fn read(path: &Path) -> Read {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Absent,
        Err(e) => return Read::Unreadable(e.to_string()),
    };
    match serde_json::from_slice(&bytes) {
        Ok(s) => Read::Valid(Box::new(s)),
        Err(e) => {
            let stamp =
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let backup = path.with_file_name(format!("settings.illisible-{stamp}.json"));
            if std::fs::rename(path, &backup).is_ok() {
                Read::SetAside(e.to_string(), backup)
            } else {
                Read::Unreadable(e.to_string())
            }
        }
    }
}

impl SettingsFile {
    /// Charge les réglages de `path` ; l'éventuel message d'erreur est à afficher.
    ///
    /// - absent : réglages par défaut ;
    /// - contenu invalide : le fichier est mis de côté (`settings.illisible-<horodatage>.json`) ;
    /// - illisible (verrouillé, droits…), ou invalide sans pouvoir être mis de côté : réglages par défaut, et rien
    ///   n'est écrit tant que le fichier n'a pas pu être relu (voir [`SettingsFile::save`]).
    pub fn load(path: &Path) -> (SettingsFile, Settings, Option<String>) {
        let file = |unread| SettingsFile { path: path.to_path_buf(), unread };
        match read(path) {
            Read::Absent => (file(None), Settings::default(), None),
            Read::Valid(s) => (file(None), *s, None),
            Read::SetAside(e, backup) => {
                let msg = tr!(
                    "Réglages illisibles ({e}) : fichier mis de côté dans {backup} — réglages par défaut utilisés.",
                    e,
                    backup = backup.display()
                );
                (file(None), Settings::default(), Some(msg))
            }
            Read::Unreadable(e) => {
                let msg = tr!(
                    "Réglages illisibles ({e}) : réglages par défaut utilisés. {path} n'est pas écrasé : vos modifications seront enregistrées quand il aura pu être relu.",
                    e,
                    path = path.display()
                );
                (file(Some(Unread { base: Settings::default(), error: e })), Settings::default(), Some(msg))
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Dernière erreur de lecture, tant que le fichier n'a pas pu être lu.
    pub fn read_error(&self) -> Option<&str> {
        self.unread.as_ref().map(|u| u.error.as_str())
    }

    /// Vrai si `settings` contient des modifications de la session que le fichier, pas encore lu, n'a pas reçues.
    pub fn unsaved(&self, settings: &Settings) -> bool {
        self.unread.as_ref().is_some_and(|u| u.base != *settings)
    }

    /// Enregistre `settings`. Si le fichier n'a pas pu être lu au démarrage, il est d'abord relu : toujours illisible,
    /// rien n'est écrit ; relu, la fusion de ses réglages et de ceux de la session est écrite et renvoyée ; absent ou
    /// mis de côté, les réglages de la session sont écrits.
    pub fn save(&mut self, settings: &Settings) -> std::io::Result<Saved> {
        let Some(unread) = &mut self.unread else {
            settings.save_to(&self.path)?;
            return Ok(Saved::Written);
        };
        let saved = match read(&self.path) {
            Read::Unreadable(e) => {
                unread.error = e;
                return Ok(Saved::NotWritten);
            }
            Read::Absent => {
                settings.save_to(&self.path)?;
                Saved::Written
            }
            Read::SetAside(e, backup) => {
                settings.save_to(&self.path)?;
                Saved::SetAside(tr!(
                    "Réglages relus invalides ({e}) : fichier mis de côté dans {backup} — réglages de cette session enregistrés.",
                    e,
                    backup = backup.display()
                ))
            }
            Read::Valid(theirs) => {
                let merged = merge_session(&unread.base, settings, &theirs);
                merged.save_to(&self.path)?;
                Saved::Recovered(Box::new(merged))
            }
        };
        // Écrit : le fichier est désormais celui de la session.
        self.unread = None;
        Ok(saved)
    }
}

/// Fusion à la relecture d'un fichier illisible au démarrage (L3) : `base` (réglages de départ de la session, ceux par
/// défaut), `mine` (ceux de la session) et `theirs` (ceux du fichier relu).
/// - Un réglage que la session n'a pas changé prend la valeur du fichier ; un réglage changé garde celle de la session.
/// - Tables (réglages par dépôt, espaces de travail…) : la même règle, entrée par entrée.
/// - Listes (dépôts récents, favoris, comptes, dossiers scannés, commandes…) : ce que la session a ajouté, en tête,
///   puis la liste du fichier sans ce que la session a retiré.
///
/// Onglets ouverts et onglet actif suivent la même règle ; l'application les réécrit d'après ses onglets au prochain
/// enregistrement périodique.
pub fn merge_session(base: &Settings, mine: &Settings, theirs: &Settings) -> Settings {
    use serde_json::Value;
    let json = |s: &Settings| serde_json::to_value(s).ok();
    let deep = || {
        let merged = merge_value(Some(&json(base)?), &json(mine)?, Some(&json(theirs)?))?;
        serde_json::from_value(merged).ok()
    };
    // Forme inattendue (énumération à données fusionnée entrée par entrée…) : la règle au premier niveau seulement.
    let shallow = || {
        let (Value::Object(b), Value::Object(m), Value::Object(t)) = (json(base)?, json(mine)?, json(theirs)?) else {
            return None;
        };
        let fields = m.into_iter().map(|(k, v)| {
            let unchanged = b.get(&k) == Some(&v);
            let value = if unchanged { t.get(&k).cloned().unwrap_or(v) } else { v };
            (k, value)
        });
        serde_json::from_value(Value::Object(fields.collect())).ok()
    };
    // En dernier recours, le fichier tel quel : les modifications de la session sont perdues, jamais le fichier.
    let mut merged: Settings = deep().or_else(shallow).unwrap_or_else(|| theirs.clone());
    // Plafonds des listes (voir `push_recent` et `remember_message`).
    merged.recent_repos.truncate(20);
    for list in merged.recent_messages.values_mut() {
        list.truncate(RECENT_MESSAGES_MAX);
    }
    merged
}

/// Fusion à trois d'une valeur JSON (voir [`merge_session`]) ; `None` : entrée absente du résultat.
fn merge_value(
    base: Option<&serde_json::Value>,
    mine: &serde_json::Value,
    theirs: Option<&serde_json::Value>,
) -> Option<serde_json::Value> {
    use serde_json::Value;
    if base == Some(mine) {
        // Inchangé pendant la session : la valeur du fichier, ou rien s'il ne l'a plus.
        return theirs.cloned();
    }
    let Some(theirs) = theirs else { return Some(mine.clone()) };
    match (base, mine, theirs) {
        (None | Some(Value::Object(_)), Value::Object(m), Value::Object(t)) => {
            let empty = serde_json::Map::new();
            let b = match base {
                Some(Value::Object(b)) => b,
                _ => &empty,
            };
            let mut out = serde_json::Map::new();
            for (key, value) in m {
                if let Some(v) = merge_value(b.get(key), value, t.get(key)) {
                    out.insert(key.clone(), v);
                }
            }
            // Entrées du fichier seul, sauf celles que la session a retirées.
            for (key, value) in t {
                if !m.contains_key(key) && !b.contains_key(key) {
                    out.insert(key.clone(), value.clone());
                }
            }
            Some(Value::Object(out))
        }
        (None | Some(Value::Array(_)), Value::Array(m), Value::Array(t)) => {
            let b = match base {
                Some(Value::Array(b)) => b.as_slice(),
                _ => &[],
            };
            let mut out: Vec<Value> = m.iter().filter(|v| !b.contains(v)).cloned().collect();
            for v in t {
                let removed = b.contains(v) && !m.contains(v);
                if !removed && !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Some(Value::Array(out))
        }
        _ => Some(mine.clone()),
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

    /// Fichier tenu ouvert par un autre programme (antivirus, synchronisation…) : `share` est ce qu'il laisse faire
    /// aux autres (0 : rien, pas même lire).
    #[cfg(windows)]
    fn hold_open(file: &Path, share: u32) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).share_mode(share).open(file).unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn a_locked_settings_file_is_never_overwritten_and_is_merged_once_it_can_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let original = r#"{"theme":"Dark","show_tags":false,"favorites":["C:\\DEV\\Fabrikam"]}"#;
        std::fs::write(&file, original).unwrap();
        // Verrouillé au démarrage : réglages par défaut, erreur signalée.
        let lock = hold_open(&file, 0);
        let (mut store, mut settings, error) = SettingsFile::load(&file);
        assert_eq!(settings, Settings::default());
        assert!(error.is_some(), "l'erreur de lecture est signalée");
        assert!(store.read_error().is_some());
        assert!(!store.unsaved(&settings), "rien de modifié pour l'instant");

        // Modifications de la session, fichier toujours verrouillé : rien n'est écrit.
        settings.show_remote_branches = false;
        settings.favorites.push(PathBuf::from(r"C:\DEV\Contoso"));
        assert!(store.unsaved(&settings));
        assert_eq!(store.save(&settings).unwrap(), Saved::NotWritten);
        drop(lock);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), original, "le fichier n'est jamais écrasé");
        assert!(store.unsaved(&settings), "modifications toujours en attente");

        // Lisible : relu à l'enregistrement suivant, fusionné puis écrit.
        let saved = store.save(&settings).unwrap();
        let Saved::Recovered(merged) = saved else { panic!("fichier relu attendu : {saved:?}") };
        assert_eq!(merged.theme, ThemeChoice::Dark, "réglage que la session n'a pas changé : celui du fichier");
        assert!(!merged.show_tags, "idem");
        assert!(!merged.show_remote_branches, "réglage changé pendant la session : celui de la session");
        assert_eq!(merged.favorites, [PathBuf::from(r"C:\DEV\Contoso"), PathBuf::from(r"C:\DEV\Fabrikam")]);
        assert_eq!(Settings::load_from(&file), *merged, "fusion écrite");
        assert!(store.read_error().is_none() && !store.unsaved(&merged));
        assert_eq!(store.save(&merged).unwrap(), Saved::Written, "ensuite, enregistrements habituels");
    }

    #[cfg(windows)]
    #[test]
    fn an_invalid_settings_file_that_cannot_be_set_aside_is_never_overwritten() {
        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ contenu invalide").unwrap();
        // Lisible mais impossible à renommer pendant qu'un autre programme le tient ouvert.
        let held = hold_open(&file, FILE_SHARE_READ);
        let (mut store, mut settings, error) = SettingsFile::load(&file);
        assert!(error.is_some());
        settings.show_tags = false;
        assert_eq!(store.save(&settings).unwrap(), Saved::NotWritten);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ contenu invalide");
        drop(held);

        // Toujours invalide, mais déplaçable : mis de côté comme au démarrage, réglages de la session écrits.
        let Saved::SetAside(msg) = store.save(&settings).unwrap() else { panic!("fichier mis de côté attendu") };
        assert_eq!(Settings::load_from(&file), settings);
        let backup = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p != &file)
            .expect("copie du fichier invalide");
        assert!(msg.contains(&backup.display().to_string()), "{msg}");
        assert_eq!(std::fs::read_to_string(backup).unwrap(), "{ contenu invalide");
        assert!(!store.unsaved(&settings));
    }

    #[test]
    fn the_merge_keeps_the_file_values_the_session_did_not_change() {
        use crate::external_tools::ToolPreset;
        let base = Settings::default();
        let account = |id: &str| Account { id: id.into(), ..account("x") };
        let repo_a = Path::new(r"C:\DEV\Fabrikam");
        let repo_b = Path::new(r"C:\DEV\Contoso");
        let mut theirs = Settings {
            theme: ThemeChoice::Dark,
            ui_scale: 1.2,
            show_tags: false,
            recent_repos: vec![repo_a.into(), repo_b.into()],
            accounts: vec![account("compte-fichier")],
            ..Settings::default()
        };
        theirs.scan_skip_dirs.push("vendor".into());
        theirs.diff_tool = ToolChoice::Preset(ToolPreset::WinMerge);
        theirs.merge_tool = ToolChoice::Preset(ToolPreset::KDiff3);
        theirs.toggle_hidden_ref(repo_a, "refs/heads/fichier");
        theirs.toggle_hidden_ref(repo_b, "refs/heads/autre");
        theirs.set_account_for_repo(repo_b, Some("compte-fichier"));

        let mut mine = base.clone();
        mine.theme = ThemeChoice::Light;
        mine.push_recent(Path::new(r"C:\DEV\Northwind"));
        mine.accounts.push(account("compte-session"));
        let removed = mine.scan_skip_dirs.remove(0);
        mine.toggle_hidden_ref(repo_a, "refs/heads/session");
        mine.diff_tool = ToolChoice::Preset(ToolPreset::VsCode);

        let merged = merge_session(&base, &mine, &theirs);
        // Réglages simples : la session pour ceux qu'elle a changés, le fichier pour les autres.
        assert_eq!(merged.theme, ThemeChoice::Light);
        // Énumérations à données : `{"Preset": …}` contre "Git" au départ.
        assert_eq!(merged.diff_tool, ToolChoice::Preset(ToolPreset::VsCode));
        assert_eq!(merged.merge_tool, ToolChoice::Preset(ToolPreset::KDiff3));
        assert_eq!((merged.ui_scale, merged.show_tags), (1.2, false));
        // Listes : ajouts de la session en tête, puis celle du fichier sans ce que la session a retiré.
        assert_eq!(merged.recent_repos, [Path::new(r"C:\DEV\Northwind"), repo_a, repo_b]);
        let ids: Vec<_> = merged.accounts.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["compte-session", "compte-fichier"]);
        assert!(!merged.scan_skip_dirs.contains(&removed), "retiré pendant la session");
        assert!(merged.scan_skip_dirs.iter().any(|d| d == "vendor"), "ajouté dans le fichier");
        // Tables par dépôt : entrée par entrée.
        assert_eq!(merged.hidden_refs_of(repo_a), ["refs/heads/session", "refs/heads/fichier"]);
        assert_eq!(merged.hidden_refs_of(repo_b), ["refs/heads/autre"]);
        assert_eq!(merged.account_for_repo(repo_b).map(|a| a.id.as_str()), Some("compte-fichier"));

        // Session sans modification : les réglages du fichier tels quels ; plafonds des listes respectés.
        assert_eq!(merge_session(&base, &base, &theirs), theirs);
        let mut many = base.clone();
        for i in 0..20 {
            many.push_recent(&PathBuf::from(format!(r"C:\DEV\r{i}")));
        }
        assert_eq!(merge_session(&base, &many, &theirs).recent_repos.len(), 20);
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
