//! Dépôt ouvert : chemins et opérations de lecture.

use std::path::{Path, PathBuf};

use dragontree_i18n::tr;

use crate::diff::{DiffOptions, FileChange, FileDiff, MAX_DIFF_BYTES, parse_name_status_z, parse_unified_diff_bytes};
use crate::error::GitError;
use crate::log::{Commit, LogOptions, LogParser, LogScope, RefSelection, log_args};
use crate::oid::Oid;
use crate::refs::{FOR_EACH_REF_FORMAT, RefsSnapshot, parse_refs, read_head};
use crate::runner::{CancelToken, Git, GitCommand};
use crate::stash::{STASH_FORMAT, StashEntry, parse_stash_list};
use crate::status::{STATUS_ARGS, StatusSnapshot, parse_status_v2};

/// Arbre vide de git (SHA-1), utilisé comme parent d'un commit racine.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[derive(Debug, Clone)]
pub struct Repo {
    pub git: Git,
    pub workdir: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
}

/// Opération en cours dans le dépôt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpState {
    None,
    Merge,
    Rebase { interactive: bool },
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: Oid,
    pub parents: Vec<Oid>,
    pub author: String,
    pub author_email: String,
    pub author_time: i64,
    pub committer: String,
    pub committer_email: String,
    pub committer_time: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submodule {
    pub path: String,
    pub oid: Option<Oid>,
    /// ' ' à jour, '-' non initialisé, '+' commit différent, 'U' en conflit.
    pub status: char,
}

/// Chemin renvoyé par git (`C:/a/b`) converti au format natif.
pub(crate) fn native_path(p: &str) -> PathBuf {
    if cfg!(windows) { PathBuf::from(p.replace('/', "\\")) } else { PathBuf::from(p) }
}

impl Repo {
    /// Ouvre le dépôt contenant `path` (qui peut être un sous-dossier).
    pub fn open(git: &Git, path: &Path) -> Result<Repo, GitError> {
        let not_repo = || GitError::NotARepository { path: path.display().to_string() };
        if !path.is_dir() {
            return Err(not_repo());
        }
        let out = git
            .cmd(path)
            .read_only()
            .args(["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-dir", "--git-common-dir"])
            .run()
            .map_err(|e| match e {
                GitError::Failed { ref stderr, .. } if stderr.contains("not a git repository") => not_repo(),
                other => other,
            })?;
        let text = out.stdout_str();
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        match (lines.next(), lines.next(), lines.next()) {
            (Some(top), Some(git_dir), Some(common)) => Ok(Repo {
                git: git.clone(),
                workdir: native_path(top),
                git_dir: native_path(git_dir),
                common_dir: native_path(common),
            }),
            _ => Err(GitError::Parse(tr!("rev-parse inattendu : {text}", text = format!("{text:?}")))),
        }
    }

    /// Nom affiché (dossier de travail).
    pub fn name(&self) -> String {
        self.workdir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.workdir.display().to_string())
    }

    pub(crate) fn cmd(&self) -> GitCommand {
        self.git.cmd(&self.workdir)
    }

    /// Commande de lecture : sans verrou optionnel.
    pub(crate) fn read(&self) -> GitCommand {
        self.cmd().read_only()
    }

    /// Lecture portant sur des chemins fournis par l'utilisateur, pris littéralement (`[ab].txt`).
    /// Réservé à ces commandes : la variable casse par exemple `git stash -u`.
    pub(crate) fn read_paths(&self) -> GitCommand {
        self.read().env("GIT_LITERAL_PATHSPECS", "1")
    }

    pub fn refs(&self) -> Result<RefsSnapshot, GitError> {
        let head = read_head(&self.git_dir)?;
        let out = self
            .read()
            .args([
                "for-each-ref",
                &format!("--format={FOR_EACH_REF_FORMAT}"),
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ])
            .run()?;
        Ok(parse_refs(&out.stdout_str(), head))
    }

    pub fn status(&self) -> Result<StatusSnapshot, GitError> {
        let out = self.read().args(STATUS_ARGS).run()?;
        parse_status_v2(&out.stdout)
    }

    /// Historique ; vide pour un dépôt sans commit.
    pub fn log(&self, opts: &LogOptions, cancel: &CancelToken) -> Result<Vec<Commit>, GitError> {
        match self.run_log(opts, cancel) {
            Err(GitError::Failed { stderr, .. })
                if (stderr.contains("unknown revision")
                    || stderr.contains("bad revision")
                    || stderr.contains("does not have any commits"))
                    && !self.head_exists() =>
            {
                // HEAD sans commit : on garde les autres références éventuelles.
                let without_head = match &opts.scope {
                    LogScope::All => Some(RefSelection { head: false, ..RefSelection::default() }),
                    LogScope::Refs(sel) if sel.head => Some(RefSelection { head: false, ..sel.clone() }),
                    _ => None,
                };
                match without_head {
                    Some(sel) => self.run_log(&LogOptions { scope: LogScope::Refs(sel), ..opts.clone() }, cancel),
                    None => Ok(Vec::new()),
                }
            }
            other => other,
        }
    }

    fn run_log(&self, opts: &LogOptions, cancel: &CancelToken) -> Result<Vec<Commit>, GitError> {
        let mut parser = LogParser::new();
        let mut commits = Vec::new();
        let mut parse_err = None;
        let cmd = if opts.paths.is_empty() { self.read() } else { self.read_paths() };
        cmd.args(log_args(opts)).cancel(cancel.clone()).run_streaming(
            &mut |chunk| {
                if parse_err.is_none()
                    && let Err(e) = parser.feed(chunk, &mut commits)
                {
                    parse_err = Some(e);
                }
            },
            &mut |_| {},
        )?;
        if let Some(e) = parse_err {
            return Err(e);
        }
        parser.finish(&mut commits)?;
        Ok(commits)
    }

    /// HEAD pointe sur un commit existant.
    pub fn head_exists(&self) -> bool {
        self.read().args(["rev-parse", "-q", "--verify", "HEAD"]).run().is_ok()
    }

    pub fn commit_info(&self, id: Oid) -> Result<CommitInfo, GitError> {
        let out = self
            .read()
            .args([
                "log",
                "-1",
                "--no-show-signature",
                "--no-color",
                "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ce%x1f%ct%x1f%B",
                &id.to_hex(),
                "--",
            ])
            .run()?;
        let text = out.stdout_str();
        let f: Vec<&str> = text.splitn(9, '\x1f').collect();
        if f.len() != 9 {
            return Err(GitError::Parse(tr!("commit {id} illisible", id)));
        }
        Ok(CommitInfo {
            id: Oid::from_hex(f[0].trim()).ok_or_else(|| GitError::Parse(f[0].to_string()))?,
            parents: f[1].split_whitespace().filter_map(Oid::from_hex).collect(),
            author: f[2].to_string(),
            author_email: f[3].to_string(),
            author_time: f[4].parse().unwrap_or(0),
            committer: f[5].to_string(),
            committer_email: f[6].to_string(),
            committer_time: f[7].parse().unwrap_or(0),
            message: f[8].trim_end().to_string(),
        })
    }

    /// Fichiers modifiés par `id` par rapport à `parent` (premier parent ; `None` pour une racine).
    pub fn commit_files(&self, id: Oid, parent: Option<Oid>) -> Result<Vec<FileChange>, GitError> {
        let mut cmd = self.read().args([
            "-c",
            "core.quotepath=false",
            "diff-tree",
            "--no-commit-id",
            "-r",
            "-z",
            "--name-status",
            "-M",
        ]);
        cmd = match parent {
            Some(p) => cmd.args([p.to_hex(), id.to_hex()]),
            None => cmd.args(["--root".to_string(), id.to_hex()]),
        };
        Ok(parse_name_status_z(&cmd.run()?.stdout))
    }

    pub fn diff_commit_file(
        &self,
        id: Oid,
        parent: Option<Oid>,
        change: &FileChange,
        o: &DiffOptions,
    ) -> Result<FileDiff, GitError> {
        let from = parent.map(|p| p.to_hex()).unwrap_or_else(|| EMPTY_TREE.to_string());
        let mut cmd = self.read_paths().args(diff_base_args(o)).arg("-M").args([from, id.to_hex(), "--".into()]);
        if let Some(old) = &change.old_path {
            cmd = cmd.arg(old);
        }
        cmd = cmd.arg(&change.path);
        let out = cmd.run()?;
        Ok(pick_diff(&out.stdout, change.old_path.as_deref(), &change.path))
    }

    /// Diff d'un fichier de la copie de travail : indexé (`staged`), non indexé, ou non suivi.
    pub fn diff_worktree_file(
        &self,
        path: &str,
        staged: bool,
        untracked: bool,
        o: &DiffOptions,
    ) -> Result<FileDiff, GitError> {
        let mut cmd = self.read_paths().args(diff_base_args(o)).arg("--no-renames");
        if untracked {
            let full = self.workdir.join(path);
            if std::fs::metadata(&full).map(|m| m.len() as usize > MAX_DIFF_BYTES).unwrap_or(false) {
                return Ok(FileDiff {
                    new_path: Some(path.to_string()),
                    is_new: true,
                    truncated: true,
                    ..Default::default()
                });
            }
            cmd = cmd.args(["--no-index", "--", "/dev/null", path]).allow_codes(&[1]);
        } else if staged {
            cmd = cmd.args(["--cached", "--", path]);
        } else {
            cmd = cmd.args(["--", path]);
        }
        let out = cmd.run()?;
        let mut d = pick_diff(&out.stdout, None, path);
        if untracked {
            d.is_new = true;
            d.old_path = None;
            d.new_path = Some(path.to_string());
        }
        Ok(d)
    }

    pub fn stashes(&self) -> Result<Vec<StashEntry>, GitError> {
        let out = self.read().args(["stash", "list", "-z", &format!("--format={STASH_FORMAT}")]).run()?;
        Ok(parse_stash_list(&out.stdout_str()))
    }

    pub fn remotes(&self) -> Result<Vec<Remote>, GitError> {
        let out = self.read().args(["remote", "-v"]).run()?;
        let mut remotes: Vec<Remote> = Vec::new();
        for line in out.stdout_str().lines() {
            let Some((name, rest)) = line.split_once('\t') else { continue };
            let (url, kind) = match rest.rsplit_once(' ') {
                Some((url, kind)) => (url.to_string(), kind),
                None => (rest.to_string(), "(fetch)"),
            };
            let idx = match remotes.iter().position(|r| r.name == name) {
                Some(i) => i,
                None => {
                    remotes.push(Remote { name: name.to_string(), fetch_url: String::new(), push_url: String::new() });
                    remotes.len() - 1
                }
            };
            if kind == "(push)" {
                remotes[idx].push_url = url;
            } else {
                remotes[idx].fetch_url = url;
            }
        }
        for r in &mut remotes {
            if r.push_url.is_empty() {
                r.push_url = r.fetch_url.clone();
            }
        }
        Ok(remotes)
    }

    pub fn submodules(&self) -> Result<Vec<Submodule>, GitError> {
        if !self.workdir.join(".gitmodules").exists() {
            return Ok(Vec::new());
        }
        let out = self.read().args(["-c", "core.quotepath=false", "submodule", "status"]).run()?;
        Ok(out
            .stdout_str()
            .lines()
            .filter_map(|line| {
                let mut chars = line.chars();
                let status = chars.next()?;
                let rest = chars.as_str();
                let (sha, tail) = rest.split_once(' ')?;
                let path = tail.rsplit_once(" (").map_or(tail, |(p, _)| p);
                Some(Submodule { path: path.to_string(), oid: Oid::from_hex(sha), status })
            })
            .collect())
    }

    pub fn operation_state(&self) -> OpState {
        let gd = &self.git_dir;
        if gd.join("rebase-merge").is_dir() {
            OpState::Rebase { interactive: gd.join("rebase-merge").join("interactive").exists() }
        } else if gd.join("rebase-apply").is_dir() {
            OpState::Rebase { interactive: false }
        } else if gd.join("MERGE_HEAD").exists() {
            OpState::Merge
        } else if gd.join("CHERRY_PICK_HEAD").exists() {
            OpState::CherryPick
        } else if gd.join("REVERT_HEAD").exists() {
            OpState::Revert
        } else {
            OpState::None
        }
    }

    pub fn last_commit_message(&self) -> Result<String, GitError> {
        let out = self.read().args(["log", "-1", "--no-show-signature", "--format=%B"]).run()?;
        Ok(out.stdout_str().trim_end().to_string())
    }

    /// Contenu d'un fichier à une révision (`rev` vide : version indexée).
    pub fn blob(&self, rev: &str, path: &str) -> Result<Vec<u8>, GitError> {
        let spec = format!("{rev}:{path}");
        Ok(self.read().args(["cat-file", "blob", &spec]).run()?.stdout)
    }

    pub fn config(&self, key: &str) -> Option<String> {
        let out = self.read().args(["config", "--get", key]).run().ok()?;
        Some(out.stdout_str().trim_end().to_string())
    }
}

fn diff_base_args(o: &DiffOptions) -> Vec<String> {
    let mut args = vec![
        "-c".to_string(),
        "core.quotepath=false".to_string(),
        "diff".to_string(),
        "--no-color".to_string(),
        "--no-ext-diff".to_string(),
        format!("-U{}", o.context),
    ];
    if o.ignore_whitespace {
        args.push("-w".to_string());
    }
    args
}

/// Choisit le diff du fichier demandé dans la sortie (et gère la taille maximale).
fn pick_diff(stdout: &[u8], old_path: Option<&str>, path: &str) -> FileDiff {
    if stdout.len() > MAX_DIFF_BYTES {
        return FileDiff {
            old_path: old_path.map(str::to_string),
            new_path: Some(path.to_string()),
            truncated: true,
            ..Default::default()
        };
    }
    let mut diffs = parse_unified_diff_bytes(stdout);
    let idx = diffs.iter().position(|d| d.new_path.as_deref() == Some(path) || d.old_path.as_deref() == Some(path));
    match idx {
        Some(i) => diffs.swap_remove(i),
        None if !diffs.is_empty() => diffs.swap_remove(0),
        None => {
            FileDiff { old_path: old_path.map(str::to_string), new_path: Some(path.to_string()), ..Default::default() }
        }
    }
}
