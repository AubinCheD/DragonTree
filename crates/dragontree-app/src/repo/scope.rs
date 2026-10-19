//! Ce que parcourt le graphe de l'historique : options du journal, références masquées et visibilité des
//! étiquettes de références.

use std::path::Path;

use dragontree_git::{LogOptions, LogOrder, LogScope, RefKind, RefLabel, RefSelection};

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
            // HEAD reste parcourue même si sa branche est masquée.
            LogScope::Refs(RefSelection {
                tags: shared.settings.show_tags,
                remotes: shared.settings.show_remote_branches,
                exclude: shared.settings.hidden_refs_of(&self.path).to_vec(),
                ..RefSelection::default()
            })
        };
        let order = if shared.settings.topological_order { LogOrder::Topo } else { LogOrder::Date };
        LogOptions { scope, order, ..LogOptions::default() }
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
