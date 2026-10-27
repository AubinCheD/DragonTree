//! Clés SSH : génération, liste, empreinte, refus d'écraser, et `core.sshCommand` (config globale temporaire).

mod common;

use std::path::{Path, PathBuf};

use dragontree_git::GitError;
use dragontree_git::ssh::{self, ConfigScope, KeyType};

fn keygen() -> PathBuf {
    ssh::find_ssh_keygen(common::git().exe())
}

#[test]
fn generates_lists_and_fingerprints_a_key_without_overwriting() {
    let tmp = tempfile::tempdir().unwrap();
    // Dossier .ssh inexistant : il est créé.
    let dir = tmp.path().join("sous").join(".ssh");
    let keygen = keygen();
    assert!(ssh::list_keys(&dir, &keygen).unwrap().is_empty());

    let key = ssh::generate_key(&dir, &keygen, "id_ed25519", KeyType::Ed25519, "jean@fabrikam.test", "").unwrap();
    assert_eq!((key.name.as_str(), key.key_type.as_str(), key.bits), ("id_ed25519", "ED25519", 256));
    assert_eq!(key.comment, "jean@fabrikam.test");
    assert!(key.fingerprint.starts_with("SHA256:"), "{}", key.fingerprint);
    assert!(key.public_key.starts_with("ssh-ed25519 ") && key.public_key.ends_with("jean@fabrikam.test"));

    // Une clé avec phrase secrète, et une clé publique orpheline (ignorée).
    let locked = ssh::generate_key(&dir, &keygen, "travail", KeyType::Ed25519, "w", "secret phrase").unwrap();
    let unlock = |phrase: &str| {
        std::process::Command::new(&keygen)
            .args(["-y", "-P", phrase, "-f"])
            .arg(&locked.private_path)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
            .status
            .success()
    };
    assert!(unlock("secret phrase"), "la clé s'ouvre avec sa phrase secrète");
    assert!(!unlock(""), "la clé est protégée par sa phrase secrète");
    std::fs::write(dir.join("orpheline.pub"), "ssh-ed25519 AAAA x").unwrap();
    let keys = ssh::list_keys(&dir, &keygen).unwrap();
    assert_eq!(keys.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["id_ed25519", "travail"]);
    assert_eq!(keys[1].fingerprint, locked.fingerprint);

    // Refus d'écraser : la clé existante ne change pas.
    let before = std::fs::read(&key.private_path).unwrap();
    let err = ssh::generate_key(&dir, &keygen, "id_ed25519", KeyType::Ed25519, "autre", "").unwrap_err();
    assert!(matches!(err, GitError::Refused(_)), "{err:?}");
    assert_eq!(std::fs::read(&key.private_path).unwrap(), before);
    assert_eq!(ssh::list_keys(&dir, &keygen).unwrap()[0].comment, "jean@fabrikam.test");

    // Un nom qui sortirait du dossier est refusé.
    let err = ssh::generate_key(&dir, &keygen, "../evade", KeyType::Ed25519, "", "").unwrap_err();
    assert!(matches!(err, GitError::Refused(_)), "{err:?}");
    assert!(!tmp.path().join("sous").join("evade").exists());
}

#[test]
fn generates_rsa_4096() {
    let tmp = tempfile::tempdir().unwrap();
    let key = ssh::generate_key(tmp.path(), &keygen(), "id_rsa", KeyType::Rsa4096, "rsa", "").unwrap();
    assert_eq!((key.key_type.as_str(), key.bits), ("RSA", 4096));
}

/// La configuration globale de test est un état du processus (`isolate_global_git_config`) : ce test est le seul de
/// ce fichier à la positionner, et les autres n'exécutent pas git config.
#[test]
fn core_ssh_command_is_written_then_removed_in_the_global_and_local_config() {
    let global_config = common::isolate_global_git_config("[user]\n\temail = jean@fabrikam.test\n");
    let global = global_config.file();

    let git = common::git();
    let repo = common::TestRepo::new();
    let key = Path::new(r"C:\Users\Contoso User\.ssh\id_ed25519");
    let wanted = r#"ssh -i "C:/Users/Contoso User/.ssh/id_ed25519" -o IdentitiesOnly=yes"#;

    assert_eq!(ssh::global_email(&git, repo.path()).as_deref(), Some("jean@fabrikam.test"));
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap(), None);

    ssh::use_key(&git, ConfigScope::Global, repo.path(), key, false).unwrap();
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap().as_deref(), Some(wanted));
    assert!(std::fs::read_to_string(&global).unwrap().contains("sshCommand"), "écrit dans le fichier temporaire");
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Local, repo.path()).unwrap(), None);

    // Local : écrit dans le dépôt, pas dans la config globale.
    ssh::use_key(&git, ConfigScope::Local, repo.path(), Path::new("D:/clés/deploy"), false).unwrap();
    let local = ssh::configured_ssh_command(&git, ConfigScope::Local, repo.path()).unwrap().unwrap();
    assert_eq!(ssh::key_from_ssh_command(&local).as_deref(), Some("D:/clés/deploy"));
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap().as_deref(), Some(wanted));

    ssh::reset_key(&git, ConfigScope::Local, repo.path(), false).unwrap();
    ssh::reset_key(&git, ConfigScope::Global, repo.path(), false).unwrap();
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Local, repo.path()).unwrap(), None);
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap(), None);
    // Retirer ce qui n'existe plus n'est pas une erreur.
    ssh::reset_key(&git, ConfigScope::Global, repo.path(), false).unwrap();
    assert!(!std::fs::read_to_string(&global).unwrap().contains("sshCommand"));

    // Une commande écrite à la main survit au retour par défaut et à « utiliser », sauf confirmation.
    let foreign = "ssh -o StrictHostKeyChecking=no";
    git.cmd(repo.path()).args(["config", "--global", "core.sshCommand", foreign]).run().unwrap();
    assert_eq!(ssh::foreign_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap().as_deref(), Some(foreign));
    let err = ssh::reset_key(&git, ConfigScope::Global, repo.path(), false).unwrap_err();
    assert!(matches!(&err, GitError::Refused(m) if m.contains("StrictHostKeyChecking")), "{err:?}");
    assert!(ssh::use_key(&git, ConfigScope::Global, repo.path(), key, false).is_err());
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap().as_deref(), Some(foreign));
    ssh::use_key(&git, ConfigScope::Global, repo.path(), key, true).unwrap();
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap().as_deref(), Some(wanted));
    git.cmd(repo.path()).args(["config", "--global", "core.sshCommand", foreign]).run().unwrap();
    ssh::reset_key(&git, ConfigScope::Global, repo.path(), true).unwrap();
    assert_eq!(ssh::configured_ssh_command(&git, ConfigScope::Global, repo.path()).unwrap(), None);
}
