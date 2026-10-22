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
    /// Opération en arrière-plan (fetch automatique) : aucune fenêtre d'authentification, et une connexion HTTP qui ne
    /// transmet presque plus rien est abandonnée (voir `with_net`).
    pub non_interactive: bool,
}

/// Débit (octets par seconde) et durée en dessous desquels une opération en arrière-plan abandonne une connexion HTTP
/// (`http.lowSpeedLimit`, `http.lowSpeedTime`) : un fetch automatique bloqué sur un serveur qui ne répond plus se termine
/// de lui-même au lieu de retenir indéfiniment les opérations réseau visibles qui l'attendent. Sans effet sur SSH : un
/// fetch automatique bloqué en SSH dure jusqu'à la fermeture de l'application, et l'opération visible qui l'attend
/// peut toujours être annulée.
const BACKGROUND_LOW_SPEED: (&str, &str) = ("http.lowSpeedLimit=1000", "http.lowSpeedTime=60");

pub(crate) fn with_net(cmd: GitCommand, n: &NetOptions) -> GitCommand {
    let cmd = cmd.env("GIT_EDITOR", "true").envs(&n.env).cancel(n.cancel.clone());
    if n.non_interactive {
        // Options de git avant la sous-commande, que l'appelant ajoute ensuite.
        let (limit, time) = BACKGROUND_LOW_SPEED;
        cmd.non_interactive().args(["-c", limit, "-c", time])
    } else {
        cmd
    }
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
        run_with_progress(self.fetch_command(remote, prune, n), on)
    }

    fn fetch_command(&self, remote: Option<&str>, prune: bool, n: &NetOptions) -> GitCommand {
        let mut cmd = self.net(n).args(["fetch", "--progress"]);
        if prune {
            cmd = cmd.arg("--prune");
        }
        match remote {
            Some(r) => cmd.arg(r),
            None => cmd.arg("--all"),
        }
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

/// Clone `url` dans `dest` (dont le dossier parent est créé si besoin). Le clone d'un dépôt SHA-256 se termine, puis
/// est signalé par `GitError::UnsupportedObjectFormat` (L1) : `dest` reste sur le disque, à l'appelant de le garder
/// ou non (l'interface supprime le dossier qu'elle a créé, comme après un clonage échoué). Seul un format non pris en
/// charge fait échouer un clonage réussi : tout autre échec de cette vérification (dossier d'un autre propriétaire…)
/// laisse le clone en place, et son ouverture le signale.
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
    run_with_progress(cmd.arg("--").arg(url).arg(dest), on)?;
    // Format du clone terminé (L1), dans l'environnement du clonage. Un autre échec de la vérification est laissé à
    // l'ouverture du dépôt, qui le signale avec sa suite (« Faire confiance à ce dépôt » pour `safe.directory`).
    match crate::repo::check_object_format(git.cmd(dest).envs(&n.env), dest) {
        Err(e @ GitError::UnsupportedObjectFormat { .. }) => Err(e),
        _ => Ok(()),
    }
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Ligne de commande du fetch du dépôt (sans le lancer).
    fn fetch_line(non_interactive: bool) -> String {
        let repo = Repo {
            git: Git::with_exe(PathBuf::from("git")),
            workdir: PathBuf::from("."),
            git_dir: PathBuf::from(".git"),
            common_dir: PathBuf::from(".git"),
        };
        let n = NetOptions { non_interactive, ..Default::default() };
        repo.fetch_command(None, true, &n).display()
    }

    /// Un fetch automatique abandonne une connexion HTTP qui ne transmet presque plus rien ; un fetch visible, non.
    #[test]
    fn a_background_fetch_gives_up_a_stalled_http_connection() {
        assert_eq!(
            fetch_line(true),
            "git -c http.lowSpeedLimit=1000 -c http.lowSpeedTime=60 fetch --progress --prune --all",
            "options avant la sous-commande"
        );
        assert_eq!(fetch_line(false), "git fetch --progress --prune --all");
    }
}
