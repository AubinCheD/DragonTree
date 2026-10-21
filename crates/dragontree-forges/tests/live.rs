//! Tests contre les vrais services, ignorés par défaut :
//! `cargo test -p dragontree-forges -- --ignored`
//! Ils n'affichent jamais de jeton et ne font que des lectures.

use dragontree_forges::{
    Account, ForgeError, ForgeKind, PullRequestDraft, RepoLocation, create_pull_request, list_pull_requests,
    list_repos, new_account_id, secrets, verify,
};

/// Vérifie un vrai jeton Azure DevOps (variable `AZURE_DEVOPS_EXT_PAT`) puis liste les
/// dépôts de ses organisations. N'affiche que des nombres.
///
/// Un jeton limité à une organisation n'a pas accès au profil global : `verify` sans
/// organisation renvoie alors `Unauthorized`. `DRAGONTREE_AZURE_ORGS` (liste séparée par des
/// virgules) sert d'indication pour le repli `connectionData`.
#[test]
#[ignore = "réseau : lit AZURE_DEVOPS_EXT_PAT (et DRAGONTREE_AZURE_ORGS si présent)"]
fn azure_live_verify_and_list() {
    let pat = match std::env::var("AZURE_DEVOPS_EXT_PAT") {
        Ok(pat) if !pat.trim().is_empty() => pat,
        _ => {
            eprintln!("AZURE_DEVOPS_EXT_PAT absent : rien à vérifier");
            return;
        }
    };
    let hint: Vec<String> = std::env::var("DRAGONTREE_AZURE_ORGS")
        .unwrap_or_default()
        .split(',')
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect();
    let user = match verify(ForgeKind::AzureDevOps, "dev.azure.com", &pat, &[]) {
        Ok(user) => {
            println!("vérification : profil global");
            user
        }
        Err(ForgeError::Unauthorized) if !hint.is_empty() => {
            println!(
                "vérification : profil global refusé, repli connectionData ({} organisation(s) indiquée(s))",
                hint.len()
            );
            verify(ForgeKind::AzureDevOps, "dev.azure.com", &pat, &hint)
                .unwrap_or_else(|e| panic!("verify (repli) : {e}"))
        }
        Err(e) => {
            panic!("verify : {e} (jeton limité à une organisation ? définir DRAGONTREE_AZURE_ORGS)")
        }
    };
    assert!(!user.username.is_empty());
    let account = Account {
        id: new_account_id(),
        kind: ForgeKind::AzureDevOps,
        host: "dev.azure.com".into(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        orgs: user.orgs.clone(),
        avatar_url: user.avatar_url.clone(),
    };
    let started = std::time::Instant::now();
    let repos = list_repos(&account, &pat).unwrap_or_else(|e| panic!("list_repos : {e}"));
    println!("organisations : {}", user.orgs.len());
    println!("dépôts : {}", repos.len());
    println!("dépôts publics : {}", repos.iter().filter(|r| !r.private).count());
    println!("durée de list_repos : {} ms", started.elapsed().as_millis());
}

/// Un jeton invalide doit donner `Unauthorized` sur chaque forge (TLS natif, statuts
/// renvoyés comme données, page de connexion Azure en 203).
#[test]
#[ignore = "réseau : appelle api.github.com, gitlab.com et Azure DevOps"]
fn live_invalid_tokens_are_unauthorized() {
    for kind in ForgeKind::ALL {
        let result = verify(kind, kind.default_host(), "jeton-invalide-dragontree", &[]);
        assert!(matches!(result, Err(ForgeError::Unauthorized)), "{}: {result:?}", kind.label());
    }
}

/// Aller-retour dans le Gestionnaire d'identifiants Windows avec un identifiant jetable,
/// toujours supprimé à la fin.
#[test]
#[ignore = "écrit puis supprime une entrée du Gestionnaire d'identifiants Windows"]
fn secrets_round_trip_windows_credential_manager() {
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = secrets::delete_token(&self.0);
        }
    }

    let id = format!("test-{}", new_account_id());
    let _cleanup = Cleanup(id.clone());

    assert_eq!(secrets::load_token(&id).unwrap(), None);
    secrets::store_token(&id, "jeton-de-test").unwrap();
    assert_eq!(secrets::load_token(&id).unwrap().as_deref(), Some("jeton-de-test"));
    secrets::store_token(&id, "jeton-remplacé-é").unwrap();
    assert_eq!(secrets::load_token(&id).unwrap().as_deref(), Some("jeton-remplacé-é"));
    secrets::delete_token(&id).unwrap();
    assert_eq!(secrets::load_token(&id).unwrap(), None);
    secrets::delete_token(&id).unwrap();
}

/// Un jeton invalide doit donner `Unauthorized` à la création d'une pull request (GitHub, GitLab, Azure DevOps).
#[test]
#[ignore = "réseau : appelle api.github.com, gitlab.com et dev.azure.com"]
fn live_pull_request_with_an_invalid_token_is_unauthorized() {
    let draft = PullRequestDraft {
        source_branch: "feature/essai".into(),
        target_branch: "main".into(),
        title: "Essai DragonTree".into(),
        ..Default::default()
    };
    for (kind, url) in [
        (ForgeKind::GitHub, "https://github.com/contoso/northwind.git"),
        (ForgeKind::GitLab, "https://gitlab.com/fabrikam/outils.git"),
        (ForgeKind::AzureDevOps, "https://dev.azure.com/fabrikam/Projet/_git/api"),
    ] {
        let account = Account {
            id: new_account_id(),
            kind,
            host: kind.default_host().into(),
            username: "essai".into(),
            display_name: String::new(),
            orgs: vec![],
            avatar_url: None,
        };
        let location = RepoLocation::parse(url, Some(&account)).expect("URL analysable");
        let result = create_pull_request(&account, "jeton-invalide-dragontree", &location, &draft);
        assert!(matches!(result, Err(ForgeError::Unauthorized)), "{}: {result:?}", kind.label());
    }
}

/// Crée une vraie pull request (brouillon) : `DRAGONTREE_LIVE_PR_REMOTE` (URL du remote), `DRAGONTREE_LIVE_PR_TOKEN`,
/// `DRAGONTREE_LIVE_PR_SOURCE` (branche déjà poussée) et `DRAGONTREE_LIVE_PR_TARGET`. À fermer à la main ensuite.
/// N'affiche jamais le jeton, seulement le numéro et l'adresse de la pull request.
#[test]
#[ignore = "réseau : crée une vraie pull request (brouillon) avec les variables DRAGONTREE_LIVE_PR_*"]
fn live_create_a_draft_pull_request() {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let (Some(remote), Some(token), Some(source), Some(target)) = (
        var("DRAGONTREE_LIVE_PR_REMOTE"),
        var("DRAGONTREE_LIVE_PR_TOKEN"),
        var("DRAGONTREE_LIVE_PR_SOURCE"),
        var("DRAGONTREE_LIVE_PR_TARGET"),
    ) else {
        eprintln!("variables DRAGONTREE_LIVE_PR_* absentes : rien à créer");
        return;
    };
    let location = RepoLocation::parse(&remote, None).expect("forge non reconnue d'après l'URL");
    let account = Account {
        id: new_account_id(),
        kind: location.kind(),
        host: location.host().into(),
        username: String::new(),
        display_name: String::new(),
        orgs: vec![],
        avatar_url: None,
    };
    let draft = PullRequestDraft {
        source_branch: source,
        target_branch: target,
        title: "Essai DragonTree (à fermer)".into(),
        description: "Créée par le test réel de DragonTree.".into(),
        draft: true,
        reviewers: vec![],
    };
    let created = create_pull_request(&account, &token, &location, &draft)
        .unwrap_or_else(|e| panic!("create_pull_request : {e}"));
    println!("pull request n° {} : {}", created.number, created.url);
}

/// Un jeton invalide doit donner `Unauthorized` à la lecture des pull requests ouvertes (GitHub GraphQL, GitLab, Azure
/// DevOps `connectionData`).
#[test]
#[ignore = "réseau : appelle api.github.com, gitlab.com et dev.azure.com"]
fn live_pull_requests_with_an_invalid_token_are_unauthorized() {
    for kind in ForgeKind::ALL {
        let account = Account {
            id: new_account_id(),
            kind,
            host: kind.default_host().into(),
            username: "essai".into(),
            display_name: String::new(),
            orgs: vec!["fabrikam".into()],
            avatar_url: None,
        };
        let result = list_pull_requests(&account, "jeton-invalide-dragontree");
        assert!(matches!(result, Err(ForgeError::Unauthorized)), "{}: {result:?}", kind.label());
    }
}

/// Lit les vraies pull requests ouvertes d'un compte : `DRAGONTREE_LIVE_PRS_KIND` (`github`, `gitlab` ou `azure`),
/// `DRAGONTREE_LIVE_PRS_TOKEN`, et facultativement `DRAGONTREE_LIVE_PRS_HOST` et `DRAGONTREE_LIVE_PRS_ORGS` (Azure,
/// séparées par des virgules). N'affiche que des nombres, jamais le jeton ni les titres.
#[test]
#[ignore = "réseau : lit les pull requests d'un vrai compte avec les variables DRAGONTREE_LIVE_PRS_*"]
fn live_list_open_pull_requests() {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let (Some(kind), Some(token)) = (var("DRAGONTREE_LIVE_PRS_KIND"), var("DRAGONTREE_LIVE_PRS_TOKEN")) else {
        eprintln!("variables DRAGONTREE_LIVE_PRS_* absentes : rien à lire");
        return;
    };
    let kind = match kind.to_ascii_lowercase().as_str() {
        "github" => ForgeKind::GitHub,
        "gitlab" => ForgeKind::GitLab,
        "azure" => ForgeKind::AzureDevOps,
        other => panic!("DRAGONTREE_LIVE_PRS_KIND inconnu : {other}"),
    };
    let host = var("DRAGONTREE_LIVE_PRS_HOST").unwrap_or_else(|| kind.default_host().to_string());
    let hint: Vec<String> = var("DRAGONTREE_LIVE_PRS_ORGS")
        .unwrap_or_default()
        .split(',')
        .map(|o| o.trim().to_string())
        .filter(|o| !o.is_empty())
        .collect();
    let user = verify(kind, &host, &token, &hint).unwrap_or_else(|e| panic!("verify : {e}"));
    let account = Account {
        id: new_account_id(),
        kind,
        host,
        username: user.username,
        display_name: user.display_name,
        orgs: user.orgs,
        avatar_url: user.avatar_url,
    };
    let started = std::time::Instant::now();
    let prs = list_pull_requests(&account, &token).unwrap_or_else(|e| panic!("list_pull_requests : {e}"));
    println!("pull requests ouvertes : {}", prs.len());
    println!("créées par l'utilisateur : {}", prs.iter().filter(|pr| pr.created_by_me).count());
    println!("relecture demandée : {}", prs.iter().filter(|pr| pr.review_requested).count());
    println!("brouillons : {}", prs.iter().filter(|pr| pr.draft).count());
    println!("avec branches : {}", prs.iter().filter(|pr| pr.source_branch.is_some()).count());
    println!("durée : {} ms", started.elapsed().as_millis());
    assert!(prs.iter().all(|pr| !pr.url.is_empty() && pr.account_id == account.id));
}
