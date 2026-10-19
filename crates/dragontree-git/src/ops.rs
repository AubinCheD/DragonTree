//! Opérations d'écriture sur un dépôt.

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;
use crate::patch::PatchMode;
use crate::repo::{OpState, Repo};
use crate::runner::GitCommand;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// Avance rapide si possible, sinon commit de fusion.
    Default,
    NoFf,
    FfOnly,
    Squash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

fn nul_joined(paths: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in paths {
        out.extend_from_slice(p.as_bytes());
        out.push(0);
    }
    out
}

impl Repo {
    /// Commande d'écriture : jamais d'éditeur interactif.
    pub(crate) fn write(&self) -> GitCommand {
        self.cmd().env("GIT_EDITOR", "true")
    }

    /// Écriture sur des chemins fournis par l'utilisateur, pris littéralement. Réservé à ces
    /// commandes : `GIT_LITERAL_PATHSPECS` casse `git stash -u` (fichiers non suivis laissés en place).
    pub(crate) fn write_paths(&self) -> GitCommand {
        self.write().env("GIT_LITERAL_PATHSPECS", "1")
    }

    fn run_write<I, S>(&self, args: I) -> Result<(), GitError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.write().args(args).run().map(|_| ())
    }

    /// Commande avec une liste de chemins passée sur stdin (pas de limite de longueur Windows).
    fn run_with_paths(&self, args: &[&str], paths: &[String]) -> Result<(), GitError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.write_paths()
            .args(args)
            .args(["--pathspec-from-file=-", "--pathspec-file-nul"])
            .stdin(nul_joined(paths))
            .run()
            .map(|_| ())
    }

    /// Indexe les chemins (ajouts, modifications et suppressions).
    pub fn stage(&self, paths: &[String]) -> Result<(), GitError> {
        self.run_with_paths(&["add", "-A"], paths)
    }

    /// Désindexe les chemins (fonctionne aussi avant le premier commit).
    pub fn unstage(&self, paths: &[String]) -> Result<(), GitError> {
        if self.head_exists() {
            self.run_with_paths(&["restore", "--staged"], paths)
        } else {
            self.run_with_paths(&["rm", "--cached", "-r", "-q", "--ignore-unmatch"], paths)
        }
    }

    /// Annule les modifications non indexées des fichiers suivis et supprime les non suivis.
    /// Refuse (sans rien toucher) si un dossier non suivi est un dépôt git imbriqué.
    pub fn discard(&self, tracked: &[String], untracked: &[String]) -> Result<(), GitError> {
        if let Some(nested) = untracked.iter().find(|rel| self.workdir.join(rel.as_str()).join(".git").exists()) {
            return Err(GitError::Refused(tr!(
                "« {nested} » est un dépôt git imbriqué : DragonTree ne le supprime pas (son historique serait perdu). Supprimez-le à la main si c'est voulu.",
                nested
            )));
        }
        self.run_with_paths(&["restore", "--worktree"], tracked)?;
        for rel in untracked {
            let full = self.workdir.join(rel);
            if full.is_dir() {
                std::fs::remove_dir_all(&full)?;
            } else if full.exists() {
                std::fs::remove_file(&full)?;
            }
            // Comme `git clean -d` : on retire les dossiers parents devenus vides.
            let mut parent = full.parent();
            while let Some(dir) = parent {
                if dir == self.workdir || !dir.starts_with(&self.workdir) {
                    break;
                }
                if std::fs::remove_dir(dir).is_err() {
                    break;
                }
                parent = dir.parent();
            }
        }
        Ok(())
    }

    pub fn apply_patch(&self, patch: &[u8], mode: PatchMode) -> Result<(), GitError> {
        let mut cmd = self.write().args(["apply", "--recount", "--whitespace=nowarn"]);
        cmd = match mode {
            PatchMode::Stage => cmd.arg("--cached"),
            PatchMode::Unstage => cmd.args(["--cached", "--reverse"]),
            PatchMode::Discard => cmd.arg("--reverse"),
        };
        cmd.arg("-").stdin(patch.to_vec()).run().map(|_| ())
    }

    /// `git add -N` : rend un fichier non suivi visible dans le diff pour l'indexer ligne à ligne.
    pub fn intent_to_add(&self, path: &str) -> Result<(), GitError> {
        self.write_paths().args(["add", "-N", "--", path]).run().map(|_| ())
    }

    pub fn commit(&self, message: &str, amend: bool) -> Result<(), GitError> {
        let mut cmd = self.write().arg("commit");
        if amend {
            cmd = cmd.arg("--amend");
        }
        if message.trim().is_empty() && amend {
            cmd = cmd.arg("--no-edit");
        } else {
            cmd = cmd.args(["-F", "-"]).stdin(message.as_bytes().to_vec());
        }
        cmd.run().map(|_| ())
    }

    pub fn checkout(&self, target: &str) -> Result<(), GitError> {
        self.run_write(["checkout", target, "--"])
    }

    pub fn checkout_new_tracking(&self, remote_branch: &str, local: &str) -> Result<(), GitError> {
        self.run_write(["checkout", "-b", local, "--track", remote_branch])
    }

    pub fn create_branch(&self, name: &str, start: &str, checkout: bool) -> Result<(), GitError> {
        if checkout { self.run_write(["checkout", "-b", name, start]) } else { self.run_write(["branch", name, start]) }
    }

    pub fn rename_branch(&self, old: &str, new: &str) -> Result<(), GitError> {
        self.run_write(["branch", "-m", old, new])
    }

    pub fn delete_branch(&self, name: &str, force: bool) -> Result<(), GitError> {
        self.run_write(["branch", if force { "-D" } else { "-d" }, name])
    }

    pub fn merge(&self, rev: &str, mode: MergeMode) -> Result<(), GitError> {
        match mode {
            MergeMode::Default => self.run_write(["merge", "--no-edit", rev]),
            MergeMode::NoFf => self.run_write(["merge", "--no-edit", "--no-ff", rev]),
            MergeMode::FfOnly => self.run_write(["merge", "--ff-only", rev]),
            MergeMode::Squash => self.run_write(["merge", "--squash", rev]),
        }
    }

    pub fn rebase(&self, onto: &str) -> Result<(), GitError> {
        self.run_write(["rebase", onto])
    }

    pub fn cherry_pick(&self, ids: &[Oid]) -> Result<(), GitError> {
        self.write().arg("cherry-pick").args(ids.iter().map(Oid::to_hex)).run().map(|_| ())
    }

    pub fn revert(&self, id: Oid) -> Result<(), GitError> {
        self.run_write(["revert", "--no-edit", &id.to_hex()])
    }

    pub fn reset(&self, rev: &str, mode: ResetMode) -> Result<(), GitError> {
        let flag = match mode {
            ResetMode::Soft => "--soft",
            ResetMode::Mixed => "--mixed",
            ResetMode::Hard => "--hard",
        };
        self.run_write(["reset", "-q", flag, rev])
    }

    pub fn continue_op(&self, state: OpState) -> Result<(), GitError> {
        match state {
            OpState::None => Ok(()),
            OpState::Merge => self.run_write(["commit", "--no-edit"]),
            OpState::Rebase { .. } => self.run_write(["rebase", "--continue"]),
            OpState::CherryPick => self.run_write(["cherry-pick", "--continue"]),
            OpState::Revert => self.run_write(["revert", "--continue"]),
        }
    }

    pub fn abort_op(&self, state: OpState) -> Result<(), GitError> {
        match state {
            OpState::None => Ok(()),
            OpState::Merge => self.run_write(["merge", "--abort"]),
            OpState::Rebase { .. } => self.run_write(["rebase", "--abort"]),
            OpState::CherryPick => self.run_write(["cherry-pick", "--abort"]),
            OpState::Revert => self.run_write(["revert", "--abort"]),
        }
    }

    /// Passe le commit courant d'un rebase arrêté.
    pub fn skip_rebase_commit(&self) -> Result<(), GitError> {
        self.run_write(["rebase", "--skip"])
    }

    pub fn create_tag(&self, name: &str, rev: &str, message: Option<&str>) -> Result<(), GitError> {
        match message {
            Some(msg) => {
                self.write().args(["tag", "-a", name, rev, "-F", "-"]).stdin(msg.as_bytes().to_vec()).run().map(|_| ())
            }
            None => self.run_write(["tag", name, rev]),
        }
    }

    pub fn delete_tag(&self, name: &str) -> Result<(), GitError> {
        self.run_write(["tag", "-d", name])
    }

    pub fn stash_save(&self, message: Option<&str>, include_untracked: bool, keep_index: bool) -> Result<(), GitError> {
        let mut args = vec!["stash", "push"];
        if include_untracked {
            args.push("--include-untracked");
        }
        if keep_index {
            args.push("--keep-index");
        }
        if let Some(m) = message.filter(|m| !m.trim().is_empty()) {
            args.extend(["-m", m]);
        }
        self.run_write(args)
    }

    /// Index actuel d'un stash identifié par son commit (la liste a pu changer depuis l'affichage).
    fn stash_index_of(&self, oid: Oid) -> Result<usize, GitError> {
        self.stashes()?.into_iter().find(|s| s.oid == oid).map(|s| s.index).ok_or_else(|| {
            GitError::Refused(tr!("Ce stash n'existe plus (la liste a changé) : actualisez puis réessayez.").into())
        })
    }

    pub fn stash_apply_oid(&self, oid: Oid) -> Result<(), GitError> {
        self.stash_apply(self.stash_index_of(oid)?)
    }

    pub fn stash_pop_oid(&self, oid: Oid) -> Result<(), GitError> {
        self.stash_pop(self.stash_index_of(oid)?)
    }

    pub fn stash_drop_oid(&self, oid: Oid) -> Result<(), GitError> {
        self.stash_drop(self.stash_index_of(oid)?)
    }

    /// Ajoute un motif à la fin du `.gitignore` (octets existants préservés, encodage compris).
    pub fn add_to_gitignore(&self, pattern: &str) -> Result<(), GitError> {
        use std::io::Write;
        let path = self.workdir.join(".gitignore");
        let needs_newline = match std::fs::read(&path) {
            Ok(bytes) => !bytes.is_empty() && !bytes.ends_with(b"\n"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        };
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        if needs_newline {
            file.write_all(b"\n")?;
        }
        file.write_all(pattern.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    pub fn stash_apply(&self, index: usize) -> Result<(), GitError> {
        self.run_write(["stash", "apply", &format!("stash@{{{index}}}")])
    }

    pub fn stash_pop(&self, index: usize) -> Result<(), GitError> {
        self.run_write(["stash", "pop", &format!("stash@{{{index}}}")])
    }

    pub fn stash_drop(&self, index: usize) -> Result<(), GitError> {
        self.run_write(["stash", "drop", &format!("stash@{{{index}}}")])
    }

    pub fn add_remote(&self, name: &str, url: &str) -> Result<(), GitError> {
        self.run_write(["remote", "add", name, url])
    }

    pub fn remove_remote(&self, name: &str) -> Result<(), GitError> {
        self.run_write(["remote", "remove", name])
    }

    pub fn set_upstream(&self, local: &str, remote_branch: &str) -> Result<(), GitError> {
        self.run_write(["branch", &format!("--set-upstream-to={remote_branch}"), local])
    }

    /// Vérifie un nom de branche (`git check-ref-format --branch`).
    pub fn is_valid_branch_name(&self, name: &str) -> bool {
        !name.is_empty()
            && !name.starts_with('-')
            && self.read().args(["check-ref-format", "--branch", name]).run().is_ok()
    }
}
