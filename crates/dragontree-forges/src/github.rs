//! API REST v3 de GitHub (github.com, GitHub Enterprise Server et `*.ghe.com`).

use serde::Deserialize;

use crate::http::{self, MAX_PAGES, PAGE_SIZE, parse_error};
use crate::model::{ForgeError, RemoteRepo, VerifiedUser};

/// Nombre maximal de pages d'organisations lues.
const MAX_ORG_PAGES: usize = 10;

/// Base de l'API pour un hôte normalisé.
pub(crate) fn api_base(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_string()
    } else if host.ends_with(".ghe.com") {
        // GitHub Enterprise Cloud avec résidence des données.
        format!("https://api.{host}")
    } else {
        format!("https://{host}/api/v3")
    }
}

/// `GET /user` puis `GET /user/orgs` (un 403 sur les organisations donne une liste vide).
pub(crate) fn verify(host: &str, token: &str) -> Result<VerifiedUser, ForgeError> {
    let base = api_base(host);
    let response = http::check_status(get(&format!("{base}/user"), token)?)?;
    let mut user = parse_user(&response.body)?;
    user.orgs = http::paginate(PAGE_SIZE, MAX_ORG_PAGES, |page| {
        let url = format!("{base}/user/orgs?per_page={PAGE_SIZE}&page={page}");
        orgs_from_response(get(&url, token)?)
    })?;
    Ok(user)
}

/// `GET /user/repos`, toutes les pages.
pub(crate) fn list_repos(host: &str, token: &str, account_id: &str) -> Result<Vec<RemoteRepo>, ForgeError> {
    let base = api_base(host);
    let raw = http::paginate(PAGE_SIZE, MAX_PAGES, |page| {
        parse_repo_page(&http::check_status(get(&repos_page_url(&base, page), token)?)?.body)
    })?;
    Ok(into_remotes(raw, account_id))
}

fn get(url: &str, token: &str) -> Result<http::Response, ForgeError> {
    let authorization = format!("Bearer {token}");
    http::get(
        url,
        &[
            ("Authorization", &authorization),
            ("Accept", "application/vnd.github+json"),
            ("X-GitHub-Api-Version", "2022-11-28"),
        ],
    )
}

/// Réponse de `GET /user/orgs` : un 403 (jeton sans `read:org`, SSO) donne une liste vide.
fn orgs_from_response(response: http::Response) -> Result<Vec<String>, ForgeError> {
    if response.status == 403 {
        return Ok(Vec::new());
    }
    parse_orgs(&http::check_status(response)?.body)
}

fn repos_page_url(api_base: &str, page: usize) -> String {
    format!(
        "{api_base}/user/repos?per_page={PAGE_SIZE}&page={page}&sort=updated\
         &affiliation=owner,collaborator,organization_member"
    )
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GhOrg {
    login: String,
}

#[derive(Deserialize)]
struct GhOwner {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhRepo {
    name: Option<String>,
    full_name: Option<String>,
    owner: Option<GhOwner>,
    clone_url: Option<String>,
    ssh_url: Option<String>,
    html_url: Option<String>,
    description: Option<String>,
    private: Option<bool>,
    default_branch: Option<String>,
    pushed_at: Option<String>,
    updated_at: Option<String>,
}

impl GhRepo {
    /// `None` si le nom ou l'URL de clonage manque.
    fn into_remote(self, account_id: &str) -> Option<RemoteRepo> {
        let name = self.name.filter(|n| !n.is_empty())?;
        let clone_url = self.clone_url.filter(|u| !u.is_empty())?;
        let namespace = self
            .owner
            .and_then(|o| o.login)
            .or_else(|| self.full_name.as_deref().and_then(|f| f.split_once('/')).map(|(ns, _)| ns.to_string()))
            .unwrap_or_default();
        let web_url = self.html_url.unwrap_or_else(|| clone_url.trim_end_matches(".git").to_string());
        Some(RemoteRepo {
            account_id: account_id.to_string(),
            name,
            namespace,
            clone_url,
            ssh_url: self.ssh_url,
            web_url,
            description: self.description,
            private: self.private.unwrap_or(false),
            default_branch: self.default_branch,
            updated_at: self.pushed_at.or(self.updated_at),
        })
    }
}

fn parse_user(body: &str) -> Result<VerifiedUser, ForgeError> {
    let user: GhUser = serde_json::from_str(body).map_err(parse_error)?;
    let display_name = user.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| user.login.clone());
    Ok(VerifiedUser { username: user.login, display_name, avatar_url: user.avatar_url, orgs: Vec::new() })
}

fn parse_orgs(body: &str) -> Result<Vec<String>, ForgeError> {
    let orgs: Vec<GhOrg> = serde_json::from_str(body).map_err(parse_error)?;
    Ok(orgs.into_iter().map(|o| o.login).collect())
}

/// Page brute de `GET /user/repos` (la pagination compte ces éléments).
fn parse_repo_page(body: &str) -> Result<Vec<GhRepo>, ForgeError> {
    serde_json::from_str(body).map_err(parse_error)
}

/// Dépôts utilisables d'une liste brute.
fn into_remotes(raw: Vec<GhRepo>, account_id: &str) -> Vec<RemoteRepo> {
    raw.into_iter().filter_map(|repo| repo.into_remote(account_id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOS: &str = include_str!("../tests/fixtures/github_repos.json");
    const USER: &str = include_str!("../tests/fixtures/github_user.json");
    const ORGS: &str = include_str!("../tests/fixtures/github_orgs.json");

    #[test]
    fn github_api_base_for_dotcom_enterprise_and_ghe_com() {
        assert_eq!(api_base("github.com"), "https://api.github.com");
        assert_eq!(api_base("github.example.com"), "https://github.example.com/api/v3");
        assert_eq!(api_base("github.example.com:8443"), "https://github.example.com:8443/api/v3");
        assert_eq!(api_base("fabrikam.ghe.com"), "https://api.fabrikam.ghe.com");
    }

    #[test]
    fn github_repos_url_requests_full_pages() {
        let url = repos_page_url("https://api.github.com", 3);
        assert_eq!(
            url,
            "https://api.github.com/user/repos?per_page=100&page=3&sort=updated\
             &affiliation=owner,collaborator,organization_member"
        );
    }

    #[test]
    fn github_user_fixture_parses() {
        let user = parse_user(USER).unwrap();
        assert_eq!(user.username, "octocat");
        assert_eq!(user.display_name, "The Octocat");
        assert_eq!(user.avatar_url.as_deref(), Some("https://avatars.githubusercontent.com/u/583231?v=4"));
        assert!(user.orgs.is_empty());
    }

    #[test]
    fn github_user_without_name_uses_login() {
        let user = parse_user(r#"{"login":"ghost","id":10137,"name":null,"avatar_url":null}"#).unwrap();
        assert_eq!(user.display_name, "ghost");
        assert_eq!(user.avatar_url, None);
    }

    #[test]
    fn github_orgs_403_is_tolerated() {
        let response =
            |status: u16, body: &str| http::Response { status, body: body.into(), ratelimit_remaining: None };
        let forbidden = response(403, r#"{"message":"Resource not accessible by personal access token"}"#);
        assert!(orgs_from_response(forbidden).unwrap().is_empty());
        assert_eq!(orgs_from_response(response(200, ORGS)).unwrap(), ["github", "fabrikam-labs"]);
        assert!(matches!(orgs_from_response(response(401, "{}")), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn github_orgs_fixture_parses() {
        assert_eq!(parse_orgs(ORGS).unwrap(), ["github", "fabrikam-labs"]);
    }

    #[test]
    fn github_repos_fixture_parses() {
        let repos = into_remotes(parse_repo_page(REPOS).unwrap(), "acc-1");
        assert_eq!(repos.len(), 3);

        let hello = &repos[0];
        assert_eq!(hello.account_id, "acc-1");
        assert_eq!(hello.name, "Hello-World");
        assert_eq!(hello.namespace, "octocat");
        assert_eq!(hello.clone_url, "https://github.com/octocat/Hello-World.git");
        assert_eq!(hello.ssh_url.as_deref(), Some("git@github.com:octocat/Hello-World.git"));
        assert_eq!(hello.web_url, "https://github.com/octocat/Hello-World");
        assert_eq!(hello.description.as_deref(), Some("My first repository on GitHub!"));
        assert!(!hello.private);
        assert_eq!(hello.default_branch.as_deref(), Some("master"));
        // pushed_at (dernier push) prime sur updated_at.
        assert_eq!(hello.updated_at.as_deref(), Some("2011-01-26T19:06:43Z"));

        let billing = &repos[1];
        assert_eq!(billing.namespace, "fabrikam-labs");
        assert!(billing.private);
        assert_eq!(billing.description, None);
        assert_eq!(billing.default_branch.as_deref(), Some("develop"));
        // pushed_at nul → updated_at.
        assert_eq!(billing.updated_at.as_deref(), Some("2024-02-29T10:00:00Z"));
    }

    #[test]
    fn github_error_object_is_parse_error() {
        let err = parse_repo_page(r#"{"message":"Bad credentials","status":"401"}"#).err().unwrap();
        assert!(matches!(err, ForgeError::Parse(_)), "{err:?}");
    }
}
