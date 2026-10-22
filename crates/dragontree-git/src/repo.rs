//! Dépôt ouvert : chemins et opérations de lecture.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use dragontree_i18n::tr;

use crate::diff::{DiffOptions, FileChange, FileDiff, MAX_DIFF_BYTES, parse_name_status_z, parse_unified_diff_bytes};
use crate::error::GitError;
use crate::log::{
    Commit, CommitSearch, LOG_FORMAT, LogOptions, LogParser, LogScope, LostCommits, RefSelection, log_args, log_stdin,
    lost_args, lost_stdin, parse_lost, search_args,
};
use crate::oid::Oid;
use crate::refs::{FOR_EACH_REF_FORMAT, RefsSnapshot, parse_refs, read_head};
use crate::runner::{CancelToken, Git, GitCommand, Output};
use crate::stash::{STASH_FORMAT, StashEntry, parse_stash_list};
use crate::status::{STATUS_ARGS, STATUS_IGNORED_ARG, StatusSnapshot, parse_status_v2};

/// git se plaint d'un HEAD sans commit.
fn unborn_head_error(stderr: &str) -> bool {
    stderr.contains("unknown revision")
        || stderr.contains("bad revision")
        || stderr.contains("does not have any commits")
}

/// Les mêmes options sans HEAD (dépôt sans commit), quand la portée en contient d'autres.
fn without_head(opts: &LogOptions) -> Option<LogOptions> {
    let selection = match &opts.scope {
        LogScope::All => RefSelection { head: false, ..RefSelection::default() },
        LogScope::Refs(sel) if sel.head => RefSelection { head: false, ..sel.clone() },
        _ => return None,
    };
    Some(LogOptions { scope: LogScope::Refs(selection), ..opts.clone() })
}

/// Arbre vide de git (SHA-1), utilisé comme parent d'un commit racine.
pub const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Seul format d'objets pris en charge : identifiants SHA-1 de 20 octets (`Oid`). Un dépôt SHA-256 est refusé à
/// l'ouverture et au clonage (L1).
const SUPPORTED_OBJECT_FORMAT: &str = "sha1";

/// Erreur explicite si le dépôt de travail `dir` n'utilise pas des identifiants SHA-1 (`git rev-parse
/// --show-object-format`) ; `cmd` : commande git lancée dans `dir`, à compléter (environnement de l'appelant). Son
/// dossier git est désigné explicitement (`--git-dir=.git`) : sans recherche du dépôt, git ne refuse pas un dossier
/// d'un autre propriétaire (`safe.directory`).
pub(crate) fn check_object_format(cmd: GitCommand, dir: &Path) -> Result<(), GitError> {
    let out = cmd.read_only().args(["--git-dir=.git", "rev-parse", "--show-object-format"]).run()?;
    match out.stdout_str().trim() {
        SUPPORTED_OBJECT_FORMAT => Ok(()),
        format => {
            Err(GitError::UnsupportedObjectFormat { format: format.to_string(), path: Some(dir.display().to_string()) })
        }
    }
}

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
    /// Nom dans `.gitmodules` (clé `submodule.<nom>`), le plus souvent identique au chemin.
    pub name: String,
    /// URL de `.gitmodules` (vide si absente).
    pub url: String,
}

/// Chemin renvoyé par git (`C:/a/b`) converti au format natif.
pub(crate) fn native_path(p: &str) -> PathBuf {
    if cfg!(windows) { PathBuf::from(p.replace('/', "\\")) } else { PathBuf::from(p) }
}

impl Repo {
    /// Ouvre le dépôt contenant `path` (qui peut être un sous-dossier). Un dépôt dont les identifiants ne sont pas
    /// SHA-1 (`extensions.objectFormat=sha256`) est refusé : `GitError::UnsupportedObjectFormat` (L1).
    pub fn open(git: &Git, path: &Path) -> Result<Repo, GitError> {
        let not_repo = || GitError::NotARepository { path: path.display().to_string() };
        if !path.is_dir() {
            return Err(not_repo());
        }
        let out = git
            .cmd(path)
            .read_only()
            .args([
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--git-dir",
                "--git-common-dir",
                "--show-object-format",
            ])
            .run()
            .map_err(|e| match e {
                GitError::Failed { ref stderr, .. } if stderr.contains("not a git repository") => not_repo(),
                other => other,
            })?;
        let text = out.stdout_str();
        let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
        match (lines.next(), lines.next(), lines.next(), lines.next()) {
            (Some(top), Some(_), Some(_), Some(format)) if format != SUPPORTED_OBJECT_FORMAT => {
                Err(GitError::UnsupportedObjectFormat {
                    format: format.to_string(),
                    path: Some(native_path(top).display().to_string()),
                })
            }
            (Some(top), Some(git_dir), Some(common), Some(_)) => Ok(Repo {
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

    /// Racine de la copie de travail principale quand ce dépôt est un worktree lié (dossier parent du répertoire
    /// commun `.git`) ; `None` pour la copie principale elle-même, ou si le dépôt principal est nu.
    pub fn main_workdir(&self) -> Option<PathBuf> {
        if self.git_dir == self.common_dir || self.common_dir.file_name()? != ".git" {
            return None;
        }
        self.common_dir.parent().map(Path::to_path_buf)
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
        self.status_with_ignored(false)
    }

    /// Statut ; avec `show_ignored`, git liste aussi les fichiers ignorés (`StatusSnapshot::ignored`).
    pub fn status_with_ignored(&self, show_ignored: bool) -> Result<StatusSnapshot, GitError> {
        let cmd = self.read().args(STATUS_ARGS);
        let cmd = if show_ignored { cmd.args([STATUS_IGNORED_ARG]) } else { cmd };
        parse_status_v2(&cmd.run()?.stdout)
    }

    /// Historique ; vide pour un dépôt sans commit.
    pub fn log(&self, opts: &LogOptions, cancel: &CancelToken) -> Result<Vec<Commit>, GitError> {
        match self.run_log(opts, cancel) {
            Err(GitError::Failed { stderr, .. }) if unborn_head_error(&stderr) && !self.head_exists() => {
                // HEAD sans commit : on garde les autres références éventuelles.
                match without_head(opts) {
                    Some(opts) => self.run_log(&opts, cancel),
                    None => Ok(Vec::new()),
                }
            }
            other => other,
        }
    }

    /// Oids des commits de la portée de `opts` que `search` trouve (`git log -G` ou par chemin), sans les lire un à
    /// un. Une expression régulière invalide revient en `GitError::Failed` avec le message de git.
    pub fn search_commits(
        &self,
        opts: &LogOptions,
        search: &CommitSearch,
        cancel: &CancelToken,
    ) -> Result<HashSet<Oid>, GitError> {
        let run = |opts: &LogOptions| {
            let cmd = match log_stdin(opts) {
                Some(revisions) => self.read().stdin(revisions),
                None => self.read(),
            };
            cmd.args(search_args(opts, search)).cancel(cancel.clone()).run()
        };
        let out = match run(opts) {
            Err(GitError::Failed { stderr, .. }) if unborn_head_error(&stderr) && !self.head_exists() => {
                match without_head(opts) {
                    Some(opts) => run(&opts)?,
                    None => return Ok(HashSet::new()),
                }
            }
            other => other?,
        };
        Ok(out.stdout_str().lines().filter_map(|l| Oid::from_hex(l.trim())).collect())
    }

    fn run_log(&self, opts: &LogOptions, cancel: &CancelToken) -> Result<Vec<Commit>, GitError> {
        let mut parser = LogParser::new();
        let mut commits = Vec::new();
        let mut parse_err = None;
        let cmd = if opts.paths.is_empty() { self.read() } else { self.read_paths() };
        let cmd = match log_stdin(opts) {
            Some(revisions) => cmd.stdin(revisions),
            None => cmd,
        };
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

    /// Les commits `oids`, tels quels et dans cet ordre (sans parcourir leur historique) : par exemple les stashs.
    pub fn commits_of(&self, oids: &[Oid], cancel: &CancelToken) -> Result<Vec<Commit>, GitError> {
        if oids.is_empty() {
            return Ok(Vec::new());
        }
        let stdin: Vec<u8> = oids.iter().flat_map(|oid| format!("{}\n", oid.to_hex()).into_bytes()).collect();
        let args = ["log", "-z", "--no-walk=unsorted", &format!("--format={LOG_FORMAT}"), "--stdin", "--"];
        let out = self.read().args(args).stdin(stdin).cancel(cancel.clone()).run()?;
        let (mut parser, mut commits) = (LogParser::new(), Vec::new());
        parser.feed(&out.stdout, &mut commits)?;
        parser.finish(&mut commits)?;
        Ok(commits)
    }

    /// Commits atteignables seulement par le reflog (voir [`LostCommits`]).
    pub fn lost_commits(&self, cancel: &CancelToken) -> Result<LostCommits, GitError> {
        let stashes: Vec<Oid> = self.stashes()?.iter().map(|s| s.oid).collect();
        let out = self.read().args(lost_args()).stdin(lost_stdin(&stashes)).cancel(cancel.clone()).run()?;
        Ok(parse_lost(&out.stdout_str()))
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
            id: Oid::parse(f[0].trim())?,
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

    /// Diff d'un fichier de `id` par rapport à `parent` (arbre vide pour une racine). Comme pour les autres diffs de
    /// fichier, la sortie de git n'est lue que jusqu'à [`MAX_DIFF_BYTES`] : au-delà, la lecture s'arrête, git est
    /// arrêté et le diff est `truncated`, sans hunks.
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
        let out = cmd.stdout_limit(MAX_DIFF_BYTES).run()?;
        Ok(pick_diff(&out, change.old_path.as_deref(), &change.path))
    }

    /// Fichiers qui diffèrent entre la révision `a` et la révision `b`, ou la copie de travail (fichiers suivis)
    /// quand `b` est `None`. Les révisions sont des noms de branche, de tag ou des SHA.
    pub fn compare_files(&self, a: &str, b: Option<&str>) -> Result<Vec<FileChange>, GitError> {
        let args = ["-c", "core.quotepath=false", "diff", "--no-ext-diff", "-z", "--name-status", "-M"];
        let mut cmd = self.read().args(args).arg(a);
        if let Some(b) = b {
            cmd = cmd.arg(b);
        }
        Ok(parse_name_status_z(&cmd.arg("--").run()?.stdout))
    }

    /// Diff d'un fichier de `compare_files` (mêmes révisions).
    pub fn compare_file_diff(
        &self,
        a: &str,
        b: Option<&str>,
        change: &FileChange,
        o: &DiffOptions,
    ) -> Result<FileDiff, GitError> {
        let mut cmd = self.read_paths().args(diff_base_args(o)).arg("-M").arg(a);
        if let Some(b) = b {
            cmd = cmd.arg(b);
        }
        cmd = cmd.arg("--");
        if let Some(old) = &change.old_path {
            cmd = cmd.arg(old);
        }
        let out = cmd.arg(&change.path).stdout_limit(MAX_DIFF_BYTES).run()?;
        Ok(pick_diff(&out, change.old_path.as_deref(), &change.path))
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
        let out = cmd.stdout_limit(MAX_DIFF_BYTES).run()?;
        let mut d = pick_diff(&out, None, path);
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

    /// Fichiers non suivis d'un stash (`stash -u` les range dans un troisième parent, un commit racine) : le
    /// commit qui les porte et ses fichiers. `None` si le stash n'en a pas.
    pub fn stash_untracked(&self, stash: Oid) -> Result<Option<(Oid, Vec<FileChange>)>, GitError> {
        let spec = format!("{}^3", stash.to_hex());
        // `-q --verify` sort avec le code 1 quand il n'y a pas de troisième parent : seul cas attendu.
        let out = self.read().args(["rev-parse", "-q", "--verify", &spec]).allow_codes(&[1]).run()?;
        let Some(commit) = Oid::from_hex(out.stdout_str().trim()).filter(|_| out.code == 0) else { return Ok(None) };
        Ok(Some((commit, self.commit_files(commit, None)?)))
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
        let entries = self.gitmodules_entries()?;
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
                let entry = entries.iter().find(|e| e.path == path);
                Some(Submodule {
                    path: path.to_string(),
                    oid: Oid::from_hex(sha),
                    status,
                    name: entry.map_or_else(|| path.to_string(), |e| e.name.clone()),
                    url: entry.map(|e| e.url.clone()).unwrap_or_default(),
                })
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

/// Choisit le diff du fichier demandé dans la sortie, lue avec la limite [`MAX_DIFF_BYTES`] : une sortie coupée donne un
/// diff `truncated`, sans hunks (une sortie lue avec la limite n'est jamais plus longue qu'elle).
fn pick_diff(out: &Output, old_path: Option<&str>, path: &str) -> FileDiff {
    let stdout = &out.stdout;
    if out.stdout_truncated {
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
