//! Outils externes dans l'onglet : « Ouvrir dans l'outil de diff externe » (menus des fichiers de la vue Changements
//! et des détails d'un commit) et l'outil de merge des conflits. Le choix de l'outil vient des réglages
//! (`crate::external_tools`) ; « Configuration de git » garde `git difftool` / `git mergetool` tels que configurés.

use dragontree_git::repo::EMPTY_TREE;
use dragontree_git::{GitError, Repo};
use dragontree_i18n::tr;
use egui::Ui;

use super::{Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::external_tools::{self, ExternalDiff, ToolKind, resolve, resolve_with_trust};
use crate::icons;
use crate::toasts::Toast;

/// `git difftool` pour la même comparaison que celle que DragonTree extrait pour les autres outils.
fn run_git_difftool(repo: &Repo, diff: &ExternalDiff) -> Result<(), GitError> {
    match diff {
        ExternalDiff::Working { path, staged, .. } => repo.run_difftool(&[], *staged, path),
        ExternalDiff::Commit { id, has_parent, path, .. } => {
            let parent = if *has_parent { format!("{id}^") } else { EMPTY_TREE.to_string() };
            repo.run_difftool(&[&parent, id], false, path)
        }
    }
}

impl RepoTab {
    /// Entrée « Ouvrir dans l'outil de diff externe » d'un menu de fichier.
    pub(super) fn external_diff_entry(&mut self, ui: &mut Ui, shared: &mut Shared, diff: ExternalDiff) {
        if ui.button(format!("{} {}", icons::GIT_DIFF, tr!("Ouvrir dans l'outil de diff externe"))).clicked() {
            self.open_external_diff(shared, diff);
            ui.close();
        }
    }

    /// Lance l'outil de diff choisi dans les réglages, en tâche de fond (l'outil peut rester ouvert longtemps, sans
    /// bloquer la file des opérations de l'onglet). Un échec est signalé par une notification.
    pub(super) fn open_external_diff(&mut self, shared: &mut Shared, diff: ExternalDiff) {
        let Some(repo) = self.repo.clone() else { return };
        let (choice, custom) = (shared.settings.diff_tool.clone(), shared.settings.custom_diff_tool.clone());
        let tool = match resolve(ToolKind::Diff, &choice, &custom, &shared.tools) {
            Ok(tool) => tool,
            Err(e) => {
                shared.toast(Toast::error(tr!("Outil de diff externe"), e.to_string()));
                return;
            }
        };
        let tab = self.id;
        shared.jobs.spawn("external-diff", move || {
            let result = match tool {
                None => run_git_difftool(&repo, &diff),
                Some(tool) => external_tools::run_external_diff(
                    &repo,
                    &tool,
                    &diff,
                    &external_tools::temp_root(),
                    std::env::var_os("PATH").as_deref(),
                ),
            };
            result.err().map(|e| {
                AppEvent::Repo(tab, RepoEvent::ToolError(tr!("Outil de diff externe").into(), e.stderr_summary()))
            })
        });
    }

    /// Outil de merge externe sur un fichier en conflit : celui des réglages (`git mergetool --tool=…` avec sa commande
    /// passée par `-c`), ou `git mergetool` tel que configuré.
    pub(super) fn run_external_merge(&mut self, shared: &mut Shared, path: String) {
        let (choice, custom) = (shared.settings.merge_tool.clone(), shared.settings.custom_merge_tool.clone());
        let title = tr!("Outil de merge");
        let trust = shared.settings.custom_merge_tool_trust_exit;
        match resolve_with_trust(ToolKind::Merge, &choice, &custom, trust, &shared.tools) {
            Err(e) => shared.toast(Toast::error(title, e.to_string())),
            Ok(None) => self.run_simple(shared, title, Refresh::STATUS, move |r| {
                r.run_mergetool(&path)?;
                Ok(None)
            }),
            Ok(Some(tool)) => self.run_simple(shared, title, Refresh::STATUS, move |r| {
                r.run_mergetool_with(&tool.name, &tool.command, &path, tool.trust_exit_code)?;
                Ok(None)
            }),
        }
    }
}
