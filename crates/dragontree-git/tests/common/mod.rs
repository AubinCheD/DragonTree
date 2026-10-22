#![allow(dead_code)]

mod global_config;

use std::path::{Path, PathBuf};

use dragontree_git::Git;
pub use global_config::*;

/// git des tests, configuration globale isolée (voir `global_config`) : à utiliser à la place de `Git::discover`.
pub fn git() -> Git {
    isolate_git_config_for_this_test();
    Git::discover().expect("git doit être installé pour les tests")
}

/// Dépôt temporaire configuré de façon déterministe (pas de conversion CRLF, auteur fixe), configuration globale
/// isolée.
pub struct TestRepo {
    pub dir: tempfile::TempDir,
    pub git: Git,
}

impl TestRepo {
    pub fn new() -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        let repo = TestRepo { dir, git };
        repo.run(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo
    }

    /// Dépôt « bare » servant de remote.
    pub fn new_bare() -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let git = git();
        let repo = TestRepo { dir, git };
        repo.run(&["init", "-q", "--bare", "-b", "main"]);
        repo
    }

    pub fn configure(&self) {
        for (k, v) in [
            ("user.name", "Testeur"),
            ("user.email", "test@example.com"),
            ("core.autocrlf", "false"),
            ("core.safecrlf", "false"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
            ("init.defaultBranch", "main"),
        ] {
            self.run(&["config", k, v]);
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn url(&self) -> String {
        format!("file:///{}", self.path().display().to_string().replace('\\', "/"))
    }

    pub fn file(&self, rel: &str) -> PathBuf {
        self.path().join(rel)
    }

    pub fn write(&self, rel: &str, content: &str) {
        let p = self.file(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, content).unwrap();
    }

    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.file(rel)).unwrap()
    }

    pub fn run(&self, args: &[&str]) -> String {
        self.try_run(args).unwrap_or_else(|e| panic!("git {args:?} : {e:?}"))
    }

    /// Comme `run`, sans paniquer. Isole aussi la configuration globale : le dépôt peut servir dans un autre thread
    /// que celui qui l'a créé.
    pub fn try_run(&self, args: &[&str]) -> Result<String, dragontree_git::GitError> {
        isolate_git_config_for_this_test();
        self.git.cmd(self.path()).args(args).run().map(|o| o.stdout_str())
    }

    pub fn commit_all(&self, msg: &str) -> String {
        self.run(&["add", "-A"]);
        self.run(&["commit", "-q", "-m", msg]);
        self.run(&["rev-parse", "HEAD"]).trim().to_string()
    }
}
