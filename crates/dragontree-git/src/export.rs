//! Enregistrer comme patch : un commit, une suite de commits, un stash ou des fichiers de la copie de travail.
//! Chaque patch produit s'applique avec `git apply` (voir [`crate::apply`]) ; ceux des commits sont au format mail
//! (`git format-patch`) et se rejouent avec `git am`.

use std::path::{Path, PathBuf};

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;
use crate::repo::Repo;

/// Nom de fichier proposé pour le patch d'un commit, d'après son sujet (comme `git format-patch` : minuscules,
/// lettres et chiffres, le reste devient un seul tiret).
pub fn patch_file_name(subject: &str) -> String {
    let mut slug = String::new();
    for c in subject.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug: String = slug.chars().take(52).collect();
    let slug = slug.trim_end_matches('-');
    format!("{}.patch", if slug.is_empty() { "commit" } else { slug })
}

impl Repo {
    /// Le commit a plusieurs parents. `format-patch -1` d'une fusion donnerait le patch d'un autre commit (le premier
    /// non fusion en remontant) : on le refuse avant.
    fn is_merge(&self, id: Oid) -> Result<bool, GitError> {
        let out = self.read().args(["rev-list", "--parents", "-n", "1", &id.to_hex()]).run()?;
        Ok(out.stdout_str().split_whitespace().count() > 2)
    }

    /// Patch au format mail d'un commit (`git format-patch -1 --stdout`). Un commit de fusion n'en a pas.
    pub fn commit_patch(&self, id: Oid) -> Result<Vec<u8>, GitError> {
        if self.is_merge(id)? {
            return Err(GitError::Refused(
                tr!("Un commit de fusion n'a pas de patch : choisissez ses parents ou une plage de commits.").into(),
            ));
        }
        Ok(self.read().args(["format-patch", "-1", "--stdout", "--no-color", &id.to_hex()]).run()?.stdout)
    }

    /// Les commits de `id` à HEAD au sens de `git format-patch <id>^..HEAD`, du plus ancien au plus récent, sans les
    /// fusions (elles n'ont pas de patch). Refuse un commit qui n'est pas un ancêtre de HEAD.
    pub fn commits_to_head(&self, id: Oid) -> Result<Vec<Oid>, GitError> {
        let hex = id.to_hex();
        let ancestor = self.read().args(["merge-base", "--is-ancestor", &hex, "HEAD"]).allow_codes(&[1]).run()?;
        if ancestor.code != 0 {
            return Err(GitError::Refused(tr!("Ce commit n'est pas un ancêtre de HEAD.").into()));
        }
        // Comme `git format-patch <id>^..HEAD` : tous les commits (hors fusions) atteignables depuis HEAD et pas depuis le
        // parent de `id`, branches latérales comprises. Pour une racine : tout l'historique de HEAD.
        let parent = format!("{hex}^");
        let has_parent = self.read().args(["rev-parse", "-q", "--verify", &parent]).allow_codes(&[1]).run()?.code == 0;
        let range = if has_parent { format!("{parent}..HEAD") } else { "HEAD".to_string() };
        let out = self.read().args(["rev-list", "--reverse", "--no-merges", &range]).run()?;
        Ok(out.stdout_str().lines().filter_map(|l| Oid::from_hex(l.trim())).collect())
    }

    /// Écrit un fichier de patch par commit dans `dir` (`0001-sujet.patch`…), dans l'ordre donné (du plus ancien
    /// au plus récent). Les commits de fusion, sans patch, sont ignorés. Renvoie les fichiers écrits.
    pub fn commit_patches_to_dir(&self, commits: &[Oid], dir: &Path) -> Result<Vec<PathBuf>, GitError> {
        std::fs::create_dir_all(dir)?;
        let mut files = Vec::new();
        for id in commits {
            if self.is_merge(*id)? {
                continue;
            }
            let out = self
                .read()
                .args(["format-patch", "-1", "--no-color", "--start-number"])
                .arg((files.len() + 1).to_string())
                .arg("-o")
                .arg(dir)
                .arg(id.to_hex())
                .run()?;
            files.extend(out.stdout_str().lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from));
        }
        Ok(files)
    }

    /// Patch d'un stash, fichiers non suivis compris quand git le permet (`--include-untracked`, git 2.32), sinon
    /// sans eux.
    pub fn stash_patch(&self, stash: Oid) -> Result<Vec<u8>, GitError> {
        let hex = stash.to_hex();
        let base = ["stash", "show", "-p", "--binary", "--no-color", "--no-ext-diff"];
        match self.read().args(base).arg("--include-untracked").arg(&hex).run() {
            Ok(out) => Ok(out.stdout),
            Err(GitError::Failed { stderr, .. })
                if stderr.contains("include-untracked") || stderr.contains("usage") =>
            {
                Ok(self.read().args(base).arg(&hex).run()?.stdout)
            }
            Err(e) => Err(e),
        }
    }

    /// Patch des fichiers donnés de la copie de travail : `tracked` suivis (modifications indexées si `staged`,
    /// sinon non indexées) et `untracked` non suivis (créations). Chemins pris littéralement, fichiers binaires
    /// compris. Pour un renommage indexé, donner l'ancien nom en plus du nouveau (sinon la suppression de l'ancien
    /// fichier manque au patch).
    pub fn files_patch(&self, tracked: &[String], untracked: &[String], staged: bool) -> Result<Vec<u8>, GitError> {
        let mut patch = Vec::new();
        let base = ["diff", "--binary", "--no-color", "--no-ext-diff", "--no-renames"];
        if !tracked.is_empty() {
            let mut cmd = self.read_paths().args(base);
            if staged {
                cmd = cmd.arg("--cached");
            }
            patch.extend(cmd.arg("--").args(tracked).run()?.stdout);
        }
        for path in untracked {
            // `--no-index` sort avec le code 1 quand les fichiers diffèrent.
            let out =
                self.read_paths().args(base).args(["--no-index", "--", "/dev/null", path]).allow_codes(&[1]).run()?;
            patch.extend(out.stdout);
        }
        Ok(patch)
    }
}
