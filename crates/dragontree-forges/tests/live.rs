//! Tests contre les vrais services, ignorés par défaut :
//! `cargo test -p dragontree-forges -- --ignored`
//! Ils n'affichent jamais de jeton et ne font que des lectures.

use dragontree_forges::{Account, ForgeError, ForgeKind, list_repos, new_account_id, secrets, verify};

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
