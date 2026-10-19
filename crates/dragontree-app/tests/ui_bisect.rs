//! Bisect : démarrage depuis le menu d'un commit ou Dépôt › Bisect, bandeau d'état (Bon, Mauvais, Passer, Terminer),
//! étiquettes des commits marqués et premier mauvais commit.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::icons;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, DragonTreeApp>;

const BANNER: &str = "Bisect en cours";

/// Dépôt `fabrikam` : huit commits `c1`…`c8`, `f.txt` contient `broken` à partir de `c5`.
fn repo(base: &Path) -> PathBuf {
    let dir = base.join("fabrikam");
    init_repo(&dir);
    for i in 1..=8 {
        let content = if i >= 5 { "broken\n" } else { "ok\n" };
        std::fs::write(dir.join("f.txt"), content).unwrap();
        commit_file(&dir, "n.txt", &format!("{i}\n"), &format!("c{i}"));
    }
    dir
}

fn open(base: &Path, repo: &Path) -> H {
    let mut h = harness(StartOptions { open: vec![repo.to_path_buf()], view: Some("history".into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| has(h, "c8"));
    // Les références lues (la branche dans la barre latérale) : le menu d'un commit gagne alors ses entrées de
    // fusion et de rebase, avant celles de bisect, qui se décalent. Lu plus tard, il bougerait sous le clic.
    wait_until(&mut h, "branche lue", |h| has(h, "main"));
    h
}

fn has(h: &H, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// Ouvre le menu d'un commit (clic droit sur sa ligne), clique une entrée de bisect dès qu'elle est utilisable, puis
/// attend que l'opération soit lancée (bandeau de progression « Bisect… ») ou faite (session ouverte).
fn commit_menu(h: &mut H, repo: &Path, commit: &str, entry: &str) {
    let row = h.query_all_by_label(commit).next().unwrap().rect().center();
    click_at(h, row, egui::PointerButton::Secondary);
    click_label(h, entry);
    wait_until(h, &format!("« {entry} » pris en compte"), |h| {
        has(h, "Bisect…") || repo.join(".git").join("BISECT_START").exists()
    });
}

fn mark_good() -> String {
    format!("{} Bisect : marquer comme bon", icons::CHECK)
}

#[allow(dead_code)]
fn mark_bad() -> String {
    format!("{} Bisect : marquer comme mauvais", icons::BUG)
}

fn head_summary(repo: &Path) -> Option<String> {
    try_git(repo, &["log", "-1", "--format=%s"]).map(|s| s.trim().to_string())
}

/// Clique le bouton du bandeau dès qu'il est utilisable : les réponses sont grisées tant qu'une opération tourne.
fn click_banner(h: &mut H, label: &str) {
    click_label(h, label);
    h.run_steps(3);
}

/// Bouton du bandeau affiché et utilisable.
fn usable(h: &H, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some_and(|n| !n.accesskit_node().is_disabled())
}

/// Donne la réponse `label` au commit en cours d'examen et attend que git la prenne en compte : commit suivant extrait,
/// ou premier mauvais commit annoncé. Le bouton n'est cliqué qu'utilisable — aucune opération ni relecture en cours,
/// donc pour le commit que git a extrait. Un clic perdu (bouton grisé à cet instant par une relecture que la
/// surveillance du disque a lancée) est redonné ; une réponse prise en compte ne l'est jamais deux fois, car le
/// bouton ne redevient utilisable qu'après la relecture qui suit l'opération.
fn answer(h: &mut H, repo: &Path, label: &str) {
    let before = head_summary(repo);
    let answered = |h: &H| {
        h.query_all_by_label_contains("premier mauvais commit").next().is_some()
            || head_summary(repo).is_some_and(|s| Some(s) != before)
    };
    for _ in 0..10 {
        wait_until(h, &format!("« {label} » utilisable, ou réponse prise en compte"), |h| {
            answered(h) || usable(h, label)
        });
        if answered(h) {
            return;
        }
        let button = h.get_by_label(label).rect().center();
        // L'image du clic grise aussitôt le bouton s'il est pris en compte.
        click_at(h, button, egui::PointerButton::Primary);
        h.run_steps(1);
    }
    panic!("réponse « {label} » jamais prise en compte");
}

#[test]
fn marking_a_commit_from_its_menu_starts_a_session_with_a_banner_and_labels() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    assert!(!has(&h, BANNER));
    commit_menu(&mut h, &repo, "c1", &mark_good());
    wait_until(&mut h, "bandeau", |h| has(h, BANNER));
    // HEAD (c8) est mauvais par défaut, c1 est bon : les deux portent une étiquette.
    wait_until(&mut h, "étiquettes", |h| {
        has(h, &format!("{} bisect mauvais", icons::BUG)) && has(h, &format!("{} bisect bon", icons::CHECK))
    });
    wait_until(&mut h, "révisions restantes", |h| {
        h.query_all_by_label_contains("révisions à tester").next().is_some()
    });
    assert!(repo.join(".git").join("BISECT_START").exists());
}

#[test]
fn terminate_removes_the_banner_and_restores_the_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    commit_menu(&mut h, &repo, "c1", &mark_good());
    wait_until(&mut h, "bandeau", |h| has(h, BANNER));
    click_banner(&mut h, &format!("{} Terminer", icons::X));
    wait_until(&mut h, "bandeau retiré", |h| !has(h, BANNER));
    assert!(!repo.join(".git").join("BISECT_START").exists());
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
    assert!(!has(&h, &format!("{} bisect mauvais", icons::BUG)));
}

#[test]
fn answering_from_the_banner_ends_on_the_first_broken_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    // Avant la session : ensuite, HEAD se déplace.
    let first_bad = git(&repo, &["rev-parse", "--short=7", "HEAD~3"]).trim().to_string();
    let mut h = open(tmp.path(), &repo);
    commit_menu(&mut h, &repo, "c1", &mark_good());
    wait_until(&mut h, "bandeau", |h| has(h, BANNER));
    for _ in 0..6 {
        // L'état est lu en tâche de fond : on attend la réponse possible, ou la fin, et que git ait fini d'extraire le commit.
        wait_until(&mut h, "réponse possible ou fin", |h| {
            has(h, &format!("{} Bon", icons::CHECK))
                || h.query_all_by_label_contains("premier mauvais commit").next().is_some()
        });
        if h.query_all_by_label_contains("premier mauvais commit").next().is_some() {
            break;
        }
        wait_git(&mut h, "commit extrait", || {
            let expected = std::fs::read_to_string(repo.join(".git").join("BISECT_EXPECTED_REV")).ok();
            expected.zip(try_git(&repo, &["rev-parse", "HEAD"])).is_some_and(|(a, b)| a.trim() == b.trim())
        });
        // Le commit en cours d'examen est cassé ou non : le test lit f.txt comme le ferait l'utilisateur.
        let broken = std::fs::read_to_string(repo.join("f.txt")).unwrap().trim() == "broken";
        let label = if broken { format!("{} Mauvais", icons::BUG) } else { format!("{} Bon", icons::CHECK) };
        answer(&mut h, &repo, &label);
    }
    wait_until(&mut h, "premier mauvais commit annoncé", |h| {
        h.query_all_by_label_contains(&format!("premier mauvais commit : {first_bad}")).next().is_some()
    });
    // Le premier mauvais commit est sélectionnable : « Voir le commit » ouvre ses détails.
    click_banner(&mut h, &format!("{} Voir le commit", icons::CROSSHAIR));
    wait_until(&mut h, "détails du commit", |h| h.query_all_by_label_contains(&first_bad).next().is_some());
}

#[test]
fn the_repository_menu_starts_and_ends_a_session() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    let mut h = open(tmp.path(), &repo);
    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Bisect");
    wait_until(&mut h, "« Démarrer » utilisable", |h| {
        menu_item(h, "Démarrer").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    // « Terminer » n'a pas de sens hors session.
    assert!(menu_item_disabled(&h, "Terminer"));
    click_menu_item(&mut h, "Démarrer");
    wait_until(&mut h, "bandeau", |h| has(h, BANNER));
    wait_until(&mut h, "consigne", |h| h.query_all_by_label_contains("marquez un commit bon").next().is_some());
    // Pas de réponse possible tant qu'aucun commit n'est bon.
    assert!(!has(&h, &format!("{} Mauvais", icons::BUG)));

    open_menu(&mut h, "Dépôt");
    open_menu(&mut h, "Bisect");
    assert!(menu_item_disabled(&h, "Démarrer"));
    click_menu_item(&mut h, "Terminer");
    wait_until(&mut h, "bandeau retiré", |h| !has(h, BANNER));
}

#[test]
fn the_menu_entries_are_greyed_during_a_merge_in_progress() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo(tmp.path());
    // Une fusion en cours se reconnaît à MERGE_HEAD.
    let head = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join(".git").join("MERGE_HEAD"), head).unwrap();
    let mut h = open(tmp.path(), &repo);
    wait_until(&mut h, "bandeau de fusion", |h| has(h, "Fusion en cours"));
    let row = h.query_all_by_label("c1").next().unwrap().rect().center();
    click_at(&mut h, row, egui::PointerButton::Secondary);
    let entry = mark_good();
    wait_until(&mut h, "entrée du menu", |h| has(h, &entry));
    assert!(h.get_by_label(&entry).accesskit_node().is_disabled());
    assert!(h.get_by_label(&mark_bad()).accesskit_node().is_disabled());
}
