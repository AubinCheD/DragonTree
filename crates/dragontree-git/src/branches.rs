//! Actions groupées sur les branches locales : mise à jour (avance rapide) de toutes celles qui sont en retard,
//! push de celles qui sont en avance, détection et suppression des branches périmées.

use std::collections::HashSet;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::ops::MergeMode;
use crate::refs::{LocalBranch, RefsSnapshot};
use crate::repo::Repo;

/// Pourquoi une branche locale est dite périmée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    /// Sa branche suivie a disparu du remote (`[gone]`).
    Gone,
    /// Elle est entièrement fusionnée dans la branche par défaut.
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleBranch {
    pub name: String,
    pub reason: StaleReason,
}

/// Résultat d'une action sur une branche d'un lot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchOutcome {
    pub branch: String,
    pub result: Result<(), GitError>,
}

impl RefsSnapshot {
    /// Branches en retard sur leur branche suivie et qui peuvent avancer sans fusion (aucun commit local en plus).
    pub fn updatable_branches(&self) -> Vec<&LocalBranch> {
        self.locals
            .iter()
            .filter(|b| b.upstream.is_some() && !b.upstream_gone && b.behind > 0 && b.ahead == 0)
            .collect()
    }

    /// Branches qui ont des commits à pousser vers leur branche suivie.
    pub fn pushable_branches(&self) -> Vec<&LocalBranch> {
        self.locals.iter().filter(|b| b.upstream.is_some() && !b.upstream_gone && b.ahead > 0).collect()
    }

    /// Sépare une branche suivie (`origin/feature/x`) en (remote, branche) : le nom du remote peut contenir « / »,
    /// on prend le plus long préfixe connu.
    pub fn upstream_parts(&self, upstream: &str) -> Option<(String, String)> {
        let remote = self
            .remote_names()
            .into_iter()
            .filter(|r| upstream.starts_with(&format!("{r}/")))
            .max_by_key(|r| r.len())
            .or_else(|| upstream.split_once('/').map(|(r, _)| r.to_string()))?;
        let branch = upstream[remote.len() + 1..].to_string();
        Some((remote, branch))
    }

    /// Branches périmées, d'après les branches `merged` (fusionnées dans la branche par défaut) : jamais la branche
    /// courante ni la branche par défaut elle-même.
    pub fn stale_branches(&self, default: Option<&str>, merged: &HashSet<String>) -> Vec<StaleBranch> {
        let current = self.current_branch();
        self.locals
            .iter()
            .filter(|b| !b.is_head && Some(b.name.as_str()) != current && Some(b.name.as_str()) != default)
            .filter_map(|b| {
                let reason = if b.upstream_gone {
                    StaleReason::Gone
                } else if merged.contains(&b.name) {
                    StaleReason::Merged
                } else {
                    return None;
                };
                Some(StaleBranch { name: b.name.clone(), reason })
            })
            .collect()
    }
}

impl Repo {
    /// Branche par défaut du dépôt : celle que pointe `<remote>/HEAD` (`origin` d'abord), à défaut `main` ou `master`.
    /// Nom court, sans le remote ; la branche peut ne pas exister en local.
    pub fn default_branch(&self, refs: &RefsSnapshot) -> Option<String> {
        let mut remotes = refs.remote_names();
        remotes.sort_by_key(|r| r != "origin");
        for remote in remotes {
            let out =
                self.read().args(["symbolic-ref", "--quiet", "--short", &format!("refs/remotes/{remote}/HEAD")]).run();
            if let Ok(out) = out {
                let target = out.stdout_str();
                if let Some(name) = target.trim().strip_prefix(&format!("{remote}/")) {
                    return Some(name.to_string());
                }
            }
        }
        ["main", "master"].into_iter().find(|n| refs.local(n).is_some()).map(str::to_string)
    }

    /// Branches locales périmées : à l'amont disparu, ou fusionnées dans la branche par défaut (celle-ci, locale ou
    /// à défaut distante, sert de référence). Sans branche par défaut connue, seules les `[gone]` sont listées.
    pub fn stale_branches(&self, refs: &RefsSnapshot) -> Result<Vec<StaleBranch>, GitError> {
        let default = self.default_branch(refs);
        let mut merged = HashSet::new();
        if let Some(default) = &default {
            let reference = if refs.local(default).is_some() {
                Some(default.clone())
            } else {
                refs.remotes.iter().find(|r| &r.name == default && !r.is_symbolic).map(|r| r.full.clone())
            };
            if let Some(reference) = reference {
                let out = self
                    .read()
                    .args(["for-each-ref", "--format=%(refname)", &format!("--merged={reference}"), "refs/heads"])
                    .run()?;
                merged = out
                    .stdout_str()
                    .lines()
                    .filter_map(|l| l.strip_prefix("refs/heads/"))
                    .map(str::to_string)
                    .collect();
            }
        }
        let mut stale = refs.stale_branches(default.as_deref(), &merged);
        // Une branche sortie dans un autre worktree ne se supprime pas : git refuse.
        if let Ok(worktrees) = self.worktrees() {
            stale.retain(|s| !worktrees.iter().any(|w| !w.is_current && w.branch.as_deref() == Some(s.name.as_str())));
        }
        Ok(stale)
    }

    /// Avance chaque branche `(nom, branche suivie)` : la branche courante par `merge --ff-only`, les autres sans
    /// checkout. Une branche qui ne peut pas avancer en avance rapide n'est jamais déplacée ; chaque échec est
    /// rapporté sans arrêter les suivantes.
    pub fn update_branches(&self, plan: &[(String, String)], current: Option<&str>) -> Vec<BranchOutcome> {
        plan.iter()
            .map(|(branch, upstream)| {
                let result = if Some(branch.as_str()) == current {
                    self.merge(upstream, MergeMode::FfOnly)
                } else if let Some(wt) = self.worktree_holding(branch) {
                    // Avant git 2.35, `fetch .` déplacerait la branche sous le worktree qui l'a sortie.
                    Err(GitError::Refused(tr!(
                        "la branche est sortie dans le worktree {path}",
                        path = wt.path.display()
                    )))
                } else {
                    self.fast_forward_branch(branch, upstream)
                };
                BranchOutcome { branch: branch.clone(), result }
            })
            .collect()
    }

    /// Supprime des branches locales (`-d`, ou `-D` si `force`), chacune rapportée séparément.
    pub fn delete_branches(&self, names: &[String], force: bool) -> Vec<BranchOutcome> {
        names
            .iter()
            .map(|name| BranchOutcome { branch: name.clone(), result: self.delete_branch(name, force) })
            .collect()
    }
}
