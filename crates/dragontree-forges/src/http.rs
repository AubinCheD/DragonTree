//! Client HTTP partagé (ureq 3, native-tls/SChannel) et helpers de réponse.
//!
//! Proxy : celui des variables d'environnement (`ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY`, avec `NO_PROXY`) s'il y en a
//! un, sinon celui que la configuration de Windows désigne pour chaque URL (voir `proxy`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use dragontree_i18n::tr;

use crate::model::ForgeError;
use crate::proxy::{self, ProxySource};

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
    let mut request = agent_for(url).get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read_response(request.call().map_err(map_transport_error)?)
}

/// Requête POST avec un corps JSON et les en-têtes donnés. Les statuts non 2xx sont renvoyés tels quels.
pub(crate) fn post_json(url: &str, headers: &[(&str, &str)], body: &str) -> Result<Response, ForgeError> {
    post_json_with(&agent_for(url), url, headers, body)
}

/// `post_json` avec un agent donné (les tests en passent un sans proxy, indépendant de l'environnement).
fn post_json_with(
    agent: &ureq::Agent,
    url: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Result<Response, ForgeError> {
    let mut request = agent.post(url).header("Content-Type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    read_response(request.send(body).map_err(map_transport_error)?)
}

/// Lit le statut, l'en-tête de limitation de débit et le corps d'une réponse.
fn read_response(mut response: ureq::http::Response<ureq::Body>) -> Result<Response, ForgeError> {
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

/// Agent pour `url` : celui du proxy des variables d'environnement s'il y en a un (construit une fois), sinon celui
/// du proxy que Windows désigne pour cette URL.
fn agent_for(url: &str) -> ureq::Agent {
    static FROM_ENV: OnceLock<Option<ureq::Agent>> = OnceLock::new();
    match FROM_ENV.get_or_init(|| ureq::Proxy::try_from_env().map(|proxy| build_agent(Some(proxy)))) {
        Some(agent) => agent.clone(),
        None => system_agent(url, &proxy::SYSTEM),
    }
}

/// Agent du proxy que la configuration `source` désigne pour `url` (connexion directe sans proxy) ; un agent par
/// proxy, gardé pour les requêtes suivantes (connexions réutilisées).
fn system_agent(url: &str, source: &dyn ProxySource) -> ureq::Agent {
    static AGENTS: OnceLock<Mutex<HashMap<Option<String>, ureq::Agent>>> = OnceLock::new();
    // Un proxy illisible pour ureq : connexion directe, comme sans proxy.
    let proxy = proxy::system_proxy_for(url, source).filter(|uri| ureq::Proxy::new(uri).is_ok());
    let mut agents = AGENTS.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    agents
        .entry(proxy)
        .or_insert_with_key(|proxy| build_agent(proxy.as_deref().and_then(|uri| ureq::Proxy::new(uri).ok())))
        .clone()
}

/// Agent avec le TLS du système, les délais et le proxy donnés.
fn build_agent(proxy: Option<ureq::Proxy>) -> ureq::Agent {
    use std::time::Duration;
    use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

    let tls = TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        // Magasin de certificats Windows : respecte les autorités d'entreprise
        // (proxy filtrant), comme git avec SChannel.
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .proxy(proxy)
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .user_agent(USER_AGENT)
        .build()
        .new_agent()
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

    /// Agent des tests de bouclage : sans proxy, quelles que soient `HTTP_PROXY` et `NO_PROXY` de la machine.
    fn loopback_agent() -> ureq::Agent {
        ureq::Agent::config_builder().proxy(None).http_status_as_error(false).build().new_agent()
    }

    /// Serveur local à usage unique : lit une requête complète, la renvoie par le canal et répond `status`.
    fn one_shot_server(status: &'static str, reply: &'static str) -> (u16, std::sync::mpsc::Receiver<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut data = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                data.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&data).into_owned();
                if let Some((head, body)) = text.split_once("\r\n\r\n") {
                    let length = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap_or(0usize);
                    if body.len() >= length || n == 0 {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            sender.send(String::from_utf8_lossy(&data).into_owned()).unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (port, receiver)
    }

    #[test]
    fn post_json_sends_the_body_and_headers_and_returns_the_status() {
        let (port, received) = one_shot_server("201 Created", r#"{"number":5}"#);
        let url = format!("http://127.0.0.1:{port}/repos/o/r/pulls");
        let response =
            post_json_with(&loopback_agent(), &url, &[("Authorization", "Bearer jeton-de-test")], r#"{"title":"é"}"#)
                .unwrap();
        assert_eq!(response.status, 201);
        assert_eq!(response.body, r#"{"number":5}"#);
        let request = received.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert!(request.starts_with("POST /repos/o/r/pulls HTTP/1.1"), "{request}");
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("content-type: application/json"), "{request}");
        assert!(lower.contains("authorization: bearer jeton-de-test"), "{request}");
        assert!(request.ends_with(r#"{"title":"é"}"#), "{request}");
    }

    #[test]
    fn post_json_returns_error_statuses_as_data() {
        let (port, _received) = one_shot_server("422 Unprocessable Entity", r#"{"message":"Validation Failed"}"#);
        let response = post_json_with(&loopback_agent(), &format!("http://127.0.0.1:{port}/x"), &[], "{}").unwrap();
        assert_eq!(response.status, 422);
        assert!(matches!(check_status(response), Err(ForgeError::Http { status: 422, .. })));
    }

    #[test]
    fn agent_uses_native_tls_and_returns_statuses_as_data() {
        let agent = build_agent(None);
        let config = agent.config();
        assert_eq!(config.tls_config().provider(), ureq::tls::TlsProvider::NativeTls);
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().connect, Some(std::time::Duration::from_secs(10)));
        assert_eq!(config.timeouts().global, Some(std::time::Duration::from_secs(30)));
    }

    /// Proxy manuel de Windows simulé (la configuration de la machine n'est pas lue).
    struct WindowsProxy {
        proxy: String,
        bypass: &'static str,
    }

    impl ProxySource for WindowsProxy {
        fn settings(&self) -> Option<proxy::SystemSettings> {
            Some(proxy::SystemSettings {
                proxy: Some(self.proxy.clone()),
                bypass: Some(self.bypass.into()),
                ..Default::default()
            })
        }

        fn auto_proxy(&self, _: &str, _: bool, _: Option<&str>) -> Option<proxy::AutoProxy> {
            None
        }
    }

    fn proxy_of(agent: &ureq::Agent) -> Option<(String, u16)> {
        agent.config().proxy().map(|p| (p.host().to_string(), p.port()))
    }

    /// RV4c : sans variable d'environnement, l'agent prend le proxy de Windows pour l'URL, sauf exclusion.
    #[test]
    fn the_agent_takes_the_windows_proxy_of_the_url_unless_it_is_bypassed() {
        let source = WindowsProxy { proxy: "proxy.contoso.com:8080".into(), bypass: "<local>;*.contoso.com" };
        let agent = system_agent("https://api.github.com/user", &source);
        assert_eq!(proxy_of(&agent), Some(("proxy.contoso.com".into(), 8080)));
        assert_eq!(proxy_of(&system_agent("https://git.contoso.com/api/v4/user", &source)), None, "exclusion");
        assert_eq!(proxy_of(&system_agent("https://intranet/api/v4/user", &source)), None, "<local>");
        assert_eq!(proxy_of(&system_agent("http://127.0.0.1:8080/api", &source)), None, "bouclage");
        // Le même agent sert les requêtes suivantes vers ce proxy (connexions réutilisées).
        let again = system_agent("https://dev.azure.com/fabrikam", &source);
        assert!(std::ptr::eq(agent.config(), again.config()), "agent gardé");
    }

    /// La requête passe vraiment par le proxy de Windows (un serveur local qui joue le proxy la reçoit).
    #[test]
    fn a_request_goes_through_the_windows_proxy() {
        let (port, received) = one_shot_server("502 Bad Gateway", "");
        let source = WindowsProxy { proxy: format!("http=127.0.0.1:{port}"), bypass: "" };
        let url = "http://gitlab.contoso.test/api/v4/user";
        let _ = system_agent(url, &source).get(url).call();
        let request = received.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        assert!(request.contains("gitlab.contoso.test"), "{request}");
    }
}
