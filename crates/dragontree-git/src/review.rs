//! Textes que l'IA lit (Réglages › IA) : le diff indexé d'un commit à écrire, et la différence d'une branche avec
//! sa base. Ce sont des lectures ordinaires ; rien n'est envoyé d'ici.

use crate::error::GitError;
use crate::repo::Repo;
use crate::runner::CancelToken;

/// Ce que la revue d'une branche lit : les commits de la branche depuis sa base et leur diff cumulé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchReview {
    /// Ancêtre commun de la branche et de sa base (SHA complet).
    pub merge_base: String,
    /// Sujet de chaque commit de la branche absent de la base, le plus récent d'abord.
    pub commits: Vec<String>,
    /// `git diff <merge-base> <branche>` ; vide si la branche n'ajoute rien à sa base.
    pub diff: String,
}

impl Repo {
    /// Diff de l'index par rapport à HEAD, en texte (`git diff --cached`), sans programme de diff externe.
    pub fn staged_diff_text(&self, cancel: &CancelToken) -> Result<String, GitError> {
        let out = self
            .read()
            .args(["diff", "--cached", "--no-ext-diff", "--no-textconv", "--no-color", "-M"])
            .cancel(cancel.clone())
            .run()?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Ce qu'apporte `branch` par rapport à `base` : diff de leur ancêtre commun (`merge-base`) à la branche, plus
    /// les sujets des commits. `base` et `branch` sont des noms de branche ou des SHA.
    pub fn branch_review(&self, branch: &str, base: &str, cancel: &CancelToken) -> Result<BranchReview, GitError> {
        let merge_base = self.read().args(["merge-base", base, branch]).cancel(cancel.clone()).run()?.stdout_str();
        let merge_base = merge_base.trim().to_string();
        let range = format!("{merge_base}..{branch}");
        let log = self.read().args(["log", "--no-color", "--format=%s", &range]).cancel(cancel.clone()).run()?;
        let diff = self
            .read()
            .args(["diff", "--no-ext-diff", "--no-textconv", "--no-color", "-M", &merge_base, branch, "--"])
            .cancel(cancel.clone())
            .run()?;
        Ok(BranchReview {
            merge_base,
            commits: log.stdout_str().lines().map(str::to_string).collect(),
            diff: String::from_utf8_lossy(&diff.stdout).into_owned(),
        })
    }
}
