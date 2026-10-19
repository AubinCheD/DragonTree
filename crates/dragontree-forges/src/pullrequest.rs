//! Création d'une pull request (GitHub), merge request (GitLab) ou pull request (Azure DevOps) par l'API REST.
//!
//! La construction des requêtes et l'analyse des réponses sont séparées de l'envoi : elles se testent sans réseau.

use dragontree_i18n::tr;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::basic_auth_value;
use crate::azure::{self, encode_path, org_base};
use crate::http::{self, parse_error};
use crate::location::{RepoLocation, encode_component};
use crate::model::{Account, ForgeError, ForgeKind};
use crate::{github, gitlab};

/// Ce que l'utilisateur demande : branches, texte et options de la pull request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullRequestDraft {
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
    /// Créée en brouillon.
    pub draft: bool,
    /// Relecteurs facultatifs (identifiants de connexion), non requis. Azure DevOps : ignorés.
    pub reviewers: Vec<String>,
}

/// Pull request créée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedPullRequest {
    /// Page web de la pull request.
    pub url: String,
    pub number: u64,
    /// Ce qui n'a pas pu être fait sans empêcher la création (relecteurs refusés ou inconnus).
    pub warning: Option<String>,
}

/// Requête JSON prête à envoyer : adresse et corps.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JsonRequest {
    pub url: String,
    pub body: Value,
}

/// Crée la pull request sur la forge du dépôt `location`, authentifiée par le jeton du compte. Une erreur de la forge
/// (branche absente de son serveur, pull request déjà ouverte…) est renvoyée avec son message.
pub fn create_pull_request(
    account: &Account,
    token: &str,
    location: &RepoLocation,
    draft: &PullRequestDraft,
) -> Result<CreatedPullRequest, ForgeError> {
    let token = crate::clean_token(token)?;
    if account.kind != location.kind() {
        return Err(ForgeError::InvalidInput(tr!(
            "le compte {account} n'est pas un compte {forge}",
            account = account.title(),
            forge = location.kind().label()
        )));
    }
    validate(draft)?;
    match location {
        RepoLocation::GitHub { host, owner, repo } => create_github(host, token, owner, repo, draft),
        RepoLocation::GitLab { host, project } => create_gitlab(host, token, project, draft),
        RepoLocation::Azure { host, org, project, repo } => create_azure(host, token, org, project, repo, draft),
    }
}

fn validate(draft: &PullRequestDraft) -> Result<(), ForgeError> {
    let invalid = |msg: &str| Err(ForgeError::InvalidInput(msg.to_string()));
    if draft.title.trim().is_empty() {
        return invalid(tr!("le titre est vide"));
    }
    if draft.source_branch.trim().is_empty() || draft.target_branch.trim().is_empty() {
        return invalid(tr!("la branche source et la branche cible sont obligatoires"));
    }
    if draft.source_branch == draft.target_branch {
        return invalid(tr!("la branche source et la branche cible sont la même"));
    }
    Ok(())
}

fn reviewers_of(draft: &PullRequestDraft) -> Vec<&str> {
    draft.reviewers.iter().map(|r| r.trim().trim_start_matches('@')).filter(|r| !r.is_empty()).collect()
}

fn post(request: &JsonRequest, headers: &[(&str, &str)]) -> Result<http::Response, ForgeError> {
    http::post_json(&request.url, headers, &request.body.to_string())
}

// --- GitHub -------------------------------------------------------------------------------------

fn github_headers(token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("Authorization", format!("Bearer {token}")),
        ("Accept", "application/vnd.github+json".to_string()),
        ("X-GitHub-Api-Version", "2022-11-28".to_string()),
    ]
}

/// `POST /repos/{owner}/{repo}/pulls`.
fn github_pull_request(host: &str, owner: &str, repo: &str, draft: &PullRequestDraft) -> JsonRequest {
    JsonRequest {
        url: format!("{}/repos/{}/{}/pulls", github::api_base(host), encode_path(owner), encode_path(repo)),
        body: json!({
            "title": draft.title.trim(),
            "head": draft.source_branch,
            "base": draft.target_branch,
            "body": draft.description,
            "draft": draft.draft,
        }),
    }
}

/// `POST /repos/{owner}/{repo}/pulls/{number}/requested_reviewers`.
fn github_reviewers(host: &str, owner: &str, repo: &str, number: u64, reviewers: &[&str]) -> JsonRequest {
    JsonRequest {
        url: format!(
            "{}/repos/{}/{}/pulls/{number}/requested_reviewers",
            github::api_base(host),
            encode_path(owner),
            encode_path(repo)
        ),
        body: json!({ "reviewers": reviewers }),
    }
}

#[derive(Deserialize)]
struct GhPull {
    number: u64,
    html_url: String,
}

fn parse_github_pull(body: &str) -> Result<GhPull, ForgeError> {
    serde_json::from_str(body).map_err(parse_error)
}

/// Message lisible d'une erreur GitHub (`message` et `errors[].message`) : la forge y explique le refus.
fn github_error_text(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut parts = vec![value.get("message")?.as_str()?.to_string()];
    for error in value.get("errors")?.as_array().into_iter().flatten() {
        if let Some(text) = error.get("message").and_then(Value::as_str) {
            parts.push(text.to_string());
        }
    }
    Some(parts.join(" : "))
}

/// Réponse de création : 2xx, sinon l'erreur de la forge avec son message (la 422 de GitHub dit pourquoi).
fn github_check(response: http::Response) -> Result<http::Response, ForgeError> {
    match response.status {
        422 => Err(ForgeError::Http {
            status: 422,
            body: github_error_text(&response.body).unwrap_or_else(|| response.body.trim().to_string()),
        }),
        _ => http::check_status(response),
    }
}

fn create_github(
    host: &str,
    token: &str,
    owner: &str,
    repo: &str,
    draft: &PullRequestDraft,
) -> Result<CreatedPullRequest, ForgeError> {
    let headers = github_headers(token);
    let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let created =
        parse_github_pull(&github_check(post(&github_pull_request(host, owner, repo, draft), &headers)?)?.body)?;
    let reviewers = reviewers_of(draft);
    let warning = if reviewers.is_empty() {
        None
    } else {
        let request = github_reviewers(host, owner, repo, created.number, &reviewers);
        match post(&request, &headers).and_then(github_check) {
            Ok(_) => None,
            Err(e) => Some(tr!("relecteurs non ajoutés : {error}", error = e)),
        }
    };
    Ok(CreatedPullRequest { url: created.html_url, number: created.number, warning })
}

// --- GitLab -------------------------------------------------------------------------------------

/// `POST /projects/{chemin encodé}/merge_requests`. Brouillon : préfixe `Draft:` du titre (accepté par toutes les
/// versions). `reviewer_ids` : identifiants déjà résolus.
fn gitlab_merge_request(host: &str, project: &str, draft: &PullRequestDraft, reviewer_ids: &[u64]) -> JsonRequest {
    let title = if draft.draft { format!("Draft: {}", draft.title.trim()) } else { draft.title.trim().to_string() };
    let mut body = json!({
        "source_branch": draft.source_branch,
        "target_branch": draft.target_branch,
        "title": title,
        "description": draft.description,
    });
    if !reviewer_ids.is_empty() {
        body["reviewer_ids"] = json!(reviewer_ids);
    }
    JsonRequest {
        url: format!("{}/projects/{}/merge_requests", gitlab::api_base(host), encode_component(project)),
        body,
    }
}

/// `GET /users?username=…`.
fn gitlab_user_lookup_url(host: &str, username: &str) -> String {
    format!("{}/users?username={}", gitlab::api_base(host), encode_component(username))
}

#[derive(Deserialize)]
struct GlUserId {
    id: u64,
}

fn parse_gitlab_user_ids(body: &str) -> Result<Vec<u64>, ForgeError> {
    let users: Vec<GlUserId> = serde_json::from_str(body).map_err(parse_error)?;
    Ok(users.into_iter().map(|u| u.id).collect())
}

#[derive(Deserialize)]
struct GlMergeRequest {
    iid: u64,
    web_url: String,
}

fn parse_gitlab_merge_request(body: &str) -> Result<GlMergeRequest, ForgeError> {
    serde_json::from_str(body).map_err(parse_error)
}

/// Message lisible d'une erreur GitLab (`message` : texte, liste ou objet).
fn gitlab_error_text(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    match value.get("message").or_else(|| value.get("error"))? {
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn gitlab_check(response: http::Response) -> Result<http::Response, ForgeError> {
    match response.status {
        400 | 409 | 422 => Err(ForgeError::Http {
            status: response.status,
            body: gitlab_error_text(&response.body).unwrap_or_else(|| response.body.trim().to_string()),
        }),
        _ => http::check_status(response),
    }
}

fn create_gitlab(
    host: &str,
    token: &str,
    project: &str,
    draft: &PullRequestDraft,
) -> Result<CreatedPullRequest, ForgeError> {
    let headers = gitlab::auth_headers(token);
    let headers: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    // Les relecteurs se désignent par identifiant : un nom inconnu est signalé sans empêcher la création.
    let mut ids = Vec::new();
    let mut unknown = Vec::new();
    for name in reviewers_of(draft) {
        let found = http::get(&gitlab_user_lookup_url(host, name), &headers)
            .and_then(http::check_status)
            .and_then(|r| parse_gitlab_user_ids(&r.body));
        match found {
            Ok(found) if !found.is_empty() => ids.push(found[0]),
            _ => unknown.push(name),
        }
    }
    let created = parse_gitlab_merge_request(
        &gitlab_check(post(&gitlab_merge_request(host, project, draft, &ids), &headers)?)?.body,
    )?;
    let warning = (!unknown.is_empty()).then(|| tr!("relecteurs introuvables : {names}", names = unknown.join(", ")));
    Ok(CreatedPullRequest { url: created.web_url, number: created.iid, warning })
}

// --- Azure DevOps -------------------------------------------------------------------------------

/// Version de l'API : Azure DevOps Server 2019 plafonne à la 5.0.
fn azure_api_version(host: &str) -> &'static str {
    if azure::is_services_host(host) { "7.1" } else { "5.0" }
}

/// `POST {org}/{projet}/_apis/git/repositories/{dépôt}/pullrequests`.
fn azure_pull_request(host: &str, org: &str, project: &str, repo: &str, draft: &PullRequestDraft) -> JsonRequest {
    let mut body = json!({
        "sourceRefName": format!("refs/heads/{}", draft.source_branch),
        "targetRefName": format!("refs/heads/{}", draft.target_branch),
        "title": draft.title.trim(),
        "description": draft.description,
    });
    if draft.draft {
        body["isDraft"] = json!(true);
    }
    JsonRequest {
        url: format!(
            "{}/{}/_apis/git/repositories/{}/pullrequests?api-version={}",
            org_base(host, org),
            encode_path(project),
            encode_path(repo),
            azure_api_version(host)
        ),
        body,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzPullRequest {
    pull_request_id: u64,
}

fn parse_azure_pull_request(body: &str) -> Result<u64, ForgeError> {
    let created: AzPullRequest = serde_json::from_str(body).map_err(parse_error)?;
    Ok(created.pull_request_id)
}

/// Page web d'une pull request d'Azure DevOps.
fn azure_web_url(host: &str, org: &str, project: &str, repo: &str, number: u64) -> String {
    format!("{}/{}/_git/{}/pullrequest/{number}", org_base(host, org), encode_path(project), encode_path(repo))
}

fn create_azure(
    host: &str,
    token: &str,
    org: &str,
    project: &str,
    repo: &str,
    draft: &PullRequestDraft,
) -> Result<CreatedPullRequest, ForgeError> {
    let authorization = basic_auth_value("", token);
    let headers = [
        ("Authorization", authorization.as_str()),
        ("Accept", "application/json"),
        // Demande un 401 plutôt qu'une redirection vers la page de connexion.
        ("X-TFS-FedAuthRedirect", "Suppress"),
    ];
    let response = post(&azure_pull_request(host, org, project, repo, draft), &headers)?;
    azure::check_response(response.status, &response.body)?;
    let number = parse_azure_pull_request(&response.body)?;
    // Les relecteurs d'Azure DevOps se désignent par identité (GUID) : non pris en charge ici.
    let warning = (!reviewers_of(draft).is_empty()).then(|| {
        tr!("relecteurs non ajoutés : Azure DevOps demande leur identité, à ajouter dans le navigateur").into()
    });
    Ok(CreatedPullRequest { url: azure_web_url(host, org, project, repo, number), number, warning })
}

/// La forge sait ajouter des relecteurs à la création (Azure DevOps les désigne par identité : non).
pub fn supports_reviewers(kind: ForgeKind) -> bool {
    !matches!(kind, ForgeKind::AzureDevOps)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB_CREATED: &str = include_str!("../tests/fixtures/github_pull_created.json");
    const GITHUB_422: &str = include_str!("../tests/fixtures/github_pull_error_422.json");
    const GITLAB_CREATED: &str = include_str!("../tests/fixtures/gitlab_merge_request_created.json");
    const GITLAB_USERS: &str = include_str!("../tests/fixtures/gitlab_users_lookup.json");
    const AZURE_CREATED: &str = include_str!("../tests/fixtures/azure_pull_request_created.json");

    fn draft() -> PullRequestDraft {
        PullRequestDraft {
            source_branch: "feature/accueil".into(),
            target_branch: "main".into(),
            title: "  Ajoute la page d'accueil ".into(),
            description: "Description de la PR".into(),
            draft: false,
            reviewers: vec![],
        }
    }

    fn response(status: u16, body: &str) -> http::Response {
        http::Response { status, body: body.into(), ratelimit_remaining: None }
    }

    #[test]
    fn github_request_targets_the_pulls_endpoint_with_a_json_body() {
        let request = github_pull_request("github.com", "contoso", "northwind", &draft());
        assert_eq!(request.url, "https://api.github.com/repos/contoso/northwind/pulls");
        assert_eq!(
            request.body,
            json!({
                "title": "Ajoute la page d'accueil",
                "head": "feature/accueil",
                "base": "main",
                "body": "Description de la PR",
                "draft": false,
            })
        );
        let enterprise =
            github_pull_request("github.contoso.test", "o", "r", &PullRequestDraft { draft: true, ..draft() });
        assert_eq!(enterprise.url, "https://github.contoso.test/api/v3/repos/o/r/pulls");
        assert_eq!(enterprise.body["draft"], json!(true));
    }

    #[test]
    fn github_reviewers_request() {
        let request = github_reviewers("github.com", "contoso", "northwind", 1347, &["jdoe", "asmith"]);
        assert_eq!(request.url, "https://api.github.com/repos/contoso/northwind/pulls/1347/requested_reviewers");
        assert_eq!(request.body, json!({ "reviewers": ["jdoe", "asmith"] }));
    }

    #[test]
    fn github_response_fixture_gives_number_and_page() {
        let pull = parse_github_pull(GITHUB_CREATED).unwrap();
        assert_eq!(pull.number, 1347);
        assert_eq!(pull.html_url, "https://github.com/contoso/northwind/pull/1347");
        assert!(matches!(parse_github_pull("{}"), Err(ForgeError::Parse(_))));
    }

    #[test]
    fn github_422_explains_the_refusal() {
        match github_check(response(422, GITHUB_422)) {
            Err(ForgeError::Http { status: 422, body }) => {
                assert_eq!(body, "Validation Failed : A pull request already exists for contoso:feature/accueil.");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(github_check(response(401, "{}")), Err(ForgeError::Unauthorized)));
        assert!(github_check(response(201, GITHUB_CREATED)).is_ok());
    }

    #[test]
    fn gitlab_request_encodes_the_project_path_and_prefixes_drafts() {
        let request = gitlab_merge_request("gitlab.com", "fabrikam/outils/cli", &draft(), &[]);
        assert_eq!(request.url, "https://gitlab.com/api/v4/projects/fabrikam%2Foutils%2Fcli/merge_requests");
        assert_eq!(
            request.body,
            json!({
                "source_branch": "feature/accueil",
                "target_branch": "main",
                "title": "Ajoute la page d'accueil",
                "description": "Description de la PR",
            })
        );
        let with = gitlab_merge_request("gitlab.com", "a/b", &PullRequestDraft { draft: true, ..draft() }, &[42, 7]);
        assert_eq!(with.body["title"], json!("Draft: Ajoute la page d'accueil"));
        assert_eq!(with.body["reviewer_ids"], json!([42, 7]));
    }

    #[test]
    fn gitlab_responses_and_user_lookup() {
        let mr = parse_gitlab_merge_request(GITLAB_CREATED).unwrap();
        assert_eq!(mr.iid, 7);
        assert_eq!(mr.web_url, "https://gitlab.com/fabrikam/outils/cli/-/merge_requests/7");
        assert_eq!(parse_gitlab_user_ids(GITLAB_USERS).unwrap(), [42]);
        assert!(parse_gitlab_user_ids("[]").unwrap().is_empty());
        assert_eq!(gitlab_user_lookup_url("gitlab.com", "j doe"), "https://gitlab.com/api/v4/users?username=j%20doe");
    }

    #[test]
    fn gitlab_conflict_shows_the_server_message() {
        let body = r#"{"message":["Another open merge request already exists for this source branch: !5"]}"#;
        match gitlab_check(response(409, body)) {
            Err(ForgeError::Http { status: 409, body }) => assert!(body.contains("already exists"), "{body}"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(gitlab_check(response(401, "{}")), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn azure_request_uses_full_ref_names_and_the_api_version() {
        let request = azure_pull_request("dev.azure.com", "fabrikam", "Mon Projet", "api", &draft());
        assert_eq!(
            request.url,
            "https://dev.azure.com/fabrikam/Mon%20Projet/_apis/git/repositories/api/pullrequests?api-version=7.1"
        );
        assert_eq!(
            request.body,
            json!({
                "sourceRefName": "refs/heads/feature/accueil",
                "targetRefName": "refs/heads/main",
                "title": "Ajoute la page d'accueil",
                "description": "Description de la PR",
            })
        );
        let server =
            azure_pull_request("tfs.contoso.test", "tfs/Coll", "P", "r", &PullRequestDraft { draft: true, ..draft() });
        assert_eq!(
            server.url,
            "https://tfs.contoso.test/tfs/Coll/P/_apis/git/repositories/r/pullrequests?api-version=5.0"
        );
        assert_eq!(server.body["isDraft"], json!(true));
    }

    #[test]
    fn azure_response_fixture_gives_the_number_and_the_web_page() {
        let number = parse_azure_pull_request(AZURE_CREATED).unwrap();
        assert_eq!(number, 12);
        assert_eq!(
            azure_web_url("dev.azure.com", "fabrikam", "Mon Projet", "api", number),
            "https://dev.azure.com/fabrikam/Mon%20Projet/_git/api/pullrequest/12"
        );
        assert!(matches!(parse_azure_pull_request(r#"{"status":"active"}"#), Err(ForgeError::Parse(_))));
    }

    #[test]
    fn reviewers_are_trimmed_and_blank_names_dropped() {
        let d = PullRequestDraft { reviewers: vec![" @jdoe ".into(), "".into(), "asmith".into()], ..draft() };
        assert_eq!(reviewers_of(&d), ["jdoe", "asmith"]);
    }

    #[test]
    fn invalid_drafts_are_rejected_before_any_request() {
        assert!(validate(&draft()).is_ok());
        for bad in [
            PullRequestDraft { title: "  ".into(), ..draft() },
            PullRequestDraft { source_branch: String::new(), ..draft() },
            PullRequestDraft { target_branch: "feature/accueil".into(), ..draft() },
        ] {
            assert!(matches!(validate(&bad), Err(ForgeError::InvalidInput(_))), "{bad:?}");
        }
    }

    #[test]
    fn an_account_of_another_forge_is_refused_without_network() {
        let account = Account {
            id: "0123456789abcdef".into(),
            kind: ForgeKind::GitLab,
            host: "gitlab.com".into(),
            username: "jsmith".into(),
            display_name: String::new(),
            orgs: vec![],
            avatar_url: None,
        };
        let location = RepoLocation::GitHub { host: "github.com".into(), owner: "o".into(), repo: "r".into() };
        let result = create_pull_request(&account, "jeton", &location, &draft());
        assert!(matches!(result, Err(ForgeError::InvalidInput(_))), "{result:?}");
        assert!(matches!(create_pull_request(&account, " ", &location, &draft()), Err(ForgeError::InvalidInput(_))));
    }
}
