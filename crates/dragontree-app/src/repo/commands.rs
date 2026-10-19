//! Commandes de dépôt du registre (`crate::commands`) appliquées à l'onglet : quand elles sont utilisables
//! (`can_run`) et ce qu'elles font (`run_command`), pour les menus, les raccourcis, la palette Ctrl+P et la
//! barre d'outils.

use dragontree_git::{HeadState, OpState};
use dragontree_i18n::tr;

use super::{Dialog, Refresh, RepoTab, RepoView};
use crate::app::Shared;
use crate::commands::{Command, Scope};
use crate::toasts::Toast;

impl RepoTab {
    /// Une commande de dépôt est utilisable dans l'état de l'onglet (dépôt ouvert) ; grise aussi le bouton
    /// correspondant de la barre d'outils.
    /// Rien n'est utilisable tant qu'un dialogue modal est ouvert.
    pub fn can_run(&self, cmd: Command) -> bool {
        // Un dialogue modal est ouvert : il bloque la barre d'outils et les menus, les raccourcis aussi.
        if self.dialog.is_some() {
            return false;
        }
        let has_remote = !self.remotes.is_empty();
        let on_branch = matches!(self.refs.as_ref().map(|r| &r.head), Some(HeadState::Branch(_)));
        match cmd {
            Command::Fetch => has_remote,
            Command::Pull | Command::QuickPull | Command::Push | Command::QuickPush => has_remote && on_branch,
            Command::Stash | Command::Snapshot => self.status.as_ref().is_some_and(|s| !s.is_clean()),
            Command::NewBranch
            | Command::NewTag
            | Command::CreateWorktree
            | Command::AddSubmodule
            | Command::RepoSettings => self.refs.is_some(),
            // Une opération en cours (fusion, rebase, session am…) : un patch s'y mélangerait.
            Command::ApplyPatch | Command::PastePatch => self.refs.is_some() && self.op_state == OpState::None,
            Command::CollapseAllMerges => !self.history.folding.is_all(),
            Command::ExpandAllMerges => !self.history.folding.is_none(),
            Command::OpenVsCode => crate::util::vscode_available(),
            _ => true,
        }
    }

    /// Passe à l'historique et sélectionne le commit de HEAD, défilé dans la vue. Un filtre de recherche qui le
    /// masque est effacé ; un HEAD absent de la liste (pas encore chargée, HEAD détaché hors des branches
    /// affichées…) est signalé par un message.
    fn reveal_head(&mut self, shared: &mut Shared) {
        self.view = RepoView::History;
        let head = self.refs.as_ref().and_then(|r| r.head_oid);
        let Some(oid) = head.filter(|o| self.history.index.contains_key(o)) else {
            shared.toast(Toast::info(tr!("HEAD n'est pas dans l'historique affiché")));
            return;
        };
        if self.history.filtered.is_some() {
            // Même état qu'un champ de recherche vidé (voir `apply_filter`).
            self.history.filter.clear();
            self.history.filter_applied.clear();
            self.history.filtered = None;
        }
        self.select_commit(shared, oid, true);
    }

    /// Exécute une commande de dépôt (portée Repo) sur l'onglet. Une commande sans traitement ici, ou de portée
    /// App, est un oubli : assertion en debug.
    pub fn run_command(&mut self, cmd: Command, shared: &mut Shared) {
        debug_assert_eq!(cmd.scope(), Scope::Repo, "{cmd:?} : commande App envoyée à l'onglet");
        match cmd {
            Command::ShowChanges => {
                self.view = RepoView::Changes;
                self.ensure_change_selected(shared);
            }
            Command::ShowHistory => self.view = RepoView::History,
            Command::RevealHead => self.reveal_head(shared),
            Command::CurrentBranchOnly => {
                self.set_current_branch_only(shared, !self.history.current_branch_only);
            }
            Command::ShowHiddenRefs => self.show_all_hidden(shared),
            Command::ShowLostCommits => self.set_show_lost(shared, !self.history.show_lost),
            Command::CollapseAllMerges => self.collapse_all_merges(shared),
            Command::ExpandAllMerges => self.expand_all_merges(shared),
            Command::Refresh => self.refresh(shared, Refresh::ALL),
            Command::Fetch => self.op_fetch(shared, None, true, false),
            Command::Pull => self.open_pull_dialog(shared),
            Command::QuickPull => self.quick_pull(shared),
            Command::Push => self.open_push_dialog(),
            Command::QuickPush => self.quick_push(shared),
            Command::NewBranch => {
                self.dialog = Some(Dialog::CreateBranch {
                    start: "HEAD".into(),
                    start_label: "HEAD".into(),
                    name: String::new(),
                    checkout: true,
                })
            }
            Command::NewTag => self.dialog = Some(Dialog::new_tag("HEAD".into(), "HEAD".into())),
            Command::CreateWorktree => self.open_create_worktree(None),
            Command::AddSubmodule => self.open_add_submodule(),
            Command::Stash => {
                self.dialog =
                    Some(Dialog::Stash { message: String::new(), untracked: true, keep_index: false, snapshot: false })
            }
            Command::Snapshot => {
                self.dialog =
                    Some(Dialog::Stash { message: String::new(), untracked: false, keep_index: false, snapshot: true })
            }
            Command::ApplyPatch => {
                let picked = rfd::FileDialog::new()
                    .set_title(tr!("Choisir un patch"))
                    .add_filter(tr!("Patchs"), &["patch", "diff"])
                    .add_filter(tr!("Tous les fichiers"), &["*"])
                    .pick_file();
                if let Some(path) = picked {
                    self.open_patch_file(shared, path);
                }
            }
            Command::PastePatch => self.dialog = Some(Dialog::PastePatch { text: String::new() }),
            Command::RepoSettings => self.open_repo_settings(shared),
            Command::OpenExplorer => crate::util::open_in_explorer(&self.path),
            Command::OpenTerminal => crate::util::open_terminal(&self.path),
            Command::OpenVsCode => crate::util::open_in_vscode(&self.path),
            _ => debug_assert!(false, "{cmd:?} : commande de dépôt sans traitement dans RepoTab::run_command"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SettingsFile;

    /// Onglet sans git (aucune tâche lancée) et son état partagé.
    fn tab_without_git(dir: &std::path::Path) -> (RepoTab, Shared) {
        let (store, settings, _) = SettingsFile::load(&dir.join("settings.json"));
        let (mut shared, _events) = Shared::new(egui::Context::default(), settings, store, None, None);
        (RepoTab::new(1, dir.to_path_buf(), &mut shared), shared)
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "commande App envoyée à l'onglet")]
    fn an_app_command_sent_to_the_tab_is_loud_in_debug() {
        let dir = tempfile::tempdir().unwrap();
        let (mut tab, mut shared) = tab_without_git(dir.path());
        tab.run_command(Command::NewTab, &mut shared);
    }
}
