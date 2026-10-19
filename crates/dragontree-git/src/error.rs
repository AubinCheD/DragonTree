use std::fmt;

use dragontree_i18n::tr;

/// Erreur d'une opération git. Clonable pour pouvoir être affichée à plusieurs endroits
/// de l'interface (bandeau, notification, journal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    /// L'exécutable git est introuvable.
    NotFound,
    /// Le processus n'a pas pu être lancé.
    Spawn(String),
    /// git a terminé avec un code de sortie non accepté.
    Failed { cmd: String, code: i32, stderr: String },
    /// Le dépôt appartient à un autre utilisateur (`safe.directory`).
    DubiousOwnership { path: String },
    /// Le dossier n'est pas un dépôt git.
    NotARepository { path: String },
    /// Opération annulée par l'utilisateur.
    Cancelled,
    /// Sortie de git inattendue.
    Parse(String),
    /// Erreur d'entrée/sortie.
    Io(String),
    /// Opération refusée par DragonTree pour protéger des données (message explicatif).
    Refused(String),
    /// Erreur interne de DragonTree (tâche interrompue par une panique).
    Internal(String),
}

impl GitError {
    /// Construit l'erreur d'une commande échouée en reconnaissant les cas particuliers.
    pub fn from_failure(cmd: String, code: i32, stderr: String) -> GitError {
        const DUBIOUS: &str = "detected dubious ownership in repository at '";
        if let Some(start) = stderr.find(DUBIOUS) {
            // Le chemin peut contenir des apostrophes : il s'arrête à la dernière de la ligne.
            let rest = &stderr[start + DUBIOUS.len()..];
            let line = rest.lines().next().unwrap_or(rest);
            if let Some(end) = line.rfind('\'') {
                return GitError::DubiousOwnership { path: line[..end].to_string() };
            }
        }
        GitError::Failed { cmd, code, stderr }
    }

    /// Résumé lisible du stderr (lignes significatives, sans préfixes `fatal:`/`error:`).
    pub fn stderr_summary(&self) -> String {
        let stderr = match self {
            GitError::Failed { stderr, .. } => stderr,
            other => return other.to_string(),
        };
        let lines: Vec<&str> = stderr
            .lines()
            .map(str::trim_end)
            .filter(|l| !l.trim().is_empty() && !l.starts_with("hint:"))
            .map(|l| l.strip_prefix("fatal: ").or_else(|| l.strip_prefix("error: ")).unwrap_or(l))
            .collect();
        // Les messages utiles de git tiennent en quelques lignes ; on évite les pavés.
        let mut out = lines.iter().take(8).copied().collect::<Vec<_>>().join("\n");
        if lines.len() > 8 {
            out.push_str("\n…");
        }
        out.trim().to_string()
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotFound => {
                write!(f, "{}", tr!("git est introuvable : installez Git ou indiquez son chemin dans les réglages"))
            }
            GitError::Spawn(e) => write!(f, "{}", tr!("impossible de lancer git : {e}", e)),
            GitError::Failed { code, .. } => {
                let summary = self.stderr_summary();
                if summary.is_empty() {
                    write!(f, "{}", tr!("git a échoué (code {code})", code))
                } else {
                    write!(f, "{summary}")
                }
            }
            GitError::DubiousOwnership { path } => write!(
                f,
                "{}",
                tr!(
                    "Le dépôt {path} appartient à un autre utilisateur Windows : git refuse de l'ouvrir tant qu'il n'est pas déclaré sûr (safe.directory)",
                    path
                )
            ),
            GitError::NotARepository { path } => write!(f, "{}", tr!("{path} n'est pas un dépôt git", path)),
            GitError::Cancelled => write!(f, "{}", tr!("opération annulée")),
            GitError::Parse(e) => write!(f, "{}", tr!("sortie de git inattendue : {e}", e)),
            GitError::Io(e) => write!(f, "{}", tr!("erreur d'entrée/sortie : {e}", e)),
            GitError::Refused(e) => write!(f, "{e}"),
            GitError::Internal(e) => write!(f, "{}", tr!("erreur interne de DragonTree : {e}", e)),
        }
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        GitError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_detects_dubious_ownership() {
        let stderr = "fatal: detected dubious ownership in repository at 'C:/x/y'\n\
                      'C:/x/y' is owned by:\n\tBUILTIN/Administrators (S-1-5-32-544)\n\
                      but the current user is:\n\tAzureAD/Someone\n\
                      To add an exception for this directory, call:\n\n\
                      \tgit config --global --add safe.directory C:/x/y\n";
        let err = GitError::from_failure("git status".into(), 128, stderr.into());
        assert_eq!(err, GitError::DubiousOwnership { path: "C:/x/y".into() });
    }

    #[test]
    fn dubious_ownership_path_with_apostrophe() {
        let stderr = "fatal: detected dubious ownership in repository at 'C:/DEV/Projets d'équipe/x'
                      'C:/DEV/Projets d'équipe/x' is owned by:
	BUILTIN/Administrators
";
        let err = GitError::from_failure("git status".into(), 128, stderr.into());
        assert_eq!(err, GitError::DubiousOwnership { path: "C:/DEV/Projets d'équipe/x".into() });
    }

    #[test]
    fn failure_keeps_stderr() {
        let err = GitError::from_failure("git rev-parse nope".into(), 128, "fatal: bad revision 'nope'\n".into());
        match &err {
            GitError::Failed { code, stderr, .. } => {
                assert_eq!(*code, 128);
                assert!(stderr.contains("bad revision"));
            }
            other => panic!("attendu Failed, obtenu {other:?}"),
        }
        assert_eq!(err.stderr_summary(), "bad revision 'nope'");
    }

    #[test]
    fn stderr_summary_skips_hints_and_blank_lines() {
        let err = GitError::from_failure(
            "git push".into(),
            1,
            "To origin\n ! [rejected]        main -> main (fetch first)\nerror: failed to push some refs to 'origin'\nhint: Updates were rejected because the remote contains work\nhint: that you do not have locally.\n".into(),
        );
        let s = err.stderr_summary();
        assert!(s.contains("[rejected]"));
        assert!(s.contains("failed to push some refs"));
        assert!(!s.contains("hint:"));
    }
}
