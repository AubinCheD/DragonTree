//! Configuration git globale des tests, jamais celle de l'utilisateur (contrainte 6 du plan) : `GIT_CONFIG_GLOBAL`
//! désigne un fichier d'un dossier temporaire, qui sert aussi de `HOME`, de `USERPROFILE` et de `XDG_CONFIG_HOME` —
//! git avant 2.32 ignore `GIT_CONFIG_GLOBAL` (2.31 est pris en charge) et lirait puis écrirait sinon le vrai
//! `~/.gitconfig` ou `~/.config/git/config`. La configuration système (celle de Git pour Windows) n'est pas celle de
//! l'utilisateur : elle reste lue.
//!
//! Isolation par défaut : les helpers partagés (dépôt de test, commande git, harnais de l'interface) appellent
//! [`isolate_git_config_for_this_test`] avant toute commande git, et le test qui les appelle tient la configuration
//! jusqu'à sa fin : la référence est rangée dans une variable locale à son thread, libérée quand il se termine
//! (libtest lance chaque test dans son propre thread). Un test qui a besoin d'un contenu appelle
//! [`isolate_global_git_config`] avec ce contenu, avant toute commande git.
//!
//! L'environnement est celui du processus : les tests d'un même binaire qui tournent en même temps partagent le même
//! fichier. Son dossier est supprimé quand le dernier test qui le tient se termine ; l'environnement le désigne encore
//! (une commande git tardive ne lit alors aucune configuration globale, jamais le vrai fichier), et le test suivant en
//! crée un autre.
//!
//! Fichier commun aux tests de `dragontree-git` et de `dragontree-app` (tests d'intégration et tests unitaires),
//! inclus par `#[path]` : une seule logique à tenir.

#![allow(dead_code)]

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError, Weak};

/// Configuration git globale de test : son dossier temporaire est supprimé quand le dernier test qui la tient se
/// termine.
pub struct GlobalGitConfig {
    dir: tempfile::TempDir,
}

impl GlobalGitConfig {
    /// Fichier désigné par `GIT_CONFIG_GLOBAL`.
    pub fn file(&self) -> PathBuf {
        self.dir.path().join("gitconfig")
    }
}

/// Configuration que désigne l'environnement, tant qu'un test la tient.
static CURRENT: Mutex<Weak<GlobalGitConfig>> = Mutex::new(Weak::new());

thread_local! {
    /// Configuration tenue par le test de ce thread jusqu'à sa fin.
    static HELD: RefCell<Option<Arc<GlobalGitConfig>>> = const { RefCell::new(None) };
}

/// Isole la configuration globale de git avec ce contenu initial (`initial`), et la renvoie ; le test la tient aussi
/// jusqu'à sa fin. Si une configuration est déjà en vigueur (un autre test du binaire en cours, ou un helper de ce
/// test), c'est elle qui est renvoyée : un fichier encore vide reçoit `initial` ; un fichier qui a déjà un autre
/// contenu fait échouer le test (deux tests du même binaire qui veulent chacun leur contenu ne peuvent pas tourner en
/// même temps : les réunir en un seul test). Un `initial` vide accepte n'importe quel contenu.
///
/// Règle qui en découle : un test qui écrit dans la configuration globale (`git config --global`, écriture du
/// fichier) ne partage jamais son binaire avec un test qui appelle cette fonction avec un contenu : selon l'ordre des
/// tests, celui-ci trouverait le fichier déjà écrit et échouerait. Aujourd'hui, les tests d'isolation de `ui_flows`,
/// de `repo_ops` et des tests unitaires de l'application écrivent une clé de vérification ; aucun test de contenu
/// n'est dans ces binaires. Un test de contenu qui écrit lui-même (identité, clé SSH) est seul dans son binaire, ou
/// avec des tests qui n'appellent que l'isolation par défaut.
pub fn isolate_global_git_config(initial: &str) -> Arc<GlobalGitConfig> {
    let mut current = CURRENT.lock().unwrap_or_else(PoisonError::into_inner);
    let config = match current.upgrade() {
        Some(config) => {
            if !initial.is_empty() {
                let content = std::fs::read_to_string(config.file()).unwrap_or_default();
                if content.is_empty() {
                    std::fs::write(config.file(), initial).unwrap();
                } else {
                    assert_eq!(
                        content, initial,
                        "configuration git globale de test déjà en vigueur avec un autre contenu (test concurrent du \
                         même binaire)"
                    );
                }
            }
            config
        }
        None => {
            let config = GlobalGitConfig { dir: tempfile::tempdir().unwrap() };
            std::fs::write(config.file(), initial).unwrap();
            // SAFETY : sous Windows, modifier l'environnement est sûr même avec d'autres threads (documentation de
            // `set_var`) ; les processus lancés le lisent au moment de leur création.
            unsafe {
                std::env::set_var("GIT_CONFIG_GLOBAL", config.file());
                std::env::set_var("HOME", config.dir.path());
                std::env::set_var("USERPROFILE", config.dir.path());
                std::env::set_var("XDG_CONFIG_HOME", config.dir.path());
            }
            let config = Arc::new(config);
            *current = Arc::downgrade(&config);
            config
        }
    };
    drop(current);
    HELD.with(|held| *held.borrow_mut() = Some(config.clone()));
    config
}

/// Isolation par défaut des helpers partagés : la configuration globale en vigueur, ou une nouvelle, vide ; le test
/// de ce thread la tient jusqu'à sa fin. Sans effet si ce thread en tient déjà une (tant qu'elle est tenue, c'est
/// celle que désigne l'environnement).
pub fn isolate_git_config_for_this_test() {
    if HELD.with(|held| held.borrow().is_none()) {
        isolate_global_git_config("");
    }
}

/// Fichier de configuration globale que désigne l'environnement, s'il est bien dans le dossier temporaire du système
/// et sert de `HOME` (vérification des tests d'isolation).
pub fn isolated_global_file() -> Option<PathBuf> {
    let file = PathBuf::from(std::env::var_os("GIT_CONFIG_GLOBAL")?);
    let home = PathBuf::from(std::env::var_os("HOME")?);
    (file.starts_with(std::env::temp_dir()) && file.parent() == Some(home.as_path())).then_some(file)
}

/// La ligne de `git config --show-origin` vient de `file` : git écrit le chemin avec des `/`, ou entre guillemets avec
/// des `\` doublés.
pub fn origin_is(line: &str, file: &std::path::Path) -> bool {
    let norm = |s: &str| s.replace("\\\\", "/").replace('\\', "/").to_lowercase();
    norm(line).contains(&norm(&file.display().to_string()))
}
