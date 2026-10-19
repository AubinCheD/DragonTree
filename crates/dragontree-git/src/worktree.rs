//! Worktrees (`git worktree`) : liste, création, suppression et nettoyage.

use std::path::{Path, PathBuf};

use crate::error::GitError;
use crate::oid::Oid;
use crate::repo::{Repo, native_path};

/// Arbre de travail du dépôt (le principal ou un worktree lié).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    /// Dossier, au format natif.
    pub path: PathBuf,
    /// Commit de HEAD (`None` : dépôt nu, ou branche sans commit).
    pub head: Option<Oid>,
    /// Branche sortie, sans `refs/heads/` (`None` : HEAD détachée ou dépôt nu).
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    /// Verrouillé (`git worktree lock`) : git refuse de le supprimer ou de le nettoyer.
    pub locked: bool,
    /// Son dossier a disparu : `git worktree prune` le retirera.
    pub prunable: bool,
    /// Le worktree principal (le premier de la liste).
    pub is_main: bool,
    /// Le worktree du dépôt ouvert.
    pub is_current: bool,
}

/// Contenu d'un nouveau worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeStart {
    /// Une branche locale existante, sortie dans le worktree.
    Branch(String),
    /// Une branche distante (`origin/x`) : une branche locale `local` qui la suit est créée et sortie.
    Track { remote_branch: String, local: String },
    /// Un commit (ou toute révision), en HEAD détachée.
    Detached(String),
}

/// Lit `git worktree list --porcelain` (sans `-z`, absent avant git 2.36) : un bloc par worktree, séparés par
/// une ligne vide, le premier étant le worktree principal.
pub fn parse_worktrees(out: &str) -> Vec<Worktree> {
    let mut list: Vec<Worktree> = Vec::new();
    for line in out.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            list.push(Worktree {
                path: native_path(value),
                head: None,
                branch: None,
                detached: false,
                bare: false,
                locked: false,
                prunable: false,
                is_main: list.is_empty(),
                is_current: false,
            });
            continue;
        }
        let Some(wt) = list.last_mut() else { continue };
        match key {
            // Un dépôt sans commit annonce un HEAD nul.
            "HEAD" => wt.head = Oid::from_hex(value).filter(|o| !o.is_zero()),
            "branch" => wt.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).to_string()),
            "detached" => wt.detached = true,
            "bare" => wt.bare = true,
            "locked" => wt.locked = true,
            "prunable" => wt.prunable = true,
            _ => {}
        }
    }
    list
}

/// Même dossier, quelle que soit l'écriture (séparateurs, casse, barre finale).
pub(crate) fn same_dir(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase();
    key(a) == key(b)
}

impl Repo {
    /// Worktrees du dépôt, le principal d'abord ; celui du dépôt ouvert est marqué `is_current`.
    pub fn worktrees(&self) -> Result<Vec<Worktree>, GitError> {
        let out = self.read().args(["worktree", "list", "--porcelain"]).run()?;
        let mut list = parse_worktrees(&out.stdout_str());
        for wt in &mut list {
            wt.is_current = same_dir(&wt.path, &self.workdir);
        }
        Ok(list)
    }

    /// Crée un worktree dans `path` (dossier absent ou vide).
    pub fn add_worktree(&self, path: &Path, start: &WorktreeStart) -> Result<(), GitError> {
        let cmd = self.write().args(["worktree", "add"]);
        let cmd = match start {
            WorktreeStart::Branch(branch) => cmd.arg(path).arg(branch),
            WorktreeStart::Track { remote_branch, local } => {
                cmd.args(["--track", "-b", local.as_str()]).arg(path).arg(remote_branch)
            }
            WorktreeStart::Detached(rev) => cmd.arg("--detach").arg(path).arg(rev),
        };
        cmd.run().map(|_| ())
    }

    /// Supprime le worktree `path` (dossier compris). Sans `force`, git refuse un worktree qui a des
    /// modifications locales ou des fichiers non suivis : `force` ne se passe que sur confirmation explicite.
    pub fn remove_worktree(&self, path: &Path, force: bool) -> Result<(), GitError> {
        let cmd = self.write().args(["worktree", "remove"]);
        let cmd = if force { cmd.arg("--force") } else { cmd };
        cmd.arg(path).run().map(|_| ())
    }

    /// Retire les worktrees dont le dossier a disparu (`git worktree prune`).
    pub fn prune_worktrees(&self) -> Result<(), GitError> {
        self.write().args(["worktree", "prune"]).run().map(|_| ())
    }

    /// Worktree autre que celui-ci où la branche locale `branch` est sortie.
    pub(crate) fn worktree_holding(&self, branch: &str) -> Option<Worktree> {
        self.worktrees().ok()?.into_iter().find(|w| !w.is_current && w.branch.as_deref() == Some(branch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_blocks_are_parsed() {
        let a = "1111111111111111111111111111111111111111";
        let b = "2222222222222222222222222222222222222222";
        let out = format!(
            "worktree C:/DEV/depot\nHEAD {a}\nbranch refs/heads/main\n\n\
             worktree C:/DEV/depot-feature/x\nHEAD {b}\nbranch refs/heads/feature/x\nlocked raison du verrou\n\n\
             worktree C:/DEV/depot-detache\nHEAD {a}\ndetached\nprunable gitdir file points to non-existent location\n\n"
        );
        let list = parse_worktrees(&out);
        assert_eq!(list.len(), 3);
        assert!(list[0].is_main && !list[1].is_main);
        assert_eq!(list[0].branch.as_deref(), Some("main"));
        assert_eq!(list[0].head, Oid::from_hex(a));
        assert_eq!(list[1].path, native_path("C:/DEV/depot-feature/x"));
        assert_eq!(list[1].branch.as_deref(), Some("feature/x"));
        assert!(list[1].locked && !list[1].prunable);
        assert!(list[2].detached && list[2].prunable && list[2].branch.is_none());
    }

    #[test]
    fn bare_and_unborn_entries() {
        let zero = "0".repeat(40);
        let out =
            format!("worktree C:/DEV/nu.git\nbare\n\nworktree C:/DEV/vide\nHEAD {zero}\nbranch refs/heads/main\n");
        let list = parse_worktrees(&out);
        assert!(list[0].bare && list[0].head.is_none());
        assert_eq!(list[1].head, None, "branche sans commit");
        assert_eq!(list[1].branch.as_deref(), Some("main"));
    }

    #[test]
    fn same_dir_ignores_separators_case_and_trailing_slash() {
        assert!(same_dir(Path::new("C:/DEV/Depot/"), Path::new(r"c:\dev\depot")));
        assert!(!same_dir(Path::new("C:/DEV/depot"), Path::new("C:/DEV/depot-x")));
    }
}
