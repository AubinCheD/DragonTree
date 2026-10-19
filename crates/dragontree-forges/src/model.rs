//! Types partagés : forges, comptes, dépôts distants, erreurs.

use std::fmt;

use dragontree_i18n::tr;
use serde::{Deserialize, Serialize};

/// Type de forge prise en charge.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ForgeKind {
    GitHub,
    GitLab,
    AzureDevOps,
}

impl ForgeKind {
    /// Toutes les forges, dans l'ordre d'affichage.
    pub const ALL: [ForgeKind; 3] = [ForgeKind::GitHub, ForgeKind::GitLab, ForgeKind::AzureDevOps];

    /// Nom affiché dans l'interface.
    pub fn label(&self) -> &'static str {
        match self {
            ForgeKind::GitHub => "GitHub",
            ForgeKind::GitLab => "GitLab",
            ForgeKind::AzureDevOps => "Azure DevOps",
        }
    }

    /// Hôte utilisé quand l'utilisateur n'en saisit pas.
    pub fn default_host(&self) -> &'static str {
        match self {
            ForgeKind::GitHub => "github.com",
            ForgeKind::GitLab => "gitlab.com",
            ForgeKind::AzureDevOps => "dev.azure.com",
        }
    }
}

/// Compte enregistré dans les réglages (le jeton est dans le Gestionnaire d'identifiants).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Account {
    pub id: String,
    pub kind: ForgeKind,
    pub host: String,
    pub username: String,
    pub display_name: String,
    pub orgs: Vec<String>,
    pub avatar_url: Option<String>,
}

impl Account {
    /// Titre affiché, par exemple « GitHub · camille » ou « Azure DevOps · Camille Martin ».
    /// L'hôte est ajouté entre parenthèses s'il diffère de l'hôte par défaut de la forge.
    pub fn title(&self) -> String {
        // Azure : l'identifiant est une adresse e-mail, le nom affiché est plus lisible.
        let (first, second) = match self.kind {
            ForgeKind::AzureDevOps => (&self.display_name, &self.username),
            ForgeKind::GitHub | ForgeKind::GitLab => (&self.username, &self.display_name),
        };
        let who = if first.trim().is_empty() { second } else { first };
        let host = normalize_host(&self.host, self.kind);
        if host == self.kind.default_host() {
            format!("{} · {}", self.kind.label(), who)
        } else {
            format!("{} · {} ({})", self.kind.label(), who, host)
        }
    }
}

/// Identité renvoyée par la vérification d'un jeton.
#[derive(Clone, Debug)]
pub struct VerifiedUser {
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub orgs: Vec<String>,
}

/// Dépôt distant visible par un compte.
#[derive(Clone, Debug)]
pub struct RemoteRepo {
    pub account_id: String,
    pub name: String,
    pub namespace: String,
    pub clone_url: String,
    pub ssh_url: Option<String>,
    pub web_url: String,
    pub description: Option<String>,
    pub private: bool,
    pub default_branch: Option<String>,
    pub updated_at: Option<String>,
}

impl RemoteRepo {
    /// `namespace/nom`.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.namespace, self.name)
    }
}

/// Erreur d'une opération sur une forge.
#[derive(Debug, Clone)]
pub enum ForgeError {
    /// Réponse HTTP inattendue (corps tronqué à 500 caractères).
    Http { status: u16, body: String },
    /// Jeton refusé, expiré ou sans les droits nécessaires.
    Unauthorized,
    /// Erreur de transport (DNS, connexion, TLS, délai dépassé…).
    Network(String),
    /// Réponse illisible.
    Parse(String),
    /// Erreur du Gestionnaire d'identifiants Windows.
    Secret(String),
    /// Saisie invalide (jeton vide, hôte mal formé…).
    InvalidInput(String),
}

impl fmt::Display for ForgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ForgeError::Http { status, body } if body.trim().is_empty() => {
                write!(f, "{}", tr!("Le serveur a répondu avec le statut HTTP {status}", status))
            }
            ForgeError::Http { status, body } => {
                write!(
                    f,
                    "{}",
                    tr!("Le serveur a répondu avec le statut HTTP {status} : {body}", status, body = body.trim())
                )
            }
            ForgeError::Unauthorized => {
                f.write_str(tr!("Accès refusé : le jeton est invalide, expiré ou n'a pas les droits nécessaires"))
            }
            ForgeError::Network(msg) => write!(f, "{}", tr!("Erreur réseau : {msg}", msg)),
            ForgeError::Parse(msg) => write!(f, "{}", tr!("Réponse inattendue du serveur : {msg}", msg)),
            ForgeError::Secret(msg) => write!(f, "{}", tr!("Gestionnaire d'identifiants Windows : {msg}", msg)),
            ForgeError::InvalidInput(msg) => write!(f, "{}", tr!("Saisie invalide : {msg}", msg)),
        }
    }
}

impl std::error::Error for ForgeError {}

/// Nouvel identifiant de compte : 16 caractères hexadécimaux minuscules, uniques.
///
/// Une graine par processus (horloge en nanosecondes, identifiant du processus et clés
/// aléatoires de `RandomState`) est décalée par un compteur atomique puis mélangée par
/// une bijection : deux appels du même processus ne peuvent pas donner le même résultat.
pub fn new_account_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEED: OnceLock<u64> = OnceLock::new();
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let seed = *SEED.get_or_init(|| {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u128(nanos);
        hasher.write_u32(std::process::id());
        hasher.finish()
    });
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}", mix64(seed.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15))))
}

/// Finaliseur de SplitMix64 (bijectif sur `u64`).
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Normalise un hôte saisi par l'utilisateur : sans schéma, chemin, identifiant ni
/// majuscules ; hôte par défaut de la forge si la saisie est vide.
pub(crate) fn normalize_host(host: &str, kind: ForgeKind) -> String {
    let mut h = host.trim();
    if let Some(i) = h.find("://") {
        h = &h[i + 3..];
    }
    let h = h.split(['/', '?', '#']).next().unwrap_or_default();
    let h = h.rsplit('@').next().unwrap_or_default().trim();
    if h.is_empty() { kind.default_host().to_string() } else { h.to_ascii_lowercase() }
}

/// Clé de tri d'un horodatage ISO 8601 : (secondes depuis l'époque Unix, nanosecondes).
/// Accepte `AAAA-MM-JJTHH:MM:SS`, une fraction facultative et `Z`, `±HH:MM`, `±HHMM` ou
/// rien (UTC).
pub(crate) fn timestamp_key(s: &str) -> Option<(i64, u32)> {
    let s = s.trim();
    let b = s.as_bytes();
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = s.get(from..to)?;
        if part.bytes().all(|c| c.is_ascii_digit()) { part.parse().ok() } else { None }
    };
    let sep_ok = b.len() >= 19
        && b[4] == b'-'
        && b[7] == b'-'
        && matches!(b[10], b'T' | b't' | b' ')
        && b[13] == b':'
        && b[16] == b':';
    if !sep_ok {
        return None;
    }
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut rest = &s[19..];
    let mut nanos = 0u32;
    if let Some(frac) = rest.strip_prefix(['.', ',']) {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        for (i, c) in frac.bytes().take(9).take(digits).enumerate() {
            nanos += u32::from(c - b'0') * 10u32.pow(8 - i as u32);
        }
        rest = &frac[digits..];
    }

    let offset = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let digits: String = rest[1..].chars().filter(|c| *c != ':').collect();
            if digits.len() != 4 || !digits.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let (oh, om): (i64, i64) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
            sign * (oh * 3600 + om * 60)
        }
    };

    let days = days_from_civil(year, month, day);
    Some((days * 86_400 + hour * 3600 + minute * 60 + second - offset, nanos))
}

/// Nombre de jours depuis le 1970-01-01 (algorithme de H. Hinnant).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Supprime les doublons (même URL de clonage, premier gardé) puis trie par activité
/// récente décroissante, puis par nom.
pub(crate) fn finalize_repos(repos: Vec<RemoteRepo>) -> Vec<RemoteRepo> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<RemoteRepo> = repos.into_iter().filter(|r| seen.insert(r.clone_url.clone())).collect();
    out.sort_by_cached_key(|r| {
        (
            std::cmp::Reverse(r.updated_at.as_deref().and_then(timestamp_key)),
            r.name.to_lowercase(),
            r.namespace.to_lowercase(),
            r.name.clone(),
        )
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn account(kind: ForgeKind, host: &str, username: &str, display_name: &str) -> Account {
        Account {
            id: "0123456789abcdef".into(),
            kind,
            host: host.into(),
            username: username.into(),
            display_name: display_name.into(),
            orgs: vec![],
            avatar_url: None,
        }
    }

    fn repo(name: &str, namespace: &str, updated_at: Option<&str>) -> RemoteRepo {
        RemoteRepo {
            account_id: "acc".into(),
            name: name.into(),
            namespace: namespace.into(),
            clone_url: format!("https://example.com/{namespace}/{name}.git"),
            ssh_url: None,
            web_url: format!("https://example.com/{namespace}/{name}"),
            description: None,
            private: false,
            default_branch: None,
            updated_at: updated_at.map(str::to_owned),
        }
    }

    #[test]
    fn account_title_formats() {
        assert_eq!(account(ForgeKind::GitHub, "github.com", "camille", "Camille M.").title(), "GitHub · camille");
        assert_eq!(
            account(ForgeKind::AzureDevOps, "dev.azure.com", "camille@example.com", "Camille Martin").title(),
            "Azure DevOps · Camille Martin"
        );
        // Hôte auto-hébergé : précisé pour distinguer plusieurs comptes.
        assert_eq!(
            account(ForgeKind::GitLab, "gitlab.example.com", "jsmith", "Jean Smith").title(),
            "GitLab · jsmith (gitlab.example.com)"
        );
        // Nom affiché absent côté Azure : on retombe sur l'identifiant.
        assert_eq!(
            account(ForgeKind::AzureDevOps, "dev.azure.com", "a@example.com", "").title(),
            "Azure DevOps · a@example.com"
        );
    }

    #[test]
    fn account_deserializes_settings_json() {
        // Format persistant dans settings.json : les noms de variantes sont un contrat.
        let json = r#"{"id":"00ff00ff00ff00ff","kind":"AzureDevOps","host":"dev.azure.com",
            "username":"camille@example.com","display_name":"Camille","orgs":["fabrikam"],
            "avatar_url":null}"#;
        let parsed: Account = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.kind, ForgeKind::AzureDevOps);
        assert_eq!(parsed.orgs, vec!["fabrikam".to_string()]);
        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back["kind"], "AzureDevOps");

        let gh: Account = serde_json::from_str(
            r#"{"id":"1","kind":"GitHub","host":"github.com","username":"octocat","display_name":"","orgs":[],"avatar_url":"https://a/b.png"}"#,
        )
        .unwrap();
        assert_eq!(gh.kind, ForgeKind::GitHub);
        assert_eq!(gh.avatar_url.as_deref(), Some("https://a/b.png"));
        let gl: Account = serde_json::from_str(
            r#"{"id":"2","kind":"GitLab","host":"gitlab.com","username":"j","display_name":"J","orgs":[]}"#,
        )
        .unwrap();
        assert_eq!(gl.kind, ForgeKind::GitLab);
        assert_eq!(gl.avatar_url, None);
    }

    #[test]
    fn forge_error_display_keeps_details() {
        let http = ForgeError::Http { status: 502, body: "Bad Gateway".into() }.to_string();
        assert!(http.contains("502") && http.contains("Bad Gateway"), "{http}");
        let net = ForgeError::Network("hôte introuvable".into()).to_string();
        assert!(net.contains("hôte introuvable"), "{net}");
        assert!(ForgeError::Unauthorized.to_string().contains("jeton"));
    }

    #[test]
    fn new_account_id_is_unique_hex() {
        let ids: Vec<String> = std::thread::scope(|s| {
            let handles: Vec<_> =
                (0..4).map(|_| s.spawn(|| (0..2500).map(|_| new_account_id()).collect::<Vec<_>>())).collect();
            handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(ids.len(), 10_000);
        for id in &ids {
            assert_eq!(id.len(), 16, "{id}");
            assert!(id.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')), "{id}");
        }
        let unique: HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn normalize_host_strips_scheme_path_and_case() {
        assert_eq!(normalize_host("", ForgeKind::GitHub), "github.com");
        assert_eq!(normalize_host("   ", ForgeKind::GitLab), "gitlab.com");
        assert_eq!(normalize_host("", ForgeKind::AzureDevOps), "dev.azure.com");
        assert_eq!(normalize_host("  HTTPS://GitLab.Example.com/ ", ForgeKind::GitLab), "gitlab.example.com");
        assert_eq!(
            normalize_host("https://user@git.example.com:8443/some/path", ForgeKind::GitLab),
            "git.example.com:8443"
        );
        assert_eq!(normalize_host("github.example.com", ForgeKind::GitHub), "github.example.com");
    }

    #[test]
    fn timestamp_key_parses_utc_and_offsets() {
        assert_eq!(timestamp_key("1970-01-01T00:00:00Z"), Some((0, 0)));
        assert_eq!(timestamp_key("2011-01-26T19:06:43Z"), Some((1_296_068_803, 0)));
        assert_eq!(timestamp_key("2024-02-29T12:00:00+02:00"), Some((1_709_200_800, 0)));
        assert_eq!(timestamp_key("2022-06-24T17:11:26.841Z"), Some((1_656_090_686, 841_000_000)));
        assert_eq!(timestamp_key("2000-03-01T00:00:00Z"), Some((951_868_800, 0)));
        assert_eq!(timestamp_key("1969-12-31T23:59:59Z"), Some((-1, 0)));
        assert_eq!(timestamp_key("2024-01-15T09:30:12.1234567+00:00"), Some((1_705_311_012, 123_456_700)));
        assert_eq!(timestamp_key("hier"), None);
        assert_eq!(timestamp_key("2024-13-01T00:00:00Z"), None);
    }

    #[test]
    fn finalize_repos_sorts_by_recent_activity_then_name() {
        let repos = vec![
            repo("beta", "ns", None),
            // 12:00+02:00 = 10:00Z : plus ancien que 10:30Z malgré l'ordre lexical.
            repo("zeta", "ns", Some("2024-02-29T12:00:00+02:00")),
            repo("Gamma", "ns", None),
            repo("alpha", "ns", Some("2024-02-29T10:30:00Z")),
            repo("Delta", "ns", Some("2024-02-29T10:00:00Z")),
        ];
        let names: Vec<String> = finalize_repos(repos).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["alpha", "Delta", "zeta", "beta", "Gamma"]);
    }

    #[test]
    fn finalize_repos_drops_duplicate_clone_urls() {
        let mut moved = repo("app", "team", Some("2024-03-01T00:00:00Z"));
        moved.description = Some("première page".into());
        let mut dup = repo("app", "team", Some("2024-03-02T00:00:00Z"));
        dup.description = Some("page suivante".into());
        let out = finalize_repos(vec![moved, repo("lib", "team", None), dup]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].description.as_deref(), Some("première page"));
    }
}
