//! Opérations git lancées depuis l'interface : exécutées dans l'ordre sur la file du dépôt.

use std::time::{Duration, Instant};

use dragontree_git::{
    DiffOptions, GitError, HunkSelection, MergeMode, NetOptions, Oid, PatchMode, Progress, Repo, ResetMode, build_patch,
};
use dragontree_i18n::{tr, trn};

use super::{AfterCommit, PendingCommit, Refresh, RepoEvent, RepoTab, RunningOp, Side};
use crate::app::{AppEvent, Shared};
use crate::toasts::{Toast, ToastKind};

type OpFn = dyn FnOnce(&Repo, &NetOptions, &mut dyn FnMut(Progress)) -> Result<Option<String>, GitError> + Send;

#[derive(Default, Clone, Copy)]
pub struct OpOptions {
    /// Opération réseau : injecte l'authentification du compte associé au dépôt.
    pub network: bool,
    /// Pas de notification de succès, pas de fenêtre d'authentification (auto-fetch).
    pub silent: bool,
}

/// Variables d'environnement qui authentifient git avec le jeton du compte (vide sans compte ni jeton). Lit le
/// gestionnaire d'identifiants : à appeler depuis une tâche de fond.
pub(crate) fn auth_env(account: Option<&dragontree_forges::Account>) -> Vec<(String, String)> {
    if let Some(acc) = account
        && let Ok(Some(token)) = dragontree_forges::secrets::load_token(&acc.id)
    {
        return dragontree_forges::git_auth_env(acc, &token);
    }
    Vec::new()
}

impl RepoTab {
    /// Compte associé au dépôt, dont le jeton authentifie les opérations réseau.
    pub(crate) fn net_account(&self, shared: &Shared) -> Option<dragontree_forges::Account> {
        let settings = &shared.settings;
        settings.account_for_repo(&self.workdir()).or_else(|| settings.account_for_repo(&self.path)).cloned()
    }

    pub(crate) fn run_op(&mut self, shared: &Shared, title: &str, refresh: Refresh, opts: OpOptions, f: Box<OpFn>) {
        let Some(repo) = self.repo.clone() else { return };
        self.next_op_id += 1;
        let id = self.next_op_id;
        let cancel = dragontree_git::CancelToken::new();
        self.running.push(RunningOp {
            id,
            title: title.to_string(),
            progress: None,
            cancel: cancel.clone(),
            silent: opts.silent,
        });
        let account = if opts.network { self.net_account(shared) } else { None };
        let (sender, tab, title) = (shared.jobs.sender(), self.id, title.to_string());
        self.worker.submit(move || {
            let net = NetOptions { env: auth_env(account.as_ref()), cancel, non_interactive: opts.silent };
            let progress_sender = sender.clone();
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut on_progress = |p: Progress| {
                if last.elapsed() >= Duration::from_millis(80) || p.percent == Some(100) {
                    last = Instant::now();
                    progress_sender.send(AppEvent::Repo(tab, RepoEvent::OpProgress(id, p)));
                }
            };
            // Une panique devient une erreur : l'opération se termine (plus de sablier bloqué).
            let result = crate::jobs::guarded(|| f(&repo, &net, &mut on_progress));
            sender.send(AppEvent::Repo(tab, RepoEvent::OpDone { id, title, result, refresh }));
        });
    }

    /// Opération locale sans progression.
    pub(crate) fn run_simple(
        &mut self,
        shared: &Shared,
        title: &str,
        refresh: Refresh,
        f: impl FnOnce(&Repo) -> Result<Option<String>, GitError> + Send + 'static,
    ) {
        self.run_op(shared, title, refresh, OpOptions::default(), Box::new(move |r, _, _| f(r)));
    }

    /// Opération réseau avec progression et authentification.
    pub(crate) fn run_net(
        &mut self,
        shared: &Shared,
        title: &str,
        refresh: Refresh,
        silent: bool,
        f: impl FnOnce(&Repo, &NetOptions, &mut dyn FnMut(Progress)) -> Result<Option<String>, GitError> + Send + 'static,
    ) {
        self.run_op(shared, title, refresh, OpOptions { network: true, silent }, Box::new(f));
    }

    // --- Réseau -------------------------------------------------------------------------

    pub(crate) fn op_fetch(&mut self, shared: &Shared, remote: Option<String>, prune: bool, silent: bool) {
        self.run_net(shared, "Fetch", Refresh::REFS.with_extras(), silent, move |r, n, on| {
            r.fetch(remote.as_deref(), prune, n, on)?;
            Ok(Some(tr!("Fetch terminé").into()))
        });
    }

    /// Teste la connexion à un remote (`git ls-remote --heads`, avec le compte associé comme pour un fetch) ; la
    /// notification donne le résultat, ou le message de git en cas d'échec.
    pub(crate) fn test_remote_connection(&mut self, shared: &Shared, remote: String) {
        self.run_net(shared, tr!("Test de la connexion"), Refresh::NONE, false, move |r, n, _| {
            let heads = r.ls_remote_heads(&remote, n)?;
            Ok(Some(trn!(
                heads.len(),
                "Connexion à {remote} réussie : {n} branche",
                "Connexion à {remote} réussie : {n} branches",
                remote
            )))
        });
    }

    pub(crate) fn op_pull(&mut self, shared: &Shared, remote: String, branch: String, rebase: bool) {
        self.run_net(shared, "Pull", Refresh::ALL, false, move |r, n, on| {
            r.pull(&remote, &branch, rebase, n, on)?;
            Ok(Some(tr!("Pull de {remote}/{branch} terminé", remote, branch)))
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn op_push(
        &mut self,
        shared: &Shared,
        remote: String,
        local: String,
        remote_branch: String,
        set_upstream: bool,
        force: bool,
        tags: bool,
    ) {
        self.run_net(shared, "Push", Refresh::REFS, false, move |r, n, on| {
            r.push(&remote, &local, &remote_branch, set_upstream, force, n, on)?;
            if tags {
                r.push_all_tags(&remote, n, on)?;
            }
            Ok(Some(tr!("{local} poussée vers {remote}/{remote_branch}", local, remote, remote_branch)))
        });
    }

    /// Push sans dialogue (rapide, ou après un commit) : ni force ni tags.
    pub(crate) fn push_to(&mut self, shared: &Shared, t: PushTarget) {
        self.op_push(shared, t.remote, t.local, t.remote_branch, t.set_upstream, false, false);
    }

    /// Push par défaut de la branche courante : vers sa branche suivie, sinon vers une branche du même nom sur le
    /// remote par défaut, qui devient sa branche suivie. `None` : HEAD détachée ou aucun remote.
    pub(crate) fn default_push(&self) -> Option<PushTarget> {
        let local = self.refs.as_ref()?.current_branch()?.to_string();
        Some(match self.current_upstream() {
            Some((remote, remote_branch)) => PushTarget { remote, local, remote_branch, set_upstream: false },
            None => {
                PushTarget { remote: self.default_remote()?, remote_branch: local.clone(), local, set_upstream: true }
            }
        })
    }

    /// Branche distante suivie par la branche courante : (remote, branche).
    pub(crate) fn current_upstream(&self) -> Option<(String, String)> {
        let refs = self.refs.as_ref()?;
        let current = refs.current_branch()?;
        let upstream = refs.local(current)?.upstream.clone()?;
        refs.upstream_parts(&upstream)
    }

    pub(crate) fn default_remote(&self) -> Option<String> {
        if let Some((r, _)) = self.current_upstream() {
            return Some(r);
        }
        let names: Vec<String> = self.remotes.iter().map(|r| r.name.clone()).collect();
        names.iter().find(|n| *n == "origin").cloned().or_else(|| names.first().cloned())
    }

    // --- Index et copie de travail ------------------------------------------------------

    pub(crate) fn op_stage(&mut self, shared: &Shared, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.run_simple(shared, tr!("Indexation"), Refresh::STATUS, move |r| r.stage(&paths).map(|_| None));
    }

    /// Indexe quand même des fichiers ignorés (`git add -f`) : demandé explicitement, fichier par fichier.
    pub(crate) fn op_stage_force(&mut self, shared: &Shared, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.run_simple(shared, tr!("Indexation"), Refresh::STATUS, move |r| r.stage_force(&paths).map(|_| None));
    }

    pub(crate) fn op_unstage(&mut self, shared: &Shared, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        self.run_simple(shared, tr!("Désindexation"), Refresh::STATUS, move |r| r.unstage(&paths).map(|_| None));
    }

    pub(crate) fn op_discard(&mut self, shared: &Shared, tracked: Vec<String>, untracked: Vec<String>) {
        let n = tracked.len() + untracked.len();
        self.run_simple(shared, tr!("Annulation des modifications"), Refresh::STATUS, move |r| {
            r.discard(&tracked, &untracked)?;
            Ok(Some(trn!(n, "Modifications annulées ({n} fichier)", "Modifications annulées ({n} fichiers)")))
        });
    }

    /// Indexe, désindexe ou annule des hunks/lignes du diff affiché dans les changements.
    pub(crate) fn op_patch(&mut self, shared: &Shared, mode: PatchMode, sel: Vec<HunkSelection>) {
        let Some(diff) = self.changes.diff.clone() else { return };
        let Some((side, path)) = self.changes.diff_key.clone() else { return };
        let untracked = diff.is_new
            && side == Side::Unstaged
            && self
                .status
                .as_ref()
                .and_then(|s| s.entries.iter().find(|e| e.path == path))
                .is_some_and(|e| e.worktree == dragontree_git::FileState::Untracked);
        let o = Self::diff_options(shared);
        let title = match mode {
            PatchMode::Stage => tr!("Indexation"),
            PatchMode::Unstage => tr!("Désindexation"),
            PatchMode::Discard => tr!("Annulation"),
        };
        self.run_simple(shared, title, Refresh::STATUS, move |r| {
            let diff = if untracked {
                // Fichier non suivi : `git add -N` puis diff ordinaire (mêmes lignes, même ordre).
                r.intent_to_add(&path)?;
                r.diff_worktree_file(
                    &path,
                    false,
                    false,
                    &DiffOptions { context: o.context, ignore_whitespace: false },
                )?
            } else {
                diff
            };
            let Some(patch) = build_patch(&diff, &sel, mode.is_reverse()) else { return Ok(None) };
            r.apply_patch(&patch, mode)?;
            Ok(None)
        });
    }

    /// Commit (puis push si demandé). Le message saisi n'est effacé qu'après un commit réussi.
    pub(crate) fn op_commit(&mut self, shared: &Shared, message: String, amend: bool, push_after: bool) {
        let after = if push_after { self.push_after_commit(amend) } else { AfterCommit::Nothing };
        let before = self.next_op_id;
        self.run_simple(shared, "Commit", Refresh::ALL, move |r| {
            r.commit(&message, amend)?;
            Ok(Some(if amend { tr!("Commit modifié") } else { tr!("Commit créé") }.into()))
        });
        if self.next_op_id != before {
            self.changes.pending_commit = Some(PendingCommit { op: self.next_op_id, after });
        }
    }

    /// Push qui suivra le commit. Jamais forcé : si le commit modifié (amend) était déjà poussé,
    /// le push réécrirait l'historique distant, ce que seul un push forcé explicite doit faire.
    fn push_after_commit(&self, amend: bool) -> AfterCommit {
        if self.refs.as_ref().and_then(|r| r.current_branch()).is_none() {
            return AfterCommit::PushSkipped(tr!("HEAD détachée : aucune branche à pousser.").into());
        }
        let Some(target) = self.default_push() else {
            return AfterCommit::PushSkipped(tr!("Aucun remote n'est configuré.").into());
        };
        let already_pushed = self.status.as_ref().is_some_and(|s| s.ahead == 0);
        if amend && already_pushed && !target.set_upstream {
            let PushTarget { remote, remote_branch, .. } = target;
            return AfterCommit::PushSkipped(tr!(
                "Le commit d'origine était déjà sur {remote}/{remote_branch} : le remplacer demande un push forcé. Utilisez Push › Forcer si c'est voulu.",
                remote,
                remote_branch
            ));
        }
        AfterCommit::Push(target)
    }

    /// Après un commit réussi : vider la zone de saisie, puis pousser si c'était demandé.
    pub(super) fn after_commit(&mut self, shared: &mut Shared, after: AfterCommit) {
        self.changes.subject.clear();
        self.changes.body.clear();
        self.changes.amend = false;
        self.changes.saved_message = None;
        // Le modèle de message revient dans la zone vidée.
        self.load_commit_template(shared);
        match after {
            AfterCommit::Nothing => {}
            AfterCommit::Push(target) => self.push_to(shared, target),
            AfterCommit::PushSkipped(reason) => shared.toast(Toast {
                kind: ToastKind::Info,
                title: tr!("Le commit n'a pas été poussé").into(),
                detail: Some(reason),
            }),
        }
    }

    /// Charge le message du dernier commit (pour « Amend »).
    pub(crate) fn load_last_message(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        let id = self.id;
        shared.jobs.spawn("last-msg", move || {
            Some(AppEvent::Repo(id, RepoEvent::LastMessage(repo.last_commit_message().unwrap_or_default())))
        });
    }

    // --- Branches, tags, historique -----------------------------------------------------

    pub(crate) fn op_checkout(&mut self, shared: &Shared, target: String) {
        self.run_simple(shared, "Checkout", Refresh::ALL, move |r| {
            r.checkout(&target)?;
            Ok(Some(tr!("Checkout de {target}", target)))
        });
    }

    pub(crate) fn op_checkout_remote(&mut self, shared: &Shared, remote_full: String, local: String) {
        self.run_simple(shared, "Checkout", Refresh::ALL, move |r| {
            r.checkout_new_tracking(&remote_full, &local)?;
            Ok(Some(tr!("Branche {local} créée depuis {remote_full}", local, remote_full)))
        });
    }

    pub(crate) fn op_create_branch(&mut self, shared: &Shared, name: String, start: String, checkout: bool) {
        self.run_simple(shared, tr!("Nouvelle branche"), Refresh::ALL, move |r| {
            r.create_branch(&name, &start, checkout)?;
            Ok(Some(tr!("Branche {name} créée", name)))
        });
    }

    pub(crate) fn op_merge(&mut self, shared: &Shared, rev: String, mode: MergeMode) {
        self.run_simple(shared, "Merge", Refresh::ALL, move |r| {
            r.merge(&rev, mode)?;
            Ok(Some(tr!("{rev} fusionnée", rev)))
        });
    }

    pub(crate) fn op_rebase(&mut self, shared: &Shared, onto: String) {
        self.run_simple(shared, "Rebase", Refresh::ALL, move |r| {
            r.rebase(&onto)?;
            Ok(Some(tr!("Rebase sur {onto} terminé", onto)))
        });
    }

    pub(crate) fn op_cherry_pick(&mut self, shared: &Shared, ids: Vec<Oid>) {
        self.run_simple(shared, "Cherry-pick", Refresh::ALL, move |r| {
            r.cherry_pick(&ids)?;
            Ok(Some(tr!("Cherry-pick terminé").into()))
        });
    }

    pub(crate) fn op_revert(&mut self, shared: &Shared, id: Oid) {
        self.run_simple(shared, "Revert", Refresh::ALL, move |r| {
            r.revert(id)?;
            Ok(Some(tr!("Commit {sha} annulé par un revert", sha = id.short())))
        });
    }

    pub(crate) fn op_reset(&mut self, shared: &Shared, rev: String, mode: ResetMode) {
        self.run_simple(shared, "Reset", Refresh::ALL, move |r| {
            r.reset(&rev, mode)?;
            Ok(Some(tr!("Branche réinitialisée").into()))
        });
    }
}

impl RepoTab {
    pub(crate) fn op_stash_save(&mut self, shared: &Shared, message: String, untracked: bool, keep_index: bool) {
        self.run_simple(shared, "Stash", Refresh::STATUS.with_extras(), move |r| {
            r.stash_save(Some(&message), untracked, keep_index)?;
            Ok(Some(tr!("Modifications mises de côté (stash)").into()))
        });
    }

    /// Stash des seuls fichiers choisis.
    pub(crate) fn op_stash_files(&mut self, shared: &Shared, message: String, paths: Vec<String>) {
        self.run_simple(shared, "Stash", Refresh::STATUS.with_extras(), move |r| {
            r.stash_push_paths(Some(&message), &paths)?;
            Ok(Some(tr!("Fichiers mis de côté (stash)").into()))
        });
    }

    /// Snapshot : un stash qui laisse les modifications en place.
    pub(crate) fn op_stash_snapshot(&mut self, shared: &Shared, message: String) {
        self.run_simple(shared, "Snapshot", Refresh::STATUS.with_extras(), move |r| {
            r.stash_snapshot(Some(&message))?;
            Ok(Some(tr!("Snapshot enregistré (stash)").into()))
        });
    }

    pub(crate) fn op_stash_rename(&mut self, shared: &Shared, oid: Oid, label: String, message: String) {
        self.run_simple(shared, tr!("Renommage du stash"), Refresh::STATUS.with_extras(), move |r| {
            r.stash_rename_oid(oid, message.trim())?;
            Ok(Some(tr!("Stash renommé : {label}", label)))
        });
    }

    /// Agit sur le stash `oid` (pas sur un index : la liste a pu changer depuis l'affichage).
    pub(crate) fn op_stash(&mut self, shared: &Shared, oid: Oid, label: String, kind: StashOp) {
        self.run_simple(shared, kind.title(), Refresh::STATUS.with_extras(), move |r| {
            match kind {
                StashOp::Apply => r.stash_apply_oid(oid)?,
                StashOp::Pop => r.stash_pop_oid(oid)?,
                StashOp::Drop => r.stash_drop_oid(oid)?,
            }
            Ok(Some(tr!("{title} : {label}", title = kind.title(), label)))
        });
    }
}

/// Destination d'un push sans dialogue (voir `RepoTab::default_push`).
pub(crate) struct PushTarget {
    pub remote: String,
    pub local: String,
    pub remote_branch: String,
    /// Pas encore de branche suivie : le push la crée (`--set-upstream`).
    pub set_upstream: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum StashOp {
    Apply,
    Pop,
    Drop,
}

impl StashOp {
    pub fn title(&self) -> &'static str {
        match self {
            StashOp::Apply => tr!("Appliquer le stash"),
            StashOp::Pop => tr!("Pop du stash"),
            StashOp::Drop => tr!("Suppression du stash"),
        }
    }
}
