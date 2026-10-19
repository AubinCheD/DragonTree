//! Actions sur plusieurs branches à la fois : Dépôt › Mettre à jour les branches locales, Pousser plusieurs
//! branches, Supprimer les branches périmées. Un dialogue liste les branches concernées (cases à cocher), puis une
//! seule opération de la file de l'onglet les traite une par une ; son rapport donne le résultat de chacune. La
//! logique est dans le moteur (`dragontree_git::branches`).

use dragontree_git::{BranchOutcome, GitError, StaleBranch, StaleReason};
use dragontree_i18n::{tr, trn};
use egui::{RichText, Ui};

use super::dialogs::warning;
use super::{Dialog, Refresh, RepoEvent, RepoTab};
use crate::app::{AppEvent, Shared};
use crate::theme::Palette;
use crate::toasts::Toast;

/// Une branche proposée dans un dialogue de lot : cochée ou non, avec sa branche suivie (mise à jour, push) et une
/// précision affichée à côté du nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchChoice {
    pub name: String,
    pub upstream: String,
    pub detail: String,
    pub checked: bool,
}

/// Raison d'un échec en une ligne : la ligne `!` (refus) ou d'erreur de git, pas les en-têtes `From .` et `To <url>`
/// qui précèdent le plus souvent le message utile.
fn failure_reason(e: &GitError) -> String {
    let summary = e.stderr_summary();
    let lines: Vec<&str> = summary.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines
        .iter()
        .find(|l| l.starts_with('!'))
        .or_else(|| lines.iter().find(|l| !l.starts_with("From ") && !l.starts_with("To ")))
        .or(lines.first())
        .map(|l| l.to_string())
        .unwrap_or_default()
}

/// Rapport d'un lot : une réussite pour toutes les branches, ou une erreur dont le détail donne le résultat de
/// chacune (réussies comprises, pour qu'un échec partiel soit lisible en un seul endroit). Une annulation reste une
/// annulation.
fn report(outcomes: Vec<BranchOutcome>, all_done: impl FnOnce(usize) -> String) -> Result<Option<String>, GitError> {
    if outcomes.iter().any(|o| o.result == Err(GitError::Cancelled)) {
        return Err(GitError::Cancelled);
    }
    if outcomes.iter().all(|o| o.result.is_ok()) {
        return Ok(Some(all_done(outcomes.len())));
    }
    let lines: Vec<String> = outcomes
        .iter()
        .map(|o| match &o.result {
            Ok(()) => format!("✓ {}", o.branch),
            Err(e) => format!("✗ {} : {}", o.branch, failure_reason(e)),
        })
        .collect();
    Err(GitError::Refused(lines.join("\n")))
}

fn checked(items: &[BranchChoice]) -> Vec<&BranchChoice> {
    items.iter().filter(|i| i.checked).collect()
}

impl RepoTab {
    /// Branches à mettre à jour : en retard et fast-forwardables, hors celles qu'un autre worktree a sorties (git
    /// ne les avance pas, ou les déplacerait sous ce worktree avant git 2.35) ; aucune pendant une opération en cours
    /// (pendant un rebase, HEAD est détachée et la branche rebasée passerait par `fetch .`).
    pub(super) fn updatable_here(&self) -> Vec<&dragontree_git::LocalBranch> {
        if self.op_state != dragontree_git::OpState::None {
            return Vec::new();
        }
        let Some(refs) = &self.refs else { return Vec::new() };
        refs.updatable_branches().into_iter().filter(|b| self.checked_out_elsewhere(&b.name).is_none()).collect()
    }

    pub(super) fn open_update_branches(&mut self) {
        let items = self
            .updatable_here()
            .into_iter()
            .map(|b| BranchChoice {
                name: b.name.clone(),
                upstream: b.upstream.clone().unwrap_or_default(),
                detail: trn!(b.behind, "en retard de {n} commit", "en retard de {n} commits"),
                checked: true,
            })
            .collect();
        self.dialog = Some(Dialog::UpdateBranches { items });
    }

    pub(super) fn open_push_branches(&mut self) {
        let Some(refs) = &self.refs else { return };
        let items = refs.pushable_branches().into_iter().map(push_choice).collect();
        self.dialog = Some(Dialog::PushBranches { items });
    }

    /// Ouvre le dialogue des branches périmées ; la liste (qui demande des commandes git) est lue en tâche de fond.
    pub(super) fn open_delete_stale(&mut self, shared: &Shared) {
        let Some(repo) = self.repo.clone() else { return };
        self.dialog = Some(Dialog::DeleteStaleBranches { items: None, force: false });
        let id = self.id;
        shared.jobs.spawn("stale-branches", move || {
            let found = repo.refs().and_then(|refs| repo.stale_branches(&refs));
            Some(AppEvent::Repo(id, RepoEvent::StaleBranches(found)))
        });
    }

    /// La liste des branches périmées est lue : elle remplit le dialogue qui l'attend (aucune case cochée : la
    /// suppression est destructrice).
    pub(super) fn on_stale_branches(&mut self, shared: &mut Shared, found: Result<Vec<StaleBranch>, GitError>) {
        let Some(Dialog::DeleteStaleBranches { items, .. }) = &mut self.dialog else { return };
        if items.is_some() {
            return;
        }
        match found {
            Ok(stale) => {
                *items = Some(
                    stale
                        .into_iter()
                        .map(|s| BranchChoice {
                            name: s.name,
                            upstream: String::new(),
                            detail: match s.reason {
                                StaleReason::Gone => tr!("branche suivie supprimée sur le remote").into(),
                                StaleReason::Merged => tr!("fusionnée dans la branche par défaut").into(),
                            },
                            checked: false,
                        })
                        .collect(),
                );
            }
            Err(e) => {
                self.dialog = None;
                shared.toast(Toast::error(tr!("Branches périmées illisibles"), e.to_string()));
            }
        }
    }

    /// Liste de cases à cocher ; renvoie vrai si au moins une branche est cochée.
    fn choices_list(ui: &mut Ui, items: &mut [BranchChoice]) -> bool {
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for item in items.iter_mut() {
                let label = format!("{}  ·  {}", item.name, item.detail);
                ui.checkbox(&mut item.checked, label).on_hover_text(&item.upstream);
            }
        });
        items.iter().any(|i| i.checked)
    }

    pub(super) fn update_branches_body(ui: &mut Ui, items: &mut [BranchChoice], p: &Palette) -> bool {
        ui.label(
            RichText::new(tr!(
                "Ces branches avancent jusqu'à leur branche suivie, sans fusion : une avance rapide seulement."
            ))
            .color(p.weak),
        );
        Self::choices_list(ui, items)
    }

    pub(super) fn push_branches_body(ui: &mut Ui, items: &mut [BranchChoice], p: &Palette) -> bool {
        ui.label(
            RichText::new(tr!("Ces branches sont poussées vers leur branche suivie (jamais de push forcé)."))
                .color(p.weak),
        );
        Self::choices_list(ui, items)
    }

    pub(super) fn delete_stale_body(
        ui: &mut Ui,
        items: &mut Option<Vec<BranchChoice>>,
        force: &mut bool,
        p: &Palette,
    ) -> bool {
        let Some(items) = items else {
            ui.label(RichText::new(tr!("Recherche des branches périmées…")).color(p.weak));
            return false;
        };
        if items.is_empty() {
            ui.label(tr!("Aucune branche périmée."));
            return false;
        }
        warning(ui, tr!("Les branches cochées seront supprimées de ce dépôt (pas sur le remote)."), p);
        let any = Self::choices_list(ui, items);
        ui.checkbox(force, tr!("Forcer (-D) : supprimer même si une branche n'est pas fusionnée")).on_hover_text(tr!(
            "Sans cette case, git refuse de supprimer une branche dont les commits ne sont dans aucune autre branche."
        ));
        any
    }

    pub(super) fn op_update_branches(&mut self, shared: &Shared, items: Vec<BranchChoice>) {
        let plan: Vec<(String, String)> =
            checked(&items).into_iter().map(|i| (i.name.clone(), i.upstream.clone())).collect();
        let current = self.refs.as_ref().and_then(|r| r.current_branch()).map(str::to_string);
        self.run_simple(shared, tr!("Mise à jour des branches"), Refresh::ALL, move |r| {
            report(r.update_branches(&plan, current.as_deref()), |n| {
                trn!(n, "{n} branche mise à jour", "{n} branches mises à jour")
            })
        });
    }

    pub(super) fn op_push_branches(&mut self, shared: &Shared, items: Vec<BranchChoice>) {
        let Some(refs) = self.refs.clone() else { return };
        let plan: Vec<(String, String, String)> = checked(&items)
            .into_iter()
            .filter_map(|i| refs.upstream_parts(&i.upstream).map(|(remote, branch)| (i.name.clone(), remote, branch)))
            .collect();
        self.run_net(shared, tr!("Push de plusieurs branches"), Refresh::REFS, false, move |r, n, on| {
            let outcomes = plan
                .iter()
                .map(|(local, remote, remote_branch)| BranchOutcome {
                    branch: local.clone(),
                    result: r.push(remote, local, remote_branch, false, false, n, on),
                })
                .collect();
            report(outcomes, |n| trn!(n, "{n} branche poussée", "{n} branches poussées"))
        });
    }

    pub(super) fn op_delete_stale(&mut self, shared: &Shared, items: Vec<BranchChoice>, force: bool) {
        let names: Vec<String> = checked(&items).into_iter().map(|i| i.name.clone()).collect();
        self.run_simple(shared, tr!("Suppression des branches"), Refresh::ALL, move |r| {
            report(r.delete_branches(&names, force), |n| trn!(n, "{n} branche supprimée", "{n} branches supprimées"))
        });
    }
}

/// Branche proposée au push : une branche qui a aussi divergé (en retard sur sa branche suivie) verrait son push
/// refusé sans forcer : elle n'est pas cochée d'office et le dit.
fn push_choice(b: &dragontree_git::LocalBranch) -> BranchChoice {
    let mut detail = trn!(b.ahead, "en avance de {n} commit", "en avance de {n} commits");
    if b.behind > 0 {
        detail = format!("{detail}, {}", tr!("divergée : le push sera refusé sans forcer"));
    }
    BranchChoice {
        name: b.name.clone(),
        upstream: b.upstream.clone().unwrap_or_default(),
        detail,
        checked: b.behind == 0,
    }
}

#[cfg(test)]
mod tests {
    use dragontree_git::Oid;

    use super::*;

    fn failed(stderr: &str) -> GitError {
        GitError::Failed { cmd: "git".into(), code: 1, stderr: stderr.into() }
    }

    fn outcome(branch: &str, result: Result<(), GitError>) -> BranchOutcome {
        BranchOutcome { branch: branch.into(), result }
    }

    #[test]
    fn the_report_keeps_the_reason_not_the_from_or_to_header() {
        let push = failed(
            "To file:///x/remote.git\n ! [rejected]        dev -> dev (non-fast-forward)\nerror: failed to push some refs to 'x'\nhint: Updates were rejected",
        );
        let fetch = failed("From .\nfatal: Not possible to fast-forward, aborting.");
        let out = report(vec![outcome("ok", Ok(())), outcome("dev", Err(push)), outcome("main", Err(fetch))], |_| {
            String::new()
        });
        let Err(GitError::Refused(text)) = out else { panic!("{out:?}") };
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "✓ ok");
        assert_eq!(lines[1], "✗ dev : ! [rejected]        dev -> dev (non-fast-forward)");
        assert_eq!(lines[2], "✗ main : Not possible to fast-forward, aborting.");
    }

    #[test]
    fn a_cancellation_is_not_wrapped_in_a_failure_report() {
        let out = report(vec![outcome("a", Ok(())), outcome("b", Err(GitError::Cancelled))], |_| String::new());
        assert_eq!(out, Err(GitError::Cancelled));
        assert_eq!(report(vec![outcome("a", Ok(()))], |n| format!("{n}")), Ok(Some("1".into())));
    }

    #[test]
    fn a_diverged_branch_is_not_prechecked_for_push() {
        let branch = |ahead, behind| dragontree_git::LocalBranch {
            name: "dev".into(),
            oid: Oid::from_hex(&"1".repeat(40)).unwrap(),
            upstream: Some("origin/dev".into()),
            ahead,
            behind,
            upstream_gone: false,
            is_head: false,
        };
        assert!(push_choice(&branch(2, 0)).checked);
        let diverged = push_choice(&branch(2, 1));
        assert!(!diverged.checked);
        assert!(diverged.detail.contains("divergée"), "{}", diverged.detail);
    }
}
