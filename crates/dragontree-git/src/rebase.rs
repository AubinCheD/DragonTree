//! Rebase interactif piloté par DragonTree (liste préparée par l'interface).

use std::path::Path;

use crate::error::GitError;
use crate::log::{Commit, LogOptions, LogOrder, LogScope};
use crate::oid::Oid;
use crate::repo::{OpState, Repo};
use crate::runner::CancelToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    /// Nouveau message complet.
    Reword(String),
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseItem {
    pub id: Oid,
    pub action: RebaseAction,
    pub summary: String,
}

/// `s` entre apostrophes pour `sh` : rien n'y est interprété (une apostrophe devient `'\''`).
fn sh_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Contenu du fichier « todo » de git. Un `Reword` devient `pick` suivi d'un
/// `exec git commit --amend` lisant le message dans `msg_dir` (aucun éditeur ouvert).
pub fn build_todo(items: &[RebaseItem], msg_dir: &Path) -> std::io::Result<String> {
    std::fs::create_dir_all(msg_dir)?;
    let mut out = String::new();
    for item in items {
        let verb = match item.action {
            RebaseAction::Pick | RebaseAction::Reword(_) => "pick",
            RebaseAction::Edit => "edit",
            RebaseAction::Squash => "squash",
            RebaseAction::Fixup => "fixup",
            RebaseAction::Drop => "drop",
        };
        out.push_str(&format!("{verb} {} {}\n", item.id, item.summary.replace(['\n', '\r'], " ")));
        if let RebaseAction::Reword(message) = &item.action {
            let file = msg_dir.join(format!("reword-{}.txt", item.id));
            std::fs::write(&file, message)?;
            // Entre apostrophes : git passe la ligne `exec` à `sh`, et le chemin contient le nom du dossier du worktree
            // (souvent repris d'un nom de branche), où `$(…)` ou `` ` `` seraient exécutés entre guillemets doubles.
            let path = sh_quoted(&file.to_string_lossy().replace('\\', "/"));
            out.push_str(&format!("exec git commit --amend --only --no-verify -F {path}\n"));
        }
    }
    Ok(out)
}

impl Repo {
    /// Commits rejouables par un rebase interactif sur `base`, du plus ancien au plus récent
    /// (hors commits de fusion, comme git).
    pub fn rebase_candidates(&self, base: &str) -> Result<Vec<Commit>, GitError> {
        let opts = LogOptions {
            scope: LogScope::Revs(vec![format!("{base}..HEAD")]),
            order: LogOrder::Topo,
            ..LogOptions::default()
        };
        let mut commits = self.log(&opts, &CancelToken::new())?;
        commits.retain(|c| c.parents.len() <= 1);
        commits.reverse();
        Ok(commits)
    }

    /// Lance `git rebase -i base` avec la liste `items` (du plus ancien au plus récent).
    /// Les messages des « reword » sont rangés dans le dossier git pour survivre à un arrêt.
    pub fn interactive_rebase(&self, base: &str, items: &[RebaseItem]) -> Result<(), GitError> {
        let dir = self.git_dir.join("dragontree-rebase");
        let todo = build_todo(items, &dir)?;
        let todo_file = dir.join("todo.txt");
        std::fs::write(&todo_file, todo)?;
        // git lance l'éditeur de séquence via sh : `cp <notre todo> <fichier de git>`.
        let src = sh_quoted(&todo_file.to_string_lossy().replace('\\', "/"));
        let result = self
            .cmd()
            .env("GIT_SEQUENCE_EDITOR", format!("cp {src}"))
            .env("GIT_EDITOR", "true")
            .args(["rebase", "-i", base])
            .run()
            .map(|_| ());
        self.cleanup_rebase_dir();
        result
    }

    /// Supprime le dossier `dragontree-rebase` (todo et messages des « reword ») quand plus aucune opération n'est en
    /// cours : à appeler après un rebase, qu'il se termine, soit arrêté puis continué, passé ou abandonné.
    pub(crate) fn cleanup_rebase_dir(&self) {
        if self.operation_state() == OpState::None {
            let _ = std::fs::remove_dir_all(self.git_dir.join("dragontree-rebase"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u8, action: RebaseAction) -> RebaseItem {
        RebaseItem { id: Oid([n; 20]), action, summary: format!("commit {n}") }
    }

    #[test]
    fn todo_reword_uses_exec() {
        let dir = tempfile::tempdir().unwrap();
        let todo = build_todo(
            &[item(1, RebaseAction::Pick), item(2, RebaseAction::Reword("Nouveau\n\nCorps".into()))],
            dir.path(),
        )
        .unwrap();
        let lines: Vec<&str> = todo.lines().collect();
        assert!(lines[0].starts_with(&format!("pick {}", Oid([1; 20]))));
        assert!(lines[1].starts_with(&format!("pick {}", Oid([2; 20]))));
        assert!(lines[2].starts_with("exec git commit --amend --only --no-verify -F "), "{}", lines[2]);
        let path = lines[2].split("-F ").nth(1).unwrap().trim_matches('\'');
        assert_eq!(std::fs::read_to_string(path).unwrap(), "Nouveau\n\nCorps");
    }

    #[test]
    fn todo_actions() {
        let dir = tempfile::tempdir().unwrap();
        let todo = build_todo(
            &[
                item(1, RebaseAction::Edit),
                item(2, RebaseAction::Squash),
                item(3, RebaseAction::Fixup),
                item(4, RebaseAction::Drop),
            ],
            dir.path(),
        )
        .unwrap();
        let verbs: Vec<&str> = todo.lines().map(|l| l.split(' ').next().unwrap()).collect();
        assert_eq!(verbs, vec!["edit", "squash", "fixup", "drop"]);
    }
}
