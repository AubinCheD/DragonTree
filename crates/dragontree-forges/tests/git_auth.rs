//! RV4d : l'en-tête d'authentification d'un compte atteint-il git pour l'URL d'un remote ? Vérifié avec git lui-même
//! (`git config --get-urlmatch`), sans réseau : git compare le chemin des URL en respectant la casse, alors que
//! l'association d'un compte à un remote l'ignore.

use std::path::Path;
use std::process::Command;

use dragontree_forges::{Account, ForgeKind, account_matches_url, git_auth_env};

/// En-tête que git utiliserait pour `url` avec les variables `env` ; configuration de la machine écartée (système
/// ignoré, configuration globale vide, dossier qui n'est pas un dépôt).
fn header_for(dir: &Path, env: &[(String, String)], url: &str) -> Option<String> {
    let global = dir.join("gitconfig");
    std::fs::write(&global, "").unwrap();
    let out = Command::new("git")
        .args(["config", "--get-urlmatch", "http.extraheader", url])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &global)
        .env("HOME", dir)
        .env("USERPROFILE", dir)
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .output()
        .expect("git lancé");
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn azure(host: &str, orgs: &[&str]) -> Account {
    Account {
        id: "0123456789abcdef".into(),
        kind: ForgeKind::AzureDevOps,
        host: host.into(),
        username: "camille@contoso.test".into(),
        display_name: "Camille".into(),
        orgs: orgs.iter().map(|o| o.to_string()).collect(),
        avatar_url: None,
    }
}

#[test]
fn the_azure_header_reaches_a_remote_whose_organisation_is_written_with_another_case() {
    let dir = tempfile::tempdir().unwrap();
    let account = azure("dev.azure.com", &["contoso"]);
    let remote = "https://Contoso@dev.azure.com/Contoso/Site%20web/_git/Site";
    assert!(account_matches_url(&account, remote), "le compte est associé au remote (casse ignorée)");
    // L'origine du compte seule (`https://dev.azure.com/contoso/`) ne couvre pas ce remote pour git.
    assert_eq!(header_for(dir.path(), &git_auth_env(&account, "azpat", &[]), remote), None, "casse du chemin");
    let env = git_auth_env(&account, "azpat", &[remote]);
    // base64(":azpat")
    assert_eq!(header_for(dir.path(), &env, remote).as_deref(), Some("Authorization: Basic OmF6cGF0"));
    assert_eq!(
        header_for(dir.path(), &env, "https://dev.azure.com/contoso/Site/_git/Site").as_deref(),
        Some("Authorization: Basic OmF6cGF0"),
        "casse du compte"
    );
    assert_eq!(header_for(dir.path(), &env, "https://dev.azure.com/northwind/X/_git/Y"), None, "autre organisation");
}
