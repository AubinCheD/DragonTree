//! Pull requests ouvertes qui concernent l'utilisateur d'un compte, pour la vue centralisée : recherche GraphQL de
//! GitHub, merge requests de GitLab, pull requests actives de chaque organisation Azure DevOps.
//!
//! Comme pour la création (`pullrequest.rs`), la construction des requêtes et l'analyse des réponses sont séparées de
//! l'envoi : chaque forge reçoit sa fonction d'envoi (le client HTTP, ou des réponses préparées dans les tests).

use dragontree_i18n::tr;
use serde::Deserialize;
use serde_json::json;

use crate::http::{self, PAGE_SIZE, parse_error};
use crate::location::{RepoLocation, encode_component};
use crate::model::{Account, ForgeError, ForgeKind, normalize_host, timestamp_key};
use crate::pullrequest::JsonRequest;
use crate::{azure, github, gitlab};

/// Pull request (merge request sur GitLab) ouverte qui concerne l'utilisateur du compte. Seules les pull requests
/// ouvertes sont lues : leur état se résume à `draft`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// Compte qui l'a lue.
    pub account_id: String,
    /// Dépôt de la forge (la forge elle-même : [`PullRequest::kind`]).
    pub repo: RepoLocation,
    /// Numéro (GitHub, Azure DevOps) ou `iid` (GitLab) : celui de son adresse.
    pub number: u64,
    pub title: String,
    /// Identifiant de connexion (GitHub, GitLab) ou nom affiché (Azure DevOps) de l'auteur ; vide s'il est inconnu
    /// (compte supprimé).
    pub author: String,
    pub draft: bool,
    /// Relecteurs demandés : identifiants (GitHub, GitLab) ou noms affichés (Azure DevOps), équipes comprises.
    pub reviewers: Vec<String>,
    /// Ouverte par l'utilisateur du compte.
    pub created_by_me: bool,
    /// Relecture attendue de l'utilisateur du compte (demandée directement ou à une de ses équipes, sans vote
    /// encore sur Azure DevOps).
    pub review_requested: bool,
    /// Page web de la pull request.
    pub url: String,
    /// Dernière mise à jour (ISO 8601). Azure DevOps : date de création, son API n'en donne pas d'autre.
    pub updated_at: Option<String>,
    pub source_branch: Option<String>,
    pub target_branch: Option<String>,
}

impl PullRequest {
    pub fn kind(&self) -> ForgeKind {
        self.repo.kind()
    }

    /// Date de mise à jour en secondes Unix (UTC), si elle est connue et lisible.
    pub fn updated_timestamp(&self) -> Option<i64> {
        self.updated_at.as_deref().and_then(timestamp_key).map(|(secs, _)| secs)
    }

    /// Référence courte : `!7` sur GitLab, `#7` ailleurs.
    pub fn reference(&self) -> String {
        match self.kind() {
            ForgeKind::GitLab => format!("!{}", self.number),
            ForgeKind::GitHub | ForgeKind::AzureDevOps => format!("#{}", self.number),
        }
    }
}

/// Même identifiant, sans tenir compte de la casse (logins GitHub et GitLab, GUID d'Azure DevOps).
fn same_user(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && a.trim().eq_ignore_ascii_case(b.trim())
}

fn refs<'a>(headers: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    headers.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// Pull requests ouvertes qui concernent l'utilisateur du compte, de la plus récemment mise à jour à la plus ancienne :
/// - GitHub : celles qui le concernent (auteur, assigné, mentionné, commentateur) et celles dont la relecture lui est
///   demandée (`involves:` puis `review-requested:`), en GraphQL : branches, brouillon et relecteurs viennent avec la
///   recherche (la recherche REST demanderait un appel de plus par pull request) ;
/// - GitLab : créées par lui, assignées à lui, et celles dont il est relecteur ;
/// - Azure DevOps : créées par lui et celles dont il est relecteur, dans chaque organisation du compte (une
///   organisation qui échoue est ignorée si une autre répond, comme pour la liste des dépôts).
pub fn list_pull_requests(account: &Account, token: &str) -> Result<Vec<PullRequest>, ForgeError> {
    let token = crate::clean_token(token)?;
    let host = normalize_host(&account.host, account.kind);
    let prs = match account.kind {
        ForgeKind::GitHub => {
            let headers = github::auth_headers(token);
            let headers = refs(&headers);
            github_list(&host, &account.username, |request| {
                http::post_json(&request.url, &headers, &request.body.to_string())
            })?
        }
        ForgeKind::GitLab => {
            let headers = gitlab::auth_headers(token);
            let headers = refs(&headers);
            gitlab_list(&host, &account.username, |url| Ok(http::check_status(http::get(url, &headers)?)?.body))?
        }
        ForgeKind::AzureDevOps => azure_list(&host, &account.orgs, |url| azure::get(url, token))?,
    };
    Ok(finalize(prs, &account.id))
}

// --- GitHub -------------------------------------------------------------------------------------

/// Résultats par page de la recherche GitHub (100 au plus ; 50 avec 20 relecteurs chacun restent loin des limites de
/// coût de GraphQL).
const GITHUB_PAGE_SIZE: usize = 50;
/// Pages lues au plus par recherche.
const GITHUB_MAX_PAGES: usize = 10;
/// Pages lues au plus par liste de GitLab ou d'Azure DevOps (100 éléments chacune).
const MAX_LIST_PAGES: usize = 10;

/// Recherche GraphQL : une page de pull requests avec leurs branches, leur brouillon et leurs relecteurs demandés.
const GITHUB_SEARCH_QUERY: &str = "query($q: String!, $first: Int!, $after: String) {
  search(query: $q, type: ISSUE, first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes {
      ... on PullRequest {
        number title url isDraft updatedAt headRefName baseRefName
        author { login }
        repository { name owner { login } }
        reviewRequests(first: 20) {
          nodes { requestedReviewer { ... on User { login } ... on Team { name } } }
        }
      }
    }
  }
}";

/// Recherches GitHub de l'utilisateur `username` (`@me` s'il est inconnu) : les pull requests ouvertes qui le
/// concernent, puis celles dont la relecture lui est demandée, que `involves:` ne couvre pas.
fn github_queries(username: &str) -> [String; 2] {
    let who = match username.trim() {
        "" => "@me",
        name => name,
    };
    ["involves", "review-requested"]
        .map(|qualifier| format!("is:pr is:open archived:false {qualifier}:{who} sort:updated-desc"))
}

/// Une page de la recherche `query`, après le curseur `after`.
fn github_search_request(host: &str, query: &str, after: Option<&str>) -> JsonRequest {
    JsonRequest {
        url: github::graphql_url(host),
        body: json!({ "query": GITHUB_SEARCH_QUERY, "variables": { "q": query, "first": GITHUB_PAGE_SIZE, "after": after } }),
    }
}

#[derive(Deserialize)]
struct GqlResponse {
    data: Option<GqlData>,
    #[serde(default)]
    errors: Vec<GqlError>,
}

#[derive(Deserialize)]
struct GqlError {
    message: Option<String>,
}

#[derive(Deserialize)]
struct GqlData {
    search: Option<GqlSearch>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlSearch {
    page_info: Option<GqlPageInfo>,
    #[serde(default)]
    nodes: Vec<Option<GhPullNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPageInfo {
    #[serde(default)]
    has_next_page: bool,
    end_cursor: Option<String>,
}

/// Résultat de la recherche : vide (`{}`) pour ce qui n'est pas une pull request.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPullNode {
    number: Option<u64>,
    title: Option<String>,
    url: Option<String>,
    is_draft: Option<bool>,
    updated_at: Option<String>,
    head_ref_name: Option<String>,
    base_ref_name: Option<String>,
    author: Option<GhLogin>,
    repository: Option<GhRepository>,
    review_requests: Option<GhNodes<GhReviewRequest>>,
}

#[derive(Deserialize)]
struct GhLogin {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhRepository {
    name: String,
    owner: GhLogin,
}

#[derive(Deserialize)]
struct GhNodes<T> {
    #[serde(default = "Vec::new")]
    nodes: Vec<Option<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReviewRequest {
    requested_reviewer: Option<GhReviewer>,
}

/// Utilisateur (`login`) ou équipe (`name`).
#[derive(Deserialize)]
struct GhReviewer {
    login: Option<String>,
    name: Option<String>,
}

impl GhPullNode {
    /// `None` si ce n'est pas une pull request lisible (numéro, adresse ou dépôt absent).
    fn into_pull_request(self, host: &str, username: &str) -> Option<PullRequest> {
        let number = self.number?;
        let url = self.url.filter(|u| !u.is_empty())?;
        let repository = self.repository?;
        let author = self.author.and_then(|a| a.login).unwrap_or_default();
        let reviewers: Vec<String> = self
            .review_requests
            .map(|r| r.nodes)
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|request| request.requested_reviewer)
            .filter_map(|reviewer| reviewer.login.or(reviewer.name))
            .filter(|name| !name.is_empty())
            .collect();
        Some(PullRequest {
            account_id: String::new(),
            repo: RepoLocation::GitHub {
                host: host.to_string(),
                owner: repository.owner.login.unwrap_or_default(),
                repo: repository.name,
            },
            number,
            title: self.title.unwrap_or_default(),
            created_by_me: same_user(&author, username),
            review_requested: reviewers.iter().any(|r| same_user(r, username)),
            author,
            draft: self.is_draft.unwrap_or(false),
            reviewers,
            url,
            updated_at: self.updated_at,
            source_branch: self.head_ref_name,
            target_branch: self.base_ref_name,
        })
    }
}

/// Page de résultats de la recherche et curseur de la suivante.
struct GhPage {
    prs: Vec<PullRequest>,
    next: Option<String>,
}

/// Réponse GraphQL : des erreurs sans résultat (limite de débit, requête refusée) sont renvoyées avec leur message ;
/// avec des résultats (organisation protégée par SSO…), elles sont ignorées.
fn parse_github_search(body: &str, host: &str, username: &str) -> Result<GhPage, ForgeError> {
    let response: GqlResponse = serde_json::from_str(body).map_err(parse_error)?;
    let Some(search) = response.data.and_then(|d| d.search) else {
        let messages: Vec<String> = response.errors.into_iter().filter_map(|e| e.message).collect();
        return Err(ForgeError::Parse(if messages.is_empty() {
            tr!("réponse sans résultat de recherche").into()
        } else {
            messages.join(" ; ")
        }));
    };
    let next = search.page_info.filter(|p| p.has_next_page).and_then(|p| p.end_cursor);
    let prs = search.nodes.into_iter().flatten().filter_map(|node| node.into_pull_request(host, username)).collect();
    Ok(GhPage { prs, next })
}

fn github_list(
    host: &str,
    username: &str,
    mut post: impl FnMut(&JsonRequest) -> Result<http::Response, ForgeError>,
) -> Result<Vec<PullRequest>, ForgeError> {
    let [involves, review_requested] = github_queries(username);
    let mut all = Vec::new();
    for (query, requested) in [(involves, false), (review_requested, true)] {
        let mut after: Option<String> = None;
        for _ in 0..GITHUB_MAX_PAGES {
            let response = http::check_status(post(&github_search_request(host, &query, after.as_deref()))?)?;
            let page = parse_github_search(&response.body, host, username)?;
            all.extend(page.prs.into_iter().map(|mut pr| {
                // Trouvée par `review-requested:` : la relecture est demandée à l'utilisateur ou à une de ses équipes.
                pr.review_requested |= requested;
                pr
            }));
            match page.next {
                Some(cursor) => after = Some(cursor),
                None => break,
            }
        }
    }
    Ok(all)
}

// --- GitLab -------------------------------------------------------------------------------------

/// Merge requests demandées à GitLab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitlabScope {
    CreatedByMe,
    AssignedToMe,
    /// Celles dont l'utilisateur est relecteur (`reviewer_username`, GitLab 13.8 et suivants).
    Reviewer,
}

fn gitlab_merge_requests_url(api_base: &str, scope: GitlabScope, username: &str, page: usize) -> String {
    let filter = match scope {
        GitlabScope::CreatedByMe => "scope=created_by_me&state=opened".to_string(),
        GitlabScope::AssignedToMe => "scope=assigned_to_me&state=opened".to_string(),
        GitlabScope::Reviewer => {
            format!("scope=all&state=opened&reviewer_username={}", encode_component(username.trim()))
        }
    };
    format!("{api_base}/merge_requests?{filter}&order_by=updated_at&sort=desc&per_page={PAGE_SIZE}&page={page}")
}

#[derive(Deserialize)]
struct GlUserRef {
    username: Option<String>,
}

#[derive(Deserialize)]
struct GlReferences {
    full: Option<String>,
}

#[derive(Deserialize)]
struct GlMergeRequest {
    iid: Option<u64>,
    title: Option<String>,
    author: Option<GlUserRef>,
    #[serde(default)]
    reviewers: Vec<GlUserRef>,
    draft: Option<bool>,
    /// Nom de `draft` avant GitLab 14.
    work_in_progress: Option<bool>,
    web_url: Option<String>,
    references: Option<GlReferences>,
    updated_at: Option<String>,
    source_branch: Option<String>,
    target_branch: Option<String>,
}

impl GlMergeRequest {
    /// `None` si le numéro, l'adresse ou le projet manque.
    fn into_pull_request(self, host: &str, username: &str) -> Option<PullRequest> {
        let number = self.iid?;
        let url = self.web_url.filter(|u| !u.is_empty())?;
        // `groupe/projet!7`, sinon l'adresse de la page : `https://hôte/groupe/projet/-/merge_requests/7`.
        let project = self
            .references
            .and_then(|r| r.full)
            .and_then(|full| full.rsplit_once('!').map(|(project, _)| project.to_string()))
            .or_else(|| {
                let (page, _) = url.split_once("/-/merge_requests/")?;
                let (_, path) = page.split_once("://")?.1.split_once('/')?;
                Some(path.to_string())
            })
            .filter(|p| !p.is_empty())?;
        let author = self.author.and_then(|a| a.username).unwrap_or_default();
        let reviewers: Vec<String> = self.reviewers.into_iter().filter_map(|r| r.username).collect();
        Some(PullRequest {
            account_id: String::new(),
            repo: RepoLocation::GitLab { host: host.to_string(), project },
            number,
            title: self.title.unwrap_or_default(),
            created_by_me: same_user(&author, username),
            review_requested: reviewers.iter().any(|r| same_user(r, username)),
            author,
            draft: self.draft.or(self.work_in_progress).unwrap_or(false),
            reviewers,
            url,
            updated_at: self.updated_at,
            source_branch: self.source_branch,
            target_branch: self.target_branch,
        })
    }
}

fn gitlab_list(
    host: &str,
    username: &str,
    mut get: impl FnMut(&str) -> Result<String, ForgeError>,
) -> Result<Vec<PullRequest>, ForgeError> {
    let base = gitlab::api_base(host);
    let mut scopes = vec![GitlabScope::CreatedByMe, GitlabScope::AssignedToMe];
    if !username.trim().is_empty() {
        scopes.push(GitlabScope::Reviewer);
    }
    let mut all = Vec::new();
    for scope in scopes {
        let raw: Vec<GlMergeRequest> = http::paginate(PAGE_SIZE, MAX_LIST_PAGES, |page| {
            serde_json::from_str(&get(&gitlab_merge_requests_url(&base, scope, username, page))?).map_err(parse_error)
        })?;
        let prs = raw.into_iter().filter_map(|mr| mr.into_pull_request(host, username));
        // Une instance qui ignorerait `reviewer_username` renverrait toutes les merge requests visibles : seules
        // celles dont l'utilisateur est relecteur sont gardées.
        all.extend(prs.filter(|pr| scope != GitlabScope::Reviewer || pr.review_requested));
    }
    Ok(all)
}

// --- Azure DevOps -------------------------------------------------------------------------------

/// Pull requests actives d'une organisation dont l'utilisateur `identity` est l'auteur (`creatorId`) ou un relecteur
/// (`reviewerId`).
fn azure_pull_requests_url(host: &str, org: &str, criterion: &str, identity: &str, page: usize) -> String {
    format!(
        "{}/_apis/git/pullrequests?searchCriteria.status=active&searchCriteria.{criterion}={}\
         &$top={PAGE_SIZE}&$skip={}&api-version={}",
        azure::org_base(host, org),
        encode_component(identity),
        (page - 1) * PAGE_SIZE,
        azure::api_version(host)
    )
}

#[derive(Deserialize)]
struct AzList<T> {
    value: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzIdentityRef {
    display_name: Option<String>,
    unique_name: Option<String>,
    id: Option<String>,
}

impl AzIdentityRef {
    /// Nom affiché, sans le préfixe `[Projet]\` d'une équipe ; sinon l'identifiant unique.
    fn name(&self) -> String {
        let display = self.display_name.as_deref().map(|n| n.rsplit('\\').next().unwrap_or(n).trim());
        match display.filter(|n| !n.is_empty()) {
            Some(name) => name.to_string(),
            None => self.unique_name.clone().unwrap_or_default(),
        }
    }

    fn is(&self, identity: &str) -> bool {
        self.id.as_deref().is_some_and(|id| same_user(id, identity))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzReviewer {
    #[serde(flatten)]
    identity: AzIdentityRef,
    vote: Option<i32>,
}

#[derive(Deserialize)]
struct AzProjectRef {
    name: Option<String>,
}

#[derive(Deserialize)]
struct AzRepositoryRef {
    name: Option<String>,
    project: Option<AzProjectRef>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AzPullRequest {
    pull_request_id: Option<u64>,
    title: Option<String>,
    created_by: Option<AzIdentityRef>,
    creation_date: Option<String>,
    source_ref_name: Option<String>,
    target_ref_name: Option<String>,
    is_draft: Option<bool>,
    #[serde(default)]
    reviewers: Vec<AzReviewer>,
    repository: Option<AzRepositoryRef>,
}

/// Nom court d'une branche (`refs/heads/main` → `main`).
fn short_branch(name: Option<String>) -> Option<String> {
    name.map(|n| n.strip_prefix("refs/heads/").map(str::to_owned).unwrap_or(n))
}

impl AzPullRequest {
    /// `None` si le numéro ou le dépôt manque. `reviewer_query` : trouvée par le critère `reviewerId`.
    fn into_pull_request(self, host: &str, org: &str, identity: &str, reviewer_query: bool) -> Option<PullRequest> {
        let number = self.pull_request_id?;
        let repository = self.repository?;
        let repo = repository.name.filter(|n| !n.is_empty())?;
        let project = repository.project.and_then(|p| p.name).filter(|n| !n.is_empty()).unwrap_or_else(|| repo.clone());
        let created_by_me = self.created_by.as_ref().is_some_and(|c| c.is(identity));
        // Relecture attendue : l'utilisateur n'a pas encore voté ; absent de la liste alors que le critère
        // `reviewerId` l'a trouvé, c'est une de ses équipes qui est relectrice (sauf sur ses propres pull requests).
        let review_requested = match self.reviewers.iter().find(|r| r.identity.is(identity)) {
            Some(mine) => mine.vote.unwrap_or(0) == 0,
            None => reviewer_query && !created_by_me,
        };
        Some(PullRequest {
            account_id: String::new(),
            url: crate::pullrequest::azure_web_url(host, org, &project, &repo, number),
            repo: RepoLocation::Azure { host: azure::web_host(host).to_string(), org: org.to_string(), project, repo },
            number,
            title: self.title.unwrap_or_default(),
            author: self.created_by.as_ref().map(AzIdentityRef::name).unwrap_or_default(),
            created_by_me,
            review_requested,
            draft: self.is_draft.unwrap_or(false),
            reviewers: self.reviewers.iter().map(|r| r.identity.name()).filter(|n| !n.is_empty()).collect(),
            // L'API ne donne pas de date de mise à jour.
            updated_at: self.creation_date,
            source_branch: short_branch(self.source_ref_name),
            target_branch: short_branch(self.target_ref_name),
        })
    }
}

/// Pull requests d'une organisation : identité de l'utilisateur (`connectionData`), puis celles qu'il a créées et
/// celles dont il est relecteur.
fn azure_org_pull_requests(
    host: &str,
    org: &str,
    get: &mut impl FnMut(&str) -> Result<String, ForgeError>,
) -> Result<Vec<PullRequest>, ForgeError> {
    let identity =
        azure::authenticated_user_id(&get(&format!("{}/_apis/connectionData", azure::org_base(host, org)))?)?;
    let mut prs = Vec::new();
    for (criterion, reviewer_query) in [("creatorId", false), ("reviewerId", true)] {
        let raw: Vec<AzPullRequest> = http::paginate(PAGE_SIZE, MAX_LIST_PAGES, |page| {
            let body = get(&azure_pull_requests_url(host, org, criterion, &identity, page))?;
            Ok(serde_json::from_str::<AzList<AzPullRequest>>(&body).map_err(parse_error)?.value)
        })?;
        prs.extend(raw.into_iter().filter_map(|pr| pr.into_pull_request(host, org, &identity, reviewer_query)));
    }
    Ok(prs)
}

fn azure_list(
    host: &str,
    orgs: &[String],
    mut get: impl FnMut(&str) -> Result<String, ForgeError>,
) -> Result<Vec<PullRequest>, ForgeError> {
    let orgs = azure::clean_orgs(orgs);
    azure::merge_org_results(orgs.into_iter().map(|org| azure_org_pull_requests(host, org, &mut get)))
}

// --- Commun -------------------------------------------------------------------------------------

/// Une entrée par pull request (même page web : les indicateurs des différentes recherches se cumulent), rattachée au
/// compte, de la plus récemment mise à jour à la plus ancienne, puis par dépôt et numéro.
fn finalize(prs: Vec<PullRequest>, account_id: &str) -> Vec<PullRequest> {
    let mut out: Vec<PullRequest> = Vec::with_capacity(prs.len());
    for mut pr in prs {
        if let Some(seen) = out.iter_mut().find(|p| p.url == pr.url) {
            seen.created_by_me |= pr.created_by_me;
            seen.review_requested |= pr.review_requested;
            continue;
        }
        pr.account_id = account_id.to_string();
        out.push(pr);
    }
    out.sort_by_cached_key(|pr| {
        (
            std::cmp::Reverse(pr.updated_at.as_deref().and_then(timestamp_key)),
            pr.repo.display_name().to_lowercase(),
            std::cmp::Reverse(pr.number),
        )
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB_SEARCH: &str = include_str!("../tests/fixtures/github_pull_requests_search.json");
    const GITLAB_MERGE_REQUESTS: &str = include_str!("../tests/fixtures/gitlab_merge_requests.json");
    const AZURE_PULL_REQUESTS: &str = include_str!("../tests/fixtures/azure_pull_requests.json");
    const AZURE_CONNECTION_DATA: &str = include_str!("../tests/fixtures/azure_connection_data.json");

    fn response(status: u16, body: &str) -> http::Response {
        http::Response { status, body: body.into(), ratelimit_remaining: None }
    }

    fn titles(prs: &[PullRequest]) -> Vec<&str> {
        prs.iter().map(|pr| pr.title.as_str()).collect()
    }

    // --- GitHub ---

    #[test]
    fn github_searches_involve_the_user_then_the_review_requests() {
        assert_eq!(
            github_queries("jsmith"),
            [
                "is:pr is:open archived:false involves:jsmith sort:updated-desc",
                "is:pr is:open archived:false review-requested:jsmith sort:updated-desc",
            ]
        );
        assert!(github_queries(" ")[0].contains("involves:@me"), "utilisateur inconnu : @me");
    }

    #[test]
    fn github_search_request_targets_the_graphql_endpoint_of_the_host() {
        let first = github_search_request("github.com", "is:pr involves:jsmith", None);
        assert_eq!(first.url, "https://api.github.com/graphql");
        assert_eq!(first.body["variables"], json!({ "q": "is:pr involves:jsmith", "first": 50, "after": null }));
        let query = first.body["query"].as_str().unwrap();
        for field in ["search(", "type: ISSUE", "headRefName", "baseRefName", "isDraft", "reviewRequests", "endCursor"]
        {
            assert!(query.contains(field), "{field} absent de la requête");
        }
        let next = github_search_request("github.contoso.test", "q", Some("Y3Vyc29yOjQ="));
        assert_eq!(next.url, "https://github.contoso.test/api/graphql");
        assert_eq!(next.body["variables"]["after"], json!("Y3Vyc29yOjQ="));
    }

    #[test]
    fn github_search_fixture_parses() {
        let page = parse_github_search(GITHUB_SEARCH, "github.com", "jsmith").unwrap();
        assert_eq!(page.next.as_deref(), Some("Y3Vyc29yOjQ="));
        // Le nœud vide (résultat qui n'est pas une pull request) est ignoré.
        assert_eq!(
            titles(&page.prs),
            ["Ajoute la page d'accueil", "Corrige le calcul des remises", "Met à jour les dépendances"]
        );

        let home = &page.prs[0];
        assert_eq!(
            home.repo,
            RepoLocation::GitHub { host: "github.com".into(), owner: "contoso".into(), repo: "northwind".into() }
        );
        assert_eq!(home.number, 1347);
        assert_eq!(home.url, "https://github.com/contoso/northwind/pull/1347");
        assert_eq!(home.author, "JSmith");
        assert!(home.created_by_me, "le login se compare sans la casse");
        assert!(!home.review_requested);
        assert!(!home.draft);
        assert_eq!(home.reviewers, ["asmith", "Relecteurs"]);
        assert_eq!(home.updated_at.as_deref(), Some("2026-10-20T08:15:00Z"));
        assert_eq!(
            (home.source_branch.as_deref(), home.target_branch.as_deref()),
            (Some("feature/accueil"), Some("main"))
        );
        assert_eq!(home.reference(), "#1347");

        let billing = &page.prs[1];
        assert!(billing.draft);
        assert!(!billing.created_by_me);
        assert!(billing.review_requested, "relecture demandée à l'utilisateur");
        assert_eq!(billing.repo.display_name(), "fabrikam-labs/billing");

        // Auteur supprimé (« ghost ») : vide.
        assert_eq!(page.prs[2].author, "");
        assert!(page.prs[2].reviewers.is_empty());
    }

    #[test]
    fn github_errors_without_results_are_reported() {
        let refused = r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded for user ID 1."}]}"#;
        match parse_github_search(refused, "github.com", "jsmith") {
            Err(ForgeError::Parse(msg)) => assert!(msg.contains("rate limit"), "{msg}"),
            Err(other) => panic!("{other:?}"),
            Ok(_) => panic!("erreur attendue"),
        }
        // Résultats partiels (organisation protégée par SSO) : les erreurs n'empêchent pas la liste.
        let partial = r#"{"data":{"search":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[]}},
            "errors":[{"type":"FORBIDDEN","message":"Resource protected by organization SAML enforcement."}]}"#;
        let page = parse_github_search(partial, "github.com", "jsmith").unwrap();
        assert!(page.prs.is_empty() && page.next.is_none());
    }

    #[test]
    fn github_list_follows_the_cursor_and_marks_the_review_requests() {
        let last_page = r#"{"data":{"search":{"pageInfo":{"hasNextPage":false,"endCursor":"Zg=="},"nodes":[]}}}"#;
        let review_page = r#"{"data":{"search":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[
            {"number":1347,"title":"Ajoute la page d'accueil","url":"https://github.com/contoso/northwind/pull/1347",
             "isDraft":false,"updatedAt":"2026-10-20T08:15:00Z","headRefName":"feature/accueil","baseRefName":"main",
             "author":{"login":"JSmith"},"repository":{"name":"northwind","owner":{"login":"contoso"}},
             "reviewRequests":{"nodes":[]}}]}}}"#;
        let mut sent = Vec::new();
        let prs = github_list("github.com", "jsmith", |request| {
            let query = request.body["variables"]["q"].as_str().unwrap().to_string();
            let after = request.body["variables"]["after"].as_str().map(str::to_owned);
            sent.push((query.clone(), after.clone()));
            Ok(response(
                200,
                match (query.contains("involves:"), after.as_deref()) {
                    (true, None) => GITHUB_SEARCH,
                    (true, Some(_)) => last_page,
                    (false, _) => review_page,
                },
            ))
        })
        .unwrap();
        assert_eq!(sent.len(), 3, "deux pages de la première recherche, une de la seconde : {sent:?}");
        assert_eq!(sent[1].1.as_deref(), Some("Y3Vyc29yOjQ="));
        // La pull request trouvée par la seconde recherche est marquée « relecture demandée ».
        let home = prs.iter().find(|pr| pr.number == 1347 && pr.review_requested);
        assert!(home.is_some(), "{prs:?}");
    }

    #[test]
    fn github_list_stops_at_the_first_error() {
        let result = github_list("github.com", "jsmith", |_| Ok(response(401, r#"{"message":"Bad credentials"}"#)));
        assert!(matches!(result, Err(ForgeError::Unauthorized)), "{result:?}");
    }

    // --- GitLab ---

    #[test]
    fn gitlab_urls_ask_for_the_opened_merge_requests_of_the_user() {
        let base = "https://gitlab.contoso.test/api/v4";
        assert_eq!(
            gitlab_merge_requests_url(base, GitlabScope::CreatedByMe, "jsmith", 1),
            "https://gitlab.contoso.test/api/v4/merge_requests?scope=created_by_me&state=opened\
             &order_by=updated_at&sort=desc&per_page=100&page=1"
        );
        assert_eq!(
            gitlab_merge_requests_url(base, GitlabScope::AssignedToMe, "jsmith", 2),
            "https://gitlab.contoso.test/api/v4/merge_requests?scope=assigned_to_me&state=opened\
             &order_by=updated_at&sort=desc&per_page=100&page=2"
        );
        assert_eq!(
            gitlab_merge_requests_url(base, GitlabScope::Reviewer, "j smith", 1),
            "https://gitlab.contoso.test/api/v4/merge_requests?scope=all&state=opened&reviewer_username=j%20smith\
             &order_by=updated_at&sort=desc&per_page=100&page=1"
        );
    }

    /// Réponses GitLab préparées : la fixture pour les MR créées par l'utilisateur, une liste vide pour les autres.
    fn gitlab_answers(url: &str) -> Result<String, ForgeError> {
        Ok(if url.contains("scope=created_by_me") { GITLAB_MERGE_REQUESTS.to_string() } else { "[]".to_string() })
    }

    #[test]
    fn gitlab_merge_requests_fixture_parses() {
        let prs = finalize(gitlab_list("gitlab.contoso.test", "jsmith", gitlab_answers).unwrap(), "acc-2");
        // De la plus récente à la plus ancienne.
        assert_eq!(titles(&prs), ["Draft: Migre vers la nouvelle API", "Ajoute l'export CSV"]);

        let export = &prs[1];
        assert_eq!(export.account_id, "acc-2");
        assert_eq!(
            export.repo,
            RepoLocation::GitLab { host: "gitlab.contoso.test".into(), project: "fabrikam/outils/cli".into() }
        );
        assert_eq!(export.number, 7);
        assert_eq!(export.reference(), "!7");
        assert_eq!(export.url, "https://gitlab.contoso.test/fabrikam/outils/cli/-/merge_requests/7");
        assert_eq!(export.author, "jsmith");
        assert!(export.created_by_me && !export.review_requested && !export.draft);
        assert_eq!(export.reviewers, ["asmith"]);
        assert_eq!(export.updated_at.as_deref(), Some("2026-10-19T14:30:00.123Z"));
        assert_eq!(
            (export.source_branch.as_deref(), export.target_branch.as_deref()),
            (Some("feature/export-csv"), Some("main"))
        );

        // Ancienne version de GitLab : `work_in_progress` au lieu de `draft`, pas de `references` (projet lu dans
        // l'adresse de la page).
        let migration = &prs[0];
        assert!(migration.draft);
        assert_eq!(migration.repo.display_name(), "northwind/api");
        assert!(migration.review_requested, "relecteur (casse ignorée)");
        assert!(!migration.created_by_me);
    }

    #[test]
    fn gitlab_reviewer_results_are_kept_only_when_the_user_is_a_reviewer() {
        // Une instance qui ignorerait `reviewer_username` renverrait toutes les merge requests : seules celles dont
        // l'utilisateur est relecteur sont gardées.
        let prs = gitlab_list("gitlab.contoso.test", "jsmith", |url| {
            Ok(if url.contains("reviewer_username=") { GITLAB_MERGE_REQUESTS.to_string() } else { "[]".to_string() })
        })
        .unwrap();
        assert_eq!(titles(&prs), ["Draft: Migre vers la nouvelle API"]);
    }

    #[test]
    fn gitlab_list_reads_every_page_of_each_scope() {
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|iid| {
                json!({"iid": iid, "title": format!("MR {iid}"), "author": {"username": "jsmith"},
                       "web_url": format!("https://gitlab.com/a/b/-/merge_requests/{iid}"),
                       "references": {"full": format!("a/b!{iid}")}})
            })
            .collect();
        let full = serde_json::to_string(&full).unwrap();
        let mut requested = Vec::new();
        let prs = gitlab_list("gitlab.com", "jsmith", |url| {
            requested.push(url.to_string());
            Ok(if url.contains("scope=created_by_me") && url.ends_with("page=1") { full.clone() } else { "[]".into() })
        })
        .unwrap();
        assert_eq!(prs.len(), 100);
        let created: Vec<&String> = requested.iter().filter(|u| u.contains("created_by_me")).collect();
        assert_eq!(created.len(), 2, "page pleine : la suivante est demandée");
        assert_eq!(requested.len(), 4, "une page de plus pour les MR assignées et pour les relectures");
    }

    // --- Azure DevOps ---

    #[test]
    fn azure_urls_filter_the_active_pull_requests_by_identity() {
        assert_eq!(
            azure_pull_requests_url("dev.azure.com", "fabrikam", "creatorId", "5d1c7a2e", 1),
            "https://dev.azure.com/fabrikam/_apis/git/pullrequests?searchCriteria.status=active\
             &searchCriteria.creatorId=5d1c7a2e&$top=100&$skip=0&api-version=7.1"
        );
        assert_eq!(
            azure_pull_requests_url("tfs.contoso.test", "tfs/DefaultCollection", "reviewerId", "5d1c7a2e", 3),
            "https://tfs.contoso.test/tfs/DefaultCollection/_apis/git/pullrequests?searchCriteria.status=active\
             &searchCriteria.reviewerId=5d1c7a2e&$top=100&$skip=200&api-version=5.0"
        );
    }

    /// Réponses Azure DevOps préparées pour l'organisation `fabrikam` : l'identité, puis la fixture pour les pull
    /// requests de relecture et une liste vide pour celles créées par l'utilisateur.
    fn azure_answers(url: &str) -> Result<String, ForgeError> {
        if url.ends_with("/_apis/connectionData") {
            return Ok(AZURE_CONNECTION_DATA.to_string());
        }
        assert!(url.contains("searchCriteria.status=active"), "{url}");
        assert!(url.contains("=5d1c7a2e-3b4f-4c6d-8e9f-0a1b2c3d4e5f&"), "identité de connectionData : {url}");
        Ok(if url.contains("reviewerId") {
            AZURE_PULL_REQUESTS.to_string()
        } else {
            r#"{"value":[],"count":0}"#.into()
        })
    }

    #[test]
    fn azure_pull_requests_fixture_parses() {
        let orgs = ["fabrikam".to_string()];
        let prs = finalize(azure_list("dev.azure.com", &orgs, azure_answers).unwrap(), "acc-3");
        assert_eq!(titles(&prs), ["Prépare la version 2", "Ajoute la recherche"]);

        let search = &prs[1];
        assert_eq!(
            search.repo,
            RepoLocation::Azure {
                host: "dev.azure.com".into(),
                org: "fabrikam".into(),
                project: "Fabrikam Web".into(),
                repo: "Fabrikam-Fiber".into()
            }
        );
        assert_eq!(search.url, "https://dev.azure.com/fabrikam/Fabrikam%20Web/_git/Fabrikam-Fiber/pullrequest/12");
        assert_eq!(search.number, 12);
        assert_eq!(search.reference(), "#12");
        assert_eq!(search.author, "Jean Smith");
        assert!(!search.created_by_me);
        assert!(search.review_requested, "relecteur sans vote (identité comparée sans la casse)");
        // Le nom d'une équipe perd son préfixe de projet.
        assert_eq!(search.reviewers, ["Camille Martin", "Fabrikam Web Team"]);
        assert_eq!(
            (search.source_branch.as_deref(), search.target_branch.as_deref()),
            (Some("feature/recherche"), Some("main"))
        );
        // Pas de date de mise à jour dans l'API : celle de création.
        assert_eq!(search.updated_at.as_deref(), Some("2026-10-17T10:00:00.1234567Z"));

        let release = &prs[0];
        assert!(release.created_by_me && release.draft);
        assert!(!release.review_requested, "l'auteur n'est pas relecteur");
    }

    #[test]
    fn azure_review_already_voted_is_not_requested_anymore() {
        // Premier relecteur de la première pull request : l'utilisateur, qui approuve.
        let voted = AZURE_PULL_REQUESTS.replacen("\"vote\": 0,", "\"vote\": 10,", 1);
        let orgs = ["fabrikam".to_string()];
        let prs = azure_list("dev.azure.com", &orgs, |url| {
            if url.contains("reviewerId") { Ok(voted.clone()) } else { azure_answers(url) }
        })
        .unwrap();
        let search = prs.iter().find(|pr| pr.number == 12).unwrap();
        assert!(!search.review_requested, "l'utilisateur a déjà voté");
    }

    #[test]
    fn azure_review_requested_from_a_team_of_the_user() {
        // L'utilisateur n'est pas relecteur en personne : le critère `reviewerId` l'a trouvé par une de ses équipes.
        let team_only =
            AZURE_PULL_REQUESTS.replace("5D1C7A2E-3B4F-4C6D-8E9F-0A1B2C3D4E5F", "11111111-2222-4333-8444-555555555555");
        let orgs = ["fabrikam".to_string()];
        let found_as_reviewer = azure_list("dev.azure.com", &orgs, |url| {
            if url.contains("reviewerId") { Ok(team_only.clone()) } else { azure_answers(url) }
        })
        .unwrap();
        assert!(found_as_reviewer.iter().any(|pr| pr.number == 12 && pr.review_requested));
        // Trouvée comme auteur, sans y figurer comme relecteur : rien à relire.
        let found_as_author = azure_list("dev.azure.com", &orgs, |url| {
            if url.contains("creatorId") {
                Ok(team_only.clone())
            } else if url.contains("reviewerId") {
                Ok(r#"{"value":[]}"#.into())
            } else {
                azure_answers(url)
            }
        })
        .unwrap();
        assert!(found_as_author.iter().all(|pr| !pr.review_requested), "{found_as_author:?}");
    }

    #[test]
    fn azure_failing_organisations_are_ignored_when_another_answers() {
        let orgs = ["contoso".to_string(), "fabrikam".to_string()];
        let prs = azure_list("dev.azure.com", &orgs, |url| {
            if url.starts_with("https://dev.azure.com/contoso/") {
                Err(ForgeError::Unauthorized)
            } else {
                azure_answers(url)
            }
        })
        .unwrap();
        assert_eq!(prs.len(), 2);
        let failed = azure_list("dev.azure.com", &orgs[..1], |_| Err(ForgeError::Unauthorized));
        assert!(matches!(failed, Err(ForgeError::Unauthorized)));
        assert!(azure_list("dev.azure.com", &[], |_| panic!("aucune requête sans organisation")).unwrap().is_empty());
    }

    #[test]
    fn azure_anonymous_identity_is_unauthorized() {
        let anonymous = r#"{"authenticatedUser":{"id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
            "descriptor":"System:PublicAccess;aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"}}"#;
        let orgs = ["fabrikam".to_string()];
        let result = azure_list("dev.azure.com", &orgs, |url| {
            assert!(url.ends_with("/_apis/connectionData"), "aucune liste sans identité : {url}");
            Ok(anonymous.to_string())
        });
        assert!(matches!(result, Err(ForgeError::Unauthorized)), "{result:?}");
    }

    // --- Commun ---

    fn pr(url: &str, updated_at: Option<&str>) -> PullRequest {
        PullRequest {
            account_id: String::new(),
            repo: RepoLocation::GitHub { host: "github.com".into(), owner: "contoso".into(), repo: "northwind".into() },
            number: 1,
            title: url.into(),
            author: "jsmith".into(),
            draft: false,
            reviewers: vec![],
            created_by_me: false,
            review_requested: false,
            url: url.into(),
            updated_at: updated_at.map(str::to_owned),
            source_branch: None,
            target_branch: None,
        }
    }

    #[test]
    fn finalize_merges_duplicates_and_sorts_by_update_date() {
        let mut mine = pr("a", Some("2026-10-01T00:00:00Z"));
        mine.created_by_me = true;
        let mut requested = pr("a", Some("2026-10-01T00:00:00Z"));
        requested.review_requested = true;
        let prs = finalize(
            vec![
                mine,
                pr("b", None),
                pr("c", Some("2026-10-02T00:00:00+02:00")),
                requested,
                pr("d", Some("2026-10-03T00:00:00Z")),
            ],
            "acc",
        );
        assert_eq!(titles(&prs), ["d", "c", "a", "b"]);
        let a = &prs[2];
        assert!(a.created_by_me && a.review_requested, "les indicateurs des deux recherches se cumulent");
        assert!(prs.iter().all(|pr| pr.account_id == "acc"));
    }

    #[test]
    fn an_empty_token_is_rejected_without_network() {
        let account = Account {
            id: "0123456789abcdef".into(),
            kind: ForgeKind::GitHub,
            host: "github.com".into(),
            username: "jsmith".into(),
            display_name: String::new(),
            orgs: vec![],
            avatar_url: None,
        };
        assert!(matches!(list_pull_requests(&account, " \n"), Err(ForgeError::InvalidInput(_))));
    }
}
