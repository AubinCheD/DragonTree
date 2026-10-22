//! Comptes GitHub / GitLab / Azure DevOps de DragonTree : vérification des jetons, liste des
//! dépôts distants, jetons dans le Gestionnaire d'identifiants Windows et authentification
//! des processus git.
//!
//! Toutes les fonctions réseau sont bloquantes : à appeler hors du thread de l'interface.

mod auth;
mod azure;
mod github;
mod gitlab;
mod http;
mod location;
mod model;
mod open_pulls;
mod proxy;
mod pullrequest;
pub mod secrets;

use dragontree_i18n::tr;

pub use auth::{account_matches_url, git_auth_env, recommended_scopes, token_creation_url};
pub use location::RepoLocation;
pub use model::{Account, ForgeError, ForgeKind, RemoteRepo, VerifiedUser, new_account_id};
pub use open_pulls::{PullRequest, PullRequestList, list_pull_requests};
pub use pullrequest::{CreatedPullRequest, PullRequestDraft, create_pull_request, supports_reviewers};

/// Vérifie un jeton et renvoie l'identité associée. `orgs_hint` : organisations saisies
/// par l'utilisateur (Azure : indispensables pour un jeton limité à une organisation ou
/// pour Azure DevOps Server), ajoutées aux organisations découvertes, sans doublon.
///
/// Sert aussi à revérifier un compte enregistré : un ancien compte saisi en `http://…` reste appelé en HTTPS, comme
/// avant (voir [`verify_new`] pour un compte à ajouter).
pub fn verify(kind: ForgeKind, host: &str, token: &str, orgs_hint: &[String]) -> Result<VerifiedUser, ForgeError> {
    let token = clean_token(token)?;
    let host = model::normalize_host(host, kind);
    let mut user = match kind {
        ForgeKind::GitHub => github::verify(&host, token)?,
        ForgeKind::GitLab => gitlab::verify(&host, token)?,
        ForgeKind::AzureDevOps => azure::verify(&host, token, orgs_hint)?,
    };
    user.orgs = merge_orgs(user.orgs, orgs_hint);
    Ok(user)
}

/// Vérifie le jeton d'un compte à ajouter : comme [`verify`], après avoir refusé une adresse saisie en HTTP simple
/// (voir `reject_plain_http`).
pub fn verify_new(kind: ForgeKind, host: &str, token: &str, orgs_hint: &[String]) -> Result<VerifiedUser, ForgeError> {
    reject_plain_http(host)?;
    verify(kind, host, token, orgs_hint)
}

/// Dépôts visibles par le compte, du plus récemment actif au plus ancien, puis par nom.
pub fn list_repos(account: &Account, token: &str) -> Result<Vec<RemoteRepo>, ForgeError> {
    let token = clean_token(token)?;
    let host = model::normalize_host(&account.host, account.kind);
    let repos = match account.kind {
        ForgeKind::GitHub => github::list_repos(&host, token, &account.id)?,
        ForgeKind::GitLab => gitlab::list_repos(&host, token, &account.id)?,
        ForgeKind::AzureDevOps => azure::list_repos(&host, &account.orgs, token, &account.id)?,
    };
    Ok(model::finalize_repos(repos))
}

/// Adresse de serveur saisie en `http://` pour un compte à ajouter : refusée avec une explication, plutôt qu'appelée
/// en HTTPS (le schéma est retiré par `normalize_host`) avec une erreur TLS obscure. Un compte déjà enregistré ainsi
/// n'est pas refusé : il reste appelé en HTTPS, comme avant. Les forges ne sont appelées qu'en HTTPS : le jeton ne
/// circule jamais en clair, et les adresses de l'API, des pages web et de l'en-tête d'authentification de git sont
/// toutes construites en HTTPS (limite documentée dans le README).
fn reject_plain_http(host: &str) -> Result<(), ForgeError> {
    if host.trim().get(..7).is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://")) {
        return Err(ForgeError::InvalidInput(
            tr!(
                "serveur en HTTP simple : DragonTree ne joint les forges qu'en HTTPS, pour que le jeton ne circule jamais en clair. Indiquez l'adresse HTTPS du serveur."
            )
            .into(),
        ));
    }
    Ok(())
}

/// Jeton sans espaces ni retour à la ligne (copier-coller), jamais vide.
fn clean_token(token: &str) -> Result<&str, ForgeError> {
    let token = token.trim();
    if token.is_empty() { Err(ForgeError::InvalidInput(tr!("le jeton est vide").into())) } else { Ok(token) }
}

/// Organisations découvertes puis indiquées, sans doublon (casse ignorée), dans l'ordre.
fn merge_orgs(found: Vec<String>, hint: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(found.len() + hint.len());
    for org in found.iter().chain(hint) {
        let org = org.trim();
        if !org.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(org)) {
            out.push(org.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_orgs_dedupes_case_insensitively_and_keeps_order() {
        let found = vec!["fabrikam".to_string(), "Contoso".to_string()];
        let hint = [" contoso ".to_string(), "northwind".to_string(), String::new()];
        assert_eq!(merge_orgs(found, &hint), ["fabrikam", "Contoso", "northwind"]);
    }

    #[test]
    fn empty_token_is_rejected_without_network() {
        let err = verify(ForgeKind::GitHub, "github.com", "  \n", &[]).unwrap_err();
        assert!(matches!(err, ForgeError::InvalidInput(_)), "{err:?}");
        let account = Account {
            id: "0123456789abcdef".into(),
            kind: ForgeKind::GitLab,
            host: "gitlab.com".into(),
            username: "jsmith".into(),
            display_name: "Jean Smith".into(),
            orgs: vec![],
            avatar_url: None,
        };
        assert!(matches!(list_repos(&account, ""), Err(ForgeError::InvalidInput(_))));
    }

    /// RV4d : une instance en HTTP simple est refusée avec une explication à l'ajout du compte, sans requête (HTTPS
    /// seulement).
    #[test]
    fn a_plain_http_server_is_rejected_without_network() {
        for kind in [ForgeKind::GitLab, ForgeKind::GitHub, ForgeKind::AzureDevOps] {
            match verify_new(kind, " HTTP://gitlab.contoso.test:8080/ ", "jeton", &["DefaultCollection".into()]) {
                Err(ForgeError::InvalidInput(msg)) => assert!(msg.contains("HTTPS"), "{msg}"),
                other => panic!("{kind:?} : {other:?}"),
            }
        }
    }

    /// Un compte déjà enregistré en `http://…` (revérifié par « Vérifier ») n'est pas refusé : il reste appelé en
    /// HTTPS, comme avant. Serveur local qui ferme aussitôt : la poignée de main TLS échoue (erreur réseau).
    #[test]
    fn a_saved_plain_http_account_is_still_checked_over_https() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                drop(stream);
            }
        });
        match verify(ForgeKind::GitLab, &format!("http://127.0.0.1:{port}"), "jeton", &[]) {
            Err(ForgeError::Network(_)) => {}
            other => panic!("appel HTTPS attendu (erreur réseau) : {other:?}"),
        }
    }
}
