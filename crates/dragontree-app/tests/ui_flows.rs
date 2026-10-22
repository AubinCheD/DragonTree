//! Parcours d'interface de bout en bout (egui_kittest) sur de vrais dépôts temporaires.
//! Les commandes git tournent en arrière-plan : on fait avancer l'interface jusqu'au résultat.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::*;
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

/// Le champ « Résumé du commit » est vide (son texte indicatif est alors exposé).
fn subject_is_empty(h: &Harness<'static, DragonTreeApp>) -> bool {
    h.query_by(|n| n.placeholder() == Some("Résumé du commit (obligatoire)"))
        .is_some_and(|n| n.accesskit_node().value().unwrap_or_default().is_empty())
}

/// Contrainte 6 du plan : sans rien demander, les helpers des tests d'interface (dépôt de test, commandes git, harnais)
/// isolent la configuration git globale. git, celui du test comme celui de l'application (même environnement de
/// processus), lit pour la portée globale un fichier temporaire et lui seul, jamais le `~/.gitconfig` de
/// l'utilisateur.
#[test]
fn ui_tests_never_read_the_users_global_git_config() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = demo_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], ..start(tmp.path()) });
    wait_until(&mut h, "dépôt chargé", |h| h.query_by_label("main").is_some());
    // Vérifié avant toute écriture : en cas d'échec, rien n'est écrit dans la vraie configuration.
    let global = std::env::var_os("GIT_CONFIG_GLOBAL")
        .map(PathBuf::from)
        .filter(|f| f.starts_with(std::env::temp_dir()))
        .expect("GIT_CONFIG_GLOBAL désigne un fichier du dossier temporaire");
    for var in ["HOME", "USERPROFILE", "XDG_CONFIG_HOME"] {
        assert_eq!(std::env::var_os(var).map(PathBuf::from).as_deref(), global.parent(), "{var} temporaire");
    }

    git(&repo, &["config", "--global", "dragontree.isolation", "oui"]);
    let scopes = git(&repo, &["config", "--list", "--show-scope", "--show-origin"]);
    let global_lines: Vec<&str> = scopes.lines().filter(|l| l.starts_with("global\t")).collect();
    assert!(
        global_lines.iter().any(|l| l.ends_with("dragontree.isolation=oui")),
        "clé lue en portée globale : {scopes}"
    );
    assert!(global_lines.iter().all(|l| origin_is(l, &global)), "portée globale lue hors de {global:?} : {scopes}");
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
    wait_until(&mut h, "résumé saisi", |h| has_text_input(h, "Commit depuis l'interface"));
    click_label_containing(&mut h, "Commit (2 fichiers)");

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
    wait_until(&mut h, "résumé saisi", |h| has_text_input(h, "Message à garder"));
    click_label_containing(&mut h, "Commit (2 fichiers)");

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

/// Option « Espaces » (L6) : le diff affiché ignore les espaces ; « Indexer le hunk » indexe le hunk avec les
/// changements d'espaces de ses lignes (ligne passée de CRLF à LF, ligne réindentée), pas ceux d'en dehors. Index
/// vérifié octet pour octet ; copie de travail intacte.
#[test]
fn a_hunk_is_staged_with_whitespace_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("depot");
    init_repo(&repo);
    let old = [
        "class Form",
        "{",
        "    void Load()",
        "    {",
        "        x = 1;",
        "        y = 2;",
        "    }",
        "",
        "    // un",
        "    // deux",
        "    // trois",
        "    // quatre",
        "    void Save()",
        "    {",
        "        z = 3;",
        "    }",
        "}",
    ];
    // Lignes en CRLF, sauf les remplacements `(numéro, ligne complète)`.
    let form = |changes: &[(usize, &str)]| -> Vec<u8> {
        let line =
            |i: usize, l: &str| changes.iter().find(|(n, _)| *n == i + 1).map_or(format!("{l}\r\n"), |c| c.1.into());
        old.iter().enumerate().flat_map(|(i, l)| line(i, l).into_bytes()).collect()
    };
    std::fs::write(repo.join("form.cs"), form(&[])).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let inside = [(4, "    {\n"), (5, "        x = 10;\r\n"), (6, "\t\ty = 2;\r\n")];
    let outside = [(15, "\tz = 3;\r\n"), (16, "    }  \r\n")];
    let worktree = form(&[inside.as_slice(), outside.as_slice()].concat());
    std::fs::write(repo.join("form.cs"), &worktree).unwrap();

    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("settings.json"), r#"{"ignore_whitespace": true}"#).unwrap();
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    wait_until(&mut h, "bouton « Indexer le hunk »", |h| h.query_by_label_contains("Indexer le hunk").is_some());
    assert_eq!(h.query_all_by_label_contains("Indexer le hunk").count(), 1, "un seul hunk sans les espaces");
    assert!(h.query_by_label_contains("Espaces ignorés à l'affichage").is_some(), "règle rappelée");
    h.get_by_label_contains("Indexer le hunk").click();
    let index = || test_git().cmd(&repo).args(["cat-file", "blob", ":form.cs"]).run().ok().map(|o| o.stdout);
    wait_git(&mut h, "hunk indexé", || index().is_some_and(|b| b != form(&[])));
    assert_eq!(index().unwrap(), form(&inside), "changements d'espaces du hunk seulement");
    assert_eq!(std::fs::read(repo.join("form.cs")).unwrap(), worktree, "copie de travail intacte");
}

/// Dépôt `depot` dont le fichier `name` passe de `old` (commité) à `new` (copie de travail), interface lancée sur la
/// vue des changements avec l'option « Espaces », une fois `prepare` passé sur le dépôt (le premier fichier est choisi).
fn whitespace_ignored(
    base: &Path,
    name: &str,
    old: &[u8],
    new: &[u8],
    prepare: impl FnOnce(&Path),
) -> Harness<'static, DragonTreeApp> {
    let repo = base.join("depot");
    init_repo(&repo);
    std::fs::write(repo.join(name), old).unwrap();
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    std::fs::write(repo.join(name), new).unwrap();
    prepare(&repo);
    let profile = base.join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("settings.json"), r#"{"ignore_whitespace": true}"#).unwrap();
    harness(StartOptions { open: vec![repo], view: Some("changes".into()), ..start(base) })
}

/// Revue finale (C1) : avec « Espaces », un diff complet illisible ou incomplet (ici au-delà de la limite de lecture
/// de 8 Mio) ne fait plus échouer l'affichage et ne sert pas aux patchs : le diff sans les espaces reste affiché, en
/// lecture seule pour l'indexation, avec l'explication.
#[test]
fn a_full_diff_that_cannot_be_read_whole_leaves_the_whitespace_diff_read_only() {
    let tmp = tempfile::tempdir().unwrap();
    // Changement visible en tête ; espaces ajoutés à 100 longues lignes : le diff complet dépasse 8 Mio, pas le diff
    // affiché.
    let long = "a".repeat(50_000);
    let text = |first: &str, tail: &str| -> Vec<u8> {
        let mut s = format!("{first}\n");
        for i in 0..9 {
            s.push_str(&format!("ligne {i}\n"));
        }
        for _ in 0..100 {
            s.push_str(&format!("{long}{tail}\n"));
        }
        s.into_bytes()
    };
    let mut h = whitespace_ignored(tmp.path(), "gros.txt", &text("x = 1", ""), &text("x = 2", " "), |_| {});

    wait_until(&mut h, "diff en lecture seule expliqué", |h| h.query_by_label_contains("Lecture seule").is_some());
    assert!(h.query_by_label_contains("Indexer le hunk").is_none(), "pas d'indexation sur un diff incomplet");
    assert_eq!(h.query_all_by_label_contains("Diff impossible").count(), 0);
}

/// Revue finale (C3) : la note du dialogue « Annuler des lignes » sur les changements d'espaces d'un hunk entier n'est
/// affichée que si le hunk en contient.
#[test]
fn the_discard_note_on_whitespace_shows_only_for_a_hunk_that_has_whitespace_changes() {
    for (new, expected) in [("a\r\nB\r\nc\r\n", false), ("a\nB\r\nc\r\n", true)] {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = whitespace_ignored(tmp.path(), "f.txt", b"a\r\nb\r\nc\r\n", new.as_bytes(), |_| {});
        click_label_containing(&mut h, "Annuler le hunk");
        wait_until(&mut h, "dialogue d'annulation", |h| {
            h.query_by_label_contains("Le hunk sera annulé dans la copie de travail").is_some()
        });
        let note = h.query_by_label_contains("Ses changements d'espaces").is_some();
        assert_eq!(note, expected, "note pour {new:?}");
    }
}

/// Revue finale (C4) : avec « Espaces », un changement de mode ne suit pas les hunks ni les lignes : le diff le dit.
#[test]
fn a_mode_change_is_pointed_out_when_whitespace_is_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    // Contenu et mode indexés (le mode passe par l'index : `core.fileMode` est faux sous Windows) ; le fichier indexé,
    // seul modifié, est choisi.
    let mut h = whitespace_ignored(tmp.path(), "run.sh", b"a\nb\n", b"a\nB\n", |repo| {
        git(repo, &["add", "run.sh"]);
        git(repo, &["update-index", "--chmod=+x", "run.sh"]);
    });
    wait_until(&mut h, "changement de mode signalé", |h| {
        h.query_by_label_contains("Changement de mode (100644 → 100755)").is_some()
    });
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
    wait_until(&mut h, "nom saisi", |h| has_text_input(h, "feature/depuis-ui"));
    click_label(&mut h, "Créer");
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
    assert!(test_git().cmd(&dir).args(["merge", "feature/reglages"]).run().is_err(), "le merge doit être en conflit");
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

/// Fichier Windows-1252 (é = E9, € = 80, œ = 9C, ’ = 92) avec ces deux lignes variables, séparées par assez de lignes
/// communes pour que git en fasse deux conflits.
fn cp1252_form(title: &[u8], price: &[u8]) -> Vec<u8> {
    [
        b"// Fen\xeatre principale\r\n".as_slice(),
        title,
        b"// L\x92\x9cuvre compl\xe8te\n// d\xe9but\n// suite\n// encore\n// fin du milieu\n",
        price,
        b"// Fin du fichier \xe0 garder\n",
    ]
    .concat()
}

/// Dépôt en fusion arrêtée sur deux conflits dans `form.cs`, fichier Windows-1252 (non UTF-8).
fn cp1252_conflicted_repo(base: &Path) -> PathBuf {
    let dir = base.join("ansi");
    init_repo(&dir);
    let commit = |content: Vec<u8>, msg: &str| {
        std::fs::write(dir.join("form.cs"), content).unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", msg]);
    };
    commit(cp1252_form(b"titre = \"R\xe9glages\"\n", b"prix = \"10 \x80\"\n"), "base");
    git(&dir, &["checkout", "-q", "-b", "autre"]);
    commit(cp1252_form(b"titre = \"Pr\xe9f\xe9rences\"\n", b"prix = \"12 \x80\"\n"), "leur version");
    git(&dir, &["checkout", "-q", "main"]);
    commit(cp1252_form(b"titre = \"Param\xe8tres\"\n", b"prix = \"11 \x80\"\n"), "notre version");
    assert!(try_git(&dir, &["merge", "autre"]).is_none(), "le merge doit être en conflit");
    dir
}

/// L2 : l'éditeur de conflits ouvre un fichier Windows-1252 (texte décodé), le résout région par région (notre
/// version pour la première, la leur pour la seconde) et écrit les octets d'origine, hors conflits compris.
#[test]
fn a_cp1252_conflict_is_resolved_region_by_region_in_the_editor() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = cp1252_conflicted_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo.clone()], view: Some("changes".into()), ..start(tmp.path()) });

    click_label_containing(&mut h, "Ouvrir l'éditeur de conflits");
    // Les lignes des côtés sont décodées (dans leur région, et dans l'aperçu tant qu'elle n'est pas résolue).
    wait_until(&mut h, "deux conflits décodés", |h| {
        h.query_all_by_label("Notre version").count() == 2 && h.query_all_by_label("prix = \"11 €\"").next().is_some()
    });
    assert!(h.query_all_by_label("titre = \"Préférences\"").next().is_some(), "leur côté décodé");
    assert!(h.query_by_label_contains("Fichier en Windows-1252").is_some(), "encodage signalé");
    h.query_all_by_label("Notre version").next().unwrap().click();
    h.run_steps(3);
    h.query_all_by_label("Leur version").nth(1).unwrap().click();
    h.run_steps(3);
    click_label_containing(&mut h, "Enregistrer et marquer résolu");
    wait_git(&mut h, "conflit résolu et indexé", || {
        try_git(&repo, &["diff", "--name-only", "--diff-filter=U"]).is_some_and(|s| s.trim().is_empty())
    });
    let expected = cp1252_form(b"titre = \"Param\xe8tres\"\n", b"prix = \"12 \x80\"\n");
    assert_eq!(std::fs::read(repo.join("form.cs")).unwrap(), expected, "octets d'origine, sans réencodage");
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

#[test]
fn closing_the_clone_window_cleans_up_the_cancelled_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let clones = tmp.path().join("clones");
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    let settings = serde_json::json!({ "default_clone_dir": clones });
    std::fs::write(profile.join("settings.json"), settings.to_string()).unwrap();
    // Serveur muet : le clone reste bloqué.
    let server = SilentServer::start();
    let port = server.port;
    let mut h = harness(StartOptions { window: Some("clone".into()), ..start(tmp.path()) });

    let hint = "https://github.com/organisation/depot.git";
    wait_until(&mut h, "fenêtre de clonage", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    let url = h.get_by(|n| n.placeholder() == Some(hint));
    url.focus();
    url.type_text(&format!("http://127.0.0.1:{port}/depot.git"));
    wait_until(&mut h, "dossier déduit de l'URL", |h| has_text_input(h, "depot"));
    click_label(&mut h, &format!("{} Cloner", dragontree_app::icons::DOWNLOAD_SIMPLE));
    let dest = clones.join("depot");
    wait_git(&mut h, "dossier du clone créé", || dest.exists());

    h.get_by_label("Close window").click();
    wait_git(&mut h, "clone annulé nettoyé", || !dest.exists());
}

/// Dépôt SHA-256 avec un commit, ou `None` (message) si le git installé ne sait pas en créer (git 2.29 et suivants).
fn sha256_repo(base: &Path) -> Option<PathBuf> {
    if !git_version::sha256_supported(&test_git()) {
        return None;
    }
    let dir = base.join("sha256");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main", "--object-format=sha256"]);
    for (k, v) in [("user.name", "Testeur"), ("user.email", "t@example.com"), ("commit.gpgsign", "false")] {
        git(&dir, &["config", k, v]);
    }
    commit_file(&dir, "a.txt", "a\n", "Premier");
    Some(dir)
}

/// L1 : un dépôt SHA-256 s'ouvre sur une erreur explicite, et son clonage échoue avec la même erreur (le dossier créé
/// par le clonage est supprimé, comme après un clonage échoué).
#[test]
fn a_sha256_repository_is_refused_with_an_explicit_error_at_open_and_clone() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(repo) = sha256_repo(tmp.path()) else { return };
    let clones = tmp.path().join("clones");
    let profile = tmp.path().join("profil");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(profile.join("settings.json"), serde_json::json!({ "default_clone_dir": clones }).to_string())
        .unwrap();
    let explicit = "utilise des identifiants SHA-256 : DragonTree ne prend en charge que les dépôts SHA-1";

    let mut h = harness(StartOptions { open: vec![repo.clone()], ..start(tmp.path()) });
    wait_until(&mut h, "erreur d'ouverture explicite", |h| h.query_by_label_contains(explicit).is_some());
    assert!(
        h.query_by_label_contains("Utilisez git en ligne de commande pour ce dépôt").is_some(),
        "la page d'erreur renvoie vers git en ligne de commande"
    );
    drop(h);

    let mut h = harness(StartOptions { window: Some("clone".into()), ..start(tmp.path()) });
    let hint = "https://github.com/organisation/depot.git";
    wait_until(&mut h, "fenêtre de clonage", |h| h.query_by(|n| n.placeholder() == Some(hint)).is_some());
    let url = h.get_by(|n| n.placeholder() == Some(hint));
    url.focus();
    url.type_text(&format!("file:///{}", repo.display().to_string().replace('\\', "/")));
    wait_until(&mut h, "dossier déduit de l'URL", |h| has_text_input(h, "sha256"));
    click_label(&mut h, &format!("{} Cloner", dragontree_app::icons::DOWNLOAD_SIMPLE));
    // Le message nomme le dossier du clone : il vient de la vérification faite sur le clone terminé, que git a donc
    // créé là. La fin du clonage n'arrive qu'après son nettoyage : le dossier n'existe plus.
    let dest = clones.join("sha256");
    let from_clone = format!("Le dépôt {} {explicit}", dest.display());
    wait_until(&mut h, "erreur de clonage explicite", |h| h.query_by_label_contains(&from_clone).is_some());
    assert!(!dest.exists(), "dossier du clone refusé supprimé");
    assert!(clones.exists(), "dossier parent gardé");
}
