//! Sous-modules (`git submodule`) : lecture de `.gitmodules`, ajout, déplacement, changement d'URL, suppression.

use std::path::{Component, Path, PathBuf};

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::net::{NetOptions, with_net};
use crate::repo::{Repo, native_path};

/// Entrée de `.gitmodules`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GitmodulesEntry {
    pub name: String,
    pub path: String,
    pub url: String,
}

/// Version majeure et mineure d'une sortie de `git --version` (`git version 2.44.0.windows.1` donne `(2, 44)`).
pub fn parse_git_version(out: &str) -> Option<(u32, u32)> {
    let version = out.trim().strip_prefix("git version ")?;
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Lit la sortie de `git config -z --get-regexp` sur `.gitmodules` : `submodule.<nom>.<clé>\nvaleur\0` ; le nom
/// peut contenir des points.
pub(crate) fn parse_gitmodules(out: &str) -> Vec<GitmodulesEntry> {
    let mut entries: Vec<GitmodulesEntry> = Vec::new();
    for record in out.split('\0').filter(|r| !r.is_empty()) {
        let (key, value) = record.split_once('\n').unwrap_or((record, ""));
        let Some((name, field)) = key.strip_prefix("submodule.").and_then(|k| k.rsplit_once('.')) else { continue };
        let idx = match entries.iter().position(|e| e.name == name) {
            Some(i) => i,
            None => {
                entries.push(GitmodulesEntry { name: name.to_string(), ..Default::default() });
                entries.len() - 1
            }
        };
        match field {
            "path" => entries[idx].path = value.to_string(),
            "url" => entries[idx].url = value.to_string(),
            _ => {}
        }
    }
    entries
}

/// Chemin relatif au dépôt, sans `..` ni racine : refuse ce qui sortirait de la copie de travail.
pub(crate) fn check_relative(path: &str) -> Result<(), GitError> {
    let p = Path::new(path);
    let bad = path.trim().is_empty()
        || p.is_absolute()
        || path.starts_with(['/', '\\'])
        || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir));
    if bad {
        return Err(GitError::Refused(tr!("Le chemin doit être relatif au dépôt, sans « .. ».").into()));
    }
    Ok(())
}

impl Repo {
    /// Entrées de `.gitmodules` (vide sans fichier).
    pub(crate) fn gitmodules_entries(&self) -> Result<Vec<GitmodulesEntry>, GitError> {
        if !self.workdir.join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        let out = self
            .read()
            .args(["config", "-z", "-f", ".gitmodules", "--get-regexp", r"^submodule\..*\.(path|url)$"])
            .allow_codes(&[1])
            .run()?;
        Ok(parse_gitmodules(&out.stdout_str()))
    }

    /// Nom (clé de `.gitmodules`) du sous-module installé dans `path`.
    fn submodule_name(&self, path: &str) -> Result<String, GitError> {
        let path = path.trim_end_matches('/');
        self.gitmodules_entries()?
            .into_iter()
            .find(|e| e.path == path)
            .map(|e| e.name)
            .ok_or_else(|| GitError::Refused(tr!("« {path} » n'est pas un sous-module de ce dépôt.", path)))
    }

    /// `git submodule add [-b branche] <url> <chemin>` : clone le sous-module, l'enregistre dans `.gitmodules` et
    /// l'index (changements prêts à être commités). Un dépôt local est refusé par git sans
    /// `protocol.file.allow` : son message est renvoyé tel quel.
    pub fn add_submodule(
        &self,
        url: &str,
        path: &str,
        branch: Option<&str>,
        n: &NetOptions,
        on: &mut dyn FnMut(crate::Progress),
    ) -> Result<(), GitError> {
        check_relative(path)?;
        let mut cmd = with_net(self.write_paths(), n).args(["submodule", "add"]);
        if let Some(branch) = branch.map(str::trim).filter(|b| !b.is_empty()) {
            cmd = cmd.args(["-b", branch]);
        }
        crate::net::run_with_progress(cmd.args(["--", url.trim(), path.trim()]), on)
    }

    /// Déplace le sous-module `old` vers `new` (`git mv` : dossier, `.gitmodules` et index), en créant au besoin
    /// le dossier parent de la destination.
    pub fn move_submodule(&self, old: &str, new: &str) -> Result<(), GitError> {
        check_relative(new)?;
        self.submodule_name(old)?;
        self.ensure_gitmodules_staged()?;
        // Dossiers créés pour la destination, du plus haut au plus bas : retirés si `git mv` échoue.
        let mut created: Vec<PathBuf> = Vec::new();
        if let Some(parent) = Path::new(new.trim()).parent().filter(|p| !p.as_os_str().is_empty()) {
            created = parent
                .ancestors()
                .filter(|a| !a.as_os_str().is_empty() && !self.workdir.join(a).exists())
                .map(|a| self.workdir.join(a))
                .collect();
            std::fs::create_dir_all(self.workdir.join(parent)).map_err(|e| GitError::Io(e.to_string()))?;
        }
        let result = self.write_paths().args(["mv", "--", old.trim_end_matches('/'), new.trim()]).run().map(|_| ());
        if result.is_err() {
            // `ancestors` va du plus profond au plus haut : on retire dans cet ordre.
            for dir in &created {
                let _ = std::fs::remove_dir(dir);
            }
        }
        result
    }

    /// Refuse d'agir quand `.gitmodules` a des modifications non indexées : `git mv` et `git rm` s'arrêtent alors
    /// (« please stage your changes to .gitmodules »), parfois après avoir déjà modifié le sous-module.
    fn ensure_gitmodules_staged(&self) -> Result<(), GitError> {
        let out = self.read().args(["diff", "--quiet", "--", ".gitmodules"]).allow_codes(&[1]).run()?;
        if out.code == 1 {
            return Err(GitError::Refused(
                tr!("Le fichier .gitmodules a des modifications non indexées : indexez-les ou annulez-les d'abord.")
                    .into(),
            ));
        }
        Ok(())
    }

    /// Indexe `.gitmodules` après un changement d'URL, comme le font `add`, `mv` et `rm`.
    fn stage_gitmodules(&self) -> Result<(), GitError> {
        self.write_paths().args(["add", "--", ".gitmodules"]).run().map(|_| ())
    }

    /// Change l'URL du sous-module `path` dans `.gitmodules` et dans la configuration du dépôt :
    /// `git submodule set-url` (git ≥ 2.25), sinon [`Repo::set_submodule_url_via_config`].
    pub fn set_submodule_url(&self, path: &str, url: &str) -> Result<(), GitError> {
        let recent = self.git.version().ok().and_then(|v| parse_git_version(&v)).is_some_and(|v| v >= (2, 25));
        if !recent {
            return self.set_submodule_url_via_config(path, url);
        }
        self.submodule_name(path)?;
        self.write_paths().args(["submodule", "set-url", "--", path.trim_end_matches('/'), url.trim()]).run()?;
        self.stage_gitmodules()
    }

    /// Repli pour git < 2.25 : `git config -f .gitmodules` puis `git submodule sync`.
    pub fn set_submodule_url_via_config(&self, path: &str, url: &str) -> Result<(), GitError> {
        let name = self.submodule_name(path)?;
        self.write().args(["config", "-f", ".gitmodules", &format!("submodule.{name}.url"), url.trim()]).run()?;
        self.write_paths().args(["submodule", "sync", "--", path.trim_end_matches('/')]).run()?;
        self.stage_gitmodules()
    }

    /// Supprime complètement le sous-module `path` : `submodule deinit -f` (même avec des modifications
    /// locales : l'appelant a confirmé), `git rm -f` (dossier, entrée de `.gitmodules` et index), puis le dossier
    /// `.git/modules/<nom>` qui garde le clone.
    pub fn remove_submodule(&self, path: &str) -> Result<(), GitError> {
        check_relative(path)?;
        let name = self.submodule_name(path)?;
        let path = path.trim_end_matches('/');
        self.ensure_gitmodules_staged()?;
        self.write_paths().args(["submodule", "deinit", "-f", "--", path]).run()?;
        self.write_paths().args(["rm", "-f", "--", path]).run()?;
        // Le nom vient de `.gitmodules` : il ne doit pas pouvoir sortir de `.git/modules`.
        if Path::new(&name).components().all(|c| matches!(c, Component::Normal(_))) {
            let out = self.read().args(["rev-parse", "--git-path", &format!("modules/{name}")]).run()?;
            let dir = native_path(out.stdout_str().trim());
            let dir: PathBuf = if dir.is_absolute() { dir } else { self.workdir.join(dir) };
            if dir.is_dir() {
                std::fs::remove_dir_all(&dir).map_err(|e| GitError::Io(e.to_string()))?;
                // Un nom à barres (`vendor/lib`) laisse des dossiers parents vides dans `.git/modules`.
                let root = self.read().args(["rev-parse", "--git-path", "modules"]).run()?;
                let root = native_path(root.stdout_str().trim());
                let root: PathBuf = if root.is_absolute() { root } else { self.workdir.join(root) };
                let mut parent = dir.parent();
                while let Some(p) = parent.filter(|p| *p != root && p.starts_with(&root)) {
                    if std::fs::remove_dir(p).is_err() {
                        break;
                    }
                    parent = p.parent();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotted_name_is_read_whole() {
        let out = "submodule.vendor.lib.v2.path\nthird/lib\0submodule.vendor.lib.v2.url\nhttps://x.test/l.git\0";
        let entries = parse_gitmodules(out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "vendor.lib.v2");
        assert_eq!((entries[0].path.as_str(), entries[0].url.as_str()), ("third/lib", "https://x.test/l.git"));
    }
}
