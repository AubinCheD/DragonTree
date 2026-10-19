//! Ignorer des fichiers : aperçu d'un motif et ajout dans le `.gitignore` racine, celui d'un dossier ou
//! `.git/info/exclude`.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::repo::{Repo, native_path};
use crate::submodule::check_relative;

/// Nombre maximal de fichiers listés par catégorie dans un aperçu (les totaux restent exacts).
pub const PREVIEW_LIMIT: usize = 200;

/// Fichier qui reçoit le motif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnoreTarget {
    /// `.gitignore` à la racine du dépôt.
    Root,
    /// `.gitignore` d'un dossier (chemin relatif au dépôt, séparateur `/`) : le motif s'applique à partir de lui.
    Dir(String),
    /// `.git/info/exclude` : propre à ce clone, jamais commité.
    InfoExclude,
}

/// Fichiers concernés par un motif.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IgnorePreview {
    /// Fichiers non suivis que le motif ferait ignorer (ceux que la configuration ignore déjà sont écartés),
    /// limités à [`PREVIEW_LIMIT`].
    pub untracked: Vec<String>,
    pub untracked_total: usize,
    /// Fichiers déjà suivis qui correspondent : ignorer ne les retire pas de l'index.
    pub tracked: Vec<String>,
    pub tracked_total: usize,
}

/// Ajoute `pattern` sur une ligne à la fin de `path` (créé au besoin ; octets existants préservés, encodage
/// compris).
pub(crate) fn append_line(path: &Path, pattern: &str) -> Result<(), GitError> {
    let needs_newline = match std::fs::read(path) {
        Ok(bytes) => !bytes.is_empty() && !bytes.ends_with(b"\n"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(e.into()),
    };
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    if needs_newline {
        file.write_all(b"\n")?;
    }
    file.write_all(pattern.as_bytes())?;
    file.write_all(b"\n")?;
    Ok(())
}

fn normalized_dir(dir: &str) -> String {
    dir.trim().replace('\\', "/").trim_matches('/').to_string()
}

/// Motif tel que git le lit pour `--exclude` (relatif à la racine), et dossier où limiter la recherche. Un motif
/// ancré (`/x`, `a/b`) d'un `.gitignore` de dossier se lit à partir de ce dossier. `None` : rien à prévisualiser
/// (vide, commentaire, négation).
fn effective_exclude(pattern: &str, target: &IgnoreTarget) -> Option<(String, Option<String>)> {
    let pattern = pattern.trim();
    if pattern.is_empty() || pattern.starts_with(['#', '!']) {
        return None;
    }
    match target {
        IgnoreTarget::Root | IgnoreTarget::InfoExclude => Some((pattern.to_string(), None)),
        IgnoreTarget::Dir(dir) => {
            let dir = normalized_dir(dir);
            if dir.is_empty() {
                return Some((pattern.to_string(), None));
            }
            let anchored = pattern.trim_end_matches('/').contains('/');
            // Un `.gitignore` de dossier ne vaut qu'à l'intérieur : `--exclude=<motif> -- <dossier>` toucherait aussi
            // le dossier lui-même et ses parents. `/<dossier>/**/<motif>` donne exactement le résultat de git.
            let exclude = if anchored {
                format!("/{dir}/{}", pattern.trim_start_matches('/'))
            } else {
                format!("/{dir}/**/{pattern}")
            };
            Some((exclude, Some(dir)))
        }
    }
}

impl Repo {
    /// Sortie de `git ls-files -z <args>` (limitée à `dir` si donné), chemins sans guillemets.
    fn list_files(&self, args: &[&str], dir: Option<&str>) -> Result<Vec<String>, GitError> {
        let mut cmd = self.read_paths().args(["-c", "core.quotepath=false", "ls-files", "-z"]).args(args);
        if let Some(dir) = dir {
            cmd = cmd.args(["--", dir]);
        }
        let out = cmd.run()?;
        Ok(out.stdout_str().split('\0').filter(|p| !p.is_empty()).map(str::to_string).collect())
    }

    /// Aperçu de `pattern` pour `target` : fichiers non suivis qui seraient ignorés (`ls-files --others --ignored
    /// --exclude`, moins ceux déjà ignorés) et fichiers suivis qui correspondent.
    pub fn ignore_preview(&self, pattern: &str, target: &IgnoreTarget) -> Result<IgnorePreview, GitError> {
        let Some((exclude, dir)) = effective_exclude(pattern, target) else { return Ok(IgnorePreview::default()) };
        let flag = format!("--exclude={exclude}");
        let dir = dir.as_deref();
        let matching = self.list_files(&["--others", "--ignored", &flag], dir)?;
        let already: HashSet<String> = if matching.is_empty() {
            HashSet::new()
        } else {
            self.list_files(&["--others", "--ignored", "--exclude-standard"], dir)?.into_iter().collect()
        };
        let mut untracked: Vec<String> = matching.into_iter().filter(|p| !already.contains(p)).collect();
        let mut tracked = self.list_files(&["--cached", "--ignored", &flag], dir)?;
        let (untracked_total, tracked_total) = (untracked.len(), tracked.len());
        untracked.truncate(PREVIEW_LIMIT);
        tracked.truncate(PREVIEW_LIMIT);
        Ok(IgnorePreview { untracked, untracked_total, tracked, tracked_total })
    }

    /// Ajoute `pattern` dans le fichier désigné par `target` (créé au besoin). Refuse un motif vide ou sur
    /// plusieurs lignes, et un dossier absent ou hors du dépôt.
    pub fn add_ignore_pattern(&self, pattern: &str, target: &IgnoreTarget) -> Result<(), GitError> {
        let pattern = pattern.trim();
        if pattern.is_empty() || pattern.contains(['\n', '\r']) {
            return Err(GitError::Refused(tr!("Le motif doit tenir sur une seule ligne, non vide.").into()));
        }
        match target {
            IgnoreTarget::Root => append_line(&self.workdir.join(".gitignore"), pattern),
            IgnoreTarget::Dir(dir) => {
                let dir = normalized_dir(dir);
                check_relative(&dir)?;
                let folder = self.workdir.join(&dir);
                if !folder.is_dir() {
                    return Err(GitError::Refused(tr!("Le dossier « {dir} » n'existe pas.", dir)));
                }
                append_line(&folder.join(".gitignore"), pattern)
            }
            IgnoreTarget::InfoExclude => {
                let out = self.read().args(["rev-parse", "--git-path", "info/exclude"]).run()?;
                let path = native_path(out.stdout_str().trim());
                let path = if path.is_absolute() { path } else { self.workdir.join(path) };
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                append_line(&path, pattern)
            }
        }
    }
}
