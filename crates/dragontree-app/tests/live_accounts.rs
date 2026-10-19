//! Tests réels (ignorés par défaut) avec les comptes de l'utilisateur.
//! `AZURE_DEVOPS_EXT_PAT=<pat> DRAGONTREE_AZURE_ORG=<org> DRAGONTREE_AZURE_REPO=<nom> cargo test -p dragontree-app --test live_accounts -- --ignored --nocapture`

use dragontree_forges::{Account, ForgeKind};
use dragontree_git::{Git, NetOptions, Repo, clone_repo};

#[test]
#[ignore]
fn azure_clone_with_injected_auth_only() {
    let (Ok(pat), Ok(org)) = (std::env::var("AZURE_DEVOPS_EXT_PAT"), std::env::var("DRAGONTREE_AZURE_ORG")) else {
        eprintln!("variables absentes : test sauté");
        return;
    };
    let wanted = std::env::var("DRAGONTREE_AZURE_REPO").unwrap_or_else(|_| "AppConfigUtil".into());
    let user = dragontree_forges::verify(ForgeKind::AzureDevOps, "dev.azure.com", &pat, std::slice::from_ref(&org))
        .expect("vérification du compte");
    let account = Account {
        id: "test".into(),
        kind: ForgeKind::AzureDevOps,
        host: "dev.azure.com".into(),
        username: user.username,
        display_name: user.display_name,
        orgs: user.orgs,
        avatar_url: None,
    };
    let repos = dragontree_forges::list_repos(&account, &pat).expect("liste des dépôts");
    let repo =
        repos.iter().find(|r| r.name == wanted).unwrap_or_else(|| panic!("{wanted} absent ({} dépôts)", repos.len()));
    println!("{} dépôts visibles ; clonage de {}", repos.len(), repo.full_name());

    // En-tête du compte uniquement : aucun gestionnaire d'identifiants en secours.
    let mut env = dragontree_forges::git_auth_env(&account, &pat);
    let n: usize = env.iter().find(|(k, _)| k == "GIT_CONFIG_COUNT").map(|(_, v)| v.parse().unwrap()).unwrap();
    env.retain(|(k, _)| k != "GIT_CONFIG_COUNT");
    env.push((format!("GIT_CONFIG_KEY_{n}"), "credential.helper".into()));
    env.push((format!("GIT_CONFIG_VALUE_{n}"), String::new()));
    env.push(("GIT_CONFIG_COUNT".into(), (n + 1).to_string()));

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("clone");
    let git = Git::discover().unwrap();
    let mut phases = 0;
    clone_repo(
        &git,
        &repo.clone_url,
        &dest,
        None,
        false,
        &NetOptions { env, non_interactive: true, ..Default::default() },
        &mut |_| phases += 1,
    )
    .expect("clonage authentifié par l'en-tête du compte");
    let opened = Repo::open(&git, &dest).unwrap();
    let remote = opened.remotes().unwrap();
    assert!(remote[0].fetch_url.contains("dev.azure.com") || remote[0].fetch_url.contains("visualstudio.com"));
    assert!(!remote[0].fetch_url.contains(&pat), "le jeton ne doit jamais être écrit dans l'URL");
    let config = std::fs::read_to_string(dest.join(".git/config")).unwrap();
    assert!(
        !config.contains(&pat) && !config.to_lowercase().contains("extraheader"),
        "rien n'est persisté dans .git/config"
    );
    println!("clone OK ({} évènements de progression)", phases);
}
