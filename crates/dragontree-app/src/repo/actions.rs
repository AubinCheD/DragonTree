//! Opérations git lancées depuis l'interface : exécutées dans l'ordre sur la file du dépôt.

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use dragontree_git::{
    DiffOptions, GitError, HunkSelection, MergeMode, NetOptions, Oid, PatchMode, Progress, Repo, ResetMode,
    build_patch, build_patch_ignoring_whitespace,
};
use dragontree_i18n::{tr, trn};

use super::dialogs::{ConfirmAction, Dialog};
use super::{AfterCommit, PendingCommit, Refresh, RepoEvent, RepoTab, RunningOp, Side};
use crate::app::{AppEvent, Shared};
use crate::op_queue::PendingOp;
use crate::toasts::{Toast, ToastKind};

type OpFn = dyn FnOnce(&Repo, &NetOptions, &mut dyn FnMut(Progress)) -> Result<Option<String>, GitError> + Send;

/// Type d'une opération : l'état « occupé » des boutons (barre d'outils, commit) se lit sur lui (`RepoTab::op_running`),
/// jamais sur le titre, qui est traduit (RV5b). Un bouton est occupé pendant sa propre opération, d'où qu'elle soit
/// lancée (bouton, menu, raccourci, palette, push après un commit).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    /// Toute autre opération : push d'un tag, fetch et pull LFS, renommage d'un stash…
    #[default]
    Other,
    /// Fetch des remotes (bouton Fetch ; le fetch automatique, silencieux, n'occupe pas le bouton).
    Fetch,
    /// Pull de la branche suivie ou choisie.
    Pull,
    /// Push : dialogue Push, push rapide, push après un commit, push de plusieurs branches (branches en avance).
    Push,
    Commit,
    /// Mise de côté par le dialogue Stash (stash ou snapshot) ou des fichiers choisis.
    Stash,
    /// Pop d'un stash.
    StashPop,
}

#[derive(Default, Clone, Copy)]
pub struct OpOptions {
    /// Type de l'opération (état « occupé » des boutons).
    pub kind: OpKind,
    /// Opération réseau : injecte l'authentification du compte associé au dépôt.
    pub network: bool,
    /// Pas de notification de succès, pas de fenêtre d'authentification (auto-fetch).
    pub silent: bool,
    /// Opération locale qui réécrit les refs distantes (suppression d'un remote) : comme une opération réseau
    /// visible, elle ne tourne pas en même temps qu'un fetch automatique (voir `op_queue`).
    pub remote_refs: bool,
}

/// Variables d'environnement qui authentifient git avec le jeton du compte (vide sans compte ni jeton) pour les URL
/// du compte et celles des remotes `remote_urls` (voir `dragontree_forges::git_auth_env`). Lit le gestionnaire
/// d'identifiants : à appeler depuis une tâche de fond.
pub(crate) fn auth_env(account: Option<&dragontree_forges::Account>, remote_urls: &[String]) -> Vec<(String, String)> {
    if let Some(acc) = account
        && let Ok(Some(token)) = dragontree_forges::secrets::load_token(&acc.id)
    {
        let urls: Vec<&str> = remote_urls.iter().map(String::as_str).collect();
        return dragontree_forges::git_auth_env(acc, &token, &urls);
    }
    Vec::new()
}

/// Raison donnée quand le commit modifié par un amend, déjà poussé, ne l'est pas de nouveau (L5 : push forcé annulé
/// ou impossible à proposer).
pub(super) fn amended_not_pushed(remote: &str, remote_branch: &str) -> String {
    tr!(
        "Le commit d'origine était déjà sur {remote}/{remote_branch} : le remplacer demande un push forcé. Utilisez Push › Forcer si c'est voulu.",
        remote,
        remote_branch
    )
}

/// Notification d'un push demandé après le commit mais non lancé.
pub(super) fn commit_not_pushed(shared: &mut Shared, reason: String) {
    shared.toast(Toast {
        kind: ToastKind::Info,
        title: tr!("Le commit n'a pas été poussé").into(),
        detail: Some(reason),
        action: None,
    });
}

impl RepoTab {
    /// Compte associé au dépôt (voir `RepoTab::account`), dont le jeton authentifie les opérations réseau.
    pub(crate) fn net_account(&self, shared: &Shared) -> Option<dragontree_forges::Account> {
        self.account(&shared.settings).cloned()
    }

    /// URL des remotes du dépôt (lecture et push), pour l'authentification (voir `auth_env`).
    pub(crate) fn remote_urls(&self) -> Vec<String> {
        self.remotes.iter().flat_map(|r| [r.fetch_url.clone(), r.push_url.clone()]).collect()
    }

    /// Lance `f` sur une file du dépôt, partagée avec ses autres onglets (voir `op_queue`) : celle du réseau
    /// silencieux pour une opération réseau silencieuse (fetch automatique), sinon celle des écritures. À son tour,
    /// une opération réseau visible (ou qui réécrit les refs distantes) annule le fetch automatique qui n'a pas
    /// commencé et attend la fin de celui qui tourne, sans l'arrêter : il met à jour les mêmes refs. L'interface
    /// affiche cette attente (`RepoEvent::OpWaiting`) ; annuler l'opération l'interrompt.
    pub(crate) fn run_op(&mut self, shared: &Shared, title: &str, refresh: Refresh, opts: OpOptions, f: Box<OpFn>) {
        let Some(repo) = self.repo.clone() else { return };
        let Some(queues) = self.queues(shared) else { return };
        self.next_op_id += 1;
        let id = self.next_op_id;
        let cancel = dragontree_git::CancelToken::new();
        self.running.push(RunningOp {
            id,
            title: title.to_string(),
            kind: opts.kind,
            progress: None,
            cancel: cancel.clone(),
            silent: opts.silent,
            waiting: false,
        });
        let account = if opts.network { self.net_account(shared) } else { None };
        let urls = if account.is_some() { self.remote_urls() } else { Vec::new() };
        let (sender, tab, title) = (shared.jobs.sender(), self.id, title.to_string());
        let pending = PendingOp {
            tab,
            id,
            title: title.clone(),
            network: opts.network || opts.remote_refs,
            silent: opts.silent,
            cancel: cancel.clone(),
        };
        let (queue, background) = match (opts.network, opts.silent) {
            (true, true) => (queues.background, None),
            _ if opts.network || opts.remote_refs => (queues.writes, Some(queues.background)),
            _ => (queues.writes, None),
        };
        queue.submit(pending, move || {
            if let Some(background) = background {
                let mut waited = false;
                let gave_way = background.make_way(&cancel, || {
                    waited = true;
                    sender.send(AppEvent::Repo(tab, RepoEvent::OpWaiting(id, true)));
                });
                if waited {
                    sender.send(AppEvent::Repo(tab, RepoEvent::OpWaiting(id, false)));
                }
                // « Annuler » pendant l'attente : l'opération s'arrête là, sans lancer git.
                if !gave_way {
                    let result = Err(GitError::Cancelled);
                    sender.send(AppEvent::Repo(tab, RepoEvent::OpDone { id, title, result, refresh }));
                    return;
                }
            }
            let net = NetOptions { env: auth_env(account.as_ref(), &urls), cancel, non_interactive: opts.silent };
            let progress_sender = sender.clone();
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut on_progress = |p: Progress| {
                if last.elapsed() >= Duration::from_millis(80) || p.percent == Some(100) {
                    last = Instant::now();
                    progress_sender.send(AppEvent::Repo(tab, RepoEvent::OpProgress(id, p)));
                }
            };
            // Annulée avant son tour (fetch automatique qui cède la place) : git n'est pas lancé. Une panique devient
            // une erreur : l'opération se termine (plus de sablier bloqué).
            let result = if net.cancel.is_cancelled() {
                Err(GitError::Cancelled)
            } else {
                crate::jobs::guarded(|| f(&repo, &net, &mut on_progress))
            };
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
        self.run_simple_as(shared, OpKind::Other, title, refresh, f);
    }

    /// Opération locale sans progression, d'un type donné (état « occupé » des boutons).
    fn run_simple_as(
        &mut self,
        shared: &Shared,
        kind: OpKind,
        title: &str,
        refresh: Refresh,
        f: impl FnOnce(&Repo) -> Result<Option<String>, GitError> + Send + 'static,
    ) {
        self.run_op(shared, title, refresh, OpOptions { kind, ..OpOptions::default() }, Box::new(move |r, _, _| f(r)));
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
        self.run_net_as(shared, OpKind::Other, title, refresh, silent, f);
    }

    /// Opération réseau d'un type donné (état « occupé » des boutons).
    pub(super) fn run_net_as(
        &mut self,
        shared: &Shared,
        kind: OpKind,
        title: &str,
        refresh: Refresh,
        silent: bool,
        f: impl FnOnce(&Repo, &NetOptions, &mut dyn FnMut(Progress)) -> Result<Option<String>, GitError> + Send + 'static,
    ) {
        let opts = OpOptions { kind, network: true, silent, ..OpOptions::default() };
        self.run_op(shared, title, refresh, opts, Box::new(f));
    }

    // --- Réseau -------------------------------------------------------------------------

    pub(crate) fn op_fetch(&mut self, shared: &Shared, remote: Option<String>, prune: bool, silent: bool) {
        let (all_remotes, before) = (remote.is_none(), self.next_op_id);
        self.run_net_as(shared, OpKind::Fetch, "Fetch", Refresh::REFS.with_extras(), silent, move |r, n, on| {
            r.fetch(remote.as_deref(), prune, n, on)?;
            Ok(Some(tr!("Fetch terminé").into()))
        });
        // Seul un fetch de tous les remotes réussi efface l'authentification refusée (L4, voir `fetch_auth_error`).
        if all_remotes && self.next_op_id != before {
            self.fetches_of_all_remotes.insert(self.next_op_id);
        }
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
        self.run_net_as(shared, OpKind::Pull, "Pull", Refresh::ALL, false, move |r, n, on| {
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
        self.run_net_as(shared, OpKind::Push, "Push", Refresh::REFS, false, move |r, n, on| {
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

    /// Indexe, désindexe ou annule des hunks/lignes du diff affiché dans les changements. Diff affiché sans les espaces
    /// (option « Espaces ») : patch construit sur le diff complet lu avec lui (L6, `build_patch_ignoring_whitespace`),
    /// ou refusé avec un message si la sélection ne s'y reporte pas sans ambiguïté.
    pub(crate) fn op_patch(&mut self, shared: &Shared, mode: PatchMode, sel: Vec<HunkSelection>) {
        let Some(diff) = self.changes.diff.clone() else { return };
        let Some((side, path)) = self.changes.diff_key.clone() else { return };
        let full = self.changes.diff_full.clone();
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
            let patch = if untracked {
                // Fichier non suivi : `git add -N` puis diff ordinaire, sans `-w` (mêmes lignes, même ordre : elles
                // sont toutes ajoutées).
                r.intent_to_add(&path)?;
                let added = r.diff_worktree_file(
                    &path,
                    false,
                    false,
                    &DiffOptions { context: o.context, ignore_whitespace: false },
                )?;
                build_patch(&added, &sel, mode.is_reverse())
            } else if let Some(full) = &full {
                build_patch_ignoring_whitespace(&diff, full, &sel, mode.is_reverse())?
            } else {
                build_patch(&diff, &sel, mode.is_reverse())
            };
            let Some(patch) = patch else { return Ok(None) };
            r.apply_patch(&patch, mode)?;
            Ok(None)
        });
    }

    /// Commit (puis push si demandé). Le message saisi n'est effacé qu'après un commit réussi.
    pub(crate) fn op_commit(&mut self, shared: &Shared, message: String, amend: bool, push_after: bool) {
        let after = if push_after { self.push_after_commit(amend) } else { AfterCommit::Nothing };
        // Amend d'un commit déjà poussé (L5) : le commit remplacé est lu dans la même opération, juste avant l'amend.
        let replaced = match &after {
            AfterCommit::OfferForcePush { replaced, .. } => Some(Arc::clone(replaced)),
            _ => None,
        };
        let before = self.next_op_id;
        self.run_simple_as(shared, OpKind::Commit, "Commit", Refresh::ALL, move |r| {
            if let Some(replaced) = &replaced
                && let Ok(Some(head)) = r.head_if_pushed()
            {
                let _ = replaced.set(head);
            }
            r.commit(&message, amend)?;
            Ok(Some(if amend { tr!("Commit modifié") } else { tr!("Commit créé") }.into()))
        });
        if self.next_op_id != before {
            let message = (self.changes.subject.clone(), self.changes.body.clone());
            self.changes.pending_commit = Some(PendingCommit { op: self.next_op_id, after, message });
        }
    }

    /// Push qui suivra le commit. Jamais forcé d'office : si le commit modifié (amend) était déjà poussé, le push
    /// réécrirait l'historique distant ; il est alors proposé dans un dialogue de confirmation (L5).
    fn push_after_commit(&self, amend: bool) -> AfterCommit {
        if self.refs.as_ref().and_then(|r| r.current_branch()).is_none() {
            return AfterCommit::PushSkipped(tr!("HEAD détachée : aucune branche à pousser.").into());
        }
        let Some(target) = self.default_push() else {
            return AfterCommit::PushSkipped(tr!("Aucun remote n'est configuré.").into());
        };
        let already_pushed = self.status.as_ref().is_some_and(|s| s.ahead == 0);
        if amend && already_pushed && !target.set_upstream {
            return AfterCommit::OfferForcePush { target, replaced: Arc::new(OnceLock::new()) };
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
            AfterCommit::PushSkipped(reason) => commit_not_pushed(shared, reason),
            // Un dialogue déjà ouvert (lancé pendant le commit) n'est pas remplacé : rien n'est poussé.
            AfterCommit::OfferForcePush { target, replaced } => match replaced.get() {
                Some(&replaced) if self.dialog.is_none() => self.offer_force_push(target, replaced),
                _ => commit_not_pushed(shared, amended_not_pushed(&target.remote, &target.remote_branch)),
            },
        }
    }

    /// Dialogue qui propose le push forcé du commit modifié (L5) ; destructeur : Entrée ne le valide pas.
    fn offer_force_push(&mut self, target: PushTarget, replaced: Oid) {
        let PushTarget { remote, local, remote_branch, .. } = target;
        let text = tr!(
            "Le commit d'origine ({sha}) est déjà sur {remote}/{remote_branch} : le remplacer par le commit modifié demande un push forcé, qui réécrit l'historique distant.\n\nLe push est protégé (--force-with-lease) : il est refusé si {remote}/{remote_branch} n'est plus sur {sha}, par exemple si quelqu'un a poussé entre-temps.\n\nAnnuler garde le commit modifié en local, sans rien pousser.",
            sha = replaced.short(),
            remote,
            remote_branch
        );
        self.dialog = Some(Dialog::Confirm {
            title: tr!("Pousser le commit modifié ?").into(),
            text,
            action: ConfirmAction::ForcePushAmended { remote, local, remote_branch, replaced },
        });
    }

    /// Push forcé du commit modifié, confirmé dans le dialogue (L5) : bail fixé à `replaced`, le commit remplacé (voir
    /// `Repo::push_with_lease`). Si le remote a bougé entre-temps, le push est refusé et la notification le dit.
    pub(super) fn op_force_push_amended(
        &mut self,
        shared: &Shared,
        remote: String,
        local: String,
        remote_branch: String,
        replaced: Oid,
    ) {
        self.run_net_as(shared, OpKind::Push, "Push", Refresh::REFS, false, move |r, n, on| {
            r.push_with_lease(&remote, &local, &remote_branch, replaced, n, on).map_err(|e| match e {
                // Message de git (non traduit) d'un bail qui ne correspond plus à la branche distante.
                GitError::Failed { ref stderr, .. } if stderr.contains("stale info") => GitError::Refused(tr!(
                    "Push forcé refusé : {remote}/{remote_branch} n'est plus sur {sha}, le commit remplacé (quelqu'un a poussé entre-temps). Rien n'a été écrasé ; le commit modifié reste en local.",
                    remote,
                    remote_branch,
                    sha = replaced.short()
                )),
                e => e,
            })?;
            Ok(Some(tr!("{local} poussée vers {remote}/{remote_branch} (push forcé)", local, remote, remote_branch)))
        });
    }

    /// Charge le message du dernier commit (pour « Amend »).
    pub(crate) fn load_last_message(&mut self, shared: &Shared, generation: u64) {
        let Some(repo) = self.repo.clone() else { return };
        let id = self.id;
        shared.jobs.spawn("last-msg", move || {
            Some(AppEvent::Repo(id, RepoEvent::LastMessage(generation, repo.last_commit_message().unwrap_or_default())))
        });
    }

    // --- Branches, tags, historique -----------------------------------------------------

    /// Push d'un tag vers un remote (menu d'un tag) : il n'occupe pas le bouton Push (`OpKind::Other`).
    pub(crate) fn op_push_tag(&mut self, shared: &Shared, remote: String, tag: String) {
        self.run_net(shared, tr!("Push du tag"), Refresh::REFS, false, move |r, n, on| {
            r.push_tag(&remote, &tag, n, on)?;
            Ok(Some(tr!("Tag {tag} poussé", tag)))
        });
    }

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
        self.run_simple_as(shared, OpKind::Stash, "Stash", Refresh::STATUS.with_extras(), move |r| {
            r.stash_save(Some(&message), untracked, keep_index)?;
            Ok(Some(tr!("Modifications mises de côté (stash)").into()))
        });
    }

    /// Stash des seuls fichiers choisis.
    pub(crate) fn op_stash_files(&mut self, shared: &Shared, message: String, paths: Vec<String>) {
        self.run_simple_as(shared, OpKind::Stash, "Stash", Refresh::STATUS.with_extras(), move |r| {
            r.stash_push_paths(Some(&message), &paths)?;
            Ok(Some(tr!("Fichiers mis de côté (stash)").into()))
        });
    }

    /// Snapshot : un stash qui laisse les modifications en place.
    pub(crate) fn op_stash_snapshot(&mut self, shared: &Shared, message: String) {
        self.run_simple_as(shared, OpKind::Stash, "Snapshot", Refresh::STATUS.with_extras(), move |r| {
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
        let op_kind = if matches!(kind, StashOp::Pop) { OpKind::StashPop } else { OpKind::Other };
        self.run_simple_as(shared, op_kind, kind.title(), Refresh::STATUS.with_extras(), move |r| {
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::sync::{Arc, Mutex};

    use dragontree_git::{CancelToken, Remote};

    use super::*;
    use crate::app::TabId;
    use crate::settings::SettingsFile;

    /// État partagé (sans git : aucune tâche d'ouverture) et onglets ouverts sur un même dépôt temporaire.
    fn tabs_on_one_repo(dir: &std::path::Path, ids: &[TabId]) -> (Shared, Receiver<AppEvent>, Vec<RepoTab>) {
        let git = crate::test_git();
        git.cmd(dir).args(["init", "-q"]).run().unwrap();
        let (store, settings, _) = SettingsFile::load(&dir.join("settings.json"));
        let (mut shared, events) = Shared::new(egui::Context::default(), settings, store, None, None);
        let repo = Arc::new(Repo::open(&git, dir).unwrap());
        let tabs = ids
            .iter()
            .map(|&id| {
                let mut tab = RepoTab::new(id, dir.to_path_buf(), &mut shared);
                tab.repo = Some(repo.clone());
                tab
            })
            .collect();
        (shared, events, tabs)
    }

    /// Fin de l'opération `id` de l'onglet `tab` (les autres événements sont ignorés).
    fn wait_done(events: &Receiver<AppEvent>, tab: TabId, id: u64) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while let Ok(ev) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if matches!(ev, AppEvent::Repo(t, RepoEvent::OpDone { id: i, .. }) if t == tab && i == id) {
                return;
            }
        }
        panic!("opération {id} de l'onglet {tab} jamais terminée");
    }

    /// Traite les événements de `tab` comme l'application, jusqu'à ce que `until` en reconnaisse un (inclus).
    fn pump(
        events: &Receiver<AppEvent>,
        shared: &mut Shared,
        tab: &mut RepoTab,
        what: &str,
        mut until: impl FnMut(&RepoEvent) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while let Ok(ev) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            if let AppEvent::Repo(id, ev) = ev
                && id == tab.id
            {
                let reached = until(&ev);
                tab.handle_event(ev, shared);
                if reached {
                    return;
                }
            }
        }
        panic!("jamais reçu : {what}");
    }

    /// Résultat de l'opération `id` si `ev` est sa fin.
    fn done_with(ev: &RepoEvent, id: u64) -> Option<&Result<Option<String>, GitError>> {
        match ev {
            RepoEvent::OpDone { id: i, result, .. } if *i == id => Some(result),
            _ => None,
        }
    }

    #[test]
    fn two_tabs_on_the_same_repository_share_one_write_queue() {
        let dir = tempfile::tempdir().unwrap();
        let (shared, events, mut tabs) = tabs_on_one_repo(dir.path(), &[1, 2]);
        let first = tabs[0].queues(&shared).unwrap();
        let second = tabs[1].queues(&shared).unwrap();
        assert!(first.writes.same_as(&second.writes), "une seule file d'écriture pour le dépôt");
        assert!(first.background.same_as(&second.background));

        // L'écriture du second onglet attend celle du premier, même bloquée.
        let log = Arc::new(Mutex::new(Vec::new()));
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let l = log.clone();
        tabs[0].run_simple(&shared, "Première", Refresh::NONE, move |_| {
            l.lock().unwrap().push("1 début");
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
            l.lock().unwrap().push("1 fin");
            Ok(None)
        });
        let l = log.clone();
        tabs[1].run_simple(&shared, "Seconde", Refresh::NONE, move |_| {
            l.lock().unwrap().push("2");
            Ok(None)
        });
        // Avec une file par onglet, la seconde passerait pendant que la première attend : on lui en laisse le temps.
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && !log.lock().unwrap().contains(&"2") {
            std::thread::sleep(Duration::from_millis(10));
        }
        gate_tx.send(()).unwrap();
        wait_done(&events, 2, 1);
        assert_eq!(*log.lock().unwrap(), ["1 début", "1 fin", "2"]);
    }

    #[test]
    fn the_automatic_fetch_gives_way_to_a_visible_network_operation_and_to_another_tab() {
        let dir = tempfile::tempdir().unwrap();
        let (mut shared, _events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        shared.auto_fetch_every = Some(Duration::ZERO);
        let url = "https://forge.test/contoso/depot.git".to_string();
        tab.remotes = Arc::new(vec![Remote { name: "origin".into(), fetch_url: url.clone(), push_url: url }]);
        let queues = tab.queues(&shared).unwrap();
        let blocking = |network, silent| {
            let op =
                PendingOp { tab: 9, id: 1, title: "autre onglet".into(), network, silent, cancel: CancelToken::new() };
            let (gate_tx, gate_rx) = mpsc::channel::<()>();
            let wait = move || {
                let _ = gate_rx.recv_timeout(Duration::from_secs(10));
            };
            (op, gate_tx, wait)
        };

        // Pull d'un autre onglet en cours sur la file des écritures : le fetch automatique passe son tour.
        let (pull, gate, wait) = blocking(true, false);
        queues.writes.submit(pull, wait);
        tab.tick(&mut shared, true);
        assert!(tab.running.is_empty(), "pas de fetch automatique pendant un pull");
        gate.send(()).unwrap();
        assert!(queues.writes.cancel_and_wait(Duration::from_secs(10)));

        // Fetch automatique d'un autre onglet du même dépôt en cours : pas de second.
        let (fetch, gate, wait) = blocking(true, true);
        queues.background.submit(fetch, wait);
        tab.tick(&mut shared, true);
        assert!(tab.running.is_empty(), "un seul fetch automatique à la fois");
        gate.send(()).unwrap();
        assert!(queues.background.cancel_and_wait(Duration::from_secs(10)));

        // Une indexation en cours ne l'empêche plus : il part, sur la file du réseau silencieux.
        let (staging, gate, wait) = blocking(false, false);
        queues.writes.submit(staging, wait);
        tab.tick(&mut shared, true);
        assert!(tab.running.iter().any(|op| op.silent), "fetch automatique lancé");
        // La file des écritures est occupée par l'indexation : le fetch n'y attend pas, il est sur la sienne.
        assert!(!queues.writes.pending().iter().any(|op| op.tab == 1), "pas dans la file des écritures");
        gate.send(()).unwrap();
    }

    /// Un fetch automatique qui tourne n'est jamais arrêté par une opération réseau visible (git tué pendant l'écriture
    /// des refs laisserait des `*.lock`) : elle attend sa fin, et le montre. Celui qui n'a pas commencé est annulé.
    #[test]
    fn a_visible_network_operation_cancels_the_waiting_automatic_fetch_and_waits_for_the_running_one() {
        use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

        let dir = tempfile::tempdir().unwrap();
        let (mut shared, events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        // Opération 1 : fetch automatique en cours, bloqué jusqu'à `gate` (serveur lent) ; il note s'il a été annulé.
        let (started_tx, started_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (first_cancelled, first_done) = (Arc::new(Mutex::new(None)), Arc::new(AtomicBool::new(false)));
        let (cancelled, done) = (first_cancelled.clone(), first_done.clone());
        tab.run_net(&shared, "Fetch", Refresh::NONE, true, move |_, n, _| {
            started_tx.send(()).unwrap();
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
            *cancelled.lock().unwrap() = Some(n.cancel.is_cancelled());
            done.store(true, SeqCst);
            Ok(None)
        });
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // Opération 2 : un second fetch automatique, en attente derrière le premier.
        let second_ran = Arc::new(AtomicBool::new(false));
        let ran = second_ran.clone();
        tab.run_net(&shared, "Fetch", Refresh::NONE, true, move |_, _, _| {
            ran.store(true, SeqCst);
            Ok(None)
        });
        // Opération 3 : pull visible ; il note si le premier fetch était fini quand il est parti.
        let pull_saw_fetch_done = Arc::new(Mutex::new(None));
        let (saw, done) = (pull_saw_fetch_done.clone(), first_done.clone());
        tab.run_net(&shared, "Pull", Refresh::NONE, false, move |_, _, _| {
            *saw.lock().unwrap() = Some(done.load(SeqCst));
            Ok(None)
        });
        let token = |tab: &RepoTab, id| tab.running.iter().find(|op| op.id == id).unwrap().cancel.clone();

        // Le fetch qui n'a pas commencé est annulé ; celui qui tourne ne l'est pas.
        let deadline = Instant::now() + Duration::from_secs(10);
        while !token(tab, 2).is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(token(tab, 2).is_cancelled(), "fetch automatique en attente annulé");
        assert!(!token(tab, 1).is_cancelled(), "le fetch automatique en cours n'est pas arrêté");
        // Le pull attend, et l'interface le montre.
        pump(&events, &mut shared, tab, "attente du pull", |ev| matches!(ev, RepoEvent::OpWaiting(3, true)));
        assert!(tab.running.iter().any(|op| op.id == 3 && op.waiting), "attente affichée");
        assert_eq!(*pull_saw_fetch_done.lock().unwrap(), None, "le pull ne part pas pendant le fetch");

        // Fin du fetch : le pull part ensuite ; le fetch annulé n'a pas lancé git ni rien notifié.
        gate_tx.send(()).unwrap();
        let (mut second, mut pull) = (None, None);
        pump(&events, &mut shared, tab, "fin du second fetch et du pull", |ev| {
            second = second.or_else(|| done_with(ev, 2).map(|r| matches!(r, Err(GitError::Cancelled))));
            pull = pull.or_else(|| done_with(ev, 3).map(Result::is_ok));
            second.is_some() && pull.is_some()
        });
        assert_eq!((second, pull), (Some(true), Some(true)), "second fetch annulé, pull réussi");
        assert_eq!(*first_cancelled.lock().unwrap(), Some(false), "fetch jamais annulé");
        assert_eq!(*pull_saw_fetch_done.lock().unwrap(), Some(true), "le pull est parti après la fin du fetch");
        assert!(!second_ran.load(SeqCst), "le fetch annulé n'a rien lancé");
        assert!(tab.running.is_empty(), "toutes terminées (la fin du premier fetch arrive avant celle du second)");
        assert_eq!(shared.toasts.titles(), Vec::<String>::new(), "aucune notification");
    }

    /// La suppression d'un remote réécrit les refs distantes : elle compte comme une opération réseau pour le fetch
    /// automatique, et attend la fin de celui qui tourne.
    #[test]
    fn removing_a_remote_waits_for_the_running_automatic_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let (mut shared, events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        let git = crate::test_git();
        git.cmd(dir.path()).args(["remote", "add", "origin", "https://forge.test/contoso/depot.git"]).run().unwrap();
        let remotes = || git.cmd(dir.path()).arg("remote").run().unwrap().stdout_str();
        let (started_tx, started_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        tab.run_net(&shared, "Fetch", Refresh::NONE, true, move |_, _, _| {
            started_tx.send(()).unwrap();
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
            Ok(None)
        });
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        tab.execute_dialog(super::super::Dialog::RemoveRemote { name: "origin".into() }, &mut shared);
        let group = tab.queues(&shared).unwrap().group().to_string();
        assert!(shared.queues.network_busy(&group), "pas de nouveau fetch automatique pendant la suppression");
        pump(&events, &mut shared, tab, "attente de la suppression", |ev| matches!(ev, RepoEvent::OpWaiting(2, true)));
        assert_eq!(remotes().trim(), "origin", "rien n'est supprimé pendant le fetch");
        gate_tx.send(()).unwrap();
        let mut removed = None;
        pump(&events, &mut shared, tab, "fin de la suppression", |ev| {
            removed = done_with(ev, 2).map(Result::is_ok);
            removed.is_some()
        });
        assert_eq!(removed, Some(true));
        assert_eq!(remotes().trim(), "", "remote supprimé après le fetch");
    }

    /// Un fetch automatique annulé ne fait pas de notification ; une opération visible annulée, si.
    #[test]
    fn a_cancelled_automatic_fetch_shows_no_notification() {
        let dir = tempfile::tempdir().unwrap();
        let (mut shared, events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        let queues = tab.queues(&shared).unwrap();
        // Les deux files sont occupées : les opérations sont annulées avant leur tour.
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (gate2_tx, gate2_rx) = mpsc::channel::<()>();
        queues.background.then(move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        queues.writes.then(move || {
            let _ = gate2_rx.recv_timeout(Duration::from_secs(10));
        });
        tab.run_net(&shared, "Fetch", Refresh::NONE, true, |_, _, _| Ok(Some("Fetch terminé".into())));
        tab.run_simple(&shared, "Commit", Refresh::NONE, |_| Ok(Some("Commit créé".into())));
        for op in &tab.running {
            op.cancel.cancel();
        }
        gate_tx.send(()).unwrap();
        gate2_tx.send(()).unwrap();
        let (mut fetch, mut commit) = (false, false);
        pump(&events, &mut shared, tab, "fin des deux opérations", |ev| {
            fetch |= done_with(ev, 1).is_some_and(|r| matches!(r, Err(GitError::Cancelled)));
            commit |= done_with(ev, 2).is_some_and(|r| matches!(r, Err(GitError::Cancelled)));
            fetch && commit
        });
        assert_eq!(shared.toasts.titles(), ["Commit : annulé"], "seule l'opération visible le signale");
    }

    /// L'état « occupé » des boutons vient du type de l'opération, jamais de son titre traduit (RV5b) : une opération
    /// dont le titre commence comme celui d'un bouton dans une langue (« Push du tag », « Fetch LFS », « Pull LFS » en
    /// français, « Stash rename » en anglais) ne l'occupe pas ; celle du bouton, si, comme le push de plusieurs
    /// branches occupe Push. Les titres sont d'abord en français (langue des tests unitaires : elle est globale au
    /// processus), puis remplacés par leur traduction anglaise : même résultat.
    #[test]
    fn the_busy_buttons_follow_the_kind_of_operation_in_every_language() {
        use super::super::branch_batch::BranchChoice;
        use dragontree_i18n::{Lang, translate};

        let dir = tempfile::tempdir().unwrap();
        let (shared, _events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        // Branches du dépôt (le push de plusieurs branches en a besoin).
        tab.refs = Some(Arc::new(tab.repo.as_ref().unwrap().refs().unwrap()));
        // File des écritures occupée : les opérations visibles y restent en attente ; annulées à la fin, elles ne
        // lancent pas git. Le fetch automatique, silencieux, passe par la file du réseau silencieux, libre : il lance
        // vraiment `git fetch` sur le dépôt temporaire, qui n'a pas de remote (aucun accès réseau).
        let queues = tab.queues(&shared).unwrap();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        queues.writes.then(move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        let kinds = [OpKind::Fetch, OpKind::Pull, OpKind::Push, OpKind::Commit, OpKind::Stash, OpKind::StashPop];
        let busy = |tab: &RepoTab| kinds.into_iter().filter(|k| tab.op_running(*k)).collect::<Vec<_>>();
        // Titres anglais des opérations lancées (« Fetch », « Push », « Commit »… ne sont pas traduits).
        let in_english = |tab: &mut RepoTab| {
            let french = [
                "Push du tag",
                "Fetch LFS",
                "Pull LFS",
                "Renommage du stash",
                "Push de plusieurs branches",
                "Pop du stash",
            ];
            for op in &mut tab.running {
                if let Some(fr) = french.iter().find(|fr| **fr == op.title) {
                    op.title = translate(Lang::En, fr).to_string();
                }
            }
            tab.running.iter().map(|op| op.title.clone()).collect::<Vec<_>>()
        };
        let stash = Oid::from_hex("0123456789abcdef0123456789abcdef01234567").unwrap();

        // Opérations d'autres types (et le fetch automatique, silencieux) : aucun bouton occupé.
        tab.op_push_tag(&shared, "origin".into(), "v1.0".into());
        tab.op_lfs_fetch(&shared);
        tab.op_lfs_pull(&shared);
        tab.op_stash_rename(&shared, stash, "stash@{0}".into(), "Nouveau nom".into());
        tab.op_fetch(&shared, None, true, true);
        assert_eq!(busy(tab), [], "titres en français");
        assert_eq!(in_english(tab), ["Tag push", "LFS fetch", "LFS pull", "Stash rename", "Fetch"]);
        assert_eq!(busy(tab), [], "titres en anglais");

        // Le push de plusieurs branches occupe Push, et lui seul, dans les deux langues.
        let feature = BranchChoice {
            name: "feature".into(),
            upstream: "origin/feature".into(),
            detail: String::new(),
            checked: true,
        };
        tab.op_push_branches(&shared, vec![feature]);
        assert_eq!(busy(tab), [OpKind::Push], "titres en français");
        assert_eq!(in_english(tab).last().map(String::as_str), Some("Pushing several branches"));
        assert_eq!(busy(tab), [OpKind::Push], "titres en anglais");

        // Les opérations des boutons les occupent, dans les deux langues.
        tab.op_fetch(&shared, None, true, false);
        tab.op_pull(&shared, "origin".into(), "main".into(), false);
        tab.op_push(&shared, "origin".into(), "main".into(), "main".into(), false, false, false);
        tab.op_commit(&shared, "Message".into(), false, false);
        tab.op_stash_save(&shared, "Travail en cours".into(), false, false);
        tab.op_stash(&shared, stash, "stash@{0}".into(), StashOp::Pop);
        assert_eq!(busy(tab), kinds, "titres en français");
        in_english(tab);
        assert_eq!(busy(tab), kinds, "titres en anglais");

        for op in &tab.running {
            op.cancel.cancel();
        }
        gate_tx.send(()).unwrap();
        assert!(queues.writes.cancel_and_wait(Duration::from_secs(10)));
        assert!(queues.background.cancel_and_wait(Duration::from_secs(10)));
    }

    /// L4 : un fetch refusé (automatique ou non) laisse l'indicateur de l'onglet jusqu'au prochain fetch de tous les
    /// remotes réussi ; le fetch automatique ne notifie que le premier refus d'une série.
    #[test]
    fn a_fetch_authentication_failure_stays_shown_until_the_next_successful_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let (mut shared, _events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        let failed = |stderr: &str| GitError::Failed { cmd: "git fetch".into(), code: 128, stderr: stderr.into() };
        let refused = || Err(failed("fatal: Authentication failed for 'https://forge.test/contoso/depot.git/'"));
        /// Opération terminée : fetch de tous les remotes (automatique ou bouton Fetch), d'un seul, ou pull.
        #[derive(Clone, Copy, PartialEq)]
        enum Op {
            AllRemotes,
            OneRemote,
            Pull,
        }
        // Fin d'une opération, sans git : comme l'application la reçoit de la file.
        let finish = |tab: &mut RepoTab, shared: &mut Shared, op: Op, silent, result| {
            tab.next_op_id += 1;
            let id = tab.next_op_id;
            if op == Op::AllRemotes {
                tab.fetches_of_all_remotes.insert(id);
            }
            let kind = if op == Op::Pull { OpKind::Pull } else { OpKind::Fetch };
            let (title, cancel) = ("Fetch".to_string(), CancelToken::new());
            let running = RunningOp { id, title: title.clone(), kind, progress: None, cancel, silent, waiting: false };
            tab.running.push(running);
            tab.handle_event(RepoEvent::OpDone { id, title, result, refresh: Refresh::NONE }, shared);
        };
        let toasts = |shared: &Shared| shared.toasts.titles().len();

        // Fetchs automatiques refusés : indicateur avec l'erreur, une seule notification pour la série.
        finish(tab, &mut shared, Op::AllRemotes, true, refused());
        finish(tab, &mut shared, Op::AllRemotes, true, refused());
        assert!(tab.fetch_auth_error.as_deref().is_some_and(|e| e.contains("Authentication failed")));
        assert_eq!(toasts(&shared), 1, "une notification par série");
        // Un autre échec (réseau), un pull réussi ou le fetch réussi d'un seul remote ne l'effacent pas.
        finish(tab, &mut shared, Op::AllRemotes, true, Err(failed("fatal: Could not resolve host: forge.test")));
        finish(tab, &mut shared, Op::Pull, false, Ok(None));
        finish(tab, &mut shared, Op::OneRemote, false, Ok(None));
        assert!(tab.fetch_auth_error.is_some(), "seul un fetch de tous les remotes réussi efface l'indicateur");
        // Fetch manuel de tous les remotes réussi : effacé.
        finish(tab, &mut shared, Op::AllRemotes, false, Ok(None));
        assert!(tab.fetch_auth_error.is_none());
        assert!(tab.fetches_of_all_remotes.is_empty(), "opérations terminées oubliées");

        // Fetch manuel d'un remote refusé : sa notification d'erreur habituelle, et l'indicateur ; le fetch automatique
        // refusé ensuite ne notifie pas (même série), celui qui réussit efface l'indicateur.
        finish(tab, &mut shared, Op::OneRemote, false, refused());
        assert!(tab.fetch_auth_error.is_some());
        assert_eq!(toasts(&shared), 2);
        finish(tab, &mut shared, Op::AllRemotes, true, refused());
        assert_eq!(toasts(&shared), 2, "même série");
        finish(tab, &mut shared, Op::AllRemotes, true, Ok(None));
        assert!(tab.fetch_auth_error.is_none(), "fetch automatique réussi");
        // Nouvelle série : nouvelle notification.
        finish(tab, &mut shared, Op::AllRemotes, true, refused());
        assert_eq!(toasts(&shared), 3);
    }

    /// Le bouton Fetch et le fetch automatique fetchent tous les remotes ; « Fetch <remote> » de la barre latérale, un
    /// seul.
    #[test]
    fn only_a_fetch_of_every_remote_is_marked_as_such() {
        let dir = tempfile::tempdir().unwrap();
        let (shared, _events, mut tabs) = tabs_on_one_repo(dir.path(), &[1]);
        let tab = &mut tabs[0];
        tab.op_fetch(&shared, None, true, true);
        tab.op_fetch(&shared, Some("origin".into()), true, false);
        let ids: Vec<u64> = tab.running.iter().map(|op| op.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(tab.fetches_of_all_remotes.contains(&ids[0]), "fetch de tous les remotes");
        assert!(!tab.fetches_of_all_remotes.contains(&ids[1]), "fetch d'un seul remote");
        for op in &tab.running {
            op.cancel.cancel();
        }
    }
}
