//! Configuration du dépôt : clés `--local` (identité, modèle de message), valeur globale en indication.

use std::path::Path;

use crate::error::GitError;
use crate::repo::Repo;
use crate::ssh::ConfigScope;

impl Repo {
    /// Valeur d'une clé dans une seule portée (la globale respecte `GIT_CONFIG_GLOBAL`) : `None` si la clé est
    /// absente ou vide (code de sortie 1 de git) ; tout autre échec (fichier de configuration illisible ou
    /// invalide…) est une erreur, pour ne jamais prendre une lecture manquée pour une clé absente.
    /// [`Repo::config`] lit la valeur effective, toutes portées confondues.
    pub fn config_in(&self, scope: ConfigScope, key: &str) -> Result<Option<String>, GitError> {
        let out = self.read().args(["config", scope.flag(), "--get", key]).allow_codes(&[1]).run()?;
        Ok(Some(out.stdout_str().trim_end().to_string()).filter(|v| !v.is_empty()))
    }

    /// Écrit une clé de la configuration du dépôt (`git config --local`).
    pub fn set_config_local(&self, key: &str, value: &str) -> Result<(), GitError> {
        self.cmd().args(["config", ConfigScope::Local.flag(), key, value]).run().map(|_| ())
    }

    /// Retire une clé de la configuration du dépôt ; sans effet si elle n'y est pas (le dépôt hérite alors de la
    /// valeur globale).
    pub fn unset_config_local(&self, key: &str) -> Result<(), GitError> {
        self.cmd().args(["config", ConfigScope::Local.flag(), "--unset-all", key]).allow_codes(&[5]).run().map(|_| ())
    }

    /// Contenu du modèle de message de commit (`commit.template`), s'il est configuré, lisible et non vide. Un
    /// chemin relatif se lit depuis la racine du dépôt, `~` est développé par git.
    pub fn commit_template(&self) -> Option<String> {
        let out =
            self.read().args(["config", "--type=path", "--get", "commit.template"]).allow_codes(&[1]).run().ok()?;
        let configured = out.stdout_str().trim().to_string();
        if configured.is_empty() {
            return None;
        }
        let path = self.workdir.join(Path::new(&configured));
        let bytes = std::fs::read(path).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned()).filter(|t| !t.trim().is_empty())
    }
}
