//! API REST 7.1 d'Azure DevOps Services (dev.azure.com) et d'Azure DevOps Server.

use std::fmt::Write as _;

use dragontree_i18n::tr;
use serde::Deserialize;

use crate::auth::basic_auth_value;
use crate::http::{self, parse_error};
use crate::model::{ForgeError, RemoteRepo, VerifiedUser};

/// Hôte canonique d'Azure DevOps Services.
const SERVICES_HOST: &str = "dev.azure.com";
/// Profil de l'utilisateur du jeton (hors organisation).
const PROFILE_URL: &str = "https://app.vssps.visualstudio.com/_apis/profile/profiles/me?api-version=7.1";
/// Identité renvoyée par `connectionData` à un appel anonyme.
const ANONYMOUS_ID: &str = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

/// Hôte d'Azure DevOps Services (sinon : Azure DevOps Server).
pub(crate) fn is_services_host(host: &str) -> bool {
    host == SERVICES_HOST || host.ends_with(".visualstudio.com")
}

/// Vérifie le jeton. Services : profil + comptes (repli `connectionData` par organisation
/// indiquée si le jeton est limité à une organisation). Server : `connectionData` par
/// collection indiquée.
pub(crate) fn verify(host: &str, token: &str, orgs_hint: &[String]) -> Result<VerifiedUser, ForgeError> {
    let hint = clean_orgs(orgs_hint);
    if !is_services_host(host) {
        if hint.is_empty() {
            return Err(ForgeError::InvalidInput(
                tr!("Azure DevOps Server : indiquez au moins une collection (par exemple DefaultCollection)").into(),
            ));
        }
        return verify_with_orgs(host, token, &hint);
    }

    let profile = match get(PROFILE_URL, token) {
        Ok(body) => parse_profile(&body)?,
        // Jeton limité à une organisation : le profil global lui est refusé.
        Err(ForgeError::Unauthorized) if !hint.is_empty() => {
            return verify_with_orgs(host, token, &hint);
        }
        Err(e) => return Err(e),
    };
    let accounts_url = format!(
        "https://app.vssps.visualstudio.com/_apis/accounts?memberId={}&api-version=7.1",
        encode_path(&profile.member_id)
    );
    let orgs = match get(&accounts_url, token) {
        Ok(body) => parse_accounts(&body)?,
        Err(ForgeError::Unauthorized) if !hint.is_empty() => Vec::new(),
        Err(e) => return Err(e),
    };
    Ok(VerifiedUser { username: profile.username, display_name: profile.display_name, avatar_url: None, orgs })
}

/// Identité via `connectionData` de la première organisation qui répond.
fn verify_with_orgs(host: &str, token: &str, orgs: &[&str]) -> Result<VerifiedUser, ForgeError> {
    first_success(orgs.iter().map(|org| {
        let url = format!("{}/_apis/connectionData", org_base(host, org));
        get(&url, token).and_then(|body| parse_connection_data(&body))
    }))
}

/// Dépôts de chaque organisation (voir `merge_org_results`).
pub(crate) fn list_repos(
    host: &str,
    orgs: &[String],
    token: &str,
    account_id: &str,
) -> Result<Vec<RemoteRepo>, ForgeError> {
    merge_org_results(clean_orgs(orgs).into_iter().map(|org| {
        get(&repositories_url(host, org), token).and_then(|body| parse_repositories(&body, org, account_id))
    }))
}

/// Premier succès d'une suite d'essais évalués à la demande, sinon la première erreur.
fn first_success<T>(attempts: impl IntoIterator<Item = Result<T, ForgeError>>) -> Result<T, ForgeError> {
    let mut first_error = None;
    for attempt in attempts {
        match attempt {
            Ok(value) => return Ok(value),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    Err(first_error.unwrap_or_else(|| ForgeError::InvalidInput(tr!("aucune organisation indiquée").into())))
}

/// Résultats (dépôts, pull requests) de toutes les organisations : les échecs sont ignorés si au moins une
/// organisation répond ; si toutes échouent, la première erreur est renvoyée.
pub(crate) fn merge_org_results<T>(
    results: impl IntoIterator<Item = Result<Vec<T>, ForgeError>>,
) -> Result<Vec<T>, ForgeError> {
    let mut all = Vec::new();
    let mut first_error = None;
    let mut any_ok = false;
    for result in results {
        match result {
            Ok(found) => {
                any_ok = true;
                all.extend(found);
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    match first_error {
        Some(e) if !any_ok => Err(e),
        _ => Ok(all),
    }
}

/// GET authentifié ; renvoie le corps JSON.
pub(crate) fn get(url: &str, token: &str) -> Result<String, ForgeError> {
    let authorization = basic_auth_value("", token);
    let response = http::get(
        url,
        &[
            ("Authorization", &authorization),
            ("Accept", "application/json"),
            // Demande un 401 plutôt qu'une redirection vers la page de connexion.
            ("X-TFS-FedAuthRedirect", "Suppress"),
        ],
    )?;
    check_response(response.status, &response.body)?;
    Ok(response.body)
}

/// Statut 203 (page de connexion), 401/403 ou corps non JSON → `Unauthorized` ;
/// autre statut non 2xx → `Http`.
pub(crate) fn check_response(status: u16, body: &str) -> Result<(), ForgeError> {
    match status {
        // Jeton invalide ou expiré : Azure renvoie une page de connexion HTML en 203.
        203 | 401 | 403 => Err(ForgeError::Unauthorized),
        200..=299 => {
            let start = body.trim_start_matches('\u{feff}').trim_start();
            if start.starts_with('{') || start.starts_with('[') { Ok(()) } else { Err(ForgeError::Unauthorized) }
        }
        _ => Err(http::http_error(status, body)),
    }
}

/// `https://dev.azure.com/<org>` (Services) ou `https://<hôte>/<collection>` (Server).
pub(crate) fn org_base(host: &str, org: &str) -> String {
    format!("https://{}/{}", web_host(host), encode_path(org))
}

/// Version de l'API : Azure DevOps Server 2019 plafonne à la 5.0.
pub(crate) fn api_version(host: &str) -> &'static str {
    if is_services_host(host) { "7.1" } else { "5.0" }
}

/// Hôte web canonique : `dev.azure.com` pour Azure DevOps Services (`<org>.visualstudio.com` compris), l'hôte du
/// serveur sinon.
pub(crate) fn web_host(host: &str) -> &str {
    if is_services_host(host) { SERVICES_HOST } else { host }
}

fn repositories_url(host: &str, org: &str) -> String {
    format!("{}/_apis/git/repositories?api-version={}", org_base(host, org), api_version(host))
}

pub(crate) fn clean_orgs(orgs: &[String]) -> Vec<&str> {
    orgs.iter().map(|o| o.trim().trim_matches('/')).filter(|o| !o.is_empty()).collect()
}

/// Encode chaque segment d'un chemin pour une URL (les `/` sont conservés).
pub(crate) fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[derive(Deserialize)]
struct AzList<T> {
    value: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzProfile {
    display_name: Option<String>,
    public_alias: Option<String>,
    id: Option<String>,
    email_address: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzAccount {
    account_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzConnectionData {
    authenticated_user: Option<AzIdentity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzIdentity {
    id: Option<String>,
    descriptor: Option<String>,
    provider_display_name: Option<String>,
    properties: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzRepo {
    name: Option<String>,
    project: Option<AzProject>,
    remote_url: Option<String>,
    ssh_url: Option<String>,
    web_url: Option<String>,
    default_branch: Option<String>,
    is_disabled: Option<bool>,
}

#[derive(Deserialize)]
struct AzProject {
    name: Option<String>,
    visibility: Option<String>,
}

impl AzRepo {
    /// `None` si le nom ou l'URL de clonage manque.
    fn into_remote(self, org: &str, account_id: &str) -> Option<RemoteRepo> {
        let name = self.name.filter(|n| !n.is_empty())?;
        let clone_url = self.remote_url.filter(|u| !u.is_empty())?;
        let (project, visibility) = match self.project {
            Some(p) => (p.name.unwrap_or_default(), p.visibility),
            None => (String::new(), None),
        };
        let namespace = if project.is_empty() { org.to_string() } else { format!("{org}/{project}") };
        let web_url = self.web_url.unwrap_or_else(|| clone_url.clone());
        let default_branch = self.default_branch.map(|b| b.strip_prefix("refs/heads/").map(str::to_owned).unwrap_or(b));
        Some(RemoteRepo {
            account_id: account_id.to_string(),
            name,
            namespace,
            clone_url,
            ssh_url: self.ssh_url,
            web_url,
            description: None,
            // Visibilité du projet ; inconnue → privé par prudence.
            private: !visibility.is_some_and(|v| v.eq_ignore_ascii_case("public")),
            default_branch,
            // L'API des dépôts ne donne aucune date par dépôt (le `lastUpdateTime` de la réponse est celui du projet,
            // pas du dépôt : trompeur) ; le dernier push demanderait un appel par dépôt. Colonne vide.
            updated_at: None,
        })
    }
}

struct Profile {
    username: String,
    display_name: String,
    member_id: String,
}

fn parse_profile(body: &str) -> Result<Profile, ForgeError> {
    let profile: AzProfile = serde_json::from_str(body).map_err(parse_error)?;
    let member_id = profile
        .public_alias
        .or(profile.id)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| ForgeError::Parse(tr!("profil Azure DevOps sans identifiant (publicAlias)").into()))?;
    let display_name = profile.display_name.unwrap_or_default();
    let username = profile.email_address.filter(|e| !e.trim().is_empty()).unwrap_or_else(|| display_name.clone());
    Ok(Profile { username, display_name, member_id })
}

fn parse_accounts(body: &str) -> Result<Vec<String>, ForgeError> {
    let list: AzList<AzAccount> = serde_json::from_str(body).map_err(parse_error)?;
    Ok(list.value.into_iter().filter_map(|a| a.account_name).filter(|n| !n.trim().is_empty()).collect())
}

/// Utilisateur authentifié de `connectionData` ; un appel anonyme (jeton refusé en silence) est `Unauthorized`.
fn authenticated_user(body: &str) -> Result<AzIdentity, ForgeError> {
    let data: AzConnectionData = serde_json::from_str(body).map_err(parse_error)?;
    let user = data
        .authenticated_user
        .ok_or_else(|| ForgeError::Parse(tr!("connectionData sans authenticatedUser").into()))?;
    let anonymous = user.id.as_deref() == Some(ANONYMOUS_ID)
        || user.descriptor.as_deref().is_some_and(|d| d.starts_with("System:PublicAccess"));
    if anonymous { Err(ForgeError::Unauthorized) } else { Ok(user) }
}

/// Identité (GUID) de l'utilisateur du jeton dans une organisation, d'après `connectionData` : celle des critères
/// `creatorId` et `reviewerId` des pull requests.
pub(crate) fn authenticated_user_id(body: &str) -> Result<String, ForgeError> {
    authenticated_user(body)?
        .id
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| ForgeError::Parse(tr!("connectionData sans identifiant d'utilisateur").into()))
}

fn parse_connection_data(body: &str) -> Result<VerifiedUser, ForgeError> {
    let user = authenticated_user(body)?;
    let display_name = user.provider_display_name.unwrap_or_default();
    let email = user
        .properties
        .as_ref()
        .and_then(|p| p.pointer("/Account/$value"))
        .and_then(serde_json::Value::as_str)
        .filter(|e| !e.trim().is_empty())
        .map(str::to_owned);
    let username = email.unwrap_or_else(|| display_name.clone());
    Ok(VerifiedUser { username, display_name, avatar_url: None, orgs: Vec::new() })
}

/// Réponse de `_apis/git/repositories` d'une organisation → dépôts actifs.
pub(crate) fn parse_repositories(body: &str, org: &str, account_id: &str) -> Result<Vec<RemoteRepo>, ForgeError> {
    let list: AzList<AzRepo> = serde_json::from_str(body).map_err(parse_error)?;
    Ok(list
        .value
        .into_iter()
        .filter(|repo| repo.is_disabled != Some(true))
        .filter_map(|repo| repo.into_remote(org, account_id))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = include_str!("../tests/fixtures/azure_profile.json");
    const ACCOUNTS: &str = include_str!("../tests/fixtures/azure_accounts.json");
    const CONNECTION_DATA: &str = include_str!("../tests/fixtures/azure_connection_data.json");
    const REPOSITORIES: &str = include_str!("../tests/fixtures/azure_repositories.json");
    const SIGNIN_HTML: &str = include_str!("../tests/fixtures/azure_signin_203.html");

    #[test]
    fn first_success_stops_at_first_answering_org() {
        let mut evaluated = 0;
        let attempts = [Err(ForgeError::Http { status: 404, body: String::new() }), Ok(1), Ok(2)]
            .into_iter()
            .inspect(|_| evaluated += 1);
        assert_eq!(first_success(attempts).unwrap(), 1);
        assert_eq!(evaluated, 2, "aucune requête après le premier succès");
    }

    #[test]
    fn first_success_returns_first_error_when_all_fail() {
        let attempts: [Result<u8, ForgeError>; 2] =
            [Err(ForgeError::Http { status: 404, body: "x".into() }), Err(ForgeError::Unauthorized)];
        assert!(matches!(first_success(attempts), Err(ForgeError::Http { status: 404, .. })));
        let none: Vec<Result<u8, ForgeError>> = Vec::new();
        assert!(matches!(first_success(none), Err(ForgeError::InvalidInput(_))));
    }

    #[test]
    fn merge_org_results_ignores_failed_orgs_when_one_answers() {
        let fabrikam = parse_repositories(REPOSITORIES, "fabrikam", "acc").unwrap();
        let merged = merge_org_results([Err(ForgeError::Unauthorized), Ok(fabrikam)]).unwrap();
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_org_results_returns_first_error_when_all_fail() {
        let all_failed: Result<Vec<RemoteRepo>, _> =
            merge_org_results([Err(ForgeError::Network("délai dépassé".into())), Err(ForgeError::Unauthorized)]);
        assert!(matches!(all_failed, Err(ForgeError::Network(_))));
        let none: Vec<Result<Vec<RemoteRepo>, ForgeError>> = Vec::new();
        assert!(merge_org_results(none).unwrap().is_empty());
    }

    #[test]
    fn azure_services_hosts() {
        assert!(is_services_host("dev.azure.com"));
        assert!(is_services_host("fabrikam.visualstudio.com"));
        assert!(!is_services_host("tfs.example.com"));
        assert!(!is_services_host("dev.azure.com.example.com"));
    }

    #[test]
    fn azure_urls_for_services_and_server() {
        assert_eq!(org_base("dev.azure.com", "fabrikam"), "https://dev.azure.com/fabrikam");
        assert_eq!(org_base("fabrikam.visualstudio.com", "fabrikam"), "https://dev.azure.com/fabrikam");
        assert_eq!(org_base("dev.azure.com", "mon org"), "https://dev.azure.com/mon%20org");
        assert_eq!(
            org_base("tfs.example.com", "tfs/DefaultCollection"),
            "https://tfs.example.com/tfs/DefaultCollection"
        );
        assert_eq!(
            repositories_url("dev.azure.com", "fabrikam"),
            "https://dev.azure.com/fabrikam/_apis/git/repositories?api-version=7.1"
        );
        // Azure DevOps Server 2019+ ne connaît pas forcément l'API 7.1.
        assert_eq!(
            repositories_url("tfs.example.com", "DefaultCollection"),
            "https://tfs.example.com/DefaultCollection/_apis/git/repositories?api-version=5.0"
        );
    }

    #[test]
    fn azure_203_is_unauthorized() {
        assert!(matches!(check_response(203, SIGNIN_HTML), Err(ForgeError::Unauthorized)));
        // Le statut suffit, même si le corps ressemble à du JSON.
        assert!(matches!(check_response(203, "{}"), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn azure_html_body_is_unauthorized() {
        assert!(matches!(check_response(200, SIGNIN_HTML), Err(ForgeError::Unauthorized)));
        assert!(matches!(check_response(200, ""), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn azure_json_responses_keep_their_meaning() {
        assert!(check_response(200, "\u{feff}{\"count\":0,\"value\":[]}").is_ok());
        assert!(check_response(200, "  [1]").is_ok());
        assert!(matches!(check_response(401, ""), Err(ForgeError::Unauthorized)));
        assert!(matches!(check_response(403, "{}"), Err(ForgeError::Unauthorized)));
        let not_found = r#"{"$id":"1","message":"TF200016: The following project does not exist: x."}"#;
        match check_response(404, not_found) {
            Err(ForgeError::Http { status: 404, body }) => assert!(body.contains("TF200016")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn azure_profile_fixture_parses() {
        let profile = parse_profile(PROFILE).unwrap();
        assert_eq!(profile.username, "camille.martin@example.com");
        assert_eq!(profile.display_name, "Camille Martin");
        assert_eq!(profile.member_id, "3f6e0c3a-1b2d-4c5e-8f90-a1b2c3d4e5f6");
    }

    #[test]
    fn azure_profile_without_email_uses_display_name() {
        let profile =
            parse_profile(r#"{"displayName":"Camille Martin","publicAlias":"abc","emailAddress":null}"#).unwrap();
        assert_eq!(profile.username, "Camille Martin");
        assert!(matches!(parse_profile(r#"{"displayName":"X"}"#), Err(ForgeError::Parse(_))));
    }

    #[test]
    fn azure_accounts_fixture_parses() {
        assert_eq!(parse_accounts(ACCOUNTS).unwrap(), ["fabrikam", "contoso-labs"]);
        assert!(matches!(parse_accounts(r#"{"message":"oops"}"#), Err(ForgeError::Parse(_))));
    }

    #[test]
    fn azure_connection_data_fixture_parses() {
        let user = parse_connection_data(CONNECTION_DATA).unwrap();
        assert_eq!(user.display_name, "Camille Martin");
        assert_eq!(user.username, "camille.martin@example.com");
        assert!(user.orgs.is_empty());
    }

    #[test]
    fn azure_connection_data_anonymous_is_unauthorized() {
        let anonymous = r#"{"authenticatedUser":{"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "descriptor":"System:PublicAccess;aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "providerDisplayName":"Anonymous","isActive":true,"properties":{},
            "resourceVersion":2,"metaTypeId":255},"deploymentType":"hosted"}"#;
        assert!(matches!(parse_connection_data(anonymous), Err(ForgeError::Unauthorized)));
        assert!(matches!(authenticated_user_id(anonymous), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn azure_connection_data_gives_the_identity_of_the_token_user() {
        assert_eq!(authenticated_user_id(CONNECTION_DATA).unwrap(), "5d1c7a2e-3b4f-4c6d-8e9f-0a1b2c3d4e5f");
        let without_id = r#"{"authenticatedUser":{"providerDisplayName":"Camille Martin"}}"#;
        assert!(matches!(authenticated_user_id(without_id), Err(ForgeError::Parse(_))));
    }

    #[test]
    fn azure_repositories_fixture_parses() {
        let repos = parse_repositories(REPOSITORIES, "fabrikam", "acc-3").unwrap();
        // Le dépôt désactivé est ignoré.
        assert_eq!(repos.len(), 2);

        let fiber = &repos[0];
        assert_eq!(fiber.account_id, "acc-3");
        assert_eq!(fiber.name, "Fabrikam-Fiber");
        assert_eq!(fiber.namespace, "fabrikam/Fabrikam Web");
        assert_eq!(fiber.full_name(), "fabrikam/Fabrikam Web/Fabrikam-Fiber");
        assert_eq!(fiber.clone_url, "https://fabrikam@dev.azure.com/fabrikam/Fabrikam%20Web/_git/Fabrikam-Fiber");
        assert_eq!(fiber.ssh_url.as_deref(), Some("git@ssh.dev.azure.com:v3/fabrikam/Fabrikam%20Web/Fabrikam-Fiber"));
        assert_eq!(fiber.web_url, "https://dev.azure.com/fabrikam/Fabrikam%20Web/_git/Fabrikam-Fiber");
        assert_eq!(fiber.default_branch.as_deref(), Some("main"));
        assert!(fiber.private);
        assert_eq!(fiber.description, None);
        // Pas de date par dépôt : le `lastUpdateTime` de la fixture est celui du projet, on ne l'utilise pas.
        assert_eq!(fiber.updated_at, None);

        let docs = &repos[1];
        assert_eq!(docs.namespace, "fabrikam/OpenDocs");
        assert_eq!(docs.default_branch, None, "dépôt vide : pas de branche par défaut");
        assert!(!docs.private, "projet public");
    }
}
