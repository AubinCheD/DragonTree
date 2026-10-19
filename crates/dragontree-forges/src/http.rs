//! Client HTTP partagé (ureq 3, native-tls/SChannel) et helpers de réponse.

use dragontree_i18n::tr;

use crate::model::ForgeError;

/// `User-Agent` envoyé à toutes les forges.
pub(crate) const USER_AGENT: &str = "DragonTree/0.1";
/// Nombre d'éléments demandés par page.
pub(crate) const PAGE_SIZE: usize = 100;
/// Nombre maximal de pages lues pour une liste.
pub(crate) const MAX_PAGES: usize = 50;

/// Réponse HTTP lue entièrement.
#[derive(Debug)]
pub(crate) struct Response {
    pub status: u16,
    pub body: String,
    /// En-tête `x-ratelimit-remaining` (GitHub).
    pub ratelimit_remaining: Option<String>,
}

/// Taille maximale d'un corps de réponse lu en mémoire.
const BODY_LIMIT: u64 = 64 * 1024 * 1024;
/// Longueur maximale du corps conservé dans `ForgeError::Http`.
const ERROR_BODY_CHARS: usize = 500;

/// Requête GET avec les en-têtes donnés. Les statuts non 2xx sont renvoyés tels quels.
pub(crate) fn get(url: &str, headers: &[(&str, &str)]) -> Result<Response, ForgeError> {
    let mut request = agent().get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let mut response = request.call().map_err(map_transport_error)?;
    let status = response.status().as_u16();
    let ratelimit_remaining =
        response.headers().get("x-ratelimit-remaining").and_then(|v| v.to_str().ok()).map(str::to_owned);
    let body = response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .lossy_utf8(true)
        .read_to_string()
        .map_err(map_transport_error)?;
    Ok(Response { status, body, ratelimit_remaining })
}

/// 2xx → la réponse ; 401/403 → `Unauthorized` ; autre → `Http` (corps tronqué).
/// Un 403 de limitation de débit GitHub (`x-ratelimit-remaining: 0`) reste une erreur `Http`.
pub(crate) fn check_status(resp: Response) -> Result<Response, ForgeError> {
    match resp.status {
        200..=299 => Ok(resp),
        403 if resp.ratelimit_remaining.as_deref() == Some("0") => Err(http_error(403, &resp.body)),
        401 | 403 => Err(ForgeError::Unauthorized),
        status => Err(http_error(status, &resp.body)),
    }
}

/// Erreur `Http` avec le corps tronqué à 500 caractères.
pub(crate) fn http_error(status: u16, body: &str) -> ForgeError {
    ForgeError::Http { status, body: body.trim().chars().take(ERROR_BODY_CHARS).collect() }
}

/// Lit des pages numérotées à partir de 1 jusqu'à une page incomplète ou `max_pages`.
pub(crate) fn paginate<T>(
    page_size: usize,
    max_pages: usize,
    mut fetch_page: impl FnMut(usize) -> Result<Vec<T>, ForgeError>,
) -> Result<Vec<T>, ForgeError> {
    let mut all = Vec::new();
    for page in 1..=max_pages {
        let items = fetch_page(page)?;
        let full = items.len() >= page_size;
        all.extend(items);
        if !full {
            break;
        }
    }
    Ok(all)
}

/// Erreur de décodage JSON.
pub(crate) fn parse_error(e: serde_json::Error) -> ForgeError {
    ForgeError::Parse(e.to_string())
}

/// Agent partagé, construit au premier appel.
fn agent() -> &'static ureq::Agent {
    use std::sync::OnceLock;
    use std::time::Duration;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::NativeTls)
            // Magasin de certificats Windows : respecte les autorités d'entreprise
            // (proxy filtrant), comme git avec SChannel.
            .root_certs(RootCerts::PlatformVerifier)
            .build();
        ureq::Agent::config_builder()
            .tls_config(tls)
            .proxy(ureq::Proxy::try_from_env())
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .build()
            .new_agent()
    })
}

fn map_transport_error(e: ureq::Error) -> ForgeError {
    use ureq::Error as E;
    match e {
        E::BadUri(msg) => ForgeError::InvalidInput(tr!("adresse invalide ({msg})", msg)),
        E::Http(err) => ForgeError::InvalidInput(tr!("requête invalide ({err})", err)),
        E::HostNotFound => ForgeError::Network(tr!("serveur introuvable (DNS)").into()),
        E::Timeout(t) => ForgeError::Network(tr!("délai d'attente dépassé ({t})", t)),
        E::ConnectionFailed => ForgeError::Network(tr!("connexion impossible").into()),
        E::Io(err) => ForgeError::Network(err.to_string()),
        E::NativeTls(err) => ForgeError::Network(tr!("TLS : {err}", err)),
        E::TooManyRedirects => ForgeError::Network(tr!("trop de redirections").into()),
        E::BodyExceedsLimit(limit) => {
            ForgeError::Network(tr!("réponse trop volumineuse (plus de {limit} octets)", limit))
        }
        other => ForgeError::Network(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp(status: u16, body: &str) -> Response {
        Response { status, body: body.into(), ratelimit_remaining: None }
    }

    #[test]
    fn success_status_passes_through() {
        let ok = check_status(resp(200, "[]")).unwrap();
        assert_eq!(ok.body, "[]");
        assert!(check_status(resp(204, "")).is_ok());
    }

    #[test]
    fn status_401_and_403_are_unauthorized() {
        assert!(matches!(check_status(resp(401, "{}")), Err(ForgeError::Unauthorized)));
        assert!(matches!(check_status(resp(403, "{}")), Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn rate_limited_403_is_not_unauthorized() {
        let mut r = resp(403, r#"{"message":"API rate limit exceeded"}"#);
        r.ratelimit_remaining = Some("0".into());
        match check_status(r) {
            Err(ForgeError::Http { status: 403, body }) => assert!(body.contains("rate limit")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn non_2xx_status_keeps_truncated_body() {
        let long = "é".repeat(600);
        match check_status(resp(500, &long)) {
            Err(ForgeError::Http { status: 500, body }) => assert_eq!(body.chars().count(), 500),
            other => panic!("{other:?}"),
        }
        match check_status(resp(404, "Not Found")) {
            Err(ForgeError::Http { status: 404, body }) => assert_eq!(body, "Not Found"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn paginate_stops_after_short_page() {
        let sizes = [3usize, 3, 2, 3];
        let mut requested = vec![];
        let all = paginate(3, 50, |page| {
            requested.push(page);
            Ok(vec![page; sizes[page - 1]])
        })
        .unwrap();
        assert_eq!(requested, [1, 2, 3]);
        assert_eq!(all, [1, 1, 1, 2, 2, 2, 3, 3]);
    }

    #[test]
    fn paginate_stops_at_page_cap() {
        let mut calls = 0;
        let all = paginate(2, 5, |page| {
            calls += 1;
            Ok(vec![page, page])
        })
        .unwrap();
        assert_eq!(calls, 5);
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn paginate_propagates_errors() {
        let result: Result<Vec<u8>, _> =
            paginate(1, 50, |page| if page == 2 { Err(ForgeError::Unauthorized) } else { Ok(vec![0]) });
        assert!(matches!(result, Err(ForgeError::Unauthorized)));
    }

    #[test]
    fn transport_errors_map_to_network_or_invalid_input() {
        assert!(matches!(map_transport_error(ureq::Error::HostNotFound), ForgeError::Network(_)));
        assert!(matches!(map_transport_error(ureq::Error::ConnectionFailed), ForgeError::Network(_)));
        assert!(matches!(map_transport_error(ureq::Error::BadUri("x".into())), ForgeError::InvalidInput(_)));
    }

    #[test]
    fn agent_uses_native_tls_and_returns_statuses_as_data() {
        let config = agent().config();
        assert_eq!(config.tls_config().provider(), ureq::tls::TlsProvider::NativeTls);
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().connect, Some(std::time::Duration::from_secs(10)));
        assert_eq!(config.timeouts().global, Some(std::time::Duration::from_secs(30)));
    }
}
