//! Push, fetch et pull depuis la barre d'outils, avec un remote local (bare).

mod common;

use std::path::{Path, PathBuf};

use common::{click_label, git, harness, start, try_git, wait_until};
use dragontree_app::{DragonTreeApp, StartOptions};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

fn configure(dir: &Path) {
    for (k, v) in [
        ("user.name", "Testeur"),
        ("user.email", "t@example.com"),
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
    ] {
        git(dir, &["config", k, v]);
    }
}

fn enabled(h: &Harness<'static, DragonTreeApp>, label: &str) -> bool {
    h.query_by_label(label).is_some_and(|n| !n.accesskit_node().is_disabled())
}

#[test]
fn push_fetch_pull_from_toolbar() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));

    // Dépôt de travail ouvert dans DragonTree, et un « collègue » qui pousse de son côté.
    let work = tmp.path().join("travail");
    let other = tmp.path().join("collegue");
    git(tmp.path(), &["clone", "-q", &url, work.to_str().unwrap()]);
    configure(&work);
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "premier"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "local à pousser"]);

    let mut h = harness(StartOptions { open: vec![work.clone()], view: Some("history".into()), ..start(tmp.path()) });

    // Push : bouton de la barre d'outils puis bouton du dialogue (même libellé).
    wait_until(&mut h, "Push actif", |h| enabled(h, "Push") && h.query_by_label("local à pousser").is_some());
    h.get_by_label("Push").click();
    wait_until(&mut h, "dialogue de push", |h| h.query_all_by_label("Push").count() >= 2);
    h.query_all_by_label("Push").last().unwrap().click();
    wait_until(&mut h, "commit poussé", |_| {
        try_git(&bare, &["log", "-1", "--format=%s", "main"]).is_some_and(|s| s.trim() == "local à pousser")
    });

    // Le collègue pousse un commit ; Fetch puis Pull le récupèrent.
    git(tmp.path(), &["clone", "-q", &url, other.to_str().unwrap()]);
    configure(&other);
    std::fs::write(other.join("c.txt"), "c\n").unwrap();
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "commit du collègue"]);
    git(&other, &["push", "-q", "origin", "main"]);

    h.get_by_label("Fetch").click();
    wait_until(&mut h, "fetch terminé", |_| {
        try_git(&work, &["rev-parse", "origin/main"])
            .is_some_and(|s| s.trim() == git(&other, &["rev-parse", "HEAD"]).trim())
    });
    wait_until(&mut h, "Pull actif", |h| enabled(h, "Pull"));
    h.get_by_label("Pull").click();
    wait_until(&mut h, "dialogue de pull", |h| h.query_all_by_label("Pull").count() >= 2);
    h.query_all_by_label("Pull").last().unwrap().click();
    wait_until(&mut h, "pull terminé", |_| work.join("c.txt").exists());
    assert_eq!(git(&work, &["log", "-1", "--format=%s"]).trim(), "commit du collègue");
}

/// Remote nu et dépôt `travail` cloné, dont le commit « déjà poussé » est sur le remote.
struct Pushed {
    bare: PathBuf,
    work: PathBuf,
    url: String,
    /// Le commit poussé, que l'amend remplace.
    pushed: String,
}

/// Dépôt de travail dont le commit « déjà poussé » est sur le remote nu, et `b.txt` à ajouter au commit.
fn pushed_repo(base: &Path) -> Pushed {
    let bare = base.join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let work = base.join("travail");
    git(base, &["clone", "-q", &url, work.to_str().unwrap()]);
    configure(&work);
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "déjà poussé"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    let pushed = git(&bare, &["rev-parse", "main"]).trim().to_string();
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    Pushed { bare, work, url, pushed }
}

/// Un collègue clone le remote, y pousse un commit ; renvoie son identifiant.
fn colleague_pushes(base: &Path, url: &str) -> String {
    let other = base.join("collegue");
    git(base, &["clone", "-q", url, other.to_str().unwrap()]);
    configure(&other);
    std::fs::write(other.join("c.txt"), "c\n").unwrap();
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "commit du collègue"]);
    git(&other, &["push", "-q", "origin", "main"]);
    git(&other, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Amend du dernier commit avec « Pousser après le commit » : `b.txt` est ajouté au commit depuis la vue des
/// changements ; attend le commit modifié en local.
fn amend_with_push_after(base: &Path, p: &Pushed) -> Harness<'static, DragonTreeApp> {
    let mut h = harness(StartOptions { open: vec![p.work.clone()], view: Some("changes".into()), ..start(base) });
    wait_until(&mut h, "bouton « Tout indexer »", |h| h.query_by_label_contains("Tout indexer").is_some());
    h.get_by_label_contains("Tout indexer").click();
    wait_until(&mut h, "fichier indexé", |h| h.query_by_label_contains("Commit (1 fichier)").is_some());
    h.get_by_label("Amend").click();
    h.get_by_label("Pousser après le commit").click();
    wait_until(&mut h, "message du dernier commit repris", |h| {
        h.query_by(|n| n.role() == egui::accesskit::Role::TextInput && n.value().as_deref() == Some("déjà poussé"))
            .is_some()
    });
    let amend = format!("{}  Modifier le commit", dragontree_app::icons::CHECK);
    wait_until(&mut h, "bouton « Modifier le commit »", |h| enabled(h, &amend));
    h.get_by_label(&amend).click();
    wait_until(&mut h, "commit modifié localement", |_| {
        try_git(&p.work, &["ls-tree", "--name-only", "HEAD"]).is_some_and(|s| s.contains("b.txt"))
    });
    h
}

/// Amend du commit déjà poussé avec « Pousser après le commit » (L5) : attend le dialogue qui propose le push forcé,
/// dont le bail porte sur le commit remplacé ; rien n'est encore poussé.
fn amend_the_pushed_commit(base: &Path) -> (Harness<'static, DragonTreeApp>, Pushed) {
    let p = pushed_repo(base);
    let mut h = amend_with_push_after(base, &p);
    wait_until(&mut h, "dialogue du push forcé", |h| {
        h.query_by_label("Pousser le commit modifié ?").is_some() && h.query_by_label("Pousser en forçant").is_some()
    });
    let short = &p.pushed[..7];
    assert!(h.query_by_label_contains(&format!("n'est plus sur {short}")).is_some(), "bail sur le commit remplacé");
    assert_eq!(git(&p.bare, &["rev-parse", "main"]).trim(), p.pushed, "jamais de push forcé sans confirmation");
    (h, p)
}

#[test]
fn amending_a_pushed_commit_offers_a_force_push_that_replaces_it() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, p) = amend_the_pushed_commit(tmp.path());

    click_label(&mut h, "Pousser en forçant");
    wait_until(&mut h, "commit modifié poussé", |_| {
        let head = git(&p.work, &["rev-parse", "HEAD"]);
        try_git(&p.bare, &["rev-parse", "main"]).is_some_and(|s| s.trim() == head.trim())
    });
    assert_ne!(git(&p.bare, &["rev-parse", "main"]).trim(), p.pushed);
    assert!(git(&p.bare, &["ls-tree", "--name-only", "main"]).contains("b.txt"));
    wait_until(&mut h, "notification du push forcé", |h| h.query_all_by_label_contains("(push forcé)").count() > 0);
}

/// Revue finale (B1) : le push forcé confirmé pousse le commit modifié que le dialogue nomme, même si la branche
/// locale a bougé pendant que le dialogue était ouvert (ici un commit de plus, fait en ligne de commande) ; le commit
/// suivant reste local.
#[test]
fn the_force_push_pushes_the_amended_commit_even_if_the_branch_moved_meanwhile() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, p) = amend_the_pushed_commit(tmp.path());
    let amended = git(&p.work, &["rev-parse", "HEAD"]).trim().to_string();
    assert!(h.query_by_label_contains(&format!("commit modifié ({})", &amended[..7])).is_some(), "commit nommé");

    std::fs::write(p.work.join("c.txt"), "c\n").unwrap();
    git(&p.work, &["add", "c.txt"]);
    git(&p.work, &["commit", "-q", "-m", "après l'amend"]);
    click_label(&mut h, "Pousser en forçant");
    wait_until(&mut h, "notification du push forcé", |h| h.query_all_by_label_contains("(push forcé)").count() > 0);
    assert_eq!(git(&p.bare, &["rev-parse", "main"]).trim(), amended, "le commit modifié, pas la branche qui a bougé");
    assert_ne!(git(&p.work, &["rev-parse", "HEAD"]).trim(), amended, "le commit suivant reste local");
}

#[test]
fn cancelling_the_force_push_keeps_the_amended_commit_local() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, p) = amend_the_pushed_commit(tmp.path());

    click_label(&mut h, "Annuler");
    wait_until(&mut h, "notification : commit non poussé", |h| {
        h.query_all_by_label_contains("pas été poussé").count() > 0
    });
    assert!(h.query_by_label("Pousser en forçant").is_none(), "dialogue fermé");
    h.run_steps(10);
    assert_eq!(git(&p.bare, &["rev-parse", "main"]).trim(), p.pushed, "rien n'est poussé");
    assert!(git(&p.work, &["ls-tree", "--name-only", "HEAD"]).contains("b.txt"), "commit modifié gardé en local");
}

#[test]
fn the_force_push_is_refused_when_someone_pushed_meanwhile() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut h, p) = amend_the_pushed_commit(tmp.path());

    // Dialogue ouvert : un collègue pousse, puis un fetch avance origin/main dans le dépôt de travail.
    let theirs = colleague_pushes(tmp.path(), &p.url);
    git(&p.work, &["fetch", "-q", "origin"]);

    click_label(&mut h, "Pousser en forçant");
    wait_until(&mut h, "push forcé refusé et signalé", |h| {
        h.query_all_by_label_contains("Push forcé refusé").count() > 0
    });
    assert_eq!(git(&p.bare, &["rev-parse", "main"]).trim(), theirs, "le commit du collègue n'est pas écrasé");
    assert!(git(&p.work, &["ls-tree", "--name-only", "HEAD"]).contains("b.txt"), "commit modifié gardé en local");
}

/// Branche en retard sur le remote (le commit d'un collègue est arrivé par un fetch) : le commit modifié n'était pas le
/// sommet de la branche distante ; un push forcé écraserait le commit du collègue. Il n'est pas proposé, la
/// notification dit que rien n'est poussé et conseille d'intégrer d'abord les commits distants (revue finale, B3).
#[test]
fn no_force_push_is_offered_when_the_branch_is_behind_its_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let p = pushed_repo(tmp.path());
    let theirs = colleague_pushes(tmp.path(), &p.url);
    git(&p.work, &["fetch", "-q", "origin"]);

    let mut h = amend_with_push_after(tmp.path(), &p);
    wait_until(&mut h, "notification : commit non poussé", |h| {
        h.query_all_by_label_contains("pas été poussé").count() > 0
    });
    assert!(h.query_all_by_label_contains("Pull avec rebase").count() > 0, "intégrer d'abord les commits distants");
    assert_eq!(h.query_all_by_label_contains("Push › Forcer").count(), 0, "pas de push forcé conseillé");
    assert!(h.query_by_label("Pousser en forçant").is_none(), "aucun push forcé proposé");
    h.run_steps(10);
    assert_eq!(git(&p.bare, &["rev-parse", "main"]).trim(), theirs, "rien n'est poussé");
}
