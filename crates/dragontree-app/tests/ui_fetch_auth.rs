//! Fetch automatique refusé par le serveur (L4) : indicateur permanent dans l'onglet (à côté du bouton Fetch, icône
//! et infobulle de l'onglet) jusqu'au prochain fetch de tous les remotes réussi, et une seule notification par série
//! d'échecs. Serveur HTTP local qui répond 401 à tout ; aucun identifiant réel : le dépôt vide la liste des assistants
//! d'identifiants (`credential.helper` vide, donc pas de Gestionnaire d'identifiants), git ne demande rien
//! (`GIT_TERMINAL_PROMPT=0` ; pour le fetch automatique, `GitCommand::non_interactive` : ni Gestionnaire interactif ni
//! programme askpass ; pour tout le test, la même neutralisation posée par `common`, `NO_CREDENTIAL_PROMPT`).

mod common;

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui::accesskit::Role;
use egui_kittest::kittest::{AccessKitNode, NodeT, Queryable};
use egui_kittest::{Harness, Node};

const INDICATOR: &str = "Authentification refusée";
const NOTIFICATION: &str = "Fetch automatique : authentification refusée";

/// Serveur HTTP local qui refuse toute requête (401, authentification Basic demandée) ; compte les requêtes.
fn refusing_server() -> (u16, Arc<AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            // En-têtes de la requête (git n'envoie pas de corps pour la première, `info/refs`).
            let mut request = Vec::new();
            let mut buf = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut buf) {
                    Ok(n) if n > 0 => request.extend_from_slice(&buf[..n]),
                    _ => break,
                }
            }
            count.fetch_add(1, Ordering::SeqCst);
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"contoso\"\r\nContent-Length: 0\r\n\
                  Connection: close\r\n\r\n",
            );
        }
    });
    (port, requests)
}

/// Indicateur à côté du bouton Fetch.
fn indicator<'h>(h: &'h Harness<'static, DragonTreeApp>) -> Option<Node<'h>> {
    h.query_by(|n| n.role() == Role::Button && n.label().as_deref() == Some(INDICATOR))
}

/// Onglet du dépôt dans la barre d'onglets (la barre d'outils affiche aussi son nom, en texte).
fn tab<'h>(h: &'h Harness<'static, DragonTreeApp>, title: &'h str) -> Node<'h> {
    h.get_by(move |n: &AccessKitNode<'_>| n.role() == Role::Button && n.label().as_deref() == Some(title))
}

/// Dépôt avec un commit, son remote `origin` refusé par le serveur ; et un dépôt nu qui servira de remote valide.
fn repo_refused_by(base: &Path, port: u16) -> (std::path::PathBuf, std::path::PathBuf) {
    let repo = base.join("depot");
    init_repo(&repo);
    commit_file(&repo, "a.txt", "1\n", "Premier commit");
    let bare = base.join("depot-nu.git");
    git(base, &["clone", "-q", "--bare", repo.to_str().unwrap(), bare.to_str().unwrap()]);
    git(&repo, &["remote", "add", "origin", &format!("http://127.0.0.1:{port}/depot.git")]);
    // Aucun assistant d'identifiants (celui de la configuration système, le Gestionnaire d'identifiants, est retiré).
    git(&repo, &["config", "credential.helper", ""]);
    (repo, bare)
}

#[test]
fn a_refused_automatic_fetch_shows_a_lasting_indicator_until_a_fetch_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let (port, requests) = refusing_server();
    let (repo, bare) = repo_refused_by(tmp.path(), port);
    let mut h =
        harness(StartOptions { open: vec![repo.clone()], auto_fetch_every: Some(Duration::ZERO), ..start(tmp.path()) });

    // Fetchs automatiques refusés, plusieurs fois de suite : l'indicateur, une seule notification.
    wait_until(&mut h, "indicateur d'authentification refusée", |h| indicator(h).is_some());
    let seen = requests.load(Ordering::SeqCst);
    wait_until(&mut h, "fetch automatique de nouveau refusé", |_| requests.load(Ordering::SeqCst) >= seen + 2);
    assert!(indicator(&h).is_some(), "l'indicateur reste");
    assert_eq!(h.query_all_by_label(NOTIFICATION).count(), 1, "une seule notification par série");

    // Infobulles : l'erreur de git, à côté du bouton Fetch et sur l'onglet.
    indicator(&h).unwrap().hover();
    wait_until(&mut h, "infobulle de l'indicateur, avec l'erreur de git", |h| {
        h.query_by(|n| {
            text_of(n).is_some_and(|t| {
                t.contains("Authentification refusée au dernier fetch :\n") && t.contains("could not read Username")
            })
        })
        .is_some()
    });
    tab(&h, "depot").hover();
    // Première ligne de l'infobulle de l'onglet : le chemin du dépôt.
    let mut path_line = String::new();
    wait_until(&mut h, "infobulle de l'onglet", |h| {
        let tip =
            h.query_by(|n| text_of(n).is_some_and(|t| t.contains("Authentification refusée au dernier fetch : ")));
        if let Some(text) = tip.and_then(|n| text_of(&n.accesskit_node())) {
            path_line = text.lines().next().unwrap_or_default().to_string();
        }
        !path_line.is_empty()
    });

    // Remote valide : le fetch suivant réussit et efface l'indicateur, puis la ligne de l'infobulle de l'onglet.
    git(&repo, &["remote", "set-url", "origin", bare.to_str().unwrap()]);
    wait_until(&mut h, "indicateur effacé par un fetch réussi", |h| indicator(h).is_none());
    tab(&h, "depot").hover();
    wait_until(&mut h, "infobulle de l'onglet réduite au chemin du dépôt", |h| {
        h.query_by(|n| text_of(n).as_deref() == Some(path_line.as_str())).is_some()
    });
    assert!(h.query_by_label_contains("Authentification refusée au dernier fetch").is_none(), "plus dans l'onglet");
}
