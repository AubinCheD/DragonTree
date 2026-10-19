//! Ce que parcourt le graphe de l'historique : options du journal et visibilité des étiquettes de références.

use dragontree_git::{LogOptions, LogOrder, LogScope, RefKind, RefLabel, RefSelection};

use super::RepoTab;
use crate::app::Shared;

/// Ce qui décide si une étiquette de référence s'affiche (réglages de l'affichage).
#[derive(Debug, Clone, Copy)]
pub(super) struct LabelVisibility {
    pub tags: bool,
    pub remotes: bool,
}

impl LabelVisibility {
    pub fn of(shared: &Shared) -> LabelVisibility {
        LabelVisibility { tags: shared.settings.show_tags, remotes: shared.settings.show_remote_branches }
    }

    /// L'étiquette suit la visibilité des références que le graphe parcourt.
    pub fn shows(&self, label: &RefLabel) -> bool {
        match label.kind {
            RefKind::Tag => self.tags,
            RefKind::Remote => self.remotes,
            RefKind::Local | RefKind::Head => true,
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
            LogScope::Refs(RefSelection {
                tags: shared.settings.show_tags,
                remotes: shared.settings.show_remote_branches,
                ..RefSelection::default()
            })
        };
        let order = if shared.settings.topological_order { LogOrder::Topo } else { LogOrder::Date };
        LogOptions { scope, order, ..LogOptions::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_tag_and_remote_settings() {
        let label = |kind, name: &str| RefLabel { kind, name: name.into(), is_head: false };
        let visibility = |tags, remotes| LabelVisibility { tags, remotes };
        let all = visibility(true, true);
        for l in [
            label(RefKind::Tag, "v1"),
            label(RefKind::Remote, "origin/main"),
            label(RefKind::Local, "main"),
            label(RefKind::Head, "HEAD"),
        ] {
            assert!(all.shows(&l), "{l:?}");
        }
        assert!(!visibility(false, true).shows(&label(RefKind::Tag, "v1")));
        assert!(visibility(false, true).shows(&label(RefKind::Remote, "origin/main")));
        assert!(!visibility(true, false).shows(&label(RefKind::Remote, "origin/main")));
        assert!(visibility(true, false).shows(&label(RefKind::Tag, "v1")));
        // HEAD s'affiche toujours, même quand tout le reste est masqué.
        let nothing = visibility(false, false);
        assert!(nothing.shows(&label(RefKind::Head, "HEAD")));
    }
}
