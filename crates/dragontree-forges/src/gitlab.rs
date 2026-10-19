//! API REST v4 de GitLab (gitlab.com et instances auto-hébergées).

use serde::Deserialize;

use crate::http::{self, MAX_PAGES, PAGE_SIZE, parse_error};
use crate::model::{ForgeError, RemoteRepo, VerifiedUser};

/// Base de l'API pour un hôte normalisé.
pub(crate) fn api_base(host: &str) -> String {
    format!("https://{host}/api/v4")
}

/// `GET /user`.
pub(crate) fn verify(host: &str, token: &str) -> Result<VerifiedUser, ForgeError> {
    let response = http::check_status(get(&format!("{}/user", api_base(host)), token)?)?;
    parse_user(&response.body)
}

/// `GET /projects?membership=true…`, toutes les pages.
pub(crate) fn list_repos(host: &str, token: &str, account_id: &str) -> Result<Vec<RemoteRepo>, ForgeError> {
    let base = api_base(host);
    let raw = http::paginate(PAGE_SIZE, MAX_PAGES, |page| {
        parse_project_page(&http::check_status(get(&projects_page_url(&base, page), token)?)?.body)
    })?;
    Ok(into_remotes(raw, account_id))
}

/// En-têtes d'authentification : `Authorization: Bearer` (accepté par GitLab pour les PAT),
/// que le client HTTP ne transmet pas lors d'une redirection vers un autre hôte.
pub(crate) fn auth_headers(token: &str) -> Vec<(&'static str, String)> {
    vec![("Authorization", format!("Bearer {token}")), ("Accept", "application/json".to_string())]
}

fn get(url: &str, token: &str) -> Result<http::Response, ForgeError> {
    let headers = auth_headers(token);
    let refs: Vec<(&str, &str)> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    http::get(url, &refs)
}

fn projects_page_url(api_base: &str, page: usize) -> String {
    format!(
        "{api_base}/projects?membership=true&simple=true&per_page={PAGE_SIZE}&page={page}\
         &order_by=last_activity_at"
    )
}

#[derive(Deserialize)]
struct GlUser {
    username: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GlNamespace {
    full_path: Option<String>,
}

#[derive(Deserialize)]
struct GlProject {
    name: Option<String>,
    path: Option<String>,
    path_with_namespace: Option<String>,
    namespace: Option<GlNamespace>,
    http_url_to_repo: Option<String>,
    ssh_url_to_repo: Option<String>,
    web_url: Option<String>,
    description: Option<String>,
    /// Absent en mode `simple` sur certaines versions.
    visibility: Option<String>,
    default_branch: Option<String>,
    last_activity_at: Option<String>,
}

impl GlProject {
    /// `None` si le nom ou l'URL de clonage manque.
    fn into_remote(self, account_id: &str) -> Option<RemoteRepo> {
        let name = self.path.or(self.name).filter(|n| !n.is_empty())?;
        let clone_url = self.http_url_to_repo.filter(|u| !u.is_empty())?;
        let namespace = self
            .namespace
            .and_then(|ns| ns.full_path)
            .or_else(|| {
                let full = self.path_with_namespace?;
                full.rsplit_once('/').map(|(ns, _)| ns.to_string())
            })
            .unwrap_or_default();
        let web_url = self.web_url.unwrap_or_else(|| clone_url.trim_end_matches(".git").to_string());
        Some(RemoteRepo {
            account_id: account_id.to_string(),
            name,
            namespace,
            clone_url,
            ssh_url: self.ssh_url_to_repo,
            web_url,
            description: self.description,
            // Visibilité inconnue → privé par prudence.
            private: self.visibility.as_deref() != Some("public"),
            default_branch: self.default_branch,
            updated_at: self.last_activity_at,
        })
    }
}

fn parse_user(body: &str) -> Result<VerifiedUser, ForgeError> {
    let user: GlUser = serde_json::from_str(body).map_err(parse_error)?;
    let display_name = user.name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| user.username.clone());
    Ok(VerifiedUser { username: user.username, display_name, avatar_url: user.avatar_url, orgs: Vec::new() })
}

/// Page brute de `GET /projects` (la pagination compte ces éléments).
fn parse_project_page(body: &str) -> Result<Vec<GlProject>, ForgeError> {
    serde_json::from_str(body).map_err(parse_error)
}

/// Dépôts utilisables d'une liste brute.
fn into_remotes(raw: Vec<GlProject>, account_id: &str) -> Vec<RemoteRepo> {
    raw.into_iter().filter_map(|project| project.into_remote(account_id)).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn gitlab_uses_bearer_authorization() {
        // `Authorization` est retiré par ureq en cas de redirection, contrairement à PRIVATE-TOKEN.
        let h = auth_headers("tok");
        assert!(h.iter().any(|(k, v)| *k == "Authorization" && v == "Bearer tok"));
        assert!(!h.iter().any(|(k, _)| k.eq_ignore_ascii_case("PRIVATE-TOKEN")));
    }

    use super::*;

    const PROJECTS: &str = include_str!("../tests/fixtures/gitlab_projects.json");
    const USER: &str = include_str!("../tests/fixtures/gitlab_user.json");

    #[test]
    fn gitlab_projects_url_requests_full_pages() {
        assert_eq!(api_base("gitlab.example.com"), "https://gitlab.example.com/api/v4");
        assert_eq!(
            projects_page_url("https://gitlab.com/api/v4", 2),
            "https://gitlab.com/api/v4/projects?membership=true&simple=true&per_page=100&page=2\
             &order_by=last_activity_at"
        );
    }

    #[test]
    fn gitlab_user_fixture_parses() {
        let user = parse_user(USER).unwrap();
        assert_eq!(user.username, "jsmith");
        assert_eq!(user.display_name, "Jean Smith");
        assert_eq!(
            user.avatar_url.as_deref(),
            Some("https://gitlab.example.com/uploads/-/system/user/avatar/1/avatar.png")
        );
    }

    #[test]
    fn gitlab_projects_fixture_parses() {
        let repos = into_remotes(parse_project_page(PROJECTS).unwrap(), "acc-2");
        assert_eq!(repos.len(), 3);

        let client = &repos[0];
        assert_eq!(client.account_id, "acc-2");
        assert_eq!(client.name, "diaspora-client");
        assert_eq!(client.namespace, "diaspora");
        assert_eq!(client.clone_url, "https://gitlab.example.com/diaspora/diaspora-client.git");
        assert_eq!(client.ssh_url.as_deref(), Some("git@gitlab.example.com:diaspora/diaspora-client.git"));
        assert_eq!(client.web_url, "https://gitlab.example.com/diaspora/diaspora-client");
        assert_eq!(client.description.as_deref(), Some("Client for Diaspora"));
        assert!(client.private);
        assert_eq!(client.default_branch.as_deref(), Some("main"));
        assert_eq!(client.updated_at.as_deref(), Some("2022-06-24T17:11:26.841Z"));

        // Sous-groupes imbriqués ; visibilité absente (mode simple) → privé par prudence.
        let billing = &repos[1];
        assert_eq!(billing.namespace, "fabrikam/backend/services");
        assert_eq!(billing.name, "billing-api");
        assert_eq!(billing.description, None);
        assert_eq!(billing.default_branch, None);
        assert!(billing.private);

        assert!(!repos[2].private, "projet public");
    }
}
