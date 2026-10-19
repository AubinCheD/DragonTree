//! Parcours d'interface de bout en bout (egui_kittest) sur de vrais dépôts temporaires.
//! Les commandes git tournent en arrière-plan : on fait avancer l'interface jusqu'au résultat.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dragontree_app::{DragonTreeApp, StartOptions};
use dragontree_git::Git;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn git(dir: &Path, args: &[&str]) -> String {
    Git::discover().unwrap().cmd(dir).args(args).run().unwrap_or_else(|e| panic!("git {args:?} : {e}")).stdout_str()
}

/// Comme `git`, pour une attente : `None` si git échoue, par exemple en lisant `refs/stash` pendant que
/// l'application le réécrit.
fn try_git(dir: &Path, args: &[&str]) -> Option<String> {
    Git::discover().unwrap().cmd(dir).args(args).run().ok().map(|o| o.stdout_str())
}

fn init_repo(dir: &Path) {
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

fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) {
    let p = dir.join(file);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, content).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
}

/// Dépôt avec historique, un fichier modifié et un fichier non suivi.
fn demo_repo(base: &Path) -> PathBuf {
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

fn harness(start: StartOptions) -> Harness<'static, DragonTreeApp> {
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| DragonTreeApp::new(cc, start))
}

fn start(base: &Path) -> StartOptions {
    StartOptions {
        profile_dir: Some(base.join("profil")),
        screenshot_after: Duration::ZERO,
        // Les libellés cherchés par les tests sont ceux de l'interface en français.
        lang: Some(dragontree_i18n::Lang::Fr),
        ..Default::default()
    }
}

/// Fait tourner l'interface jusqu'à ce que `cond` soit vraie.
fn wait_until(
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

fn wait_git(h: &mut Harness<'static, DragonTreeApp>, what: &str, mut cond: impl FnMut() -> bool) {
    wait_until(h, what, |_| cond());
}

/// Fait tourner l'interface pendant `duration` au plus ; renvoie vrai si `cond` est devenue vraie.
fn becomes_true(h: &mut Harness<'static, DragonTreeApp>, duration: Duration, mut cond: impl FnMut() -> bool) -> bool {
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

/// Texte copié dans le presse-papiers pendant les prochaines images.
fn copied_text(h: &mut Harness<'static, DragonTreeApp>) -> Option<String> {
    for _ in 0..5 {
        h.step();
        for cmd in &h.output().platform_output.commands {
            if let egui::OutputCommand::CopyText(text) = cmd {
                return Some(text.clone());
            }
        }
    }
    None
}

/// Un champ de saisie contient exactement `value`.
fn has_text_input(h: &Harness<'static, DragonTreeApp>, value: &str) -> bool {
    h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some(value)).is_some()
}

/// Le champ « Résumé du commit » est vide (son texte indicatif est alors exposé).
fn subject_is_empty(h: &Harness<'static, DragonTreeApp>) -> bool {
    h.query_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"))
        .is_some_and(|n| n.accesskit_node().value().unwrap_or_default().is_empty())
}

#[test]
fn stage_all_then_commit_from_changes_view() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "deux fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());

    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Commit depuis l'interface");
    h.run_steps(3);
    h.get_by_label_contains("Commit (2 fichiers)").click();

    wait_git(&mut h, "commit créé", || {
        try_git(&repo, &["log", "-1", "--format=%s"]).is_some_and(|s| s.trim() == "Commit depuis l'interface")
    });
    wait_git(&mut h, "copie de travail propre", || {
        try_git(&repo, &["status", "--porcelain"]).is_some_and(|s| s.trim().is_empty())
    });
    wait_until(&mut h, "message effacé après le succès", subject_is_empty);
}

#[test]
fn failed_commit_keeps_the_message() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    std::fs::write(repo.join(".git/hooks/pre-commit"), "#!/bin/sh\necho 'refusé par le hook' >&2\nexit 1\n").unwrap();
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "deux fichiers indexés", |h| h.query_by_label_contains("Commit (2 fichiers)").is_some());
    let subject = h.get_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"));
    subject.focus();
    subject.type_text("Message à garder");
    h.run_steps(3);
    h.get_by_label_contains("Commit (2 fichiers)").click();

    wait_until(&mut h, "échec du commit signalé", |h| h.query_by_label_contains("a échoué").is_some());
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "2", "aucun commit créé");
    assert!(has_text_input(&h, "Message à garder"), "le message saisi n'est pas perdu");
}

#[test]
fn shift_click_after_the_list_shrinks_does_not_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "deux fichiers non indexés", |h| {
        h.query_by_label("notes.txt").is_some() && h.query_by_label("src/calc.rs").is_some()
    });
    // Ancre sur le dernier fichier de la liste (les non suivis viennent après les suivis),
    // qu'on indexe : la liste raccourcit.
    h.get_by_label("notes.txt").click();
    h.run_steps(3);
    h.get_by_label("notes.txt").click_secondary();
    let stage = format!("{} Indexer", dragontree_app::icons::PLUS);
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label(&stage).is_some());
    h.get_by_label(&stage).click();
    wait_git(&mut h, "fichier indexé", || {
        try_git(&repo, &["diff", "--cached", "--name-only"]).is_some_and(|s| s.trim() == "notes.txt")
    });
    wait_until(&mut h, "liste rafraîchie", |h| h.query_by_label_contains("Commit (1 fichier)").is_some());

    h.get_by_label("src/calc.rs").click_modifiers(egui::Modifiers::SHIFT);
    h.run_steps(5);
    assert!(h.query_by_label("src/calc.rs").is_some());
}

#[test]
fn ignore_whitespace_makes_the_diff_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("settings.json"), r#"{"ignore_whitespace": true}"#).unwrap();
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "note de lecture seule", |h| h.query_by_label_contains("Lecture seule").is_some());
    assert!(h.query_by_label_contains("Indexer le hunk").is_none(), "pas d'indexation partielle depuis un diff -w");
}

#[test]
fn enter_does_not_confirm_a_destructive_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "fichier modifié affiché", |h| h.query_by_label("src/calc.rs").is_some());
    h.get_by_label("src/calc.rs").click_secondary();
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label_contains("Annuler les modifications").is_some());
    h.get_by_label_contains("Annuler les modifications").click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label("Annuler les modifications").is_some());
    h.key_press(egui::Key::Enter);
    let discarded = becomes_true(&mut h, Duration::from_secs(2), || {
        std::fs::read_to_string(repo.join("src/calc.rs")).is_ok_and(|s| !s.contains("checked_add"))
    });
    assert!(!discarded, "Entrée ne doit pas valider un dialogue destructeur");
    assert!(h.query_by_label("Annuler les modifications").is_some(), "le dialogue reste ouvert");
}

#[test]
fn stage_one_hunk_from_diff() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton « Indexer le hunk »", |h| h.query_by_label_contains("Indexer le hunk").is_some());
    h.get_by_label_contains("Indexer le hunk").click();
    wait_git(&mut h, "hunk indexé", || {
        try_git(&repo, &["diff", "--cached", "--name-only"]).is_some_and(|s| s.trim() == "src/calc.rs")
    });
    assert!(git(&repo, &["diff", "--cached"]).contains("+    a.checked_add(b).unwrap()"));
    assert!(git(&repo, &["status", "--porcelain"]).contains("?? notes.txt"), "le fichier non suivi n'est pas touché");
}

#[test]
fn repo_list_scans_root_and_opens_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("DEV");
    for name in ["alpha", "beta"] {
        let dir = root.join(name);
        init_repo(&dir);
        commit_file(&dir, "README.md", name, &format!("Initial {name}"));
    }
    std::fs::create_dir_all(root.join("node_modules/ignored")).unwrap();
    let mut h = harness(StartOptions { add_root: Some(root.clone()), ..start(tmp.path()) });

    wait_until(&mut h, "dépôt beta listé", |h| {
        h.query_by_label("beta").is_some() && h.query_by_label("alpha").is_some()
    });
    h.get_by_label("beta").click();
    wait_until(&mut h, "historique de beta", |h| h.query_by_label("Initial beta").is_some());
}

#[test]
fn create_branch_from_toolbar_dialog() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "historique chargé", |h| h.query_by_label("Ajout de la multiplication").is_some());
    // Le bouton reste désactivé tant que les références ne sont pas chargées (tâche distincte de l'historique).
    wait_until(&mut h, "bouton Branche actif", |h| {
        h.query_by_label("Branche").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    h.get_by_label("Branche").click();
    wait_until(&mut h, "dialogue de branche", |h| h.query_by_label("Créer").is_some());
    let name = h.get_by(|n| n.placeholder() == Some("nom-de-la-branche (ex. feature/ma-fonction)"));
    name.focus();
    name.type_text("feature/depuis-ui");
    h.run_steps(3);
    h.get_by_label("Créer").click();
    wait_git(&mut h, "branche créée et active", || {
        try_git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).is_some_and(|s| s.trim() == "feature/depuis-ui")
    });
}

#[test]
fn selecting_a_commit_lists_its_files() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "commit listé", |h| h.query_by_label("Initial").is_some());
    h.get_by_label("Initial").click();
    wait_until(&mut h, "fichier du commit", |h| h.query_by_label("src/calc.rs").is_some());
}

/// Dépôt en cours de fusion avec un conflit sur `config.ini`.
fn conflicted_repo(base: &Path) -> PathBuf {
    let dir = base.join("conflit");
    init_repo(&dir);
    commit_file(&dir, "config.ini", "ligne 1\nvaleur = 10\nligne 3\n", "base");
    git(&dir, &["checkout", "-q", "-b", "feature/reglages"]);
    commit_file(&dir, "config.ini", "ligne 1\nvaleur = 42\nligne 3\n", "valeur 42");
    git(&dir, &["checkout", "-q", "main"]);
    commit_file(&dir, "config.ini", "ligne 1\nvaleur = 7\nligne 3\n", "valeur 7");
    assert!(
        Git::discover().unwrap().cmd(&dir).args(["merge", "feature/reglages"]).run().is_err(),
        "le merge doit être en conflit"
    );
    dir
}

#[test]
fn resolve_conflict_in_editor_then_commit_merge() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = conflicted_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton de l'éditeur de conflits", |h| {
        h.query_by_label_contains("Ouvrir l'éditeur de conflits").is_some()
    });
    h.get_by_label_contains("Ouvrir l'éditeur de conflits").click();
    wait_until(&mut h, "éditeur ouvert", |h| h.query_by_label("Leur version").is_some());
    h.get_by_label("Leur version").click();
    h.run_steps(3);
    h.get_by_label_contains("Enregistrer et marquer résolu").click();
    wait_git(&mut h, "conflit résolu et indexé", || {
        try_git(&repo, &["diff", "--name-only", "--diff-filter=U"]).is_some_and(|s| s.trim().is_empty())
    });
    assert_eq!(std::fs::read_to_string(repo.join("config.ini")).unwrap(), "ligne 1\nvaleur = 42\nligne 3\n");

    wait_until(&mut h, "bouton de validation de la fusion", |h| {
        h.query_by_label_contains("Valider la fusion").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    h.get_by_label_contains("Valider la fusion").click();
    wait_git(&mut h, "commit de fusion", || {
        try_git(&repo, &["rev-list", "--parents", "-n", "1", "HEAD"]).is_some_and(|s| s.split_whitespace().count() == 3)
    });
    assert_eq!(git(&repo, &["log", "-1", "--format=%s"]).trim(), "Merge branch 'feature/reglages'");
}

#[test]
fn interactive_rebase_reorders_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("rebase");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "base");
    commit_file(&repo, "a.txt", "a\n", "commit A");
    commit_file(&repo, "b.txt", "b\n", "commit B");
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "historique chargé", |h| h.query_by_label("base").is_some());
    h.get_by_label("base").click_secondary();
    wait_until(&mut h, "menu contextuel", |h| h.query_by_label_contains("Rebase interactif").is_some());
    h.get_by_label_contains("Rebase interactif").click();
    wait_until(&mut h, "éditeur de rebase", |h| {
        h.query_by_label("Lancer le rebase").is_some() && h.query_all_by_label("commit B").count() >= 2
    });
    // Remonter « commit B » au-dessus de « commit A » (flèche haut de la dernière ligne).
    let ups = h.query_all_by_label(dragontree_app::icons::ARROW_UP).collect::<Vec<_>>();
    ups.last().expect("bouton monter").click();
    h.run_steps(3);
    h.get_by_label("Lancer le rebase").click();
    wait_git(&mut h, "commits réordonnés", || {
        try_git(&repo, &["log", "--format=%s"])
            .is_some_and(|s| s.lines().collect::<Vec<_>>() == ["commit A", "commit B", "base"])
    });
}

#[test]
fn stash_then_pop_from_toolbar() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "fichiers modifiés affichés", |h| h.query_by_label("src/calc.rs").is_some());
    h.get_by_label("Stash").click();
    wait_until(&mut h, "dialogue de stash", |h| h.query_all_by_label("Stash").count() >= 2);
    h.query_all_by_label("Stash").last().unwrap().click();
    wait_git(&mut h, "stash créé", || try_git(&repo, &["stash", "list"]).is_some_and(|l| l.lines().count() == 1));
    assert!(git(&repo, &["status", "--porcelain"]).trim().is_empty(), "les non suivis sont inclus par défaut");

    wait_until(&mut h, "bouton Pop actif", |h| {
        h.query_by_label("Pop").is_some_and(|n| !n.accesskit_node().is_disabled())
    });
    h.get_by_label("Pop").click();
    wait_git(&mut h, "stash réappliqué", || try_git(&repo, &["stash", "list"]).is_some_and(|l| l.trim().is_empty()));
    assert!(std::fs::read_to_string(repo.join("src/calc.rs")).unwrap().contains("checked_add"));
}

#[test]
fn discard_file_after_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "fichier modifié affiché", |h| h.query_by_label("src/calc.rs").is_some());
    h.get_by_label("src/calc.rs").click_secondary();
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label_contains("Annuler les modifications").is_some());
    h.get_by_label_contains("Annuler les modifications").click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label("Annuler les modifications").is_some());
    h.get_by_label("Annuler les modifications").click();
    wait_git(&mut h, "fichier restauré", || {
        std::fs::read_to_string(repo.join("src/calc.rs")).is_ok_and(|s| !s.contains("checked_add"))
    });
    assert!(repo.join("notes.txt").exists(), "seul le fichier choisi est touché");
}

#[test]
fn abort_merge_asks_for_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = conflicted_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton d'abandon de la fusion", |h| {
        h.query_by_label_contains("Abandonner la fusion…").is_some()
    });
    h.get_by_label_contains("Abandonner la fusion…").click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label("Abandonner la fusion").is_some());
    assert!(repo.join(".git/MERGE_HEAD").exists(), "rien n'est abandonné avant la confirmation");
    h.get_by_label("Abandonner la fusion").click();
    wait_git(&mut h, "fusion abandonnée", || !repo.join(".git/MERGE_HEAD").exists());
}

#[test]
fn drop_stash_targets_the_chosen_stash_even_if_the_list_shifts() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("stashes");
    init_repo(&repo);
    commit_file(&repo, "f.txt", "base\n", "base");
    for name in ["stash A", "stash B"] {
        std::fs::write(repo.join("f.txt"), format!("{name}\n")).unwrap();
        git(&repo, &["stash", "push", "-m", name]);
    }
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "stash A listé", |h| h.query_by_label_contains("stash A").is_some());
    h.get_by_label_contains("stash A").click_secondary();
    wait_until(&mut h, "menu du stash", |h| h.query_by_label_contains("Supprimer…").is_some());
    h.get_by_label_contains("Supprimer…").click();
    wait_until(&mut h, "confirmation", |h| h.query_by_label("Supprimer").is_some());
    // Un nouveau stash arrive pendant que le dialogue est ouvert : les index se décalent.
    std::fs::write(repo.join("f.txt"), "stash C\n").unwrap();
    git(&repo, &["stash", "push", "-m", "stash C"]);
    h.get_by_label("Supprimer").click();

    wait_git(&mut h, "un stash supprimé", || {
        try_git(&repo, &["stash", "list"]).is_some_and(|l| l.lines().count() == 2)
    });
    let list = git(&repo, &["stash", "list", "--format=%s"]);
    assert!(list.contains("stash B") && list.contains("stash C"), "seul stash A est supprimé :\n{list}");
}

#[test]
fn context_menu_follows_its_commit_when_the_history_reloads() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("menu");
    init_repo(&repo);
    commit_file(&repo, "base.txt", "base\n", "base");
    commit_file(&repo, "a.txt", "a\n", "commit A");
    commit_file(&repo, "b.txt", "b\n", "commit B");
    let sha_a = git(&repo, &["rev-parse", "HEAD~1"]).trim().to_string();
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("history".into()), ..start(tmp.path()) });

    wait_until(&mut h, "historique chargé", |h| h.query_by_label("commit A").is_some());
    h.get_by_label("commit A").click_secondary();
    let copy_sha = format!("{} Copier le SHA", dragontree_app::icons::COPY);
    wait_until(&mut h, "menu du commit", |h| h.query_by_label(&copy_sha).is_some());
    // Un commit arrive pendant que le menu est ouvert : les lignes se décalent d'un cran.
    commit_file(&repo, "c.txt", "c\n", "commit C");
    wait_until(&mut h, "historique rechargé", |h| h.query_by_label("commit C").is_some());
    h.get_by_label(&copy_sha).click();
    assert_eq!(copied_text(&mut h).as_deref(), Some(sha_a.as_str()), "le menu agit sur le commit choisi");
}

#[test]
fn file_paths_are_resolved_from_the_repo_root_when_a_subfolder_is_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h =
        harness(StartOptions { open: vec![repo.join("src")], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "fichier modifié affiché", |h| h.query_by_label("src/calc.rs").is_some());
    h.get_by_label("src/calc.rs").click_secondary();
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label_contains("Copier le chemin complet").is_some());
    h.get_by_label_contains("Copier le chemin complet").click();
    let copied = copied_text(&mut h).expect("chemin copié");
    assert!(!copied.contains('/'), "séparateurs Windows attendus : {copied}");
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|e| panic!("{} : {e}", p.display()));
    assert_eq!(canon(Path::new(&copied)), canon(&repo.join("src").join("calc.rs")));
}

#[test]
fn ignore_menu_appends_to_an_existing_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    // .gitignore en Windows-1252 (« é » = 0xE9) : son contenu doit être conservé tel quel.
    std::fs::write(repo.join(".gitignore"), b"# fichiers g\xe9n\xe9r\xe9s\nbin/\n").unwrap();
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "fichier non suivi affiché", |h| h.query_by_label("notes.txt").is_some());
    h.get_by_label("notes.txt").click_secondary();
    let ignore = format!("{} Ignorer", dragontree_app::icons::EYE_SLASH);
    wait_until(&mut h, "menu du fichier", |h| h.query_by_label(&ignore).is_some());
    h.get_by_label(&ignore).click();
    wait_until(&mut h, "sous-menu", |h| h.query_by_label_contains("Ajouter « notes.txt »").is_some());
    h.get_by_label_contains("Ajouter « notes.txt »").click();
    wait_git(&mut h, "motif ajouté", || {
        std::fs::read(repo.join(".gitignore")).is_ok_and(|b| b.ends_with(b"notes.txt\n"))
    });
    assert_eq!(std::fs::read(repo.join(".gitignore")).unwrap(), b"# fichiers g\xe9n\xe9r\xe9s\nbin/\nnotes.txt\n");
}

/// Serveur HTTP qui accepte les connexions sans jamais répondre (clone bloqué).
fn silent_http_server() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    port
}

#[test]
fn closing_the_clone_window_cleans_up_the_cancelled_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let clones = tmp.path().join("clones");
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    let settings = serde_json::json!({ "default_clone_dir": clones });
    std::fs::write(profile.join("settings.json"), settings.to_string()).unwrap();
    let port = silent_http_server();
    let mut h = harness(StartOptions { window: Some("clone".into()), ..start(tmp.path()) });

    let hint = "https://github.com/organisation/depot.git";
    wait_until(&mut h, "fenêtre de clonage", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    let url = h.get_by(|n| n.placeholder() == Some(hint));
    url.focus();
    url.type_text(&format!("http://127.0.0.1:{port}/depot.git"));
    h.run_steps(3);
    let clone = format!("{} Cloner", dragontree_app::icons::DOWNLOAD_SIMPLE);
    h.get_by_label(&clone).click();
    let dest = clones.join("depot");
    wait_git(&mut h, "dossier du clone créé", || dest.exists());

    h.get_by_label("Close window").click();
    wait_git(&mut h, "clone annulé nettoyé", || !dest.exists());
}
