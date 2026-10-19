//! Registre des commandes de l'interface : la seule source des entrées de la barre de menus, des raccourcis
//! clavier et des commandes de la palette Ctrl+P.
//!
//! Ajouter une commande :
//! 1. une variante de [`Command`], ajoutée à [`Command::ALL`] ;
//! 2. son libellé ([`Command::label`]), sa portée ([`Command::scope`]), son icône pour la palette
//!    ([`Command::icon`]) et son raccourci éventuel ([`Command::shortcut`]) : un raccourci n'est déclaré qu'ici ;
//! 3. sa place dans [`MENUS`] (une entrée, un sous-menu…), ou dans [`NOT_IN_MENUS`] si elle n'a pas d'entrée
//!    de menu (raccourci ou palette seulement) ; [`Command::in_palette`] l'écarte de la palette au besoin ;
//! 4. son traitement, choisi d'après la portée : `DragonTreeApp::run_app_command` (portée App, `app.rs`) ou
//!    `RepoTab::run_command` (portée Repo, `repo/commands.rs`) ; une commande sans traitement déclenche une
//!    assertion en debug ;
//! 5. si besoin, son état calculé à l'affichage : coché pour une entrée à bascule ([`Command::checked`]),
//!    grisé quand elle n'est pas utilisable : `RepoTab::can_run` (`repo/commands.rs`) pour une commande de
//!    dépôt ; pour une commande App, un champ de [`CommandContext`] rempli par `DragonTreeApp::command_context`
//!    et un cas dans [`Command::enabled`]. Une commande grisée ne fait rien, quelle que soit la façon de la
//!    lancer (menu, raccourci, palette).

use dragontree_i18n::{Lang, tr, tr_mark};
use egui::{Key, KeyboardShortcut, ModifierNames, Modifiers};

use crate::icons;
use crate::repo::{RepoTab, RepoView};
use crate::settings::{HistoryLayout, Settings, ThemeChoice};

/// Commande de l'interface : entrée de menu, raccourci éventuel, palette Ctrl+P.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    NewTab,
    OpenRepo,
    Clone,
    InitRepo,
    CloseTab,
    ReopenClosedTab,
    QuickOpen,
    Accounts,
    SshKeys,
    /// Ferme l'application (pas de raccourci propre : Alt+F4 reste celui du système).
    Quit,
    ShowChanges,
    ShowHistory,
    CurrentBranchOnly,
    ToggleRepoList,
    ShowLog,
    /// Passe à l'historique et sélectionne le commit de HEAD.
    RevealHead,
    ToggleDetailsPanel,
    /// Place du panneau de détails de l'historique (sous la liste ou à sa droite).
    Layout(HistoryLayout),
    /// Tri des commits de l'historique : `true` topologique, `false` par date.
    SortCommits(bool),
    /// Affiche (ou masque) les tags dans le graphe.
    ShowTags,
    /// Affiche (ou masque) les branches distantes dans le graphe.
    ShowRemoteBranches,
    /// Affiche (ou masque) les stashs dans le graphe.
    ShowStashes,
    /// Affiche (ou masque) les fichiers ignorés dans la vue Changements.
    ShowIgnoredFiles,
    /// Masque (ou réaffiche) les fichiers non suivis dans la vue Changements.
    HideUntrackedFiles,
    /// Réaffiche toutes les branches, dossiers et remotes masqués du graphe du dépôt.
    ShowHiddenRefs,
    /// Ajoute au graphe les commits que seul le reflog atteint (propre à l'onglet).
    ShowLostCommits,
    /// Replie toutes les fusions de l'historique (propre à l'onglet).
    CollapseAllMerges,
    /// Déplie toutes les fusions de l'historique.
    ExpandAllMerges,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    /// Thème de l'interface.
    Theme(ThemeChoice),
    Refresh,
    Fetch,
    Pull,
    /// Pull de la branche suivie sans dialogue (aussi : Ctrl+clic sur Pull).
    QuickPull,
    Push,
    /// Push de la branche courante vers sa branche suivie, sans dialogue (aussi : Ctrl+clic sur Push).
    QuickPush,
    NewBranch,
    NewTag,
    /// Nouveau worktree, la branche choisie dans le dialogue (les menus contextuels en créent aussi).
    CreateWorktree,
    /// Ajoute un sous-module (URL, chemin et branche facultative demandés dans un dialogue).
    AddSubmodule,
    Stash,
    /// Enregistre un stash sans toucher aux modifications en cours.
    Snapshot,
    /// Ajoute un motif au .gitignore, avec un aperçu des fichiers concernés.
    IgnoreFiles,
    /// Applique un patch lu dans un fichier choisi (aussi : fichier déposé sur la fenêtre).
    ApplyPatch,
    /// Applique un patch collé dans une zone de texte (le presse-papiers n'est lisible que par un collage).
    PastePatch,
    /// Démarre une session de bisect avec HEAD mauvais.
    BisectStart,
    /// Termine la session de bisect et revient à la branche de départ.
    BisectReset,
    /// Initialise Git Flow : branches, préfixes et tag de version (clés `gitflow.*`).
    GitFlowInit,
    GitFlowStartFeature,
    GitFlowStartRelease,
    GitFlowStartHotfix,
    /// Termine la feature, la release ou le hotfix de la branche courante.
    GitFlowFinish,
    /// Active Git LFS dans ce dépôt (`git lfs install --local`).
    LfsInstall,
    /// Suit un motif de fichiers avec LFS (`git lfs track`).
    LfsTrack,
    LfsUntrack,
    LfsFetch,
    LfsPull,
    LfsPrune,
    /// Fenêtre des verrous LFS du serveur.
    LfsLocks,
    OpenExplorer,
    OpenTerminal,
    OpenVsCode,
    /// Réglages du dépôt : identité locale, liens vers les tickets, modèle de message de commit.
    RepoSettings,
    Settings,
    /// Langue de l'interface ; `None` : celle de Windows.
    Language(Option<Lang>),
}

/// À qui s'adresse une commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// L'application (onglets, fenêtres, réglages).
    App,
    /// L'onglet actif : grisée s'il n'est pas un dépôt ouvert.
    Repo,
}

/// Élément d'un menu de la barre.
pub enum MenuItem {
    Cmd(Command),
    Separator,
    /// Sous-menu : titre (marqué par `tr_mark!`) et entrées.
    Submenu(&'static str, &'static [MenuItem]),
}

use MenuItem::{Cmd, Separator, Submenu};

/// Contenu de la barre de menus : (titre du menu marqué par `tr_mark!`, entrées).
pub const MENUS: &[(&str, &[MenuItem])] = &[
    (
        tr_mark!("Fichier"),
        &[
            Cmd(Command::NewTab),
            Cmd(Command::OpenRepo),
            Cmd(Command::Clone),
            Cmd(Command::InitRepo),
            Separator,
            Cmd(Command::CloseTab),
            Cmd(Command::ReopenClosedTab),
            Separator,
            Cmd(Command::QuickOpen),
            Cmd(Command::Accounts),
            Cmd(Command::SshKeys),
            Separator,
            Cmd(Command::Quit),
        ],
    ),
    (
        tr_mark!("Affichage"),
        &[
            Cmd(Command::ShowChanges),
            Cmd(Command::ShowHistory),
            Cmd(Command::CurrentBranchOnly),
            Submenu(tr_mark!("Tri des commits"), &[Cmd(Command::SortCommits(false)), Cmd(Command::SortCommits(true))]),
            Separator,
            Cmd(Command::ShowTags),
            Cmd(Command::ShowRemoteBranches),
            Cmd(Command::ShowStashes),
            Cmd(Command::ShowHiddenRefs),
            Cmd(Command::ShowLostCommits),
            Separator,
            Cmd(Command::CollapseAllMerges),
            Cmd(Command::ExpandAllMerges),
            Separator,
            Cmd(Command::ShowIgnoredFiles),
            Cmd(Command::HideUntrackedFiles),
            Separator,
            Cmd(Command::ToggleRepoList),
            Cmd(Command::ShowLog),
            Cmd(Command::RevealHead),
            Cmd(Command::ToggleDetailsPanel),
            Submenu(
                tr_mark!("Disposition"),
                &[Cmd(Command::Layout(HistoryLayout::Horizontal)), Cmd(Command::Layout(HistoryLayout::Vertical))],
            ),
            Separator,
            Cmd(Command::ZoomIn),
            Cmd(Command::ZoomOut),
            Cmd(Command::ZoomReset),
            Submenu(
                tr_mark!("Thème"),
                &[
                    Cmd(Command::Theme(ThemeChoice::System)),
                    Cmd(Command::Theme(ThemeChoice::Light)),
                    Cmd(Command::Theme(ThemeChoice::Dark)),
                ],
            ),
        ],
    ),
    (
        tr_mark!("Dépôt"),
        &[
            Cmd(Command::Refresh),
            Separator,
            Cmd(Command::Fetch),
            Cmd(Command::Pull),
            Cmd(Command::QuickPull),
            Cmd(Command::Push),
            Cmd(Command::QuickPush),
            Separator,
            Cmd(Command::NewBranch),
            Cmd(Command::NewTag),
            Cmd(Command::CreateWorktree),
            Cmd(Command::AddSubmodule),
            Cmd(Command::Stash),
            Cmd(Command::Snapshot),
            Cmd(Command::IgnoreFiles),
            Separator,
            Cmd(Command::ApplyPatch),
            Cmd(Command::PastePatch),
            Separator,
            Submenu(tr_mark!("Bisect"), &[Cmd(Command::BisectStart), Cmd(Command::BisectReset)]),
            Submenu(
                tr_mark!("Git Flow"),
                &[
                    Cmd(Command::GitFlowInit),
                    Separator,
                    Cmd(Command::GitFlowStartFeature),
                    Cmd(Command::GitFlowStartRelease),
                    Cmd(Command::GitFlowStartHotfix),
                    Separator,
                    Cmd(Command::GitFlowFinish),
                ],
            ),
            Submenu(
                tr_mark!("Git LFS"),
                &[
                    Cmd(Command::LfsInstall),
                    Separator,
                    Cmd(Command::LfsTrack),
                    Cmd(Command::LfsUntrack),
                    Separator,
                    Cmd(Command::LfsFetch),
                    Cmd(Command::LfsPull),
                    Cmd(Command::LfsPrune),
                    Separator,
                    Cmd(Command::LfsLocks),
                ],
            ),
            Separator,
            Cmd(Command::OpenExplorer),
            Cmd(Command::OpenTerminal),
            Cmd(Command::OpenVsCode),
            Separator,
            Cmd(Command::RepoSettings),
        ],
    ),
    (
        tr_mark!("Configuration"),
        &[
            Cmd(Command::Settings),
            Submenu(
                tr_mark!("Langue"),
                &[
                    Cmd(Command::Language(None)),
                    Cmd(Command::Language(Some(Lang::Fr))),
                    Cmd(Command::Language(Some(Lang::En))),
                ],
            ),
            Separator,
            Cmd(Command::Accounts),
        ],
    ),
];

/// Commandes sans entrée de menu (raccourci ou palette seulement).
pub const NOT_IN_MENUS: &[Command] = &[];

/// État lu pour griser ou cocher les entrées, à chaque affichage et avant d'exécuter une commande. Construit
/// en un seul point, `DragonTreeApp::command_context`, pour la barre de menus, les raccourcis et la palette.
pub struct CommandContext<'a> {
    pub settings: &'a Settings,
    /// Onglet actif s'il s'agit d'un dépôt ouvert (chargé sans erreur).
    pub repo: Option<&'a RepoTab>,
    /// Au moins un onglet fermé peut être rouvert.
    pub can_reopen_tab: bool,
    /// Git LFS est installé : `None` tant que la détection n'a pas répondu.
    pub lfs_available: Option<bool>,
}

impl Command {
    /// Toutes les commandes, dans l'ordre de la palette.
    pub const ALL: &[Command] = &[
        Command::NewTab,
        Command::OpenRepo,
        Command::Clone,
        Command::InitRepo,
        Command::CloseTab,
        Command::ReopenClosedTab,
        Command::QuickOpen,
        Command::Accounts,
        Command::SshKeys,
        Command::Quit,
        Command::ShowChanges,
        Command::ShowHistory,
        Command::CurrentBranchOnly,
        Command::ToggleRepoList,
        Command::ShowLog,
        Command::RevealHead,
        Command::ToggleDetailsPanel,
        Command::Layout(HistoryLayout::Horizontal),
        Command::Layout(HistoryLayout::Vertical),
        Command::SortCommits(false),
        Command::SortCommits(true),
        Command::ShowTags,
        Command::ShowRemoteBranches,
        Command::ShowStashes,
        Command::ShowIgnoredFiles,
        Command::HideUntrackedFiles,
        Command::ShowHiddenRefs,
        Command::ShowLostCommits,
        Command::CollapseAllMerges,
        Command::ExpandAllMerges,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ZoomReset,
        Command::Theme(ThemeChoice::System),
        Command::Theme(ThemeChoice::Light),
        Command::Theme(ThemeChoice::Dark),
        Command::Refresh,
        Command::Fetch,
        Command::Pull,
        Command::QuickPull,
        Command::Push,
        Command::QuickPush,
        Command::NewBranch,
        Command::NewTag,
        Command::CreateWorktree,
        Command::AddSubmodule,
        Command::Stash,
        Command::Snapshot,
        Command::IgnoreFiles,
        Command::ApplyPatch,
        Command::PastePatch,
        Command::BisectStart,
        Command::BisectReset,
        Command::GitFlowInit,
        Command::GitFlowStartFeature,
        Command::GitFlowStartRelease,
        Command::GitFlowStartHotfix,
        Command::GitFlowFinish,
        Command::LfsInstall,
        Command::LfsTrack,
        Command::LfsUntrack,
        Command::LfsFetch,
        Command::LfsPull,
        Command::LfsPrune,
        Command::LfsLocks,
        Command::OpenExplorer,
        Command::OpenTerminal,
        Command::OpenVsCode,
        Command::RepoSettings,
        Command::Settings,
        Command::Language(None),
        Command::Language(Some(Lang::Fr)),
        Command::Language(Some(Lang::En)),
    ];

    /// Libellé affiché (traduit).
    pub fn label(self) -> &'static str {
        match self {
            Command::NewTab => tr!("Nouvel onglet"),
            Command::OpenRepo => tr!("Ouvrir un dépôt…"),
            Command::Clone => tr!("Cloner un dépôt…"),
            Command::InitRepo => tr!("Initialiser un dépôt…"),
            Command::CloseTab => tr!("Fermer l'onglet"),
            Command::ReopenClosedTab => tr!("Rouvrir l'onglet fermé"),
            Command::QuickOpen => tr!("Palette de commandes"),
            Command::Accounts => tr!("Comptes…"),
            Command::SshKeys => tr!("Clés SSH…"),
            Command::Quit => tr!("Quitter"),
            Command::ShowChanges => tr!("Modifications locales"),
            Command::ShowHistory => tr!("Tous les commits"),
            Command::CurrentBranchOnly => tr!("Branche courante seulement"),
            Command::ToggleRepoList => tr!("Liste des dépôts"),
            Command::ShowLog => tr!("Journal git"),
            Command::RevealHead => tr!("Révéler HEAD"),
            Command::ToggleDetailsPanel => tr!("Panneau de détails"),
            Command::Layout(HistoryLayout::Horizontal) => tr!("Horizontale (détails à droite)"),
            Command::Layout(HistoryLayout::Vertical) => tr!("Verticale (détails sous la liste)"),
            Command::SortCommits(false) => tr!("Par date"),
            Command::SortCommits(true) => tr!("Topologique"),
            Command::ShowTags => tr!("Afficher les tags"),
            Command::ShowRemoteBranches => tr!("Afficher les branches distantes"),
            Command::ShowStashes => tr!("Afficher les stashs"),
            Command::ShowIgnoredFiles => tr!("Afficher les fichiers ignorés"),
            Command::HideUntrackedFiles => tr!("Masquer les fichiers non suivis"),
            Command::ShowHiddenRefs => tr!("Réafficher toutes les branches masquées"),
            Command::ShowLostCommits => tr!("Commits perdus"),
            Command::CollapseAllMerges => tr!("Replier toutes les fusions"),
            Command::ExpandAllMerges => tr!("Déplier toutes les fusions"),
            Command::ZoomIn => tr!("Zoom avant"),
            Command::ZoomOut => tr!("Zoom arrière"),
            Command::ZoomReset => tr!("Taille par défaut"),
            Command::Theme(ThemeChoice::System) => tr!("Système"),
            Command::Theme(ThemeChoice::Light) => tr!("Clair"),
            Command::Theme(ThemeChoice::Dark) => tr!("Sombre"),
            Command::Refresh => tr!("Actualiser"),
            Command::Fetch => "Fetch",
            Command::Pull => "Pull…",
            Command::QuickPull => tr!("Pull rapide"),
            Command::Push => "Push…",
            Command::QuickPush => tr!("Push rapide"),
            Command::NewBranch => tr!("Nouvelle branche…"),
            Command::NewTag => tr!("Nouveau tag…"),
            Command::CreateWorktree => tr!("Créer un worktree…"),
            Command::AddSubmodule => tr!("Ajouter un sous-module…"),
            Command::Stash => "Stash…",
            Command::Snapshot => tr!("Snapshot…"),
            Command::IgnoreFiles => tr!("Ignorer des fichiers…"),
            Command::ApplyPatch => tr!("Appliquer un patch…"),
            Command::PastePatch => tr!("Coller un patch…"),
            Command::BisectStart => tr!("Démarrer"),
            Command::BisectReset => tr!("Terminer"),
            Command::GitFlowInit => tr!("Initialiser Git Flow…"),
            Command::GitFlowStartFeature => tr!("Démarrer une feature…"),
            Command::GitFlowStartRelease => tr!("Démarrer une release…"),
            Command::GitFlowStartHotfix => tr!("Démarrer un hotfix…"),
            Command::GitFlowFinish => tr!("Terminer la branche Git Flow…"),
            Command::LfsInstall => tr!("Initialiser Git LFS"),
            Command::LfsTrack => tr!("Suivre un motif…"),
            Command::LfsUntrack => tr!("Ne plus suivre…"),
            Command::LfsFetch => tr!("Fetch LFS"),
            Command::LfsPull => tr!("Pull LFS"),
            Command::LfsPrune => tr!("Prune LFS"),
            Command::LfsLocks => tr!("Verrous LFS…"),
            Command::OpenExplorer => tr!("Ouvrir dans l'Explorateur"),
            Command::OpenTerminal => tr!("Ouvrir un terminal"),
            Command::OpenVsCode => tr!("Ouvrir dans VS Code"),
            Command::RepoSettings => tr!("Réglages du dépôt…"),
            Command::Settings => tr!("Réglages…"),
            Command::Language(None) => tr!("Système"),
            // Chaque langue est nommée dans sa propre langue.
            Command::Language(Some(lang)) => lang.native_name(),
        }
    }

    /// Libellé dans la palette Ctrl+P : celui du menu, précisé par la famille de la commande quand il serait
    /// ambigu hors de son sous-menu (« Suivre un motif… » devient « Git LFS : suivre un motif… »).
    pub fn palette_label(self) -> &'static str {
        match self {
            Command::GitFlowInit => tr!("Git Flow : initialiser…"),
            Command::GitFlowStartFeature => tr!("Git Flow : démarrer une feature…"),
            Command::GitFlowStartRelease => tr!("Git Flow : démarrer une release…"),
            Command::GitFlowStartHotfix => tr!("Git Flow : démarrer un hotfix…"),
            Command::GitFlowFinish => tr!("Git Flow : terminer la branche…"),
            Command::LfsInstall => tr!("Git LFS : initialiser"),
            Command::LfsTrack => tr!("Git LFS : suivre un motif…"),
            Command::LfsUntrack => tr!("Git LFS : ne plus suivre un motif…"),
            Command::LfsFetch => tr!("Git LFS : fetch"),
            Command::LfsPull => tr!("Git LFS : pull"),
            Command::LfsPrune => tr!("Git LFS : prune"),
            Command::LfsLocks => tr!("Git LFS : verrous…"),
            _ => self.label(),
        }
    }

    /// Icône de la commande dans la palette.
    pub fn icon(self) -> &'static str {
        match self {
            Command::NewTab => icons::PLUS,
            Command::OpenRepo => icons::FOLDER_OPEN,
            Command::Clone => icons::DOWNLOAD_SIMPLE,
            Command::InitRepo => icons::FOLDER_PLUS,
            Command::CloseTab => icons::X,
            Command::ReopenClosedTab => icons::ARROW_COUNTER_CLOCKWISE,
            Command::QuickOpen => icons::MAGNIFYING_GLASS,
            Command::Accounts => icons::USER_CIRCLE,
            Command::SshKeys => icons::KEY,
            Command::Quit => icons::SIGN_OUT,
            Command::ShowChanges => icons::GIT_DIFF,
            Command::ShowHistory => icons::GIT_COMMIT,
            Command::CurrentBranchOnly => icons::GIT_BRANCH,
            Command::ToggleRepoList => icons::SIDEBAR_SIMPLE,
            Command::ShowLog => icons::LIST,
            Command::RevealHead => icons::CROSSHAIR,
            Command::ToggleDetailsPanel => icons::SIDEBAR_SIMPLE,
            Command::Layout(HistoryLayout::Horizontal) => icons::COLUMNS,
            Command::Layout(HistoryLayout::Vertical) => icons::ROWS,
            Command::SortCommits(_) => icons::SORT_ASCENDING,
            Command::ShowTags => icons::TAG,
            Command::ShowRemoteBranches => icons::CLOUD,
            Command::ShowStashes => icons::STACK,
            Command::ShowIgnoredFiles => icons::FILE_DASHED,
            Command::HideUntrackedFiles => icons::FILE_MINUS,
            Command::ShowHiddenRefs => icons::EYE,
            Command::ShowLostCommits => icons::GHOST,
            Command::CollapseAllMerges => icons::ARROWS_IN_LINE_VERTICAL,
            Command::ExpandAllMerges => icons::ARROWS_OUT_LINE_VERTICAL,
            Command::ZoomIn => icons::MAGNIFYING_GLASS_PLUS,
            Command::ZoomOut => icons::MAGNIFYING_GLASS_MINUS,
            Command::ZoomReset => icons::MAGNIFYING_GLASS,
            Command::Theme(_) => icons::CIRCLE_HALF,
            Command::Refresh => icons::ARROWS_CLOCKWISE,
            Command::Fetch => icons::CLOUD_ARROW_DOWN,
            Command::Pull => icons::ARROW_LINE_DOWN,
            Command::QuickPull => icons::LIGHTNING,
            Command::Push => icons::ARROW_LINE_UP,
            Command::QuickPush => icons::LIGHTNING,
            Command::NewBranch => icons::GIT_BRANCH,
            Command::NewTag => icons::TAG,
            Command::CreateWorktree => icons::TREE_STRUCTURE,
            Command::AddSubmodule => icons::CUBE,
            Command::Stash => icons::TRAY_ARROW_DOWN,
            Command::Snapshot => icons::CAMERA,
            Command::IgnoreFiles => icons::EYE_SLASH,
            Command::ApplyPatch => icons::FILE_ARROW_DOWN,
            Command::PastePatch => icons::CLIPBOARD_TEXT,
            Command::BisectStart | Command::BisectReset => icons::BUG,
            Command::GitFlowInit
            | Command::GitFlowStartFeature
            | Command::GitFlowStartRelease
            | Command::GitFlowStartHotfix
            | Command::GitFlowFinish => icons::FLOW_ARROW,
            Command::LfsInstall => icons::HARD_DRIVES,
            Command::LfsTrack => icons::PLUS,
            Command::LfsUntrack => icons::MINUS,
            Command::LfsFetch => icons::CLOUD_ARROW_DOWN,
            Command::LfsPull => icons::ARROW_LINE_DOWN,
            Command::LfsPrune => icons::BROOM,
            Command::LfsLocks => icons::LOCK,
            Command::OpenExplorer => icons::FOLDER_OPEN,
            Command::OpenTerminal => icons::TERMINAL_WINDOW,
            Command::OpenVsCode => icons::CODE,
            Command::RepoSettings => icons::GEAR_SIX,
            Command::Settings => icons::GEAR_SIX,
            Command::Language(_) => icons::TRANSLATE,
        }
    }

    /// À qui s'adresse la commande (application ou dépôt de l'onglet actif).
    pub fn scope(self) -> Scope {
        match self {
            Command::NewTab
            | Command::OpenRepo
            | Command::Clone
            | Command::InitRepo
            | Command::CloseTab
            | Command::ReopenClosedTab
            | Command::QuickOpen
            | Command::Accounts
            | Command::SshKeys
            | Command::Quit
            | Command::ToggleRepoList
            | Command::ShowLog
            | Command::ToggleDetailsPanel
            | Command::Layout(_)
            | Command::SortCommits(_)
            | Command::ShowTags
            | Command::ShowRemoteBranches
            | Command::ShowStashes
            | Command::ShowIgnoredFiles
            | Command::HideUntrackedFiles
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::Theme(_)
            | Command::Settings
            | Command::Language(_) => Scope::App,
            Command::ShowChanges
            | Command::ShowHistory
            | Command::CurrentBranchOnly
            | Command::RevealHead
            | Command::ShowHiddenRefs
            | Command::ShowLostCommits
            | Command::CollapseAllMerges
            | Command::ExpandAllMerges
            | Command::Refresh
            | Command::Fetch
            | Command::Pull
            | Command::QuickPull
            | Command::Push
            | Command::QuickPush
            | Command::NewBranch
            | Command::NewTag
            | Command::CreateWorktree
            | Command::AddSubmodule
            | Command::Stash
            | Command::Snapshot
            | Command::IgnoreFiles
            | Command::ApplyPatch
            | Command::PastePatch
            | Command::BisectStart
            | Command::BisectReset
            | Command::GitFlowInit
            | Command::GitFlowStartFeature
            | Command::GitFlowStartRelease
            | Command::GitFlowStartHotfix
            | Command::GitFlowFinish
            | Command::LfsInstall
            | Command::LfsTrack
            | Command::LfsUntrack
            | Command::LfsFetch
            | Command::LfsPull
            | Command::LfsPrune
            | Command::LfsLocks
            | Command::OpenExplorer
            | Command::OpenTerminal
            | Command::OpenVsCode
            | Command::RepoSettings => Scope::Repo,
        }
    }

    /// Raccourci clavier, s'il y en a un (consommé par l'application, voir [`shortcuts`]).
    pub fn shortcut(self) -> Option<KeyboardShortcut> {
        let ctrl = |key| Some(KeyboardShortcut::new(Modifiers::COMMAND, key));
        let ctrl_shift = |key| Some(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, key));
        match self {
            Command::NewTab => ctrl(Key::T),
            Command::OpenRepo => ctrl(Key::O),
            Command::Clone => ctrl_shift(Key::C),
            Command::CloseTab => ctrl(Key::W),
            Command::ReopenClosedTab => ctrl_shift(Key::T),
            Command::QuickOpen => ctrl(Key::P),
            Command::ToggleRepoList => ctrl(Key::B),
            Command::CurrentBranchOnly => ctrl_shift(Key::A),
            Command::ShowLog => ctrl_shift(Key::L),
            // Maj+. : point sur QWERTY, Maj+; sur AZERTY ; egui reçoit « . » dans les deux cas.
            Command::ShowLostCommits => ctrl_shift(Key::Period),
            Command::RevealHead => ctrl(Key::Num0),
            Command::ToggleDetailsPanel => ctrl_shift(Key::D),
            Command::ZoomIn => ctrl(Key::Equals),
            Command::ZoomOut => ctrl(Key::Minus),
            Command::Fetch => ctrl_shift(Key::F),
            Command::Pull => ctrl_shift(Key::U),
            Command::Push => ctrl_shift(Key::P),
            Command::NewBranch => ctrl_shift(Key::B),
            Command::Stash => ctrl_shift(Key::H),
            Command::Refresh => Some(KeyboardShortcut::new(Modifiers::NONE, Key::F5)),
            Command::Settings => ctrl(Key::Comma),
            _ => None,
        }
    }

    /// Second raccourci de la même commande, non affiché (Ctrl++ : le « + » du pavé numérique ou Maj+=).
    pub fn alternate_shortcut(self) -> Option<KeyboardShortcut> {
        (self == Command::ZoomIn).then(|| KeyboardShortcut::new(Modifiers::COMMAND, Key::Plus))
    }

    /// Raccourci tel qu'affiché dans les menus (« Ctrl+Maj+C », « Ctrl+, »).
    pub fn shortcut_text(self) -> Option<String> {
        // Forme courte pour la touche (« , » plutôt que « Comma ») ; noms des modificateurs de Windows.
        let names = ModifierNames { is_short: true, shift: tr!("Maj"), ..ModifierNames::NAMES };
        self.shortcut().map(|s| s.format(&names, false))
    }

    /// La commande figure dans la palette Ctrl+P.
    pub fn in_palette(self) -> bool {
        !matches!(
            self,
            Command::QuickOpen
                | Command::Language(_)
                | Command::Theme(_)
                | Command::SortCommits(_)
                | Command::Layout(_)
                // « Démarrer » et « Terminer » n'ont de sens que sous le sous-menu Bisect.
                | Command::BisectStart
                | Command::BisectReset
        )
    }

    /// Commande de Git LFS : grisée tant que `git lfs` n'est pas détecté.
    fn is_lfs(self) -> bool {
        matches!(
            self,
            Command::LfsInstall
                | Command::LfsTrack
                | Command::LfsUntrack
                | Command::LfsFetch
                | Command::LfsPull
                | Command::LfsPrune
                | Command::LfsLocks
        )
    }

    /// Pourquoi la commande est indisponible quand ce n'est pas à cause de l'onglet (infobulle d'une entrée
    /// grisée) : Git LFS absent ou pas encore détecté.
    pub fn disabled_hint(self, cx: &CommandContext) -> Option<&'static str> {
        match (self.is_lfs(), cx.lfs_available) {
            (true, Some(false)) => Some(tr!("Git LFS n'est pas installé (la commande « git lfs » est introuvable).")),
            (true, None) => Some(tr!("Détection de Git LFS en cours…")),
            _ => None,
        }
    }

    /// La commande est utilisable (sinon son entrée est grisée et elle ne fait rien) : une commande de dépôt
    /// demande un dépôt ouvert dans l'onglet actif, puis ce que l'onglet exige (`RepoTab::can_run`).
    pub fn enabled(self, cx: &CommandContext) -> bool {
        if self.is_lfs() && cx.lfs_available != Some(true) {
            return false;
        }
        match self.scope() {
            Scope::App => self != Command::ReopenClosedTab || cx.can_reopen_tab,
            // Rien à réafficher : l'entrée est grisée.
            Scope::Repo if self == Command::ShowHiddenRefs => {
                cx.repo.is_some_and(|tab| tab.can_run(self) && !cx.settings.hidden_refs_of(&tab.path).is_empty())
            }
            Scope::Repo => cx.repo.is_some_and(|tab| tab.can_run(self)),
        }
    }

    /// État d'une entrée à bascule (cochée ou non) ; `None` pour une entrée ordinaire.
    pub fn checked(self, cx: &CommandContext) -> Option<bool> {
        let view = cx.repo.map(|tab| tab.view);
        match self {
            Command::ShowChanges => Some(view == Some(RepoView::Changes)),
            Command::ShowHistory => Some(view == Some(RepoView::History)),
            Command::CurrentBranchOnly => Some(cx.repo.is_some_and(|tab| tab.history.current_branch_only)),
            Command::ToggleRepoList => Some(cx.settings.repo_list_visible),
            Command::ToggleDetailsPanel => Some(cx.settings.details_panel_visible),
            Command::Layout(layout) => Some(cx.settings.history_layout == layout),
            Command::SortCommits(topological) => Some(cx.settings.topological_order == topological),
            Command::ShowLostCommits => Some(cx.repo.is_some_and(|tab| tab.history.show_lost)),
            Command::ShowTags => Some(cx.settings.show_tags),
            Command::ShowRemoteBranches => Some(cx.settings.show_remote_branches),
            Command::ShowStashes => Some(cx.settings.show_stashes),
            Command::ShowIgnoredFiles => Some(cx.settings.show_ignored_files),
            Command::HideUntrackedFiles => Some(cx.settings.hide_untracked_files),
            Command::Language(lang) => Some(cx.settings.language == lang),
            Command::Theme(choice) => Some(cx.settings.theme == choice),
            _ => None,
        }
    }
}

/// Pourquoi un sous-menu est grisé : toutes ses commandes le sont et l'une d'elles en donne la raison (Git LFS
/// absent). Un sous-menu dont les commandes sont grisées pour une autre raison reste ouvrable.
pub fn submenu_disabled_hint(items: &[MenuItem], cx: &CommandContext) -> Option<&'static str> {
    let mut commands = items.iter().filter_map(|item| if let Cmd(cmd) = item { Some(*cmd) } else { None });
    let all_disabled = commands.clone().all(|cmd| !cmd.enabled(cx));
    all_disabled.then(|| commands.find_map(|cmd| cmd.disabled_hint(cx))).flatten()
}

/// Raccourcis du registre dans l'ordre où les consommer. egui fait correspondre un raccourci à un appui qui
/// a en plus Maj ou Alt (Ctrl+T répond à Ctrl+Maj+T) : les plus spécifiques passent donc d'abord.
pub fn shortcuts() -> Vec<(KeyboardShortcut, Command)> {
    let mut list: Vec<_> = Command::ALL
        .iter()
        .flat_map(|&cmd| [cmd.shortcut(), cmd.alternate_shortcut()].into_iter().flatten().map(move |s| (s, cmd)))
        .collect();
    most_specific_first(&mut list);
    list
}

/// Trie par nombre de modificateurs décroissant (tri stable) : un raccourci ne correspond qu'à un appui qui a
/// au moins ses modificateurs, donc passe après tous ceux qui en ont plus.
fn most_specific_first<T>(list: &mut [(KeyboardShortcut, T)]) {
    let count = |m: Modifiers| u8::from(m.alt) + u8::from(m.shift) + u8::from(m.ctrl || m.command || m.mac_cmd);
    list.sort_by_key(|(shortcut, _)| std::cmp::Reverse(count(shortcut.modifiers)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu_commands(items: &[MenuItem], out: &mut Vec<Command>) {
        for item in items {
            match item {
                Cmd(cmd) => out.push(*cmd),
                Separator => {}
                Submenu(title, sub) => {
                    assert!(!title.trim().is_empty(), "sous-menu sans titre");
                    menu_commands(sub, out);
                }
            }
        }
    }

    fn all_menu_commands() -> Vec<Command> {
        let mut found = Vec::new();
        for (title, items) in MENUS {
            assert!(!title.trim().is_empty(), "menu sans titre");
            menu_commands(items, &mut found);
        }
        found
    }

    #[test]
    fn every_command_has_a_label() {
        for cmd in Command::ALL {
            assert!(!cmd.label().trim().is_empty(), "{cmd:?} sans libellé");
        }
    }

    #[test]
    fn commands_and_shortcuts_are_not_duplicated() {
        for (i, cmd) in Command::ALL.iter().enumerate() {
            assert!(!Command::ALL[i + 1..].contains(cmd), "{cmd:?} deux fois dans Command::ALL");
        }
        let list = shortcuts();
        for (i, (shortcut, cmd)) in list.iter().enumerate() {
            if let Some((_, other)) = list[i + 1..].iter().find(|(s, _)| s == shortcut) {
                panic!("{shortcut:?} déclaré pour {cmd:?} et {other:?}");
            }
        }
    }

    #[test]
    fn more_specific_shortcuts_are_consumed_first() {
        let t = |modifiers| KeyboardShortcut::new(modifiers, Key::T);
        let mut list =
            vec![(t(Modifiers::COMMAND), "Ctrl+T"), (t(Modifiers::COMMAND | Modifiers::SHIFT), "Ctrl+Maj+T")];
        most_specific_first(&mut list);
        assert_eq!(list.iter().map(|(_, name)| *name).collect::<Vec<_>>(), ["Ctrl+Maj+T", "Ctrl+T"]);

        // Dans le registre, aucun raccourci ne prend l'appui d'un raccourci consommé après lui.
        let list = shortcuts();
        for (i, (first, cmd)) in list.iter().enumerate() {
            for (later, other) in &list[i + 1..] {
                let steals =
                    first.logical_key == later.logical_key && later.modifiers.matches_logically(first.modifiers);
                assert!(!steals, "{cmd:?} ({first:?}) consommé avant {other:?} ({later:?})");
            }
        }
    }

    #[test]
    fn every_command_is_in_a_menu_or_explicitly_outside() {
        let in_menus = all_menu_commands();
        for cmd in Command::ALL {
            assert!(
                in_menus.contains(cmd) != NOT_IN_MENUS.contains(cmd),
                "{cmd:?} doit figurer soit dans MENUS, soit dans NOT_IN_MENUS"
            );
        }
        for cmd in in_menus.iter().chain(NOT_IN_MENUS) {
            assert!(Command::ALL.contains(cmd), "{cmd:?} absent de Command::ALL");
        }
    }

    /// Sous-menu du menu Dépôt, par son titre.
    fn repository_submenu(title: &str) -> &'static [MenuItem] {
        let (_, items) = MENUS.iter().find(|(t, _)| *t == tr_mark!("Dépôt")).unwrap();
        items
            .iter()
            .find_map(|item| match item {
                Submenu(t, sub) if *t == title => Some(*sub),
                _ => None,
            })
            .unwrap_or_else(|| panic!("sous-menu {title} absent"))
    }

    #[test]
    fn the_lfs_submenu_is_greyed_with_a_reason_while_git_lfs_is_missing_or_not_detected() {
        let settings = Settings::default();
        let cx =
            |lfs_available| CommandContext { settings: &settings, repo: None, can_reopen_tab: false, lfs_available };
        let lfs = repository_submenu(tr_mark!("Git LFS"));
        let missing = submenu_disabled_hint(lfs, &cx(Some(false))).expect("grisé quand git lfs est absent");
        assert!(missing.contains("Git LFS"), "{missing}");
        assert!(submenu_disabled_hint(lfs, &cx(None)).is_some(), "grisé tant que la détection n'a pas répondu");
        // git lfs présent : le sous-menu s'ouvre (ses entrées dépendent ensuite de l'onglet).
        assert_eq!(submenu_disabled_hint(lfs, &cx(Some(true))), None);
        // Chaque commande LFS est inutilisable sans git lfs, même avec un dépôt ouvert ailleurs dans le contexte.
        for item in lfs {
            if let Cmd(cmd) = item {
                assert!(!cmd.enabled(&cx(Some(false))), "{cmd:?} doit être grisée sans git lfs");
                assert!(cmd.disabled_hint(&cx(Some(false))).is_some());
            }
        }
        // Les autres sous-menus ne sont jamais grisés pour cette raison.
        assert_eq!(submenu_disabled_hint(repository_submenu(tr_mark!("Git Flow")), &cx(Some(false))), None);
    }

    #[test]
    fn git_flow_and_lfs_commands_name_their_family_in_the_palette() {
        for cmd in Command::ALL {
            let label = format!("{cmd:?}");
            let family = if label.starts_with("GitFlow") {
                "Git Flow"
            } else if label.starts_with("Lfs") {
                "Git LFS"
            } else {
                continue;
            };
            assert!(cmd.palette_label().starts_with(family), "{cmd:?} : {}", cmd.palette_label());
        }
    }

    #[test]
    fn language_menu_offers_the_system_language_and_every_language() {
        let in_menus = all_menu_commands();
        assert!(in_menus.contains(&Command::Language(None)));
        for lang in Lang::ALL {
            assert!(in_menus.contains(&Command::Language(Some(lang))), "{lang:?} absente du menu Langue");
        }
    }

    #[test]
    fn shortcuts_are_shown_with_windows_names() {
        assert_eq!(Command::Clone.shortcut_text().as_deref(), Some("Ctrl+Maj+C"));
        assert_eq!(Command::NewTab.shortcut_text().as_deref(), Some("Ctrl+T"));
        assert_eq!(Command::Settings.shortcut_text().as_deref(), Some("Ctrl+,"));
        assert_eq!(Command::Refresh.shortcut_text().as_deref(), Some("F5"));
        assert_eq!(Command::ShowLostCommits.shortcut_text().as_deref(), Some("Ctrl+Maj+."));
        assert_eq!(Command::InitRepo.shortcut_text(), None);
    }
}
