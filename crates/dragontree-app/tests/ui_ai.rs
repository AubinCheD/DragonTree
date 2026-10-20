//! IA : « Générer avec Claude » (message de commit) et revue de la branche. La CLI est un faux `claude.cmd` qui range
//! son entrée standard et ses arguments à côté de lui, puis répond un message fixe : rien n'est envoyé nulle part.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

type H = Harness<'static, dragontree_app::DragonTreeApp>;

const GENERATE: &str = "Générer avec Claude";

/// Faux `claude` qui répond `reply` et enregistre la consigne reçue sur stdin dans `stdin.txt`.
fn seed_fake_claude(base: &Path, reply: &[&str], enabled: bool) -> PathBuf {
    seed_fake_claude_after(base, reply, enabled, 0)
}

/// Comme `seed_fake_claude`, mais ne répond qu'après `seconds` secondes (le temps de saisir un message).
fn seed_fake_claude_after(base: &Path, reply: &[&str], enabled: bool, seconds: u32) -> PathBuf {
    let dir = base.join("faux-claude");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("claude.cmd");
    let echoes: String =
        reply.iter().map(|l| if l.is_empty() { "echo.\r\n".to_string() } else { format!("echo {l}\r\n") }).collect();
    let wait = seconds + 1; // `ping -n 1` ne patiente pas
    let body = format!("@echo off\r\nmore > \"%~dp0stdin.txt\"\r\nping -n {wait} 127.0.0.1 > nul\r\n{echoes}");
    std::fs::write(&script, body).unwrap();
    let settings = Settings { ai_enabled: enabled, ai_cli_path: script.display().to_string(), ..Settings::default() };
    settings.save_to(&base.join("profil").join("settings.json")).unwrap();
    dir.join("stdin.txt")
}

fn open(base: &Path, view: &str, ready: &str) -> (H, PathBuf) {
    let repo = demo_repo(base);
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some(view.into()), ..start(base) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label_contains(ready).is_some());
    (h, repo)
}

fn stage_everything(h: &mut H) {
    wait_until(h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(h, "fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());
}

#[test]
fn generate_fills_the_summary_and_the_description_from_the_staged_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let stdin =
        seed_fake_claude(tmp.path(), &["Ajoute des notes et un calcul sûr", "", "Le calcul ne panique plus."], true);
    let (mut h, _repo) = open(tmp.path(), "changes", "Tout indexer");
    // Sans fichier indexé, le bouton est grisé : il n'y a rien à envoyer.
    wait_until(&mut h, "bouton Générer", |h| h.query_by_label_contains(GENERATE).is_some());
    assert!(h.get_by_label_contains(GENERATE).accesskit_node().is_disabled());
    stage_everything(&mut h);
    wait_until(&mut h, "bouton actif", |h| {
        h.query_by_label_contains(GENERATE).is_some_and(|b| !b.accesskit_node().is_disabled())
    });
    h.get_by_label_contains(GENERATE).click();

    wait_until(&mut h, "résumé rempli", |h| has_text_input(h, "Ajoute des notes et un calcul sûr"));
    wait_until(&mut h, "description remplie", |h| {
        h.query_all_by(|n| n.value().as_deref() == Some("Le calcul ne panique plus.")).next().is_some()
    });
    // La consigne envoyée contient le diff indexé (sur stdin, jamais dans les arguments).
    let prompt = std::fs::read_to_string(stdin).unwrap();
    assert!(prompt.contains("checked_add") && prompt.contains("+à faire"), "{prompt}");
}

#[test]
fn a_message_typed_while_claude_works_is_kept_and_the_answer_is_only_suggested() {
    let tmp = tempfile::tempdir().unwrap();
    seed_fake_claude_after(tmp.path(), &["Titre de Claude"], true, 3);
    let (mut h, _repo) = open(tmp.path(), "changes", "Tout indexer");
    stage_everything(&mut h);
    wait_until(&mut h, "bouton actif", |h| {
        h.query_by_label_contains(GENERATE).is_some_and(|b| !b.accesskit_node().is_disabled())
    });
    h.get_by_label_contains(GENERATE).click();
    // Le résumé est saisi sans attendre la réponse : il ne sera pas écrasé.
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Mon propre titre");
    wait_until(&mut h, "suggestion proposée", |h| {
        h.query_by_label_contains("Suggestion de Claude : Titre de Claude").is_some()
    });
    assert!(has_text_input(&h, "Mon propre titre"));
    // La zone de commit se met à la bonne hauteur une image après l'apparition de la ligne : on attend qu'elle soit stable.
    h.run_steps(8);
    h.get_by_label("Remplacer le message").click();
    wait_until(&mut h, "message remplacé", |h| has_text_input(h, "Titre de Claude"));
}

#[test]
fn a_missing_cli_shows_an_error_and_nothing_is_filled() {
    let tmp = tempfile::tempdir().unwrap();
    let settings = Settings {
        ai_enabled: true,
        ai_cli_path: tmp.path().join("absent").join("claude.exe").display().to_string(),
        ..Settings::default()
    };
    settings.save_to(&tmp.path().join("profil").join("settings.json")).unwrap();
    let (mut h, _repo) = open(tmp.path(), "changes", "Tout indexer");
    stage_everything(&mut h);
    wait_until(&mut h, "bouton actif", |h| {
        h.query_by_label_contains(GENERATE).is_some_and(|b| !b.accesskit_node().is_disabled())
    });
    h.get_by_label_contains(GENERATE).click();
    wait_until(&mut h, "erreur affichée", |h| {
        h.query_by_label_contains("La CLI Claude Code est introuvable").is_some()
    });
    // Rien n'est rempli : ni le résumé ni la description.
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    assert_eq!(subject.accesskit_node().value().as_deref().unwrap_or(""), "");
    assert!(h.query_by_label_contains("Suggestion de Claude").is_none());
}

#[test]
fn a_truncated_diff_is_announced_next_to_the_generated_message() {
    let tmp = tempfile::tempdir().unwrap();
    let stdin = seed_fake_claude(tmp.path(), &["Ajoute un gros fichier"], true);
    let repo = demo_repo(tmp.path());
    // Plus de 100 Ko de diff indexé.
    std::fs::write(
        repo.join("gros.txt"),
        "une ligne assez longue pour peser dans le diff
"
        .repeat(4000),
    )
    .unwrap();
    let mut h = harness(StartOptions { open: vec![repo], view: Some("changes".into()), ..start(tmp.path()) });
    wait_until(&mut h, "zone de commit", |h| h.query_by_label_contains("Tout indexer").is_some());
    wait_until(&mut h, "fichiers listés", |h| h.query_by_label_contains("gros.txt").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "fichiers indexés", |h| h.query_by_label_contains("Commit (3 fichiers)").is_some());
    wait_until(&mut h, "bouton actif", |h| {
        h.query_by_label_contains(GENERATE).is_some_and(|b| !b.accesskit_node().is_disabled())
    });
    h.get_by_label_contains(GENERATE).click();
    wait_until(&mut h, "résumé rempli", |h| has_text_input(h, "Ajoute un gros fichier"));
    wait_until(&mut h, "note de troncature", |h| h.query_by_label_contains("Diff tronqué à 100 Ko").is_some());
    let prompt = std::fs::read_to_string(stdin).unwrap();
    assert!(prompt.contains("diff truncated") && prompt.len() < 110_000, "{} octets", prompt.len());
}

#[test]
fn an_answer_arriving_after_the_commit_does_not_fill_the_next_message() {
    let tmp = tempfile::tempdir().unwrap();
    seed_fake_claude_after(tmp.path(), &["Titre tardif"], true, 4);
    let (mut h, repo) = open(tmp.path(), "changes", "Tout indexer");
    stage_everything(&mut h);
    wait_until(&mut h, "bouton actif", |h| {
        h.query_by_label_contains(GENERATE).is_some_and(|b| !b.accesskit_node().is_disabled())
    });
    h.get_by_label_contains(GENERATE).click();
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Commit immédiat");
    h.run_steps(3);
    h.get_by_label_contains("Commit (2 fichiers)").click();
    wait_git(&mut h, "commit créé", || {
        try_git(&repo, &["log", "-1", "--format=%s"]).is_some_and(|s| s.trim() == "Commit immédiat")
    });
    wait_until(&mut h, "résumé vidé", |h| !has_text_input(h, "Commit immédiat"));
    // La réponse arrive (4 s) : elle est périmée, ni remplie ni proposée.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(7);
    while std::time::Instant::now() < deadline {
        h.step();
        assert!(!has_text_input(&h, "Titre tardif"), "la réponse périmée a rempli le message suivant");
        assert!(h.query_by_label_contains("Suggestion de Claude").is_none());
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn the_buttons_are_absent_while_ai_is_disabled() {
    let tmp = tempfile::tempdir().unwrap();
    seed_fake_claude(tmp.path(), &["x"], false);
    let (mut h, _repo) = open(tmp.path(), "changes", "Tout indexer");
    h.run_steps(10);
    assert!(h.query_by_label_contains(GENERATE).is_none());
    open_menu(&mut h, "Dépôt");
    wait_until(&mut h, "entrée de revue", |h| menu_item(h, "Revue de la branche par Claude…").is_some());
    assert!(menu_item_disabled(&h, "Revue de la branche par Claude…"));
}

#[test]
fn the_branch_review_reads_the_diff_from_the_merge_base_and_shows_claudes_text() {
    let tmp = tempfile::tempdir().unwrap();
    let stdin = seed_fake_claude(tmp.path(), &["Revue : tout est clair", "Verdict : approuvé"], true);
    let repo = demo_repo(tmp.path());
    // La branche de travail ajoute un fichier à main ; l'état de main après la bifurcation n'en fait pas partie.
    git(&repo, &["stash", "-u", "-q"]);
    git(&repo, &["checkout", "-q", "-b", "fonction"]);
    commit_file(&repo, "src/neuf.rs", "fn neuf() {}\n", "Ajoute neuf");
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label_contains("Ajoute neuf").is_some());

    open_menu(&mut h, "Dépôt");
    click_menu_item(&mut h, "Revue de la branche par Claude…");
    wait_until(&mut h, "revue affichée", |h| {
        h.query_all_by(|n| n.value().as_deref().is_some_and(|v| v.contains("Verdict : approuvé"))).next().is_some()
    });
    assert!(h.query_by_label_contains("1 commit comparé à main").is_some());
    let prompt = std::fs::read_to_string(stdin).unwrap();
    assert!(prompt.contains("`fonction` against `main`") && prompt.contains("- Ajoute neuf"), "{prompt}");
    assert!(prompt.contains("+fn neuf() {}") && !prompt.contains("checked_add"), "{prompt}");
}
