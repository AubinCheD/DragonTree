//! Outils partagés des tests d'interface (egui_kittest) : dépôts temporaires, lancement de l'application,
//! attentes et recherche d'éléments.
//! Les commandes git tournent en arrière-plan : on fait avancer l'interface jusqu'au résultat.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_git::Git;
use egui_kittest::kittest::{AccessKitNode, NodeT, Queryable};
use egui_kittest::{Harness, Node};

pub fn git(dir: &Path, args: &[&str]) -> String {
    Git::discover().unwrap().cmd(dir).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e}")).stdout_str()
}

/// Comme `git`, pour une attente : `None` si git échoue, par exemple en lisant `refs/stash` pendant que
/// l'application le réécrit.
pub fn try_git(dir: &Path, args: &[&str]) -> Option<String> {
    Git::discover().unwrap().cmd(dir).args(args).run().ok().map(|o| o.stdout_str())
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

pub fn harness(start: StartOptions) -> Harness<'static, DragonTreeApp> {
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| DragonTreeApp::new(cc, start))
}

pub fn start(base: &Path) -> StartOptions {
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
