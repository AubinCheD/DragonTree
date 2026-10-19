#![allow(dead_code)]

use std::path::{Path, PathBuf};

use dragontree_git::Git;

/// Dépôt temporaire configuré de façon déterministe (pas de conversion CRLF, auteur fixe).
pub struct TestRepo {
    pub dir: tempfile::TempDir,
    pub git: Git,
}

impl TestRepo {
    pub fn new() -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::discover().unwrap();
        let repo = TestRepo { dir, git };
        repo.run(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo
    }

    /// Dépôt « bare » servant de remote.
    pub fn new_bare() -> TestRepo {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::discover().unwrap();
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
        self.git.cmd(self.path()).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e:?}")).stdout_str()
    }

    pub fn try_run(&self, args: &[&str]) -> Result<String, dragontree_git::GitError> {
        self.git.cmd(self.path()).args(args).run().map(|o| o.stdout_str())
    }

    pub fn commit_all(&self, msg: &str) -> String {
        self.run(&["add", "-A"]);
        self.run(&["commit", "-q", "-m", msg]);
        self.run(&["rev-parse", "HEAD"]).trim().to_string()
    }
}

/// Configuration git globale de test (voir [`isolate_global_git_config`]) : son dossier temporaire est supprimé quand
/// le dernier test qui la tient se termine. L'environnement continue de le désigner : une commande git tardive
/// n'atteint pas davantage le vrai fichier.
pub struct GlobalGitConfig {
    dir: tempfile::TempDir,
}

impl GlobalGitConfig {
    /// Fichier désigné par `GIT_CONFIG_GLOBAL`.
    pub fn file(&self) -> PathBuf {
        self.dir.path().join("gitconfig")
    }
}

/// Isole la configuration globale de git, jamais le `~/.gitconfig` de l'utilisateur : `GIT_CONFIG_GLOBAL` désigne un
/// fichier (contenu initial `initial`) d'un dossier temporaire, qui sert aussi de `HOME` et de `USERPROFILE` — git
/// avant 2.32 ignore `GIT_CONFIG_GLOBAL` (2.31 est pris en charge) et écrirait sinon dans le vrai fichier.
/// L'environnement est celui du processus : les tests d'un fichier qui l'appellent en même temps partagent le même
/// dossier. À appeler avant toute commande git, en gardant le résultat jusqu'à la fin du test (`let _global = …`).
pub fn isolate_global_git_config(initial: &str) -> std::sync::Arc<GlobalGitConfig> {
    static CURRENT: std::sync::Mutex<std::sync::Weak<GlobalGitConfig>> = std::sync::Mutex::new(std::sync::Weak::new());
    let mut current = CURRENT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(config) = current.upgrade() {
        return config;
    }
    let config = GlobalGitConfig { dir: tempfile::tempdir().unwrap() };
    std::fs::write(config.file(), initial).unwrap();
    // SAFETY : sous Windows, modifier l'environnement est sûr même avec d'autres threads (documentation de `set_var`).
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", config.file());
        std::env::set_var("HOME", config.dir.path());
        std::env::set_var("USERPROFILE", config.dir.path());
    }
    let config = std::sync::Arc::new(config);
    *current = std::sync::Arc::downgrade(&config);
    config
}
