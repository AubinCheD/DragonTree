//! Pour un hôte qui pilote DragonTree à distance (le téléphone de la suite) : une opération git sur un dépôt ouvert,
//! lancée comme le bouton correspondant — même file d'opérations, même compte associé au dépôt, même progression et
//! mêmes notifications, même actualisation, même état « occupé » des boutons, et une fusion ou un rebase arrêté sur
//! des conflits s'affiche dans le bandeau de l'onglet comme s'il avait été lancé ici. Ce qu'ont donné les dernières
//! opérations de l'onglet se relit ensuite (`RepoTab::outcome`), d'où qu'elles aient été lancées.

use std::path::PathBuf;
use std::time::Instant;

use dragontree_git::{MergeMode, Oid, OpState, ResetMode};
use dragontree_i18n::tr;

use super::actions::{OpKind, StashOp};
use super::dialogs::{ConfirmAction, abort_label};
use super::{Dialog, Refresh, RepoTab};
use crate::app::Shared;

/// Opérations dont le résultat est gardé, par onglet.
pub(super) const OUTCOMES_KEPT: usize = 30;

/// Une opération demandée à distance : celles des boutons, menus et dialogues de l'onglet.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteOp {
    /// Fetch de tous les remotes (bouton Fetch).
    Fetch,
    /// Pull de la branche suivie ; `rebase` : `None` pour le mode retenu dans DragonTree (comme le pull rapide).
    Pull {
        rebase: Option<bool>,
    },
    /// Push de la branche courante vers sa branche suivie, sinon publiée sur le remote par défaut (comme le push
    /// rapide) ; `force` : avec bail (`--force-with-lease`).
    Push {
        force: bool,
    },
    Stage(Vec<String>),
    Unstage(Vec<String>),
    Discard {
        tracked: Vec<String>,
        untracked: Vec<String>,
    },
    /// Conflits : garder la version de la branche courante (`ours`) ou celle qui arrive, puis marquer résolu.
    Resolve {
        paths: Vec<String>,
        ours: bool,
    },
    /// Commit des modifications indexées (sans toucher à la zone de saisie de l'onglet).
    Commit {
        message: String,
        amend: bool,
    },
    /// Mise de côté de toutes les modifications, fichiers non suivis compris.
    StashSave {
        message: String,
    },
    /// `label` : ce que la notification nomme (le message du stash).
    StashApply {
        oid: Oid,
        label: String,
    },
    StashPop {
        oid: Oid,
        label: String,
    },
    StashDrop {
        oid: Oid,
        label: String,
    },
    /// Branche locale, ou commit (HEAD détachée).
    Checkout(String),
    /// Branche locale qui suit une branche distante (`origin/x`), sortie.
    Track {
        remote_branch: String,
        local: String,
    },
    /// Branche créée depuis `start` et sortie.
    CreateBranch {
        name: String,
        start: String,
    },
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    /// Branche `branch` supprimée du serveur `remote` (« Supprimer sur le remote… », sa question confirmée).
    DeleteRemoteBranch {
        remote: String,
        branch: String,
    },
    /// Push d'une branche vers une branche du même nom, qui devient sa branche suivie.
    PushBranch {
        name: String,
        remote: String,
    },
    Merge(String),
    Rebase(String),
    CherryPick(Oid),
    Revert(Oid),
    Reset {
        rev: String,
        mode: ResetMode,
    },
    /// L'opération en cours (fusion, rebase, cherry-pick, revert) : poursuivre, abandonner, passer le commit (rebase).
    Continue,
    Abort,
    Skip,
    /// Tag léger.
    CreateTag {
        name: String,
        rev: String,
    },
    /// Tag local seulement.
    DeleteTag(String),
    PushTag {
        name: String,
        remote: String,
    },
    /// L'application ferme d'abord les onglets ouverts sur ce worktree (`DragonTreeApp::queue_operation`).
    RemoveWorktree(PathBuf),
    PruneWorktrees,
}

/// Ce qu'a donné une opération visible de l'onglet : son message de succès, ou l'erreur de git, comme sa notification.
#[derive(Clone, Debug)]
pub struct OpOutcome {
    pub id: u64,
    pub title: String,
    pub result: Result<Option<String>, String>,
    pub at: Instant,
}

impl RepoTab {
    /// Lance `op` comme le bouton correspondant ; rend son numéro (voir `outcome`, `running`), ou pourquoi elle ne
    /// part pas (dépôt pas encore ouvert, pas de branche suivie…).
    pub(crate) fn run_remote(&mut self, shared: &mut Shared, op: RemoteOp) -> Result<u64, String> {
        if let Some(e) = &self.open_error {
            return Err(e.to_string());
        }
        if self.repo.is_none() {
            return Err(tr!("Le dépôt s'ouvre encore dans DragonTree.").into());
        }
        let before = self.next_op_id;
        match op {
            RemoteOp::Fetch => self.op_fetch(shared, None, true, false),
            RemoteOp::Pull { rebase } => {
                let (remote, branch) = self.current_upstream().ok_or_else(|| {
                    tr!(
                        "Pas de branche suivie : rien à tirer (publiez la branche, ou choisissez quoi tirer sur le PC)."
                    )
                })?;
                self.op_pull(shared, remote, branch, rebase.unwrap_or(shared.pull_rebase));
            }
            RemoteOp::Push { force } => {
                let t = self.default_push().ok_or_else(|| tr!("HEAD détachée ou aucun remote : rien à pousser."))?;
                self.op_push(shared, t.remote, t.local, t.remote_branch, t.set_upstream, force, false);
            }
            RemoteOp::Stage(paths) => self.op_stage(shared, paths),
            RemoteOp::Unstage(paths) => self.op_unstage(shared, paths),
            RemoteOp::Discard { tracked, untracked } => self.op_discard(shared, tracked, untracked),
            RemoteOp::Resolve { paths, ours } => self.resolve_side(shared, paths, ours),
            RemoteOp::Commit { message, amend } => {
                self.run_simple_as(shared, OpKind::Commit, "Commit", Refresh::ALL, move |r| {
                    r.commit(&message, amend)?;
                    Ok(Some(if amend { tr!("Commit modifié") } else { tr!("Commit créé") }.into()))
                });
            }
            RemoteOp::StashSave { message } => self.op_stash_save(shared, message, true, false),
            RemoteOp::StashApply { oid, label } => self.op_stash(shared, oid, label, StashOp::Apply),
            RemoteOp::StashPop { oid, label } => self.op_stash(shared, oid, label, StashOp::Pop),
            RemoteOp::StashDrop { oid, label } => self.op_stash(shared, oid, label, StashOp::Drop),
            RemoteOp::Checkout(target) => self.op_checkout(shared, target),
            RemoteOp::Track { remote_branch, local } => self.op_checkout_remote(shared, remote_branch, local),
            RemoteOp::CreateBranch { name, start } => self.op_create_branch(shared, name, start, true),
            RemoteOp::RenameBranch { old, new } => self.execute_dialog(Dialog::RenameBranch { old, new }, shared),
            RemoteOp::DeleteBranch { name, force } => {
                self.execute_dialog(Dialog::DeleteBranch { name, force, remote: None, delete_remote: false }, shared)
            }
            RemoteOp::DeleteRemoteBranch { remote, branch } => {
                self.execute_dialog(Dialog::DeleteRemoteBranch { remote, branch }, shared)
            }
            RemoteOp::PushBranch { name, remote } => {
                self.op_push(shared, remote, name.clone(), name, true, false, false)
            }
            RemoteOp::Merge(rev) => self.op_merge(shared, rev, MergeMode::Default),
            RemoteOp::Rebase(onto) => self.op_rebase(shared, onto),
            RemoteOp::CherryPick(id) => self.op_cherry_pick(shared, vec![id]),
            RemoteOp::Revert(id) => self.op_revert(shared, id),
            RemoteOp::Reset { rev, mode } => self.op_reset(shared, rev, mode),
            RemoteOp::Continue => {
                let state = self.in_progress()?;
                self.run_simple(shared, tr!("Continuer"), Refresh::ALL, move |r| r.continue_op(state).map(|_| None));
            }
            RemoteOp::Abort => {
                let state = self.in_progress()?;
                let action = ConfirmAction::AbortOp(state);
                let title = abort_label(state).to_string();
                self.execute_dialog(Dialog::Confirm { title, text: String::new(), action }, shared);
            }
            RemoteOp::Skip => {
                if !matches!(self.op_state, OpState::Rebase { .. }) {
                    return Err(tr!("Aucun rebase en cours.").into());
                }
                self.run_simple(shared, "Rebase", Refresh::ALL, |r| r.skip_rebase_commit().map(|_| None));
            }
            RemoteOp::CreateTag { name, rev } => {
                let tag = Dialog::CreateTag {
                    rev,
                    rev_label: String::new(),
                    name,
                    message: String::new(),
                    annotated: false,
                    push: false,
                };
                self.execute_dialog(tag, shared);
            }
            RemoteOp::DeleteTag(name) => self.execute_dialog(Dialog::DeleteTag { name, remote_too: false }, shared),
            RemoteOp::PushTag { name, remote } => self.op_push_tag(shared, remote, name),
            RemoteOp::RemoveWorktree(path) => self.op_remove_worktree(shared, path, false),
            RemoteOp::PruneWorktrees => {
                self.run_simple(shared, tr!("Nettoyage des worktrees"), Refresh::REFS.with_extras(), |r| {
                    r.prune_worktrees()?;
                    Ok(Some(tr!("Worktrees nettoyés").into()))
                });
            }
        }
        if self.next_op_id == before {
            return Err(tr!("Rien à faire.").into());
        }
        Ok(self.next_op_id)
    }

    /// L'opération en cours (fusion, rebase…), ou pourquoi il n'y en a pas.
    fn in_progress(&self) -> Result<OpState, String> {
        match self.op_state {
            OpState::None => Err(tr!("Aucune opération en cours (fusion, rebase…).").into()),
            state => Ok(state),
        }
    }

    /// Ce qu'a donné l'opération `id` de l'onglet ; `None` : en cours, silencieuse, ou trop ancienne.
    pub fn outcome(&self, id: u64) -> Option<&OpOutcome> {
        self.outcomes.iter().find(|o| o.id == id)
    }

    /// Les dernières opérations visibles de l'onglet, de la plus ancienne à la plus récente.
    pub fn outcomes(&self) -> impl Iterator<Item = &OpOutcome> {
        self.outcomes.iter()
    }

    /// Erreur d'authentification du dernier fetch (indicateur de l'onglet), effacée au prochain fetch réussi.
    pub fn fetch_auth_error(&self) -> Option<&str> {
        self.fetch_auth_error.as_deref()
    }

    /// Garde ce qu'a donné une opération visible (appelée à sa fin, avant sa notification).
    pub(super) fn keep_outcome(&mut self, id: u64, title: &str, result: Result<Option<String>, String>) {
        if self.outcomes.len() == OUTCOMES_KEPT {
            self.outcomes.pop_front();
        }
        self.outcomes.push_back(OpOutcome { id, title: title.to_string(), result, at: Instant::now() });
    }
}
