//! Proxy du client HTTP des forges, quand aucune variable d'environnement n'en désigne un : la configuration proxy de
//! Windows de l'utilisateur (Paramètres › Réseau et Internet › Proxy, la même que celle d'Internet Explorer), lue par
//! WinHTTP (`WinHttpGetIEProxyConfigForCurrentUser`).
//!
//! - Détection automatique (WPAD) ou script de configuration (PAC) : WinHTTP donne le proxy de chaque URL
//!   (`WinHttpGetProxyForUrl`, qui trouve, télécharge et exécute le script). Sans réponse (aucun script trouvé),
//!   les réglages manuels s'appliquent, comme pour Windows. Une détection automatique sans résultat n'est retentée
//!   qu'après [`AUTO_DETECT_RETRY`].
//! - Proxy manuel : `hôte:port` pour tous les protocoles, ou un serveur par protocole (`http=hôte:port;https=…`).
//!   Le proxy est joint en HTTP (`CONNECT` pour une forge en HTTPS). Un proxy SOCKS n'est pas pris en charge : la
//!   connexion est alors directe.
//! - Exclusions : la liste de Windows (`<local>` pour les noms sans point, motifs avec `*` comme `*.contoso.com` ou
//!   `10.*`, éventuellement précédés d'un protocole ou suivis d'un port), et toujours le bouclage (`localhost`,
//!   `127.0.0.0/8`, `::1`).
//!
//! L'accès à Windows passe par [`ProxySource`] : la logique se teste sans lire la configuration de la machine.

use std::net::IpAddr;

/// Délai avant de retenter une détection automatique (WPAD) restée sans réponse.
pub(crate) const AUTO_DETECT_RETRY: std::time::Duration = std::time::Duration::from_secs(300);

/// Réglages proxy de l'utilisateur, tels que Windows les donne.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct SystemSettings {
    /// « Détecter automatiquement les paramètres » (WPAD).
    pub auto_detect: bool,
    /// « Utiliser un script de configuration » : adresse du script PAC.
    pub auto_config_url: Option<String>,
    /// Proxy manuel (format WinHTTP).
    pub proxy: Option<String>,
    /// Exclusions du proxy manuel (format WinHTTP).
    pub bypass: Option<String>,
}

/// Réponse de la détection automatique ou du script PAC pour une URL.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AutoProxy {
    Direct,
    /// Proxys (format WinHTTP, le premier qui convient est pris) et exclusions éventuelles.
    Named {
        proxy: String,
        bypass: Option<String>,
    },
}

/// Accès à la configuration proxy du système (WinHTTP ; une configuration fixe dans les tests).
pub(crate) trait ProxySource: Sync {
    /// Réglages de l'utilisateur ; `None` si Windows ne les donne pas.
    fn settings(&self) -> Option<SystemSettings>;
    /// Proxy de `url` par détection automatique ou par le script `config_url` ; `None` sans réponse.
    fn auto_proxy(&self, url: &str, auto_detect: bool, config_url: Option<&str>) -> Option<AutoProxy>;
}

/// Destination d'une requête : protocole, hôte (sans crochets pour IPv6) et port.
struct Target {
    scheme: String,
    host: String,
    port: u16,
}

impl Target {
    fn parse(url: &str) -> Option<Target> {
        let uri: ureq::http::Uri = url.parse().ok()?;
        let scheme = uri.scheme_str()?.to_ascii_lowercase();
        let host = uri.host()?.trim_start_matches('[').trim_end_matches(']').to_ascii_lowercase();
        let port = uri.port_u16().unwrap_or(if scheme == "http" { 80 } else { 443 });
        Some(Target { scheme, host, port })
    }

    fn is_loopback(&self) -> bool {
        self.host == "localhost"
            || self.host.ends_with(".localhost")
            || self.host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }
}

/// Proxy (`http://hôte:port`) à utiliser pour `url` d'après la configuration du système ; `None` : connexion directe.
pub(crate) fn system_proxy_for(url: &str, source: &dyn ProxySource) -> Option<String> {
    let target = Target::parse(url)?;
    if target.is_loopback() {
        return None;
    }
    let settings = source.settings()?;
    if settings.auto_detect || settings.auto_config_url.is_some() {
        match source.auto_proxy(url, settings.auto_detect, settings.auto_config_url.as_deref()) {
            Some(AutoProxy::Direct) => return None,
            Some(AutoProxy::Named { proxy, bypass }) => return choose(&target, &proxy, bypass.as_deref()),
            // Pas de script : les réglages manuels s'appliquent.
            None => {}
        }
    }
    choose(&target, settings.proxy.as_deref()?, settings.bypass.as_deref())
}

fn choose(target: &Target, proxy: &str, bypass: Option<&str>) -> Option<String> {
    if bypass.is_some_and(|list| bypassed(target, list)) {
        return None;
    }
    server_for(proxy, &target.scheme).map(|server| format!("http://{server}"))
}

/// Serveur (`hôte:port`) de la liste `list` pour le protocole `scheme` : celui qui lui est propre
/// (`https=hôte:port`), sinon le premier commun à tous (`hôte:port`). Port 80 par défaut, comme WinHTTP.
fn server_for(list: &str, scheme: &str) -> Option<String> {
    let mut common = None;
    for entry in list.split(|c: char| c == ';' || c.is_whitespace()).filter(|e| !e.is_empty()) {
        match entry.split_once('=') {
            Some((key, server)) if key.eq_ignore_ascii_case(scheme) => return normalize_server(server),
            Some(_) => {}
            None if common.is_none() => common = normalize_server(entry),
            None => {}
        }
    }
    common
}

/// `hôte:port` sans protocole ; port 80 s'il manque.
fn normalize_server(server: &str) -> Option<String> {
    let server = server.trim();
    let server = server.split_once("://").map_or(server, |(_, rest)| rest).trim_end_matches('/');
    if server.is_empty() {
        return None;
    }
    let has_port = match server.strip_prefix('[') {
        Some(v6) => v6.contains("]:"),
        None => server.rsplit_once(':').is_some_and(|(_, port)| port.parse::<u16>().is_ok()),
    };
    Some(if has_port { server.to_string() } else { format!("{server}:80") })
}

/// Vrai si `target` figure dans la liste d'exclusion `list` (entrées séparées par `;`, `,` ou des espaces).
fn bypassed(target: &Target, list: &str) -> bool {
    list.split(|c: char| c == ';' || c == ',' || c.is_whitespace()).filter(|e| !e.is_empty()).any(|entry| {
        let entry = entry.to_ascii_lowercase();
        match entry.as_str() {
            // Noms d'intranet : sans point (et pas une adresse IPv6).
            "<local>" => !target.host.contains('.') && !target.host.contains(':'),
            // Retire le bouclage implicite : sans objet ici, il reste toujours direct.
            "<-loopback>" => false,
            pattern => matches_pattern(target, pattern),
        }
    })
}

/// Motif d'exclusion : `[protocole://]hôte[:port]`, l'hôte pouvant contenir des `*`.
fn matches_pattern(target: &Target, pattern: &str) -> bool {
    let (scheme, rest) = match pattern.split_once("://") {
        Some((scheme, rest)) => (Some(scheme), rest),
        None => (None, pattern),
    };
    if scheme.is_some_and(|s| s != target.scheme) {
        return false;
    }
    let rest = rest.trim_end_matches('/');
    let (host, port) = match rest.rsplit_once(':').map(|(host, port)| (host, port.parse::<u16>())) {
        Some((host, Ok(port))) if !host.ends_with(':') => (host, Some(port)),
        _ => (rest, None),
    };
    if port.is_some_and(|p| p != target.port) {
        return false;
    }
    glob(host.trim_start_matches('[').trim_end_matches(']'), &target.host)
}

/// `text` correspond à `pattern`, où `*` vaut n'importe quelle suite de caractères (déjà en minuscules).
fn glob(pattern: &str, text: &str) -> bool {
    let (p, t) = (pattern.as_bytes(), text.as_bytes());
    let (mut pi, mut ti) = (0, 0);
    // Dernière étoile vue et position du texte où elle a commencé à correspondre.
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

/// Configuration de Windows, lue par WinHTTP.
pub(crate) struct WinHttp;

/// Source du client des forges.
pub(crate) static SYSTEM: WinHttp = WinHttp;

#[cfg(windows)]
mod windows {
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;

    use windows_sys::Win32::Foundation::{GlobalFree, HGLOBAL};
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_AUTO_DETECT_TYPE_DHCP,
        WINHTTP_AUTO_DETECT_TYPE_DNS_A, WINHTTP_AUTOPROXY_AUTO_DETECT, WINHTTP_AUTOPROXY_CONFIG_URL,
        WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
        WinHttpGetIEProxyConfigForCurrentUser, WinHttpGetProxyForUrl, WinHttpOpen, WinHttpSetTimeouts,
    };
    use windows_sys::core::PWSTR;

    use super::{AUTO_DETECT_RETRY, AutoProxy, ProxySource, SystemSettings, WinHttp};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// Copie une chaîne allouée par WinHTTP puis la libère ; `None` si elle est absente ou vide.
    ///
    /// # Safety
    /// `p` est nul ou une chaîne terminée par un zéro, allouée par WinHTTP (`GlobalAlloc`) et jamais libérée.
    unsafe fn take(p: PWSTR) -> Option<String> {
        if p.is_null() {
            return None;
        }
        // SAFETY: chaîne terminée par un zéro (contrat de la fonction).
        let text = unsafe {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
        };
        // SAFETY: mémoire allouée par WinHTTP pour l'appelant, libérée une seule fois.
        unsafe { GlobalFree(p as HGLOBAL) };
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// Session WinHTTP de la détection automatique, ouverte une fois (sans proxy) et gardée jusqu'à la fin du
    /// processus ; délais de 10 s pour le téléchargement du script.
    fn session() -> Option<*mut core::ffi::c_void> {
        static SESSION: OnceLock<usize> = OnceLock::new();
        let handle = *SESSION.get_or_init(|| {
            let agent = wide(crate::http::USER_AGENT);
            // SAFETY: chaînes terminées par un zéro ; pointeurs nuls pour « pas de proxy ».
            let session = unsafe {
                WinHttpOpen(agent.as_ptr(), WINHTTP_ACCESS_TYPE_NO_PROXY, std::ptr::null(), std::ptr::null(), 0)
            };
            if !session.is_null() {
                // SAFETY: session valide.
                unsafe { WinHttpSetTimeouts(session, 10_000, 10_000, 10_000, 10_000) };
            }
            session as usize
        });
        (handle != 0).then_some(handle as *mut core::ffi::c_void)
    }

    impl ProxySource for WinHttp {
        fn settings(&self) -> Option<SystemSettings> {
            let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
            // SAFETY: structure fournie par l'appelant, remplie par WinHTTP.
            if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut config) } == 0 {
                return None;
            }
            // SAFETY: chaînes allouées par WinHTTP pour l'appelant, prises une seule fois.
            unsafe {
                Some(SystemSettings {
                    auto_detect: config.fAutoDetect != 0,
                    auto_config_url: take(config.lpszAutoConfigUrl),
                    proxy: take(config.lpszProxy),
                    bypass: take(config.lpszProxyBypass),
                })
            }
        }

        fn auto_proxy(&self, url: &str, auto_detect: bool, config_url: Option<&str>) -> Option<AutoProxy> {
            // Détection automatique seule, restée sans réponse il y a peu : pas de nouvelle attente.
            static AUTO_DETECT_FAILED: Mutex<Option<Instant>> = Mutex::new(None);
            let lock = || AUTO_DETECT_FAILED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let detect_only = auto_detect && config_url.is_none();
            if detect_only && lock().is_some_and(|at| at.elapsed() < AUTO_DETECT_RETRY) {
                return None;
            }
            let session = session()?;
            let url = wide(url);
            let config_url = config_url.map(wide);
            let mut options = WINHTTP_AUTOPROXY_OPTIONS {
                dwFlags: if auto_detect { WINHTTP_AUTOPROXY_AUTO_DETECT } else { 0 }
                    | if config_url.is_some() { WINHTTP_AUTOPROXY_CONFIG_URL } else { 0 },
                dwAutoDetectFlags: if auto_detect {
                    WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A
                } else {
                    0
                },
                lpszAutoConfigUrl: config_url.as_ref().map_or(std::ptr::null(), |u| u.as_ptr()),
                lpvReserved: std::ptr::null_mut(),
                dwReserved: 0,
                // Script sur un serveur d'entreprise authentifié : identifiants de la session Windows.
                fAutoLogonIfChallenged: 1,
            };
            let mut info = WINHTTP_PROXY_INFO::default();
            // SAFETY: session ouverte ; chaînes terminées par un zéro, vivantes pendant l'appel ; `info` rempli par
            // WinHTTP, ses chaînes prises ci-dessous.
            if unsafe { WinHttpGetProxyForUrl(session, url.as_ptr(), &mut options, &mut info) } == 0 {
                if detect_only {
                    *lock() = Some(Instant::now());
                }
                return None;
            }
            // SAFETY: chaînes allouées par WinHTTP pour l'appelant, prises une seule fois.
            let (proxy, bypass) = unsafe { (take(info.lpszProxy), take(info.lpszProxyBypass)) };
            Some(match (info.dwAccessType, proxy) {
                (WINHTTP_ACCESS_TYPE_NAMED_PROXY, Some(proxy)) => AutoProxy::Named { proxy, bypass },
                _ => AutoProxy::Direct,
            })
        }
    }
}

#[cfg(not(windows))]
impl ProxySource for WinHttp {
    fn settings(&self) -> Option<SystemSettings> {
        None
    }

    fn auto_proxy(&self, _url: &str, _auto_detect: bool, _config_url: Option<&str>) -> Option<AutoProxy> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Configuration fixe : réglages de l'utilisateur et réponse de la détection automatique.
    #[derive(Default)]
    struct Fixed {
        settings: Option<SystemSettings>,
        auto: Option<AutoProxy>,
    }

    impl ProxySource for Fixed {
        fn settings(&self) -> Option<SystemSettings> {
            self.settings.clone()
        }

        fn auto_proxy(&self, _url: &str, _auto_detect: bool, _config_url: Option<&str>) -> Option<AutoProxy> {
            self.auto.clone()
        }
    }

    fn manual(proxy: &str, bypass: Option<&str>) -> Fixed {
        Fixed {
            settings: Some(SystemSettings {
                proxy: Some(proxy.into()),
                bypass: bypass.map(Into::into),
                ..Default::default()
            }),
            auto: None,
        }
    }

    fn proxy_of(source: &Fixed, url: &str) -> Option<String> {
        system_proxy_for(url, source)
    }

    #[test]
    fn a_single_server_serves_every_protocol() {
        let source = manual("proxy.contoso.com:8080", None);
        assert_eq!(proxy_of(&source, "https://api.github.com/user").as_deref(), Some("http://proxy.contoso.com:8080"));
        assert_eq!(
            proxy_of(&source, "http://gitlab.contoso.com/api").as_deref(),
            Some("http://proxy.contoso.com:8080")
        );
        // Sans port : 80, comme WinHTTP ; un protocole devant le serveur est ignoré.
        assert_eq!(proxy_of(&manual("proxy", None), "https://x.test/").as_deref(), Some("http://proxy:80"));
        assert_eq!(
            proxy_of(&manual("http://proxy:3128/", None), "https://x.test/").as_deref(),
            Some("http://proxy:3128")
        );
    }

    #[test]
    fn a_server_per_protocol_is_chosen_by_the_scheme_of_the_url() {
        for list in ["http=p1:8080;https=p2:8443", "http=p1:8080 https=p2:8443", "HTTP=p1:8080; HTTPS=p2:8443"] {
            let source = manual(list, None);
            assert_eq!(proxy_of(&source, "https://dev.azure.com/o").as_deref(), Some("http://p2:8443"), "{list}");
            assert_eq!(proxy_of(&source, "http://gitlab.test/").as_deref(), Some("http://p1:8080"), "{list}");
        }
        // Pas de serveur pour HTTPS : connexion directe.
        assert_eq!(proxy_of(&manual("http=p1:8080;ftp=f:21", None), "https://api.github.com/"), None);
        // SOCKS n'est pas pris en charge ; un serveur propre au protocole passe avant le serveur commun.
        assert_eq!(proxy_of(&manual("socks=s:1080", None), "https://api.github.com/"), None);
        assert_eq!(
            proxy_of(&manual("socks=s:1080;https=p:8080", None), "https://x.test/").as_deref(),
            Some("http://p:8080")
        );
        assert_eq!(
            proxy_of(&manual("all:3128;https=p:8080", None), "https://x.test/").as_deref(),
            Some("http://p:8080")
        );
        assert_eq!(
            proxy_of(&manual("all:3128;https=p:8080", None), "http://x.test/").as_deref(),
            Some("http://all:3128")
        );
        assert_eq!(
            proxy_of(&manual("https=[fd00::1]:8080", None), "https://x.test/").as_deref(),
            Some("http://[fd00::1]:8080")
        );
    }

    #[test]
    fn the_bypass_list_and_the_loopback_go_direct() {
        let bypass = "<local>;*.contoso.com; 10.*,build.northwind.test:8080;https://secure.fabrikam.test";
        let source = manual("proxy:8080", Some(bypass));
        let direct = |url: &str| proxy_of(&source, url).is_none();
        assert!(direct("https://intranet/api"), "<local> : nom sans point");
        assert!(direct("https://git.contoso.com/api"));
        assert!(direct("https://GIT.Contoso.COM/api"), "casse ignorée");
        assert!(!direct("https://contoso.com/api"), "*.contoso.com ne couvre pas contoso.com");
        assert!(direct("http://10.1.2.3/api"));
        assert!(!direct("http://110.1.2.3/api"));
        assert!(direct("http://build.northwind.test:8080/api"), "port de l'exclusion");
        assert!(!direct("https://build.northwind.test/api"), "autre port");
        assert!(direct("https://secure.fabrikam.test/api"), "protocole de l'exclusion");
        assert!(!direct("http://secure.fabrikam.test/api"), "autre protocole");
        assert!(!direct("https://api.github.com/"));
        // Bouclage : toujours direct, même sans exclusion (serveurs locaux des tests).
        let source = manual("proxy:8080", None);
        for url in ["http://127.0.0.1:4567/x", "http://localhost/x", "http://[::1]:80/x", "https://app.localhost/"] {
            assert_eq!(proxy_of(&source, url), None, "{url}");
        }
        assert!(proxy_of(&source, "https://intranet/api").is_some(), "<local> absent : proxy");
    }

    #[test]
    fn the_automatic_configuration_comes_first_and_falls_back_to_the_manual_settings() {
        let settings = SystemSettings {
            auto_detect: true,
            auto_config_url: Some("http://wpad.contoso.com/proxy.pac".into()),
            proxy: Some("manuel:8080".into()),
            bypass: None,
        };
        let with = |auto| Fixed { settings: Some(settings.clone()), auto };
        let named = |proxy: &str, bypass: Option<&str>| {
            Some(AutoProxy::Named { proxy: proxy.into(), bypass: bypass.map(Into::into) })
        };
        let url = "https://dev.azure.com/contoso";
        assert_eq!(proxy_of(&with(named("pac1:3128;pac2:3128", None)), url).as_deref(), Some("http://pac1:3128"));
        assert_eq!(proxy_of(&with(Some(AutoProxy::Direct)), url), None, "le script dit DIRECT");
        assert_eq!(proxy_of(&with(named("pac:3128", Some("*.azure.com"))), url), None, "exclusion du script");
        assert_eq!(proxy_of(&with(None), url).as_deref(), Some("http://manuel:8080"), "pas de script : manuel");
        let no_manual = Fixed { settings: Some(SystemSettings { proxy: None, ..settings.clone() }), auto: None };
        assert_eq!(proxy_of(&no_manual, url), None);
    }

    #[test]
    fn no_settings_means_a_direct_connection() {
        assert_eq!(proxy_of(&Fixed::default(), "https://api.github.com/"), None);
        let empty = Fixed { settings: Some(SystemSettings::default()), auto: None };
        assert_eq!(proxy_of(&empty, "https://api.github.com/"), None);
        assert_eq!(proxy_of(&manual("proxy:8080", None), "pas une url"), None);
    }

    #[test]
    fn glob_patterns() {
        assert!(glob("*", "n'importe"));
        assert!(glob("*.contoso.com", "a.b.contoso.com"));
        assert!(!glob("*.contoso.com", "contoso.com"));
        assert!(glob("10.*", "10.0.0.1"));
        assert!(glob("*git*", "forgit.test"));
        assert!(glob("exact.test", "exact.test"));
        assert!(!glob("exact.test", "exact.test.org"));
        assert!(glob("a*b*c", "axxbyyc"));
        assert!(!glob("a*b*c", "axxbyy"));
    }

    /// Script PAC servi par un serveur local et exécuté par WinHTTP (sans lire la configuration de la machine).
    #[cfg(windows)]
    #[test]
    fn winhttp_runs_a_proxy_auto_config_script() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let script = "function FindProxyForURL(url, host) {\n  if (host == \"direct.test\") return \"DIRECT\";\n  return \"PROXY proxy.test:3128\";\n}\n";
            for mut stream in listener.incoming().flatten() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/x-ns-proxy-autoconfig\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{script}",
                    script.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        let pac = format!("http://127.0.0.1:{port}/proxy.pac");
        assert_eq!(
            SYSTEM.auto_proxy("https://forge.test/api", false, Some(&pac)),
            Some(AutoProxy::Named { proxy: "proxy.test:3128".into(), bypass: None })
        );
        assert_eq!(SYSTEM.auto_proxy("https://direct.test/api", false, Some(&pac)), Some(AutoProxy::Direct));
    }
}
