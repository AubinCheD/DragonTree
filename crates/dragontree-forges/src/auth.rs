//! Authentification git par variables d'environnement, correspondance remote ↔ compte et
//! pages de création de jeton.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use dragontree_i18n::tr;

use crate::azure::is_services_host;
use crate::model::{Account, ForgeKind, normalize_host};

/// Variables d'environnement à passer à un processus git pour que ses requêtes HTTPS
/// portent les identifiants du compte, sans rien écrire sur disque :
/// `GIT_CONFIG_COUNT`, puis `GIT_CONFIG_KEY_<i>` = `http.<origine>.extraHeader` et
/// `GIT_CONFIG_VALUE_<i>` = `Authorization: Basic <base64(utilisateur:jeton)>`.
/// Jeton vide → aucune variable.
///
/// `remote_urls` : URL des remotes du dépôt (ou du clone). git compare le chemin des URL en respectant la casse : pour
/// Azure DevOps, dont l'organisation (ou la collection) est dans le chemin, l'origine est aussi tirée de chaque remote
/// HTTPS du compte, écrite comme lui (`https://dev.azure.com/Contoso/` pour un compte qui connaît `contoso`).
pub fn git_auth_env(account: &Account, token: &str, remote_urls: &[&str]) -> Vec<(String, String)> {
    let token = token.trim();
    if token.is_empty() {
        return Vec::new();
    }
    let user = match account.kind {
        ForgeKind::GitHub | ForgeKind::GitLab => account.username.trim(),
        // Azure DevOps : utilisateur vide, le jeton suffit.
        ForgeKind::AzureDevOps => "",
    };
    let header = format!("Authorization: {}", basic_auth_value(user, token));
    let mut origins = auth_origins(account);
    for url in remote_urls {
        if let Some(origin) = remote_origin(account, url)
            && !origins.contains(&origin)
        {
            origins.push(origin);
        }
    }
    let mut env = Vec::with_capacity(1 + 2 * origins.len());
    env.push(("GIT_CONFIG_COUNT".to_string(), origins.len().to_string()));
    for (i, origin) in origins.iter().enumerate() {
        env.push((format!("GIT_CONFIG_KEY_{i}"), format!("http.{origin}.extraHeader")));
        env.push((format!("GIT_CONFIG_VALUE_{i}"), header.clone()));
    }
    env
}

/// Préfixes d'URL couverts par l'en-tête d'authentification du compte.
fn auth_origins(account: &Account) -> Vec<String> {
    let host = normalize_host(&account.host, account.kind);
    let orgs = clean_orgs(&account.orgs);
    match account.kind {
        ForgeKind::GitHub | ForgeKind::GitLab => vec![format!("https://{host}/")],
        ForgeKind::AzureDevOps if is_services_host(&host) => {
            if orgs.is_empty() {
                return vec!["https://dev.azure.com/".to_string()];
            }
            orgs.iter()
                .flat_map(|org| {
                    [
                        format!("https://dev.azure.com/{org}/"),
                        format!("https://{}.visualstudio.com/", org.to_ascii_lowercase()),
                    ]
                })
                .collect()
        }
        ForgeKind::AzureDevOps => {
            if orgs.is_empty() {
                return vec![format!("https://{host}/")];
            }
            orgs.iter().map(|org| format!("https://{host}/{org}/")).collect()
        }
    }
}

/// Origine d'authentification tirée d'un remote HTTPS du compte Azure DevOps dont le chemin commence par une de ses
/// organisations (collections), casse ignorée : hôte et organisation tels que le remote les écrit. `None` pour les
/// autres forges (l'origine n'a pas de chemin, et git ignore la casse de l'hôte), un remote SSH ou d'un autre compte.
fn remote_origin(account: &Account, url: &str) -> Option<String> {
    if account.kind != ForgeKind::AzureDevOps || !account_matches_url(account, url) {
        return None;
    }
    let rest = url.trim();
    let rest = rest.get(..8).filter(|s| s.eq_ignore_ascii_case("https://")).map(|_| &rest[8..])?;
    let (authority, path) = rest.split_once('/')?;
    let authority = authority.rsplit('@').next()?;
    clean_orgs(&account.orgs).into_iter().find_map(|org| {
        let prefix = path.get(..org.len())?;
        let boundary = path.len() == org.len() || path[org.len()..].starts_with('/');
        (boundary && prefix.eq_ignore_ascii_case(org)).then(|| format!("https://{authority}/{prefix}/"))
    })
}

/// Indique si l'URL d'un remote (HTTPS, SSH ou forme scp `git@hôte:chemin`) relève du compte.
pub fn account_matches_url(account: &Account, url: &str) -> bool {
    let Some(remote) = parse_remote_url(url) else {
        return false;
    };
    let host = normalize_host(&account.host, account.kind);
    let host_name = strip_port(&host);
    match account.kind {
        ForgeKind::GitHub | ForgeKind::GitLab => {
            // SSH par le port 443 : ssh.github.com et altssh.gitlab.com.
            let alternate = match host_name {
                "github.com" => Some("ssh.github.com"),
                "gitlab.com" => Some("altssh.gitlab.com"),
                _ => None,
            };
            remote.host == host_name || Some(remote.host.as_str()) == alternate
        }
        ForgeKind::AzureDevOps if is_services_host(&host) => azure_services_match(&clean_orgs(&account.orgs), &remote),
        ForgeKind::AzureDevOps => {
            let orgs = clean_orgs(&account.orgs);
            let path = remote.path.to_ascii_lowercase();
            remote.host == host_name
                && (orgs.is_empty()
                    || orgs.iter().any(|org| {
                        let org = org.to_ascii_lowercase();
                        path == org || path.starts_with(&format!("{org}/"))
                    }))
        }
    }
}

/// Azure DevOps Services : `dev.azure.com/<org>/…`, `<org>.visualstudio.com`,
/// SSH `ssh.dev.azure.com:v3/<org>/…` ou `vs-ssh.visualstudio.com:v3/<org>/…`.
/// Sans organisation connue, toute URL Azure DevOps Services correspond.
fn azure_services_match(orgs: &[&str], remote: &RemoteUrl) -> bool {
    let known = |org: &str| !org.is_empty() && (orgs.is_empty() || orgs.iter().any(|o| o.eq_ignore_ascii_case(org)));
    let mut segments = remote.path.split('/');
    match remote.host.as_str() {
        "dev.azure.com" => segments.next().is_some_and(known),
        "ssh.dev.azure.com" | "vs-ssh.visualstudio.com" => {
            segments.next() == Some("v3") && segments.next().is_some_and(known)
        }
        other => match other.strip_suffix(".visualstudio.com") {
            Some(org) if !org.contains('.') => known(org),
            _ => false,
        },
    }
}

/// Hôte (minuscules, sans port) et chemin (sans `/` initial) d'une URL de remote.
pub(crate) struct RemoteUrl {
    pub(crate) host: String,
    pub(crate) path: String,
}

pub(crate) fn parse_remote_url(url: &str) -> Option<RemoteUrl> {
    let url = url.trim();
    let (authority, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(scheme.to_ascii_lowercase().as_str(), "https" | "http" | "ssh" | "git+ssh" | "ssh+git" | "git") {
            return None;
        }
        rest.split_once('/').unwrap_or((rest, ""))
    } else {
        // Forme scp : [utilisateur@]hôte:chemin, sans « / » avant les deux-points.
        let (authority, path) = url.split_once(':')?;
        if authority.contains(['/', '\\']) {
            return None;
        }
        (authority, path)
    };
    let host_port = authority.rsplit('@').next()?;
    let host = match host_port.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next()?,
        None => strip_port(host_port),
    };
    // Une seule lettre : lettre de lecteur Windows (C:\…), pas un hôte.
    let is_drive = host.len() == 1 && !url.contains("://");
    let valid = !host.is_empty()
        && !is_drive
        && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'));
    valid.then(|| RemoteUrl { host: host.to_ascii_lowercase(), path: path.trim_start_matches('/').to_string() })
}

fn strip_port(host: &str) -> &str {
    host.split(':').next().unwrap_or(host)
}

fn clean_orgs(orgs: &[String]) -> Vec<&str> {
    orgs.iter().map(|o| o.trim().trim_matches('/')).filter(|o| !o.is_empty()).collect()
}

/// Page web de création d'un jeton d'accès personnel, préremplie quand la forge le permet.
pub fn token_creation_url(kind: ForgeKind, host: &str, org: Option<&str>) -> String {
    let host = normalize_host(host, kind);
    let org = org.map(|o| o.trim().trim_matches('/')).filter(|o| !o.is_empty());
    match kind {
        ForgeKind::GitHub => {
            format!("https://{host}/settings/tokens/new?scopes=repo,read:org,workflow&description=DragonTree")
        }
        ForgeKind::GitLab => format!(
            "https://{host}/-/user_settings/personal_access_tokens?name=DragonTree&scopes=api,read_repository,write_repository"
        ),
        ForgeKind::AzureDevOps if is_services_host(&host) => match org {
            Some(org) => format!("https://dev.azure.com/{org}/_usersSettings/tokens"),
            None => "https://aex.dev.azure.com/me".to_string(),
        },
        ForgeKind::AzureDevOps => match org {
            Some(org) => format!("https://{host}/{org}/_usersSettings/tokens"),
            None => format!("https://{host}/"),
        },
    }
}

/// Droits à accorder au jeton, en clair pour l'utilisateur.
pub fn recommended_scopes(kind: ForgeKind) -> &'static str {
    match kind {
        ForgeKind::GitHub => "repo, read:org, workflow",
        ForgeKind::GitLab => "api, read_repository, write_repository",
        ForgeKind::AzureDevOps => {
            tr!("Organisation : toutes les organisations accessibles — Code (lecture et écriture), \
             Profil utilisateur (lecture)")
        }
    }
}

/// Valeur d'en-tête `Basic <base64(utilisateur:jeton)>`.
pub(crate) fn basic_auth_value(user: &str, token: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{user}:{token}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(kind: ForgeKind, host: &str, username: &str, orgs: &[&str]) -> Account {
        Account {
            id: "0123456789abcdef".into(),
            kind,
            host: host.into(),
            username: username.into(),
            display_name: String::new(),
            orgs: orgs.iter().map(|o| o.to_string()).collect(),
            avatar_url: None,
        }
    }

    fn env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn auth_env_github() {
        let acc = account(ForgeKind::GitHub, "github.com", "octocat", &[]);
        assert_eq!(
            git_auth_env(&acc, "ghp_tok", &[]),
            env(&[
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "http.https://github.com/.extraHeader"),
                // base64("octocat:ghp_tok")
                ("GIT_CONFIG_VALUE_0", "Authorization: Basic b2N0b2NhdDpnaHBfdG9r"),
            ])
        );
    }

    #[test]
    fn auth_env_gitlab_self_hosted() {
        let acc = account(ForgeKind::GitLab, "https://GitLab.Example.com:8443/", "jsmith", &[]);
        assert_eq!(
            git_auth_env(&acc, " glpat-tok\n", &[]),
            env(&[
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "http.https://gitlab.example.com:8443/.extraHeader"),
                // base64("jsmith:glpat-tok")
                ("GIT_CONFIG_VALUE_0", "Authorization: Basic anNtaXRoOmdscGF0LXRvaw=="),
            ])
        );
    }

    #[test]
    fn auth_env_azure_two_orgs() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "camille@example.com", &["fabrikam", "Contoso"]);
        // Utilisateur vide : base64(":azpat").
        let value = "Authorization: Basic OmF6cGF0";
        assert_eq!(
            git_auth_env(&acc, "azpat", &[]),
            env(&[
                ("GIT_CONFIG_COUNT", "4"),
                ("GIT_CONFIG_KEY_0", "http.https://dev.azure.com/fabrikam/.extraHeader"),
                ("GIT_CONFIG_VALUE_0", value),
                ("GIT_CONFIG_KEY_1", "http.https://fabrikam.visualstudio.com/.extraHeader"),
                ("GIT_CONFIG_VALUE_1", value),
                ("GIT_CONFIG_KEY_2", "http.https://dev.azure.com/Contoso/.extraHeader"),
                ("GIT_CONFIG_VALUE_2", value),
                ("GIT_CONFIG_KEY_3", "http.https://contoso.visualstudio.com/.extraHeader"),
                ("GIT_CONFIG_VALUE_3", value),
            ])
        );
    }

    #[test]
    fn auth_env_azure_without_orgs_covers_dev_azure_com() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &[]);
        let vars = git_auth_env(&acc, "azpat", &[]);
        assert_eq!(vars[0], ("GIT_CONFIG_COUNT".into(), "1".into()));
        assert_eq!(vars[1], ("GIT_CONFIG_KEY_0".into(), "http.https://dev.azure.com/.extraHeader".into()));
    }

    #[test]
    fn auth_env_azure_server_uses_collection_origins() {
        let acc = account(ForgeKind::AzureDevOps, "tfs.example.com", "DOMAINE\\camille", &["DefaultCollection"]);
        let vars = git_auth_env(&acc, "azpat", &[]);
        assert_eq!(
            vars,
            env(&[
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "http.https://tfs.example.com/DefaultCollection/.extraHeader"),
                ("GIT_CONFIG_VALUE_0", "Authorization: Basic OmF6cGF0"),
            ])
        );
    }

    /// RV4d : git compare le chemin en respectant la casse ; l'origine est aussi prise telle que le remote l'écrit.
    #[test]
    fn auth_env_azure_follows_the_case_of_the_organisation_in_the_remotes() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &["contoso"]);
        let remotes = [
            "https://Contoso@dev.azure.com/Contoso/Site%20web/_git/Site",
            // Même casse que le compte, SSH, autre organisation, hôte en visualstudio.com : rien de plus.
            "https://dev.azure.com/contoso/Site/_git/Autre",
            "git@ssh.dev.azure.com:v3/CONTOSO/Site/Site",
            "https://dev.azure.com/northwind/X/_git/Y",
            "https://Contoso.visualstudio.com/DefaultCollection/Site/_git/Site",
            // Préfixe seulement : pas la même organisation.
            "https://dev.azure.com/Contoso-Labs/X/_git/Y",
        ];
        let keys: Vec<String> = git_auth_env(&acc, "azpat", &remotes)
            .into_iter()
            .filter(|(k, _)| k.starts_with("GIT_CONFIG_KEY_"))
            .map(|(_, v)| v)
            .collect();
        assert_eq!(
            keys,
            [
                "http.https://dev.azure.com/contoso/.extraHeader",
                "http.https://contoso.visualstudio.com/.extraHeader",
                "http.https://dev.azure.com/Contoso/.extraHeader",
            ]
        );

        // Azure DevOps Server : collection sous un dossier virtuel, port de l'URL du remote.
        let acc =
            account(ForgeKind::AzureDevOps, "tfs.example.com:8443", "DOMAINE\\camille", &["tfs/DefaultCollection"]);
        let vars = git_auth_env(&acc, "azpat", &["https://tfs.example.com:8443/TFS/DefaultCollection/Site/_git/Site"]);
        assert!(vars.contains(&(
            "GIT_CONFIG_KEY_1".into(),
            "http.https://tfs.example.com:8443/TFS/DefaultCollection/.extraHeader".into()
        )));

        // GitHub et GitLab : l'origine n'a pas de chemin, rien à ajouter.
        let acc = account(ForgeKind::GitHub, "github.com", "octocat", &[]);
        let vars = git_auth_env(&acc, "tok", &["https://GitHub.com/Octocat/Hello.git"]);
        assert_eq!(vars[0], ("GIT_CONFIG_COUNT".into(), "1".into()));
    }

    #[test]
    fn auth_env_empty_token_injects_nothing() {
        let acc = account(ForgeKind::GitHub, "github.com", "octocat", &[]);
        assert!(git_auth_env(&acc, "  ", &[]).is_empty());
    }

    #[test]
    fn matches_url_azure_org_and_vs_host() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &["Fabrikam"]);
        assert!(account_matches_url(
            &acc,
            "https://fabrikam@dev.azure.com/fabrikam/Fabrikam%20Web/_git/Fabrikam-Fiber"
        ));
        assert!(account_matches_url(&acc, "https://dev.azure.com/FABRIKAM/Web/_git/Repo"));
        assert!(account_matches_url(&acc, "https://fabrikam.visualstudio.com/DefaultCollection/Web/_git/Repo"));
        assert!(account_matches_url(&acc, "https://Fabrikam.VisualStudio.com/Web/_git/Repo"));
        assert!(account_matches_url(&acc, "https://fabrikam@fabrikam.visualstudio.com/Web/Sub/_git/Repo"));
    }

    #[test]
    fn matches_url_azure_ssh() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &["fabrikam"]);
        assert!(account_matches_url(&acc, "git@ssh.dev.azure.com:v3/fabrikam/Fabrikam%20Web/Fabrikam-Fiber"));
        assert!(account_matches_url(&acc, "ssh://git@ssh.dev.azure.com/v3/fabrikam/Web/Repo"));
        assert!(account_matches_url(&acc, "fabrikam@vs-ssh.visualstudio.com:v3/fabrikam/Web/Repo"));
        assert!(!account_matches_url(&acc, "git@ssh.dev.azure.com:v3/contoso/Web/Repo"));
        assert!(!account_matches_url(&acc, "git@ssh.dev.azure.com:fabrikam/Web/Repo"));
    }

    #[test]
    fn matches_url_github_enterprise_host() {
        let acc = account(ForgeKind::GitHub, "github.example.com", "octocat", &[]);
        assert!(account_matches_url(&acc, "https://github.example.com/team/app.git"));
        assert!(account_matches_url(&acc, "HTTPS://GitHub.Example.com/team/app"));
        assert!(account_matches_url(&acc, "git@github.example.com:team/app.git"));
        assert!(account_matches_url(&acc, "ssh://git@github.example.com:2222/team/app.git"));
        assert!(!account_matches_url(&acc, "https://github.com/team/app.git"));
        assert!(!account_matches_url(&acc, "https://github.example.com.evil.test/team/app.git"));
    }

    #[test]
    fn matches_url_github_and_gitlab_ssh_variants() {
        let gh = account(ForgeKind::GitHub, "github.com", "octocat", &[]);
        assert!(account_matches_url(&gh, "git@github.com:octocat/Hello-World.git"));
        assert!(account_matches_url(&gh, "ssh://git@ssh.github.com:443/octocat/Hello-World.git"));
        assert!(account_matches_url(&gh, "https://octocat@github.com/octocat/Hello-World.git"));
        assert!(!account_matches_url(&gh, "https://gitlab.com/octocat/Hello-World.git"));

        let gl = account(ForgeKind::GitLab, "gitlab.example.com", "jsmith", &[]);
        assert!(account_matches_url(&gl, "ssh://git@gitlab.example.com:2222/group/sub/repo.git"));
        assert!(account_matches_url(&gl, "https://gitlab.example.com/group/sub/repo.git"));
        assert!(!account_matches_url(&gl, "git@gitlab.com:group/repo.git"));
    }

    #[test]
    fn matches_url_rejects_other_org() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &["fabrikam"]);
        assert!(!account_matches_url(&acc, "https://dev.azure.com/contoso/Web/_git/Repo"));
        assert!(!account_matches_url(&acc, "https://contoso.visualstudio.com/Web/_git/Repo"));
        assert!(!account_matches_url(&acc, "https://dev.azure.com/"));
        assert!(!account_matches_url(&acc, "https://github.com/fabrikam/repo.git"));
    }

    #[test]
    fn matches_url_azure_without_orgs_accepts_any_org() {
        let acc = account(ForgeKind::AzureDevOps, "dev.azure.com", "c@example.com", &[]);
        assert!(account_matches_url(&acc, "https://dev.azure.com/contoso/Web/_git/Repo"));
        assert!(account_matches_url(&acc, "git@ssh.dev.azure.com:v3/contoso/Web/Repo"));
        assert!(!account_matches_url(&acc, "https://github.com/contoso/repo.git"));
    }

    #[test]
    fn matches_url_azure_server_collection() {
        let acc = account(ForgeKind::AzureDevOps, "tfs.example.com", "camille", &["DefaultCollection"]);
        assert!(account_matches_url(&acc, "https://tfs.example.com/DefaultCollection/Web/_git/Repo"));
        assert!(account_matches_url(&acc, "ssh://tfs.example.com:22/defaultcollection/Web/_git/Repo"));
        assert!(!account_matches_url(&acc, "https://tfs.example.com/OtherCollection/Web/_git/Repo"));
        assert!(!account_matches_url(&acc, "https://dev.azure.com/DefaultCollection/Web/_git/Repo"));
    }

    #[test]
    fn matches_url_rejects_unparsable_input() {
        let acc = account(ForgeKind::GitHub, "github.com", "octocat", &[]);
        for url in [
            "",
            "   ",
            "pas une url",
            "C:\\DEV\\github.com\\repo",
            "C:/DEV/repo",
            "/home/me/repo",
            "file:///C:/repo",
            "https://",
            "git@:repo.git",
        ] {
            assert!(!account_matches_url(&acc, url), "{url:?}");
        }
    }

    #[test]
    fn token_url_github_has_scopes() {
        assert_eq!(
            token_creation_url(ForgeKind::GitHub, "github.com", None),
            "https://github.com/settings/tokens/new?scopes=repo,read:org,workflow&description=DragonTree"
        );
        assert_eq!(
            token_creation_url(ForgeKind::GitHub, "https://github.example.com/", None),
            "https://github.example.com/settings/tokens/new?scopes=repo,read:org,workflow&description=DragonTree"
        );
    }

    #[test]
    fn token_url_gitlab_prefills_name_and_scopes() {
        assert_eq!(
            token_creation_url(ForgeKind::GitLab, "", None),
            "https://gitlab.com/-/user_settings/personal_access_tokens?name=DragonTree&scopes=api,read_repository,write_repository"
        );
    }

    #[test]
    fn token_url_azure_with_and_without_org() {
        assert_eq!(
            token_creation_url(ForgeKind::AzureDevOps, "dev.azure.com", Some("fabrikam")),
            "https://dev.azure.com/fabrikam/_usersSettings/tokens"
        );
        assert_eq!(token_creation_url(ForgeKind::AzureDevOps, "dev.azure.com", None), "https://aex.dev.azure.com/me");
        assert_eq!(
            token_creation_url(ForgeKind::AzureDevOps, "dev.azure.com", Some("  ")),
            "https://aex.dev.azure.com/me"
        );
        assert_eq!(
            token_creation_url(ForgeKind::AzureDevOps, "tfs.example.com", Some("DefaultCollection")),
            "https://tfs.example.com/DefaultCollection/_usersSettings/tokens"
        );
    }
}
