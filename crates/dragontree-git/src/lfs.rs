//! Git LFS (`git lfs …`) : détection, installation dans le dépôt, motifs suivis (`.gitattributes`), fetch, pull,
//! prune, verrous et fichiers que LFS gère. Rien n'est écrit hors du dépôt : l'installation est toujours `--local`.

use std::collections::HashSet;
use std::path::Path;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::net::{NetOptions, run_with_progress, with_net};
use crate::progress::Progress;
use crate::repo::Repo;
use crate::runner::{Git, GitCommand};
use crate::submodule::check_relative;

/// Verrou LFS tel que le serveur le décrit (`git lfs locks --json`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsLock {
    pub id: String,
    /// Chemin du fichier verrouillé, relatif à la racine du dépôt.
    pub path: String,
    /// Nom du propriétaire du verrou (vide si le serveur ne l'indique pas).
    pub owner: String,
    /// Date de pose, telle que le serveur l'envoie (ISO 8601).
    pub locked_at: String,
}

impl Git {
    /// Sortie de `git lfs version` (par exemple `git-lfs/3.4.0 (GitHub; windows amd64; go 1.21.1)`), ou une erreur si
    /// l'extension est absente.
    pub fn lfs_version(&self) -> Result<String, GitError> {
        let out = self.cmd(&std::env::temp_dir()).args(["lfs", "version"]).run()?;
        Ok(out.stdout_str().trim().to_string())
    }
}

/// Git LFS est utilisable avec ce git : à appeler hors du thread de l'interface (lance un processus).
pub fn lfs_available(git: &Git) -> bool {
    git.lfs_version().is_ok()
}

/// Motifs suivis par LFS dans le texte d'un `.gitattributes` : les lignes qui portent `filter=lfs`, dans l'ordre,
/// sans doublon. Un motif entre guillemets peut contenir des espaces.
pub fn parse_lfs_patterns(attributes: &str) -> Vec<String> {
    let mut patterns: Vec<String> = Vec::new();
    for line in attributes.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (pattern, rest) = match line.strip_prefix('"').and_then(|l| l.split_once('"')) {
            Some((quoted, rest)) => (quoted, rest),
            None => line.split_once(char::is_whitespace).unwrap_or((line, "")),
        };
        if rest.split_whitespace().any(|attr| attr == "filter=lfs") && !patterns.iter().any(|p| p == pattern) {
            patterns.push(pattern.to_string());
        }
    }
    patterns
}

/// Verrous dans la sortie JSON de `git lfs locks --json` : un tableau, ou (avec `--verify`) un objet qui sépare
/// `ours` et `theirs` ; les deux listes sont réunies. Une sortie vide est une liste vide.
pub fn parse_locks(json: &str) -> Result<Vec<LfsLock>, GitError> {
    use serde_json::Value;
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(json).map_err(|e| GitError::Parse(e.to_string()))?;
    let lists: Vec<&Value> = match &value {
        Value::Array(_) => vec![&value],
        Value::Object(map) => ["ours", "theirs"].iter().filter_map(|k| map.get(*k)).collect(),
        Value::Null => Vec::new(),
        _ => return Err(GitError::Parse(tr!("liste de verrous attendue").into())),
    };
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut locks = Vec::new();
    for item in lists.into_iter().filter_map(Value::as_array).flatten() {
        let path = text(item, "path");
        if path.is_empty() {
            continue;
        }
        locks.push(LfsLock {
            id: text(item, "id"),
            path,
            owner: item.get("owner").map(|o| text(o, "name")).unwrap_or_default(),
            locked_at: text(item, "locked_at"),
        });
    }
    Ok(locks)
}

/// Motif ou chemin passé à `git lfs` : ni vide ni pris pour une option.
fn check_argument(value: &str) -> Result<&str, GitError> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('-') || value.contains(['\n', '\r']) {
        return Err(GitError::Refused(tr!("Motif ou chemin invalide : « {value} »", value)));
    }
    Ok(value)
}

impl Repo {
    fn lfs(&self) -> GitCommand {
        self.write().arg("lfs")
    }

    /// `git lfs install --local` : filtres et crochets de git LFS dans ce dépôt seulement (jamais la configuration
    /// globale ou système).
    pub fn lfs_install_local(&self) -> Result<(), GitError> {
        self.lfs().args(["install", "--local"]).run().map(|_| ())
    }

    /// `git lfs track "<motif>"` : ajoute le motif au `.gitattributes` de la racine.
    pub fn lfs_track(&self, pattern: &str) -> Result<(), GitError> {
        self.lfs().arg("track").arg(check_argument(pattern)?).run().map(|_| ())
    }

    /// `git lfs untrack "<motif>"` : retire le motif du `.gitattributes`.
    pub fn lfs_untrack(&self, pattern: &str) -> Result<(), GitError> {
        self.lfs().arg("untrack").arg(check_argument(pattern)?).run().map(|_| ())
    }

    /// Motifs suivis, lus dans le `.gitattributes` de la racine (une lecture de fichier, pas de commande).
    pub fn lfs_patterns(&self) -> Result<Vec<String>, GitError> {
        match std::fs::read(self.workdir.join(".gitattributes")) {
            Ok(bytes) => Ok(parse_lfs_patterns(&String::from_utf8_lossy(&bytes))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Parmi `paths` (relatifs à la racine), ceux que LFS gère d'après les attributs (`filter=lfs`). Sans motif LFS
    /// dans le `.gitattributes` de la racine, rien n'est lancé. Une erreur de git donne un ensemble vide : c'est un
    /// simple repère visuel.
    pub fn lfs_files(&self, paths: &[String]) -> HashSet<String> {
        let has_lfs = std::fs::read_to_string(self.workdir.join(Path::new(".gitattributes")))
            .is_ok_and(|text| text.contains("filter=lfs"));
        if paths.is_empty() || !has_lfs {
            return HashSet::new();
        }
        let mut input = Vec::new();
        for path in paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        let Ok(out) = self.read().args(["check-attr", "-z", "--stdin", "filter"]).stdin(input).run() else {
            return HashSet::new();
        };
        // Sortie : chemin, attribut, valeur, chacun terminé par NUL.
        let text = out.stdout_str();
        let fields: Vec<&str> = text.split('\0').collect();
        fields
            .as_chunks::<3>()
            .0
            .iter()
            .filter(|triple| triple[1] == "filter" && triple[2] == "lfs")
            .map(|triple| triple[0].to_string())
            .collect()
    }

    /// Commande réseau de git LFS. git-lfs n'affiche sa progression que si la sortie est un terminal, sauf si on la
    /// force : sans cela, la barre de progression de l'onglet resterait vide.
    fn lfs_net(&self, n: &NetOptions) -> GitCommand {
        with_net(self.cmd(), n).env("GIT_LFS_FORCE_PROGRESS", "1").arg("lfs")
    }

    /// `git lfs fetch` : télécharge les objets des références récentes, avec progression.
    pub fn lfs_fetch(&self, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
        run_with_progress(self.lfs_net(n).arg("fetch"), on)
    }

    /// `git lfs pull` : télécharge puis remplace les pointeurs de la copie de travail par les fichiers.
    pub fn lfs_pull(&self, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
        run_with_progress(self.lfs_net(n).arg("pull"), on)
    }

    /// `git lfs prune` : supprime les objets locaux anciens et déjà poussés.
    pub fn lfs_prune(&self, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
        run_with_progress(self.lfs_net(n).arg("prune"), on)
    }

    /// `git lfs locks --json` : les verrous posés sur le serveur (demande un serveur LFS).
    pub fn lfs_locks(&self, n: &NetOptions) -> Result<Vec<LfsLock>, GitError> {
        let out = self.lfs_net(n).args(["locks", "--json"]).run()?;
        parse_locks(&out.stdout_str())
    }

    /// `git lfs lock <chemin>` : verrouille un fichier (chemin relatif au dépôt).
    pub fn lfs_lock(&self, path: &str, n: &NetOptions) -> Result<(), GitError> {
        check_relative(path)?;
        self.lfs_net(n).arg("lock").arg(check_argument(path)?).run().map(|_| ())
    }

    /// `git lfs unlock <chemin>` ; `force` (`--force`) lève aussi le verrou d'un autre utilisateur.
    pub fn lfs_unlock(&self, path: &str, force: bool, n: &NetOptions) -> Result<(), GitError> {
        check_relative(path)?;
        let mut cmd = self.lfs_net(n).arg("unlock");
        if force {
            cmd = cmd.arg("--force");
        }
        cmd.arg(check_argument(path)?).run().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_are_the_lines_with_the_lfs_filter() {
        let text = "# images\n*.psd filter=lfs diff=lfs merge=lfs -text\n\n*.txt text eol=lf\n\
                    \"big file.bin\" filter=lfs diff=lfs merge=lfs -text\nassets/** filter=lfs -text\n\
                    *.psd filter=lfs diff=lfs merge=lfs -text\n*.md filter=other\n";
        assert_eq!(parse_lfs_patterns(text), ["*.psd", "big file.bin", "assets/**"]);
        assert!(parse_lfs_patterns("").is_empty());
    }

    #[test]
    fn real_git_lfs_progress_lines_are_understood() {
        use crate::progress::parse_progress;
        let p = parse_progress("Downloading LFS objects:  50% (1/2), 1.2 MB | 3.4 MB/s").unwrap();
        assert_eq!(
            (p.phase.as_str(), p.percent, p.current, p.total),
            ("Downloading LFS objects", Some(50), 1, Some(2))
        );
        let p = parse_progress("Uploading LFS objects: 100% (3/3), 12 MB | 0 B/s, done.").unwrap();
        assert_eq!((p.percent, p.current, p.total), (Some(100), 3, Some(3)));
        let p = parse_progress("Checking out LFS objects:  10% (1/10), 1.0 kB | 0 B/s").unwrap();
        assert_eq!(p.phase, "Checking out LFS objects");
    }

    #[test]
    fn locks_are_read_from_the_json_array() {
        let json = r#"[
            {"id":"17","path":"art/logo.psd","owner":{"name":"alice"},"locked_at":"2022-05-17T15:49:06Z"},
            {"id":"18","path":"docs/spec.bin","owner":{"name":"bob"},"locked_at":"2022-05-18T09:00:00+02:00"}
        ]"#;
        let locks = parse_locks(json).unwrap();
        assert_eq!(
            locks,
            [
                LfsLock {
                    id: "17".into(),
                    path: "art/logo.psd".into(),
                    owner: "alice".into(),
                    locked_at: "2022-05-17T15:49:06Z".into()
                },
                LfsLock {
                    id: "18".into(),
                    path: "docs/spec.bin".into(),
                    owner: "bob".into(),
                    locked_at: "2022-05-18T09:00:00+02:00".into()
                },
            ]
        );
    }

    #[test]
    fn verified_locks_join_ours_and_theirs() {
        let json = r#"{"ours":[{"id":"1","path":"a.bin","owner":{"name":"me"},"locked_at":"t"}],
                       "theirs":[{"id":"2","path":"b.bin","owner":{"name":"you"},"locked_at":"t"}],
                       "nextCursor":""}"#;
        let paths: Vec<_> = parse_locks(json).unwrap().into_iter().map(|l| l.path).collect();
        assert_eq!(paths, ["a.bin", "b.bin"]);
    }

    #[test]
    fn empty_outputs_and_missing_fields_are_tolerated() {
        assert!(parse_locks("").unwrap().is_empty());
        assert!(parse_locks("[]").unwrap().is_empty());
        assert!(parse_locks("null").unwrap().is_empty());
        let locks = parse_locks(r#"[{"id":"3","path":"c.bin"},{"id":"4"}]"#).unwrap();
        assert_eq!(locks.len(), 1, "une entrée sans chemin est écartée");
        assert_eq!((locks[0].owner.as_str(), locks[0].locked_at.as_str()), ("", ""));
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        assert!(matches!(parse_locks("{not json"), Err(GitError::Parse(_))));
        assert!(matches!(parse_locks("42"), Err(GitError::Parse(_))));
    }

    #[test]
    fn arguments_that_look_like_options_are_refused() {
        assert!(check_argument("--force").is_err());
        assert!(check_argument(" ").is_err());
        assert_eq!(check_argument(" *.bin ").unwrap(), "*.bin");
    }
}
