//! Application d'un patch complet (fichier, presse-papiers) : aperçu, vérification, copie de travail, index, `git am`.
//! L'application ligne à ligne d'un hunk choisi est dans [`crate::patch`].

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::repo::{OpState, Repo};

/// Où appliquer un patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTarget {
    /// Copie de travail seulement (`git apply`).
    Worktree,
    /// Index seulement (`git apply --cached`).
    Index,
    /// Index et copie de travail (`git apply --index`) : ce qu'exige `git am`, qui refuse une copie de travail différente
    /// de l'index sur les fichiers touchés.
    IndexAndWorktree,
}

/// Fichier touché par un patch (`git apply --numstat`). Les comptes sont `None` pour un fichier binaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchFile {
    pub path: String,
    pub added: Option<usize>,
    pub removed: Option<usize>,
}

/// Parse `git apply --numstat -z` : `ajouts<TAB>suppressions<TAB>chemin NUL`, ou pour un renommage
/// `ajouts<TAB>suppressions<TAB> NUL ancien NUL nouveau NUL`.
pub fn parse_numstat_z(bytes: &[u8]) -> Vec<PatchFile> {
    let text = String::from_utf8_lossy(bytes);
    let mut fields = text.split('\0');
    let mut out = Vec::new();
    while let Some(head) = fields.next() {
        let mut parts = head.trim_start_matches('\n').splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let path = if path.is_empty() {
            // Renommage : l'ancien chemin puis le nouveau suivent.
            let _old = fields.next();
            fields.next().unwrap_or_default().to_string()
        } else {
            path.to_string()
        };
        out.push(PatchFile { path, added: added.parse().ok(), removed: removed.parse().ok() });
    }
    out
}

/// Patch au format mail de `git format-patch` (première ligne `From <sha> …`) : `git am` peut en faire des commits.
pub fn is_mail_patch(patch: &[u8]) -> bool {
    let first = patch.split(|&b| b == b'\n').next().unwrap_or_default();
    first
        .strip_prefix(b"From ")
        .is_some_and(|rest| rest.len() > 40 && rest[..40].iter().all(u8::is_ascii_hexdigit) && rest[40] == b' ')
}

impl Repo {
    /// Fichiers touchés par le patch, sans rien modifier.
    pub fn patch_numstat(&self, patch: &[u8]) -> Result<Vec<PatchFile>, GitError> {
        let out = self.read().args(["apply", "--numstat", "-z", "-"]).stdin(patch.to_vec()).run()?;
        Ok(parse_numstat_z(&out.stdout))
    }

    /// Le patch s'applique-t-il à la cible ? Sans rien modifier ; l'erreur est l'explication de git.
    pub fn patch_check(&self, patch: &[u8], target: ApplyTarget) -> Result<(), GitError> {
        self.apply_command(target).arg("--check").arg("-").stdin(patch.to_vec()).run().map(|_| ())
    }

    /// Applique le patch à la cible, en entier ou pas du tout.
    pub fn apply_patch_to(&self, patch: &[u8], target: ApplyTarget) -> Result<(), GitError> {
        self.apply_command(target).arg("-").stdin(patch.to_vec()).run().map(|_| ())
    }

    fn apply_command(&self, target: ApplyTarget) -> crate::runner::GitCommand {
        let cmd = self.write().args(["apply", "--whitespace=nowarn"]);
        match target {
            ApplyTarget::Worktree => cmd,
            ApplyTarget::Index => cmd.arg("--cached"),
            ApplyTarget::IndexAndWorktree => cmd.arg("--index"),
        }
    }

    /// Crée les commits d'un patch au format mail (`git am`). Refuse quand une opération est déjà en cours (une
    /// session `am` ou `rebase --apply` de l'utilisateur serait abandonnée, HEAD ramenée en arrière). En cas
    /// d'échec, la session que cet appel vient de créer est abandonnée : le dépôt n'est pas laissé au milieu d'une
    /// opération ; si l'abandon échoue, le message le dit.
    pub fn am_patch(&self, patch: &[u8]) -> Result<(), GitError> {
        if self.operation_state() != OpState::None {
            return Err(GitError::Refused(
                tr!("Une opération (fusion, rebase, am…) est déjà en cours : terminez-la ou abandonnez-la d'abord.")
                    .into(),
            ));
        }
        let Err(e) = self.write().args(["am", "--quiet"]).stdin(patch.to_vec()).run() else { return Ok(()) };
        match self.write().args(["am", "--abort"]).run() {
            Ok(_) => Err(e),
            Err(abort) => Err(GitError::Refused(tr!(
                "{error}
L'abandon de git am a aussi échoué ({abort}) : lancez « git am --abort » dans le dépôt.",
                error = e,
                abort
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_parses_files_binaries_and_renames() {
        let out = b"2\t1\ta.txt\0-\t-\timg.png\x000\t0\t\0vieux.txt\0neuf.txt\0";
        assert_eq!(
            parse_numstat_z(out),
            [
                PatchFile { path: "a.txt".into(), added: Some(2), removed: Some(1) },
                PatchFile { path: "img.png".into(), added: None, removed: None },
                PatchFile { path: "neuf.txt".into(), added: Some(0), removed: Some(0) },
            ]
        );
    }
}
