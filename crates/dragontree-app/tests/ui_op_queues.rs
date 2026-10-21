//! Files d'opérations d'un dépôt (RV3) : le fetch automatique passe par une file à part, et ne fait plus attendre les
//! écritures. Remote fictif servi par un serveur HTTP local qui accepte les connexions sans jamais répondre.

mod common;

use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

/// Serveur HTTP local muet : il accepte les connexions et les garde ouvertes sans répondre (fetch bloqué) jusqu'à
/// `release`.
struct SilentServer {
    port: u16,
    accepted: Arc<AtomicUsize>,
    held: Arc<Mutex<Vec<TcpStream>>>,
}

impl SilentServer {
    fn start() -> SilentServer {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (accepted, held) = (Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
        let (count, streams) = (accepted.clone(), held.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                streams.lock().unwrap().push(stream);
                count.fetch_add(1, Ordering::SeqCst);
            }
        });
        SilentServer { port, accepted, held }
    }

    fn connections(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }

    /// Ferme les connexions : les fetchs bloqués échouent et se terminent.
    fn release(&self) {
        self.held.lock().unwrap().clear();
    }
}

/// Dépôt avec un commit, un remote `origin` servi par `server` et un fichier modifié (`a.txt`).
fn repo(base: &Path, server: &SilentServer) -> PathBuf {
    let dir = base.join("depot");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", &format!("http://127.0.0.1:{}/depot.git", server.port)]);
    std::fs::write(dir.join("a.txt"), "2\n").unwrap();
    dir
}

/// Fichiers indexés ; `None` si l'index est illisible un instant (l'application le remplace).
fn staged(repo: &Path) -> Option<Vec<String>> {
    try_git(repo, &["diff", "--cached", "--name-only"]).map(|out| out.lines().map(str::to_string).collect())
}

#[test]
fn staging_completes_while_the_automatic_fetch_is_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let server = SilentServer::start();
    let repo = repo(tmp.path(), &server);
    let mut h = harness(StartOptions {
        open: vec![repo.clone()],
        view: Some("changes".into()),
        auto_fetch_every: Some(Duration::ZERO),
        ..start(tmp.path())
    });

    // Le fetch automatique part dès l'ouverture et reste bloqué : le serveur ne répond jamais.
    wait_until(&mut h, "fetch automatique bloqué sur le serveur muet", |_| server.connections() > 0);
    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    // L'indexation ne fait pas la queue derrière le fetch : elle se termine pendant qu'il est bloqué.
    wait_git(&mut h, "a.txt indexé pendant le fetch", || staged(&repo).is_some_and(|s| s == ["a.txt"]));
    assert_eq!(server.connections(), 1, "un seul fetch automatique à la fois");

    server.release();
}
