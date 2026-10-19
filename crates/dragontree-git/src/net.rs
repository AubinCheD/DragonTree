//! Opérations réseau (clone, fetch, pull, push) avec progression et authentification injectée.

use std::path::Path;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::progress::{Progress, parse_progress};
use crate::repo::Repo;
use crate::runner::{CancelToken, Git, GitCommand};

#[derive(Debug, Clone, Default)]
pub struct NetOptions {
    /// Variables d'environnement supplémentaires (authentification d'un compte).
    pub env: Vec<(String, String)>,
    pub cancel: CancelToken,
    /// Interdit les fenêtres d'authentification (opérations en arrière-plan).
    pub non_interactive: bool,
}

pub(crate) fn with_net(cmd: GitCommand, n: &NetOptions) -> GitCommand {
    let cmd = cmd.env("GIT_EDITOR", "true").envs(&n.env).cancel(n.cancel.clone());
    if n.non_interactive { cmd.non_interactive() } else { cmd }
}

pub(crate) fn run_with_progress(cmd: GitCommand, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
    cmd.run_streaming(&mut |_| {}, &mut |line| {
        if let Some(p) = parse_progress(line) {
            on(p);
        }
    })
    .map(|_| ())
}

impl Repo {
    fn net(&self, n: &NetOptions) -> GitCommand {
        with_net(self.cmd(), n)
    }

    /// `remote = None` : tous les remotes.
    pub fn fetch(
        &self,
        remote: Option<&str>,
        prune: bool,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        let mut cmd = self.net(n).args(["fetch", "--progress"]);
        if prune {
            cmd = cmd.arg("--prune");
        }
        cmd = match remote {
            Some(r) => cmd.arg(r),
            None => cmd.arg("--all"),
        };
        run_with_progress(cmd, on)
    }

    pub fn pull(
        &self,
        remote: &str,
        branch: &str,
        rebase: bool,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        let mode = if rebase { "--rebase" } else { "--no-rebase" };
        run_with_progress(self.net(n).args(["pull", "--progress", mode, remote, branch]), on)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &self,
        remote: &str,
        local: &str,
        remote_branch: &str,
        set_upstream: bool,
        force_with_lease: bool,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        let mut cmd = self.net(n).args(["push", "--progress"]);
        if set_upstream {
            cmd = cmd.arg("--set-upstream");
        }
        if force_with_lease {
            // `--force-if-includes` : la protection tient même après un fetch en arrière-plan
            // (le bail seul serait satisfait par la référence distante fraîchement récupérée).
            cmd = cmd.args(["--force-with-lease", "--force-if-includes"]);
        }
        cmd = cmd.args([remote.to_string(), format!("refs/heads/{local}:refs/heads/{remote_branch}")]);
        run_with_progress(cmd, on)
    }

    /// Teste la connexion à un remote : `git ls-remote --heads <remote>` ; renvoie les noms de ses branches, triés.
    /// Un remote injoignable ou refusé donne l'erreur de git.
    pub fn ls_remote_heads(&self, remote: &str, n: &NetOptions) -> Result<Vec<String>, GitError> {
        let out = self.net(n).args(["ls-remote", "--heads", remote]).run()?;
        let mut heads: Vec<String> = out
            .stdout_str()
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(_, name)| name.strip_prefix("refs/heads/").unwrap_or(name).to_string())
            .collect();
        heads.sort();
        Ok(heads)
    }

    /// `git submodule update --init --recursive`.
    pub fn update_submodules(&self, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
        run_with_progress(self.net(n).args(["submodule", "update", "--init", "--recursive", "--progress"]), on)
    }

    /// Avance une branche locale non extraite jusqu'à `upstream` (refus si ce n'est pas une avance rapide).
    pub fn fast_forward_branch(&self, branch: &str, upstream: &str) -> Result<(), GitError> {
        let refspec = format!("refs/remotes/{upstream}:refs/heads/{branch}");
        self.cmd().args(["fetch", ".", &refspec]).run().map(|_| ())
    }

    /// `git push <remote> --tags`.
    pub fn push_all_tags(&self, remote: &str, n: &NetOptions, on: &mut dyn FnMut(Progress)) -> Result<(), GitError> {
        run_with_progress(self.net(n).args(["push", "--progress", remote, "--tags"]), on)
    }

    pub fn push_tag(
        &self,
        remote: &str,
        tag: &str,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        run_with_progress(
            self.net(n).args(["push".to_string(), "--progress".into(), remote.into(), format!("refs/tags/{tag}")]),
            on,
        )
    }

    pub fn delete_remote_branch(
        &self,
        remote: &str,
        branch: &str,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        run_with_progress(
            self.net(n).args([
                "push".to_string(),
                "--progress".into(),
                remote.into(),
                "--delete".into(),
                format!("refs/heads/{branch}"),
            ]),
            on,
        )
    }

    pub fn delete_remote_tag(
        &self,
        remote: &str,
        tag: &str,
        n: &NetOptions,
        on: &mut dyn FnMut(Progress),
    ) -> Result<(), GitError> {
        run_with_progress(
            self.net(n).args([
                "push".to_string(),
                "--progress".into(),
                remote.into(),
                "--delete".into(),
                format!("refs/tags/{tag}"),
            ]),
            on,
        )
    }
}

/// Clone `url` dans `dest` (dont le dossier parent est créé si besoin).
pub fn clone_repo(
    git: &Git,
    url: &str,
    dest: &Path,
    branch: Option<&str>,
    recurse: bool,
    n: &NetOptions,
    on: &mut dyn FnMut(Progress),
) -> Result<(), GitError> {
    let parent =
        dest.parent().ok_or_else(|| GitError::Io(tr!("destination invalide : {path}", path = dest.display())))?;
    std::fs::create_dir_all(parent)?;
    let mut cmd = with_net(git.cmd(parent), n).args(["clone", "--progress"]);
    if let Some(b) = branch.filter(|b| !b.is_empty()) {
        cmd = cmd.args(["--branch", b]);
    }
    if recurse {
        cmd = cmd.arg("--recurse-submodules");
    }
    run_with_progress(cmd.arg("--").arg(url).arg(dest), on)
}

/// Crée un dépôt vide dans `path` (créé si besoin).
pub fn init_repo(git: &Git, path: &Path) -> Result<(), GitError> {
    std::fs::create_dir_all(path)?;
    git.cmd(path).args(["init", "-q"]).run().map(|_| ())
}

/// Déclare le dépôt sûr (`git config --global --add safe.directory`).
pub fn trust_directory(git: &Git, path: &Path) -> Result<(), GitError> {
    let p = path.to_string_lossy().replace('\\', "/");
    git.cmd(&std::env::temp_dir()).args(["config", "--global", "--add", "safe.directory", &p]).run().map(|_| ())
}
