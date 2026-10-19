//! Configuration locale du dépôt : lecture, écriture, suppression d'une clé `--local`, valeur globale et modèle
//! de message de commit.

mod common;

use std::sync::Arc;

use common::{GlobalGitConfig, TestRepo, isolate_global_git_config};
use dragontree_git::Repo;
use dragontree_git::ssh::ConfigScope;

/// Dépôt de test, avec une configuration globale temporaire (jamais ~/.gitconfig) que le test tient jusqu'à sa fin.
fn repo() -> (Arc<GlobalGitConfig>, TestRepo, Repo) {
    let global = isolate_global_git_config("[user]\n\tname = Globale\n\temail = globale@fabrikam.test\n");
    let t = TestRepo::new();
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (global, t, repo)
}

#[test]
fn a_local_key_is_written_read_then_removed() {
    let (_global, _t, repo) = repo();
    repo.set_config_local("user.name", "Alice Contoso").unwrap();
    assert_eq!(repo.config_in(ConfigScope::Local, "user.name").unwrap().as_deref(), Some("Alice Contoso"));
    // La valeur globale reste lisible à part : c'est elle qu'on hérite une fois la clé locale retirée.
    assert_eq!(repo.config_in(ConfigScope::Global, "user.name").unwrap().as_deref(), Some("Globale"));
    repo.unset_config_local("user.name").unwrap();
    assert_eq!(repo.config_in(ConfigScope::Local, "user.name").unwrap(), None);
    assert_eq!(repo.config("user.name").as_deref(), Some("Globale"));
    // Retirer une clé absente n'est pas une erreur.
    repo.unset_config_local("user.name").unwrap();
}

#[test]
fn values_with_spaces_and_accents_round_trip() {
    let (_global, _t, repo) = repo();
    repo.set_config_local("user.name", "Zoé  Northwind").unwrap();
    assert_eq!(repo.config_in(ConfigScope::Local, "user.name").unwrap().as_deref(), Some("Zoé  Northwind"));
}

#[test]
fn the_commit_template_is_read_from_the_configured_file() {
    let (_global, t, repo) = repo();
    assert_eq!(repo.commit_template(), None);
    t.write("modele.txt", "feat: \n\nRefs #\n");
    // Chemin relatif : lu depuis la racine du dépôt.
    repo.set_config_local("commit.template", "modele.txt").unwrap();
    assert_eq!(repo.commit_template().as_deref(), Some("feat: \n\nRefs #\n"));
    // Chemin absolu.
    let absolute = t.file("modele.txt").display().to_string().replace('\\', "/");
    repo.set_config_local("commit.template", &absolute).unwrap();
    assert_eq!(repo.commit_template().as_deref(), Some("feat: \n\nRefs #\n"));
    // Fichier absent ou vide : pas de modèle.
    repo.set_config_local("commit.template", "absent.txt").unwrap();
    assert_eq!(repo.commit_template(), None);
    t.write("vide.txt", "");
    repo.set_config_local("commit.template", "vide.txt").unwrap();
    assert_eq!(repo.commit_template(), None);
}

#[test]
fn an_unreadable_configuration_is_an_error_not_an_absent_key() {
    let (_global, t, repo) = repo();
    // Clé absente : `Ok(None)`.
    assert_eq!(repo.config_in(ConfigScope::Local, "user.signingkey").unwrap(), None);
    // Configuration du dépôt invalide : git échoue (code 128), ce n'est pas « absent ».
    std::fs::write(t.path().join(".git").join("config"), "[user\n\tname = Zoé\n").unwrap();
    assert!(repo.config_in(ConfigScope::Local, "user.name").is_err());
}
