//! Ce que parcourt le graphe de l'historique : options du journal, références masquées et visibilité des
//! étiquettes de références.

use std::path::Path;

use dragontree_git::{LogOptions, LogOrder, LogScope, Oid, RefKind, RefLabel, RefSelection, RefsSnapshot};

use super::RepoTab;
use crate::app::Shared;

/// État d'une référence (ou d'un dossier de références) vis-à-vis des éléments masqués du graphe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Visibility {
    Shown,
    /// Masquée elle-même (son motif est dans la liste) : l'œil la réaffiche.
    Hidden,
    /// Masquée parce qu'un dossier qui la contient l'est (ce dossier doit être réaffiché d'abord).
    HiddenByFolder,
}

/// Visibilité de `pattern` (nom complet ou `dossier/*`) d'après les motifs masqués.
pub(super) fn visibility(hidden: &[String], pattern: &str) -> Visibility {
    if hidden.iter().any(|h| h == pattern) {
        return Visibility::Hidden;
    }
    let by_folder = hidden.iter().any(|h| h.strip_suffix('*').is_some_and(|folder| pattern.starts_with(folder)));
    if by_folder { Visibility::HiddenByFolder } else { Visibility::Shown }
}

/// Préfixe des références d'une arborescence de la barre latérale (`loc`, `tag`, `rem:<remote>`).
pub(super) fn namespace(tree: &str) -> String {
    match tree {
        "loc" => "refs/heads/".into(),
        "tag" => "refs/tags/".into(),
        _ => format!("refs/remotes/{}/", tree.strip_prefix("rem:").unwrap_or(tree)),
    }
}

/// Motif d'une branche locale.
pub(super) fn branch_pattern(name: &str) -> String {
    format!("refs/heads/{name}")
}

/// Motif d'une branche distante, d'après son nom court (`origin/feature/x`).
pub(super) fn remote_branch_pattern(full: &str) -> String {
    format!("refs/remotes/{full}")
}

/// Motif d'un remote entier.
pub(super) fn remote_pattern(remote: &str) -> String {
    format!("refs/remotes/{remote}/*")
}

/// Motif d'un dossier de références (`feature/`) de l'arborescence `tree` de la barre latérale.
pub(super) fn folder_pattern(tree: &str, path: &str) -> String {
    format!("{}{path}/*", namespace(tree))
}

/// Nom affiché d'un élément du filtre (bandeau) : le motif sans son préfixe `refs/heads/`, `refs/remotes/` ou
/// `refs/tags/`.
pub(super) fn display_name(pattern: &str) -> &str {
    ["refs/heads/", "refs/remotes/", "refs/tags/"].iter().find_map(|p| pattern.strip_prefix(p)).unwrap_or(pattern)
}

/// Libellé d'un stash : `stash@{n}  message` (barre latérale, détails, étiquette du graphe).
pub(super) fn stash_label(index: usize, message: &str) -> String {
    format!("stash@{{{index}}}  {message}")
}

/// Ce qui décide si une étiquette de référence s'affiche : réglages de l'affichage et éléments masqués.
pub(super) struct LabelVisibility {
    tags: bool,
    remotes: bool,
    hidden: Vec<String>,
}

impl LabelVisibility {
    pub fn of(shared: &Shared, repo: &Path) -> LabelVisibility {
        LabelVisibility {
            tags: shared.settings.show_tags,
            remotes: shared.settings.show_remote_branches,
            hidden: shared.settings.hidden_refs_of(repo).to_vec(),
        }
    }

    /// L'étiquette suit la visibilité des références que le graphe parcourt.
    pub fn shows(&self, label: &RefLabel) -> bool {
        let shown = |pattern: String| visibility(&self.hidden, &pattern) == Visibility::Shown;
        match label.kind {
            RefKind::Tag => self.tags && shown(format!("refs/tags/{}", label.name)),
            RefKind::Remote => self.remotes && shown(remote_branch_pattern(&label.name)),
            RefKind::Local => shown(branch_pattern(&label.name)),
            RefKind::Head => true,
        }
    }
}

impl RepoTab {
    /// Options du journal affichées par le graphe : l'unique point qui les construit. `max_count` reste à
    /// `None` (le premier lot le fixe).
    pub(crate) fn log_options(&self, shared: &Shared) -> LogOptions {
        let scope = if self.history.current_branch_only {
            LogScope::Head
        } else {
            // Un filtre sur des branches choisies remplace tout le reste (et HEAD) ; sinon HEAD reste parcourue
            // même si sa branche est masquée.
            let only = self.history.filter_refs.clone();
            LogScope::Refs(RefSelection {
                head: only.is_empty(),
                tags: shared.settings.show_tags,
                remotes: shared.settings.show_remote_branches,
                exclude: shared.settings.hidden_refs_of(&self.path).to_vec(),
                only,
                ..RefSelection::default()
            })
        };
        let order = if shared.settings.topological_order { LogOrder::Topo } else { LogOrder::Date };
        LogOptions { scope, order, ..LogOptions::default() }
    }

    /// Le graphe parcourt toutes les branches (ni « branche courante seulement », ni filtre sur des branches
    /// choisies). C'est le seul mode où s'ajoutent les commits perdus et les stashs : dans les autres, l'utilisateur
    /// a restreint la vue à des branches, et ces commits ramèneraient des historiques qu'il n'a pas demandés.
    pub(crate) fn walks_all_refs(&self) -> bool {
        !self.history.current_branch_only && self.history.filter_refs.is_empty()
    }

    /// Les commits perdus du reflog sont ajoutés au graphe : option cochée, en mode « toutes les branches ».
    pub(crate) fn shows_lost(&self) -> bool {
        self.history.show_lost && self.walks_all_refs()
    }

    /// Stashs candidats au graphe : réglage coché. Un stash n'y apparaît que si son commit de base est affiché par
    /// la sélection courante (voir `fold_stashes`) : dans tous les modes, jamais d'historique masqué qui revienne.
    pub(crate) fn shown_stashes(&self, shared: &Shared) -> Vec<Oid> {
        if shared.settings.show_stashes { self.stashes.iter().map(|s| s.oid).collect() } else { Vec::new() }
    }

    /// Sélection d'un commit du graphe : si c'est un stash, il se sélectionne comme dans la barre latérale (détails
    /// et diff par rapport à sa base, libellé « stash@{n} »).
    pub(super) fn stash_selection(&self, oid: Oid) -> Option<(Oid, String)> {
        let entry = self.stashes.iter().find(|s| s.oid == oid)?;
        Some((oid, stash_label(entry.index, &entry.message)))
    }

    /// Active ou désactive l'affichage des commits perdus ; relit le journal.
    pub(super) fn set_show_lost(&mut self, shared: &Shared, on: bool) {
        self.history.show_lost = on;
        self.load_log(shared);
    }

    /// Active ou désactive « branche courante seulement » (mémorisé pour le dépôt) ; l'activer retire le filtre sur des branches choisies
    /// (les deux ne se combinent pas) ; relit le journal.
    pub(super) fn set_current_branch_only(&mut self, shared: &mut Shared, on: bool) {
        self.history.current_branch_only = on;
        shared.settings.set_current_branch_only(&self.path, on);
        shared.settings_dirty = true;
        if on {
            self.history.filter_refs.clear();
        }
        self.load_log(shared);
    }

    /// Ajoute `pattern` au filtre de l'historique, ou l'en retire ; un filtre non vide désactive « branche
    /// courante seulement » (pour la session) ;
    /// retirer le dernier revient au choix mémorisé ; relit le journal.
    pub(super) fn toggle_filter_ref(&mut self, shared: &Shared, pattern: &str) {
        match self.history.filter_refs.iter().position(|p| p == pattern) {
            Some(i) => {
                self.history.filter_refs.remove(i);
            }
            None => self.history.filter_refs.push(pattern.to_string()),
        }
        if !self.history.filter_refs.is_empty() {
            self.history.current_branch_only = false;
        } else {
            self.history.current_branch_only = shared.settings.current_branch_only_of(&self.path);
        }
        self.load_log(shared);
    }

    /// Retire tout le filtre sur des branches choisies (« Tout afficher ») et revient au choix mémorisé
    /// « branche courante seulement » ; relit le journal.
    pub(super) fn clear_filter_refs(&mut self, shared: &Shared) {
        self.history.filter_refs.clear();
        self.history.current_branch_only = shared.settings.current_branch_only_of(&self.path);
        self.load_log(shared);
    }

    /// Retire du filtre les références précises qui n'existent plus (branche supprimée ou renommée) : git
    /// refuserait la révision. Les motifs (`dossier/*`) restent. Vrai si le filtre a changé.
    pub(super) fn prune_filter_refs(&mut self, refs: &RefsSnapshot) -> bool {
        let exists = |pattern: &str| {
            refs.locals.iter().any(|b| branch_pattern(&b.name) == pattern)
                || refs.remotes.iter().any(|b| remote_branch_pattern(&b.full) == pattern)
                || refs.tags.iter().any(|t| format!("refs/tags/{}", t.name) == pattern)
        };
        let before = self.history.filter_refs.len();
        self.history.filter_refs.retain(|p| p.contains(['*', '?', '[']) || exists(p));
        self.history.filter_refs.len() != before
    }

    /// Masque `pattern` du graphe, ou le réaffiche s'il l'était ; relit le journal.
    pub(super) fn toggle_hidden(&mut self, shared: &mut Shared, pattern: &str) {
        shared.settings.toggle_hidden_ref(&self.path, pattern);
        shared.settings_dirty = true;
        self.load_log(shared);
    }

    /// Réaffiche tout ce qui était masqué dans ce dépôt ; relit le journal.
    pub(super) fn show_all_hidden(&mut self, shared: &mut Shared) {
        shared.settings.clear_hidden_refs(&self.path);
        shared.settings_dirty = true;
        self.load_log(shared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Shared;

    fn hidden(patterns: &[&str]) -> Vec<String> {
        patterns.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn visibility_of_a_branch_a_folder_and_a_remote() {
        let h = hidden(&["refs/heads/x", "refs/heads/feature/*", "refs/remotes/origin/*"]);
        assert_eq!(visibility(&h, "refs/heads/x"), Visibility::Hidden);
        assert_eq!(visibility(&h, "refs/heads/xy"), Visibility::Shown);
        assert_eq!(visibility(&h, "refs/heads/feature/*"), Visibility::Hidden);
        assert_eq!(visibility(&h, "refs/heads/feature/a"), Visibility::HiddenByFolder);
        assert_eq!(visibility(&h, "refs/heads/feature/deep/*"), Visibility::HiddenByFolder);
        assert_eq!(visibility(&h, "refs/heads/featured"), Visibility::Shown);
        assert_eq!(visibility(&h, "refs/remotes/origin/main"), Visibility::HiddenByFolder);
        assert_eq!(visibility(&h, "refs/remotes/upstream/main"), Visibility::Shown);
        assert_eq!(visibility(&[], "refs/heads/x"), Visibility::Shown);
    }

    #[test]
    fn patterns_for_a_branch_a_folder_and_a_remote() {
        assert_eq!(branch_pattern("feature/x"), "refs/heads/feature/x");
        assert_eq!(folder_pattern("loc", "feature"), "refs/heads/feature/*");
        assert_eq!(folder_pattern("rem:origin", "feature/deep"), "refs/remotes/origin/feature/deep/*");
        assert_eq!(remote_pattern("origin"), "refs/remotes/origin/*");
        assert_eq!(remote_branch_pattern("origin/feature/x"), "refs/remotes/origin/feature/x");
        // Un motif de dossier masque ce que l'œil de ses lignes produirait.
        let h = vec![folder_pattern("loc", "feature")];
        assert_eq!(visibility(&h, &branch_pattern("feature/x")), Visibility::HiddenByFolder);
        assert_eq!(visibility(&h, &folder_pattern("loc", "feature/deep")), Visibility::HiddenByFolder);
    }

    #[test]
    fn the_banner_shows_names_without_the_ref_prefix() {
        assert_eq!(display_name("refs/heads/main"), "main");
        assert_eq!(display_name("refs/heads/feature/*"), "feature/*");
        assert_eq!(display_name("refs/remotes/origin/*"), "origin/*");
        assert_eq!(display_name("refs/tags/v1"), "v1");
    }

    /// Onglet sans git (aucune tâche lancée) et son état partagé.
    fn tab_without_git(dir: &std::path::Path) -> (RepoTab, Shared) {
        let (store, settings, _) = crate::settings::SettingsFile::load(&dir.join("settings.json"));
        let (mut shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        (RepoTab::new(1, dir.to_path_buf(), &mut shared), shared)
    }

    fn selection(tab: &RepoTab, shared: &Shared) -> RefSelection {
        match tab.log_options(shared).scope {
            LogScope::Refs(selection) => selection,
            other => panic!("sélection attendue, pas {other:?}"),
        }
    }

    #[test]
    fn log_options_follow_visibility_hidden_refs_and_the_branch_filter() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab_without_git(dir.path());
        assert_eq!(selection(&tab, &shared), RefSelection::default());

        shared.settings.show_tags = false;
        shared.settings.toggle_hidden_ref(&tab.path, "refs/heads/x");
        let sel = selection(&tab, &shared);
        assert!(!sel.tags && sel.remotes && sel.head);
        assert_eq!(sel.exclude, ["refs/heads/x"]);

        // Un filtre mêlant branche, dossier et remote : seulement ces références, sans HEAD.
        for pattern in ["refs/heads/main", "refs/heads/feature/*", "refs/remotes/origin/*"] {
            tab.toggle_filter_ref(&shared, pattern);
        }
        let sel = selection(&tab, &shared);
        assert!(!sel.head);
        assert_eq!(sel.only, ["refs/heads/main", "refs/heads/feature/*", "refs/remotes/origin/*"]);
        assert_eq!(sel.exclude, ["refs/heads/x"], "les éléments masqués le restent");
        tab.toggle_filter_ref(&shared, "refs/heads/feature/*");
        assert_eq!(selection(&tab, &shared).only, ["refs/heads/main", "refs/remotes/origin/*"]);
    }

    #[test]
    fn current_branch_only_and_the_branch_filter_exclude_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab_without_git(dir.path());
        tab.toggle_filter_ref(&shared, "refs/heads/a");
        tab.set_current_branch_only(&mut shared, true);
        assert!(tab.history.current_branch_only && tab.history.filter_refs.is_empty());
        assert_eq!(tab.log_options(&shared).scope, LogScope::Head);
        tab.toggle_filter_ref(&shared, "refs/heads/a");
        assert!(!tab.history.current_branch_only);
        assert_eq!(tab.history.filter_refs, ["refs/heads/a"]);
        tab.clear_filter_refs(&shared);
        assert!(tab.history.filter_refs.is_empty());
    }

    #[test]
    fn lost_commits_only_join_the_graph_of_all_branches() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab_without_git(dir.path());
        assert!(!tab.shows_lost(), "désactivé par défaut");
        tab.history.show_lost = true;
        assert!(tab.shows_lost());
        tab.toggle_filter_ref(&shared, "refs/heads/a");
        assert!(!tab.shows_lost(), "pas avec un filtre sur des branches");
        tab.clear_filter_refs(&shared);
        tab.set_current_branch_only(&mut shared, true);
        assert!(!tab.shows_lost(), "pas avec la branche courante seulement");
        assert!(tab.history.show_lost, "le choix est gardé pour le retour à toutes les branches");
    }

    #[test]
    fn stashes_are_never_extra_revisions_and_follow_the_setting() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab_without_git(dir.path());
        let oid = Oid::from_hex(&"c".repeat(40)).unwrap();
        tab.stashes =
            std::sync::Arc::new(vec![dragontree_git::StashEntry { index: 0, oid, message: "x".into(), time: 0 }]);
        assert_eq!(tab.shown_stashes(&shared), [oid]);
        // Un stash ne devient jamais une révision parcourue : elle ramènerait l'historique de sa base, masqué ou non.
        shared.settings.toggle_hidden_ref(&tab.path, "refs/heads/x");
        let sel = selection(&tab, &shared);
        assert_eq!((sel.exclude, sel.extra), (vec!["refs/heads/x".to_string()], Vec::new()));
        // Candidats dans tous les modes : `fold_stashes` ne garde que ceux dont la base est affichée.
        tab.toggle_filter_ref(&shared, "refs/heads/a");
        assert_eq!(tab.shown_stashes(&shared), [oid]);
        tab.clear_filter_refs(&shared);
        tab.set_current_branch_only(&mut shared, true);
        assert_eq!(tab.shown_stashes(&shared), [oid]);
        tab.set_current_branch_only(&mut shared, false);
        shared.settings.show_stashes = false;
        assert!(tab.shown_stashes(&shared).is_empty(), "réglage décoché");
    }

    #[test]
    fn a_stash_is_labelled_and_selected_like_in_the_sidebar() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, _shared) = tab_without_git(dir.path());
        let oid = Oid::from_hex(&"a".repeat(40)).unwrap();
        tab.stashes = std::sync::Arc::new(vec![dragontree_git::StashEntry {
            index: 2,
            oid,
            message: "On main: x".into(),
            time: 0,
        }]);
        assert_eq!(stash_label(2, "On main: x"), "stash@{2}  On main: x");
        assert_eq!(tab.stash_selection(oid), Some((oid, "stash@{2}  On main: x".to_string())));
        assert_eq!(tab.stash_selection(Oid::from_hex(&"b".repeat(40)).unwrap()), None);
    }

    #[test]
    fn labels_follow_the_tag_remote_and_hidden_settings() {
        let label = |kind, name: &str| RefLabel { kind, name: name.into(), is_head: false };
        let visibility = |tags, remotes, hidden: &[&str]| LabelVisibility {
            tags,
            remotes,
            hidden: hidden.iter().map(|p| p.to_string()).collect(),
        };
        let all = visibility(true, true, &[]);
        for l in [
            label(RefKind::Tag, "v1"),
            label(RefKind::Remote, "origin/main"),
            label(RefKind::Local, "main"),
            label(RefKind::Head, "HEAD"),
        ] {
            assert!(all.shows(&l), "{l:?}");
        }
        assert!(!visibility(false, true, &[]).shows(&label(RefKind::Tag, "v1")));
        assert!(visibility(false, true, &[]).shows(&label(RefKind::Remote, "origin/main")));
        assert!(!visibility(true, false, &[]).shows(&label(RefKind::Remote, "origin/main")));
        assert!(visibility(true, false, &[]).shows(&label(RefKind::Tag, "v1")));
        let hidden = visibility(true, true, &["refs/heads/x", "refs/remotes/origin/*"]);
        assert!(!hidden.shows(&label(RefKind::Local, "x")));
        assert!(hidden.shows(&label(RefKind::Local, "y")));
        assert!(!hidden.shows(&label(RefKind::Remote, "origin/main")));
        // HEAD s'affiche toujours, même quand tout le reste est masqué.
        let nothing = visibility(false, false, &["refs/heads/*"]);
        assert!(nothing.shows(&label(RefKind::Head, "HEAD")));
    }
    #[test]
    fn patterns_follow_the_sidebar_trees() {
        assert_eq!(namespace("loc"), "refs/heads/");
        assert_eq!(namespace("tag"), "refs/tags/");
        assert_eq!(namespace("rem:origin"), "refs/remotes/origin/");
    }
}
