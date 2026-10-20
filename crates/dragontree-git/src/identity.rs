//! Identité git (`user.name`, `user.email`) : valeurs globales (`git config --global`, qui respecte
//! `GIT_CONFIG_GLOBAL`) et valeurs effectives d'un dépôt, utilisées pour reconnaître un commit sans identité.

use std::path::Path;

use crate::error::GitError;
use crate::repo::Repo;
use crate::runner::{Git, GitCommand};

const KEY_NAME: &str = "user.name";
const KEY_EMAIL: &str = "user.email";

/// Nom et e-mail ; une chaîne vide est une clé absente.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

impl Identity {
    /// Vrai si le nom ou l'e-mail manque : git refusera de créer un commit.
    pub fn is_incomplete(&self) -> bool {
        self.name.trim().is_empty() || self.email.trim().is_empty()
    }
}

fn read_key(cmd: GitCommand, key: &str) -> Result<String, GitError> {
    // Code 1 : clé absente.
    let out = cmd.args(["config", "--get", key]).allow_codes(&[1]).run()?;
    Ok(out.stdout_str().trim_end().to_string())
}

/// Identité de la configuration globale. `cwd` est un dossier quelconque (aucun dépôt n'est nécessaire).
pub fn global_identity(git: &Git, cwd: &Path) -> Result<Identity, GitError> {
    Ok(Identity { name: read_key_global(git, cwd, KEY_NAME)?, email: read_key_global(git, cwd, KEY_EMAIL)? })
}

fn read_key_global(git: &Git, cwd: &Path, key: &str) -> Result<String, GitError> {
    let out = git.cmd(cwd).args(["config", "--global", "--get", key]).allow_codes(&[1]).run()?;
    Ok(out.stdout_str().trim_end().to_string())
}

/// Écrit l'identité globale ; une valeur vide retire la clé (sans erreur si elle est absente).
pub fn set_global_identity(git: &Git, cwd: &Path, identity: &Identity) -> Result<(), GitError> {
    for (key, value) in [(KEY_NAME, &identity.name), (KEY_EMAIL, &identity.email)] {
        let value = value.trim();
        let cmd = git.cmd(cwd).args(["config", "--global"]);
        if value.is_empty() {
            // Code 5 : clé absente.
            cmd.args(["--unset-all", key]).allow_codes(&[5]).run()?;
        } else {
            cmd.args([key, value]).run()?;
        }
    }
    Ok(())
}

impl Repo {
    /// Git accepte l'identité de l'auteur et du committer. `git var` échoue exactement quand git refuserait de
    /// créer le commit (clés user.* ou author.*/committer.*, variables GIT_*, EMAIL, détection automatique) et sans
    /// dépendre de la langue de son message.
    pub fn identity_usable(&self) -> bool {
        ["GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"].into_iter().all(|var| self.read().args(["var", var]).run().is_ok())
    }

    /// Identité effective du dépôt (toutes portées : locale, globale, système).
    pub fn effective_identity(&self) -> Result<Identity, GitError> {
        Ok(Identity { name: read_key(self.read(), KEY_NAME)?, email: read_key(self.read(), KEY_EMAIL)? })
    }
}
