//! Outils partagés des tests d'interface (egui_kittest) : dépôts temporaires, lancement de l'application,
//! attentes et recherche d'éléments.
//! Les commandes git tournent en arrière-plan : on fait avancer l'interface jusqu'au résultat.
//! Configuration git globale isolée par défaut (contrainte 6 du plan) : `test_git`, `git`, `try_git`, `init_repo` et
//! `harness` l'isolent avant toute commande git, et le test la tient jusqu'à sa fin (voir `global_config`, partagé
//! avec les tests du moteur).

#![allow(dead_code)]

#[path = "../../../dragontree-git/tests/common/git_version.rs"]
pub mod git_version;
#[path = "../../../dragontree-git/tests/common/global_config.rs"]
mod global_config;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_git::Git;
use egui_kittest::kittest::{AccessKitNode, NodeT, Queryable};
use egui_kittest::{Harness, Node};
pub use global_config::*;

/// git des tests, configuration globale isolée : à utiliser à la place de `Git::discover`.
pub fn test_git() -> Git {
    isolate_git_config_for_this_test();
    Git::discover().expect("git doit être installé pour les tests")
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    test_git().cmd(dir).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e}")).stdout_str()
}

/// Comme `git`, pour une attente : `None` si git échoue, par exemple en lisant `refs/stash` pendant que
/// l'application le réécrit.
pub fn try_git(dir: &Path, args: &[&str]) -> Option<String> {
    test_git().cmd(dir).args(args).run().ok().map(|o| o.stdout_str())
}

/// Comptes de la suite du profil de test (`<base>/profil/accounts.json`, voir `suite_accounts::accounts_dir`), sans
/// jeton : DragonTree les lit là, et non dans ses réglages.
pub fn seed_accounts(base: &Path, accounts: &[dragontree_forges::Account]) {
    let store =
        suite_accounts::Store::new(&base.join("profil"), std::sync::Arc::new(suite_accounts::MemoryVault::default()));
    for a in accounts {
        store.adopt(a.into(), None).unwrap();
    }
}

pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    for (k, v) in [
        ("user.name", "Testeur"),
        ("user.email", "t@example.com"),
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
    ] {
        git(dir, &["config", k, v]);
    }
}

pub fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) {
    let p = dir.join(file);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
}

/// Fichiers indexés ; `None` si la lecture échoue (sous Windows, l'index est illisible un instant pendant que
/// l'application le remplace) : les attentes relisent alors à l'image suivante.
pub fn staged(repo: &Path) -> Option<Vec<String>> {
    try_git(repo, &["diff", "--cached", "--name-only"]).map(|out| out.lines().map(str::to_string).collect())
}

/// Dépôt `depot` avec un commit (`a.txt`) et un remote `origin` servi par `server` (muet : un fetch y reste bloqué).
/// Il vient d'être fetché (`FETCH_HEAD`) : le fetch automatique attend son échéance, sauf `auto_fetch_every` nul.
pub fn repo_with_silent_remote(base: &Path, server: &SilentServer) -> PathBuf {
    let dir = base.join("depot");
    init_repo(&dir);
    commit_file(&dir, "a.txt", "1\n", "Premier commit");
    git(&dir, &["remote", "add", "origin", &format!("http://127.0.0.1:{}/depot.git", server.port)]);
    std::fs::write(dir.join(".git").join("FETCH_HEAD"), "").unwrap();
    dir
}

/// Dépôt avec historique, un fichier modifié et un fichier non suivi.
pub fn demo_repo(base: &Path) -> PathBuf {
    let dir = base.join("demo");
    init_repo(&dir);
    commit_file(&dir, "src/calc.rs", "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n", "Initial");
    commit_file(
        &dir,
        "src/calc.rs",
        "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\nfn mul(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
        "Ajout de la multiplication",
    );
    std::fs::write(dir.join("src/calc.rs"), "fn add(a: i32, b: i32) -> i32 {\n    a.checked_add(b).unwrap()\n}\n\nfn mul(a: i32, b: i32) -> i32 {\n    a * b\n}\n").unwrap();
    std::fs::write(dir.join("notes.txt"), "à faire\n").unwrap();
    dir
}

/// Application lancée avec `start` ; la configuration git globale est isolée avant (l'application lance git dès son
/// démarrage : détection de git lfs, ouverture des dépôts).
pub fn harness(start: StartOptions) -> Harness<'static, DragonTreeApp> {
    isolate_git_config_for_this_test();
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| DragonTreeApp::new(cc, start))
}

/// Options de lancement des tests (profil temporaire, interface en français) ; isole aussi la configuration git
/// globale, pour un harnais construit à part.
pub fn start(base: &Path) -> StartOptions {
    isolate_git_config_for_this_test();
    StartOptions {
        profile_dir: Some(base.join("profil")),
        screenshot_after: Duration::ZERO,
        // Les libellés cherchés par les tests sont ceux de l'interface en français.
        lang: Some(dragontree_i18n::Lang::Fr),
        ..Default::default()
    }
}

/// Fait tourner l'interface jusqu'à ce que `cond` soit vraie.
pub fn wait_until(
    h: &mut Harness<'static, DragonTreeApp>,
    what: &str,
    mut cond: impl FnMut(&Harness<'static, DragonTreeApp>) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        h.step();
        if cond(h) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("délai dépassé en attendant : {what}");
}

pub fn wait_git(h: &mut Harness<'static, DragonTreeApp>, what: &str, mut cond: impl FnMut() -> bool) {
    wait_until(h, what, |_| cond());
}

/// Fait tourner l'interface pendant `duration` au plus ; renvoie vrai si `cond` est devenue vraie.
pub fn becomes_true(
    h: &mut Harness<'static, DragonTreeApp>,
    duration: Duration,
    mut cond: impl FnMut() -> bool,
) -> bool {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        h.step();
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

/// Texte copié dans le presse-papiers par la dernière image (celle d'un `click_at`) ou les suivantes.
pub fn copied_text(h: &mut Harness<'static, DragonTreeApp>) -> Option<String> {
    for i in 0..6 {
        if i > 0 {
            h.step();
        }
        for cmd in &h.output().platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = cmd {
                return Some(text.clone());
            }
        }
    }
    None
}

/// L'élément unique (case, bouton radio) de libellé exactement `label` est affiché et coché : à attendre après un clic
/// dont la suite dépend (choix d'une cible avant de valider).
pub fn checked(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.query_by(|n| text_of(n).as_deref() == Some(label))
        .is_some_and(|n| n.accesskit_node().toggled() == Some(egui::accesskit::Toggled::True))
}

/// Un champ de saisie contient exactement `value`.
pub fn has_text_input(h: &Harness<'static, DragonTreeApp>, value: &str) -> bool {
    h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(value)).is_some()
}

// --- Clics ----------------------------------------------------------------------------------

/// Clique `pos` avec `button` en une seule image, la seule que joue cette fonction : ses sorties (texte copié, URL
/// ouverte) sont celles de `h.output()` au retour. Survol, appui et relâchement arrivent ensemble, et egui donne le
/// clic à l'élément qui était sous le pointeur à l'image précédente, celle d'où vient `pos`. (`Node::click` de
/// kittest répartit ces trois événements sur trois images : si l'interface bouge entre-temps — menu ou dialogue
/// centré qui se replace, liste qu'une lecture en tâche de fond allonge —, le clic tombe à côté, et un clic hors
/// d'un menu ouvert le ferme.)
pub fn click_at(h: &mut Harness<'static, DragonTreeApp>, pos: egui::Pos2, button: egui::PointerButton) {
    let events = &mut h.input_mut().events;
    events.push(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        events.push(egui::Event::PointerButton { pos, button, pressed, modifiers: egui::Modifiers::NONE });
    }
    h.step();
}

/// Attend que l'élément unique qui vérifie `pred` soit affiché et utilisable (un clic sur un élément grisé est
/// perdu : une commande l'est tant que l'état du dépôt n'est pas lu), puis le clique en une image (`click_at`).
pub fn click_by(
    h: &mut Harness<'static, DragonTreeApp>,
    what: &str,
    button: egui::PointerButton,
    pred: impl Fn(&AccessKitNode<'_>) -> bool,
) {
    let usable = |h: &Harness<'static, DragonTreeApp>| {
        h.query_by(|n| pred(n)).filter(|n| !n.accesskit_node().is_disabled()).map(|n| n.rect().center())
    };
    wait_until(h, what, |h| usable(h).is_some());
    let pos = usable(h).unwrap();
    click_at(h, pos, button);
}

/// Texte d'un élément tel que le cherchent les `*_by_label` de kittest : celui d'un `Label` est sa valeur.
pub fn text_of(node: &AccessKitNode<'_>) -> Option<String> {
    if node.role() == egui::accesskit::Role::Label { node.value() } else { node.label() }
}

/// Clic droit sur l'élément de libellé exactement `label` dès qu'il est affiché (ouvre son menu contextuel).
pub fn right_click_label(h: &mut Harness<'static, DragonTreeApp>, label: &str) {
    click_by(h, &format!("« {label} »"), egui::PointerButton::Secondary, |n| text_of(n).as_deref() == Some(label));
}

/// Clique l'élément dont le libellé contient `part` dès qu'il est affiché et utilisable.
pub fn click_label_containing(h: &mut Harness<'static, DragonTreeApp>, part: &str) {
    click_by(h, &format!("« …{part}… » utilisable"), egui::PointerButton::Primary, |n| {
        text_of(n).is_some_and(|l| l.contains(part))
    });
}

// --- Barre de menus -------------------------------------------------------------------------

/// L'élément est dans la barre de menus ou dans un menu ouvert.
fn inside_menu(node: &AccessKitNode<'_>) -> bool {
    std::iter::successors(node.parent(), |n| n.parent()).any(|n| n.role() == egui::accesskit::Role::Menu)
}

/// Titre de la barre de menus, ou entrée d'un menu ouvert, de libellé exactement `label` : le libellé
/// suivi de son raccourci quand il en a un (« Nouvel onglet Ctrl+T »). Les boutons homonymes hors des
/// menus (barre d'outils, navigation) sont ignorés.
pub fn menu_item<'h>(h: &'h Harness<'static, DragonTreeApp>, label: &'h str) -> Option<Node<'h>> {
    h.query_by(move |n| n.label().as_deref() == Some(label) && inside_menu(n))
}

/// Ouvre un menu de la barre, ou un sous-menu d'un menu ouvert, dès que son titre est utilisable (un sous-menu
/// grisé ne s'ouvre pas).
pub fn open_menu(h: &mut Harness<'static, DragonTreeApp>, title: &str) {
    let what = format!("menu « {title} » utilisable");
    click_by(h, &what, egui::PointerButton::Primary, |n| n.label().as_deref() == Some(title) && inside_menu(n));
    h.run_steps(3);
}

/// Clique une entrée d'un menu ouvert dès qu'elle est utilisable (une entrée grisée ne fait rien).
pub fn click_menu_item(h: &mut Harness<'static, DragonTreeApp>, label: &str) {
    let what = format!("entrée « {label} » utilisable");
    click_by(h, &what, egui::PointerButton::Primary, |n| n.label().as_deref() == Some(label) && inside_menu(n));
    h.run_steps(3);
}

/// L'entrée (affichée) est grisée.
pub fn menu_item_disabled(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    menu_item(h, label).unwrap_or_else(|| panic!("entrée « {label} » absente")).accesskit_node().is_disabled()
}

/// État coché de l'entrée (affichée) : `None` pour une entrée qui ne se coche pas.
pub fn menu_item_checked(h: &Harness<'static, DragonTreeApp>, label: &str) -> Option<bool> {
    let node = menu_item(h, label).unwrap_or_else(|| panic!("entrée « {label} » absente"));
    node.accesskit_node().toggled().map(|t| t == egui::accesskit::Toggled::True)
}

/// Vrai si la dernière image a demandé la fermeture de la fenêtre (`ViewportCommand::Close`).
pub fn close_requested(h: &Harness<'static, DragonTreeApp>) -> bool {
    viewport_command_sent(h, &egui::ViewportCommand::Close)
}

/// Vrai si la dernière image a envoyé `command` à la fenêtre.
pub fn viewport_command_sent(h: &Harness<'static, DragonTreeApp>, command: &egui::ViewportCommand) -> bool {
    h.output().viewport_output.values().any(|v| v.commands.contains(command))
}

/// Joue une image où Windows demande de fermer la fenêtre (croix, Alt+F4, ou suite d'un `ViewportCommand::Close`) ;
/// renvoie vrai si l'application l'a refusée (`ViewportCommand::CancelClose`), comme eframe le lit.
pub fn request_close(h: &mut Harness<'static, DragonTreeApp>) -> bool {
    let root = h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default();
    root.events.push(egui::ViewportEvent::Close);
    h.step();
    viewport_command_sent(h, &egui::ViewportCommand::CancelClose)
}

/// Serveur HTTP local muet : il accepte les connexions et les garde ouvertes sans répondre (fetch bloqué) jusqu'à
/// `release` ; il compte aussi les connexions fermées par le client (git arrêté).
pub struct SilentServer {
    pub port: u16,
    accepted: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    closed: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    held: std::sync::Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
}

impl SilentServer {
    pub fn start() -> SilentServer {
        use std::io::Read;
        use std::sync::atomic::Ordering;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server =
            SilentServer { port, accepted: Default::default(), closed: Default::default(), held: Default::default() };
        let (accepted, closed, held) = (server.accepted.clone(), server.closed.clone(), server.held.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                // Lit la requête puis attend : la lecture ne s'arrête qu'à la fermeture de la connexion par le client
                // (ou par `release`).
                if let Ok(mut reader) = stream.try_clone() {
                    let closed = closed.clone();
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 4096];
                        while matches!(reader.read(&mut buf), Ok(n) if n > 0) {}
                        closed.fetch_add(1, Ordering::SeqCst);
                    });
                }
                held.lock().unwrap().push(stream);
                accepted.fetch_add(1, Ordering::SeqCst);
            }
        });
        server
    }

    pub fn connections(&self) -> usize {
        self.accepted.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Connexions fermées (par le client, ou par `release`).
    pub fn closed(&self) -> usize {
        self.closed.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Ferme les connexions : les fetchs bloqués échouent et se terminent.
    pub fn release(&self) {
        for stream in self.held.lock().unwrap().drain(..) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Attend qu'un élément de libellé exactement `label` soit affiché et utilisable, puis le clique en une image (la
/// barre latérale se remplit après l'historique).
pub fn click_label(h: &mut Harness<'static, DragonTreeApp>, label: &str) {
    click_by(h, &format!("élément « {label} » utilisable"), egui::PointerButton::Primary, |n| {
        text_of(n).as_deref() == Some(label)
    });
}
