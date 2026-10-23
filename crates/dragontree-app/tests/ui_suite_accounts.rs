//! Comptes de toute la suite EyrieSuite : un DragonTree d'avant la suite reprend ses comptes au démarrage, et ne les
//! écrit plus jamais dans ses réglages.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;

/// Un profil d'avant la suite : un compte GitHub dans `settings.json`, associé à un dépôt.
fn legacy_profile(base: &Path) -> PathBuf {
    let profile = base.join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("settings.json"),
        r#"{"auto_fetch_minutes": 0,
            "accounts": [{"id": "0123456789abcdef", "kind": "GitHub", "host": "https://GitHub.com/",
                          "username": "jsmith", "display_name": "Jean Smith", "orgs": [], "avatar_url": null}],
            "repo_accounts": {"c:/dev/northwind": "0123456789abcdef"}}"#,
    )
    .unwrap();
    profile
}

fn suite_accounts_in(dir: &Path) -> Vec<suite_accounts::Account> {
    suite_accounts::Store::new(dir, std::sync::Arc::new(suite_accounts::MemoryVault::default())).list()
}

#[test]
fn legacy_accounts_of_the_settings_file_move_to_the_suite() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = legacy_profile(tmp.path());

    let mut h = harness(start(tmp.path()));
    h.run_steps(3);

    let list = suite_accounts_in(&profile);
    assert_eq!(list.len(), 1, "le compte est passé dans les comptes de la suite");
    assert_eq!((list[0].id.as_str(), list[0].host.as_str()), ("0123456789abcdef", "github.com"));
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(profile.join("settings.json")).unwrap()).unwrap();
    assert!(saved.get("accounts").is_none(), "plus de comptes dans les réglages de DragonTree");
    assert_eq!(saved["repo_accounts"]["c:/dev/northwind"], "0123456789abcdef", "le dépôt garde son compte");
}

/// Un hôte (Eyrie) donne le dossier des comptes de la suite : il l'emporte sur le profil, sans variable
/// d'environnement que ses processus enfants hériteraient.
#[test]
fn the_host_chooses_the_accounts_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let profile = legacy_profile(tmp.path());
    let shared = tmp.path().join("commun");

    let mut h = harness(StartOptions { accounts_dir: Some(shared.clone()), ..start(tmp.path()) });
    h.run_steps(3);

    assert_eq!(suite_accounts_in(&shared).len(), 1, "dans le dossier donné par l'hôte");
    assert!(!profile.join("accounts.json").exists(), "rien dans le profil");
}
