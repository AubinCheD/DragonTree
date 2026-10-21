//! Dépôt d'une forge désigné par l'URL d'un remote : analyse (HTTPS, SSH, Azure DevOps) et page web de création
//! d'une pull request.

use crate::auth::{RemoteUrl, account_matches_url, parse_remote_url};
use crate::azure::{encode_path, is_services_host, org_base};
use crate::model::{Account, ForgeKind, normalize_host};

/// Dépôt distant d'une forge, noms décodés (sans `%20`) et sans suffixe `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoLocation {
    GitHub {
        host: String,
        owner: String,
        repo: String,
    },
    /// `project` : chemin complet avec les groupes (`groupe/sous-groupe/projet`).
    GitLab {
        host: String,
        project: String,
    },
    /// `org` : l'organisation (Services) ou la collection (Server, éventuellement `tfs/Collection`).
    Azure {
        host: String,
        org: String,
        project: String,
        repo: String,
    },
}

impl RepoLocation {
    /// Analyse l'URL d'un remote. Avec un `account` qui correspond à l'URL, sa forge et son hôte (port compris) font
    /// foi ; sinon la forge est reconnue d'après l'hôte (github.com, gitlab.com, Azure DevOps, `_git` dans le chemin,
    /// « github » ou « gitlab » dans le nom d'hôte). `None` : URL illisible ou forge non reconnue.
    pub fn parse(url: &str, account: Option<&Account>) -> Option<RepoLocation> {
        let remote = parse_remote_url(url)?;
        let account = account.filter(|a| account_matches_url(a, url));
        let kind = match account {
            Some(a) => a.kind,
            None => detect_kind(&remote)?,
        };
        let path = remote.path.split(['?', '#']).next().unwrap_or_default();
        let mut segments: Vec<String> = path.split('/').filter(|s| !s.is_empty()).map(percent_decode).collect();
        if let Some(last) = segments.last_mut()
            && let Some(stripped) = last.strip_suffix(".git")
        {
            *last = stripped.to_string();
        }
        let account_host = account.map(|a| normalize_host(&a.host, a.kind));
        match kind {
            ForgeKind::GitHub => {
                let [owner, repo, ..] = segments.as_slice() else { return None };
                let host = account_host.unwrap_or_else(|| web_host(&remote.host));
                Some(RepoLocation::GitHub { host, owner: owner.clone(), repo: repo.clone() })
            }
            ForgeKind::GitLab => {
                if segments.len() < 2 {
                    return None;
                }
                let host = account_host.unwrap_or_else(|| web_host(&remote.host));
                Some(RepoLocation::GitLab { host, project: segments.join("/") })
            }
            ForgeKind::AzureDevOps => parse_azure(&remote, &segments, account_host),
        }
    }

    pub fn kind(&self) -> ForgeKind {
        match self {
            RepoLocation::GitHub { .. } => ForgeKind::GitHub,
            RepoLocation::GitLab { .. } => ForgeKind::GitLab,
            RepoLocation::Azure { .. } => ForgeKind::AzureDevOps,
        }
    }

    /// Hôte web de la forge (avec son port s'il y en a un).
    pub fn host(&self) -> &str {
        match self {
            RepoLocation::GitHub { host, .. }
            | RepoLocation::GitLab { host, .. }
            | RepoLocation::Azure { host, .. } => host,
        }
    }

    /// `propriétaire/dépôt`, `groupe/projet` ou `organisation/projet/dépôt`.
    pub fn display_name(&self) -> String {
        match self {
            RepoLocation::GitHub { owner, repo, .. } => format!("{owner}/{repo}"),
            RepoLocation::GitLab { project, .. } => project.clone(),
            RepoLocation::Azure { org, project, repo, .. } => format!("{org}/{project}/{repo}"),
        }
    }

    /// Même dépôt de la même forge : hôte et noms comparés sans tenir compte de la casse, que les forges ignorent
    /// (`Contoso/Northwind` et `contoso/northwind` désignent le même dépôt).
    pub fn same_repo(&self, other: &RepoLocation) -> bool {
        let eq = |a: &str, b: &str| a == b || a.to_lowercase() == b.to_lowercase();
        match (self, other) {
            (
                RepoLocation::GitHub { host, owner, repo },
                RepoLocation::GitHub { host: other_host, owner: other_owner, repo: other_repo },
            ) => eq(host, other_host) && eq(owner, other_owner) && eq(repo, other_repo),
            (
                RepoLocation::GitLab { host, project },
                RepoLocation::GitLab { host: other_host, project: other_project },
            ) => eq(host, other_host) && eq(project, other_project),
            (
                RepoLocation::Azure { host, org, project, repo },
                RepoLocation::Azure { host: other_host, org: other_org, project: other_project, repo: other_repo },
            ) => eq(host, other_host) && eq(org, other_org) && eq(project, other_project) && eq(repo, other_repo),
            _ => false,
        }
    }

    /// Page web du dépôt.
    pub fn web_url(&self) -> String {
        match self {
            RepoLocation::GitHub { host, owner, repo } => {
                format!("https://{host}/{}/{}", encode_path(owner), encode_path(repo))
            }
            RepoLocation::GitLab { host, project } => format!("https://{host}/{}", encode_path(project)),
            RepoLocation::Azure { host, org, project, repo } => {
                format!("{}/{}/_git/{}", org_base(host, org), encode_path(project), encode_path(repo))
            }
        }
    }

    /// Page web qui ouvre le formulaire de création d'une pull request (merge request) de `source` vers `target`.
    pub fn web_create_url(&self, source: &str, target: &str) -> String {
        match self {
            RepoLocation::GitHub { host, owner, repo } => format!(
                "https://{host}/{}/{}/compare/{}...{}?expand=1",
                encode_path(owner),
                encode_path(repo),
                encode_path(target),
                encode_path(source)
            ),
            RepoLocation::GitLab { host, project } => format!(
                "https://{host}/{}/-/merge_requests/new?merge_request[source_branch]={}&merge_request[target_branch]={}",
                encode_path(project),
                encode_component(source),
                encode_component(target)
            ),
            RepoLocation::Azure { host, org, project, repo } => format!(
                "{}/{}/_git/{}/pullrequestcreate?sourceRef={}&targetRef={}",
                org_base(host, org),
                encode_path(project),
                encode_path(repo),
                encode_component(source),
                encode_component(target)
            ),
        }
    }
}

/// Forge d'après l'hôte et le chemin d'une URL de remote, quand aucun compte n'en décide.
fn detect_kind(remote: &RemoteUrl) -> Option<ForgeKind> {
    let host = remote.host.as_str();
    if is_azure_services(host) || remote.path.contains("_git/") {
        Some(ForgeKind::AzureDevOps)
    } else if host.contains("github") || host.ends_with(".ghe.com") {
        Some(ForgeKind::GitHub)
    } else if host.contains("gitlab") {
        Some(ForgeKind::GitLab)
    } else {
        None
    }
}

/// Hôtes d'Azure DevOps Services, SSH compris.
fn is_azure_services(host: &str) -> bool {
    is_services_host(host) || host == "ssh.dev.azure.com"
}

/// Hôte web d'un hôte de remote : les hôtes SSH alternatifs de GitHub et GitLab redonnent le site.
fn web_host(host: &str) -> String {
    match host {
        "ssh.github.com" => "github.com".to_string(),
        "altssh.gitlab.com" => "gitlab.com".to_string(),
        other => other.to_string(),
    }
}

fn parse_azure(remote: &RemoteUrl, segments: &[String], account_host: Option<String>) -> Option<RepoLocation> {
    let services = match &account_host {
        Some(host) => is_services_host(host),
        None => is_azure_services(&remote.host),
    };
    if services {
        let host = "dev.azure.com".to_string();
        // SSH : `v3/<org>/<projet>/<dépôt>`.
        if let [v3, org, project, repo] = segments
            && v3 == "v3"
            && matches!(remote.host.as_str(), "ssh.dev.azure.com" | "vs-ssh.visualstudio.com")
        {
            return Some(RepoLocation::Azure { host, org: org.clone(), project: project.clone(), repo: repo.clone() });
        }
        let idx = segments.iter().position(|s| s == "_git")?;
        let repo = segments.get(idx + 1)?.clone();
        let mut before = &segments[..idx];
        let org = match remote.host.strip_suffix(".visualstudio.com") {
            Some(org) if !org.contains('.') => {
                // Ancienne forme : `<org>.visualstudio.com[/DefaultCollection]/<projet>/_git/<dépôt>`.
                if before.first().is_some_and(|s| s.eq_ignore_ascii_case("DefaultCollection")) {
                    before = &before[1..];
                }
                org.to_string()
            }
            _ => {
                let (org, rest) = before.split_first()?;
                before = rest;
                org.clone()
            }
        };
        let project = before.last().cloned().unwrap_or_else(|| repo.clone());
        return Some(RepoLocation::Azure { host, org, project, repo });
    }
    // Azure DevOps Server : `<hôte>/<collection>/<projet>/_git/<dépôt>`.
    let idx = segments.iter().position(|s| s == "_git")?;
    let repo = segments.get(idx + 1)?.clone();
    let (org, project) = match &segments[..idx] {
        [] => return None,
        [org] => (org.clone(), repo.clone()),
        [org @ .., project] => (org.join("/"), project.clone()),
    };
    Some(RepoLocation::Azure { host: account_host.unwrap_or_else(|| remote.host.clone()), org, project, repo })
}

/// Encode tout sauf les caractères non réservés, `/` compris : valeur d'un paramètre de requête.
pub(crate) fn encode_component(text: &str) -> String {
    encode_path(text).replace('/', "%2F")
}

/// Décode les `%XX` d'un segment de chemin (UTF-8 ; un `%` mal formé reste tel quel).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| char::from(b).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn github(host: &str, owner: &str, repo: &str) -> RepoLocation {
        RepoLocation::GitHub { host: host.into(), owner: owner.into(), repo: repo.into() }
    }

    fn gitlab(host: &str, project: &str) -> RepoLocation {
        RepoLocation::GitLab { host: host.into(), project: project.into() }
    }

    fn azure(host: &str, org: &str, project: &str, repo: &str) -> RepoLocation {
        RepoLocation::Azure { host: host.into(), org: org.into(), project: project.into(), repo: repo.into() }
    }

    fn parse(url: &str) -> Option<RepoLocation> {
        RepoLocation::parse(url, None)
    }

    fn account(kind: ForgeKind, host: &str) -> Account {
        Account {
            id: "0123456789abcdef".into(),
            kind,
            host: host.into(),
            username: "jsmith".into(),
            display_name: String::new(),
            orgs: vec![],
            avatar_url: None,
        }
    }

    #[test]
    fn github_urls_https_and_ssh() {
        let expected = github("github.com", "contoso", "northwind");
        for url in [
            "https://github.com/contoso/northwind.git",
            "https://github.com/contoso/northwind",
            "https://github.com/contoso/northwind/",
            "https://jsmith@github.com/contoso/northwind.git",
            "git@github.com:contoso/northwind.git",
            "ssh://git@github.com/contoso/northwind.git",
            "ssh://git@ssh.github.com:443/contoso/northwind.git",
        ] {
            assert_eq!(parse(url).as_ref(), Some(&expected), "{url}");
        }
    }

    #[test]
    fn gitlab_urls_keep_the_group_path() {
        let expected = gitlab("gitlab.com", "fabrikam/outils/cli");
        for url in [
            "https://gitlab.com/fabrikam/outils/cli.git",
            "git@gitlab.com:fabrikam/outils/cli.git",
            "ssh://git@altssh.gitlab.com:443/fabrikam/outils/cli.git",
        ] {
            assert_eq!(parse(url).as_ref(), Some(&expected), "{url}");
        }
        assert_eq!(parse("https://gitlab.contoso.test/a/b.git"), Some(gitlab("gitlab.contoso.test", "a/b")));
    }

    #[test]
    fn azure_devops_https_urls() {
        let expected = azure("dev.azure.com", "fabrikam", "Mon Projet", "api");
        for url in [
            "https://dev.azure.com/fabrikam/Mon%20Projet/_git/api",
            "https://fabrikam@dev.azure.com/fabrikam/Mon%20Projet/_git/api",
            "https://fabrikam.visualstudio.com/Mon%20Projet/_git/api",
            "https://fabrikam.visualstudio.com/DefaultCollection/Mon%20Projet/_git/api",
        ] {
            assert_eq!(parse(url).as_ref(), Some(&expected), "{url}");
        }
        // Projet du même nom que le dépôt : l'URL abrège.
        assert_eq!(
            parse("https://dev.azure.com/fabrikam/_git/api"),
            Some(azure("dev.azure.com", "fabrikam", "api", "api"))
        );
    }

    #[test]
    fn azure_devops_ssh_urls() {
        let expected = azure("dev.azure.com", "fabrikam", "Projet", "api");
        for url in
            ["git@ssh.dev.azure.com:v3/fabrikam/Projet/api", "fabrikam@vs-ssh.visualstudio.com:v3/fabrikam/Projet/api"]
        {
            assert_eq!(parse(url).as_ref(), Some(&expected), "{url}");
        }
    }

    #[test]
    fn azure_devops_server_uses_the_account_host() {
        let acc = account(ForgeKind::AzureDevOps, "tfs.contoso.test:8080");
        let url = "https://tfs.contoso.test:8080/DefaultCollection/Projet/_git/api";
        assert_eq!(
            RepoLocation::parse(url, Some(&acc)),
            Some(azure("tfs.contoso.test:8080", "DefaultCollection", "Projet", "api"))
        );
        // Sans compte, la forge est reconnue par `_git`, l'hôte garde le nom sans port.
        assert_eq!(parse(url), Some(azure("tfs.contoso.test", "DefaultCollection", "Projet", "api")));
    }

    #[test]
    fn an_account_decides_the_forge_of_a_self_hosted_host() {
        let url = "https://git.contoso.test/equipe/depot.git";
        assert_eq!(parse(url), None, "hôte inconnu sans compte");
        let acc = account(ForgeKind::GitLab, "git.contoso.test");
        assert_eq!(RepoLocation::parse(url, Some(&acc)), Some(gitlab("git.contoso.test", "equipe/depot")));
        // Un compte d'une autre forge ne s'applique pas à cet hôte.
        let other = account(ForgeKind::GitHub, "github.com");
        assert_eq!(RepoLocation::parse(url, Some(&other)), None);
    }

    #[test]
    fn unreadable_urls_are_rejected() {
        for url in ["", "pas une url", "C:\\depots\\x", "https://github.com/seul", "https://dev.azure.com/org"] {
            assert_eq!(parse(url), None, "{url}");
        }
    }

    #[test]
    fn web_urls_to_create_a_pull_request() {
        assert_eq!(
            github("github.com", "contoso", "northwind").web_create_url("feature/x y", "main"),
            "https://github.com/contoso/northwind/compare/main...feature/x%20y?expand=1"
        );
        assert_eq!(
            gitlab("gitlab.com", "fabrikam/outils/cli").web_create_url("feature/a&b", "main"),
            "https://gitlab.com/fabrikam/outils/cli/-/merge_requests/new\
             ?merge_request[source_branch]=feature%2Fa%26b&merge_request[target_branch]=main"
        );
        assert_eq!(
            azure("dev.azure.com", "fabrikam", "Mon Projet", "api").web_create_url("feature/x", "main"),
            "https://dev.azure.com/fabrikam/Mon%20Projet/_git/api/pullrequestcreate?sourceRef=feature%2Fx&targetRef=main"
        );
    }

    #[test]
    fn the_same_repository_is_recognised_whatever_the_case_and_the_remote_form() {
        let pr = github("github.com", "contoso", "northwind");
        for url in ["https://github.com/Contoso/Northwind.git", "git@github.com:contoso/NORTHWIND.git"] {
            assert!(parse(url).unwrap().same_repo(&pr), "{url}");
        }
        assert!(!parse("https://github.com/contoso/northwind-api.git").unwrap().same_repo(&pr));
        assert!(!parse("https://github.com/fabrikam/northwind.git").unwrap().same_repo(&pr));
        // Même nom sur une autre forge ou un autre hôte : un autre dépôt.
        assert!(!gitlab("gitlab.com", "contoso/northwind").same_repo(&pr));
        assert!(!github("github.contoso.test", "contoso", "northwind").same_repo(&pr));

        let azure_pr = azure("dev.azure.com", "fabrikam", "Mon Projet", "api");
        assert!(parse("https://fabrikam.visualstudio.com/mon%20projet/_git/API").unwrap().same_repo(&azure_pr));
        assert!(parse("git@ssh.dev.azure.com:v3/fabrikam/Mon%20Projet/api").unwrap().same_repo(&azure_pr));
        assert!(!azure("dev.azure.com", "fabrikam", "Autre", "api").same_repo(&azure_pr));
        assert!(
            parse("git@gitlab.com:Fabrikam/Outils/CLI.git")
                .unwrap()
                .same_repo(&gitlab("gitlab.com", "fabrikam/outils/cli"))
        );
    }

    #[test]
    fn repository_web_pages() {
        assert_eq!(github("github.com", "contoso", "northwind").web_url(), "https://github.com/contoso/northwind");
        assert_eq!(gitlab("gitlab.contoso.test", "a/b c").web_url(), "https://gitlab.contoso.test/a/b%20c");
        assert_eq!(
            azure("dev.azure.com", "fabrikam", "Mon Projet", "api").web_url(),
            "https://dev.azure.com/fabrikam/Mon%20Projet/_git/api"
        );
    }

    #[test]
    fn display_names() {
        assert_eq!(github("github.com", "contoso", "northwind").display_name(), "contoso/northwind");
        assert_eq!(gitlab("gitlab.com", "a/b/c").display_name(), "a/b/c");
        assert_eq!(azure("dev.azure.com", "fabrikam", "P", "api").display_name(), "fabrikam/P/api");
    }
}
