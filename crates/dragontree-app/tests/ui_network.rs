//! Push, fetch et pull depuis la barre d'outils, avec un remote local (bare).

mod common;

use std::path::Path;

use common::{git, harness, start, try_git, wait_until};
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

#[test]
fn amend_with_push_after_never_force_pushes() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let work = tmp.path().join("travail");
    git(tmp.path(), &["clone", "-q", &url, work.to_str().unwrap()]);
    configure(&work);
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "déjà poussé"]);
    git(&work, &["push", "-q", "-u", "origin", "main"]);
    let pushed = git(&bare, &["rev-parse", "main"]).trim().to_string();
    std::fs::write(work.join("b.txt"), "b\n").unwrap();

    let mut h = harness(StartOptions { open: vec![work.clone()], view: Some("changes".into()), ..start(tmp.path()) });

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
        try_git(&work, &["ls-tree", "--name-only", "HEAD"]).is_some_and(|s| s.contains("b.txt"))
    });
    wait_until(&mut h, "push écarté (notification) ou effectué", |h| {
        h.query_all_by_label_contains("pas été poussé").count() > 0
            || try_git(&bare, &["rev-parse", "main"]).is_some_and(|s| s.trim() != pushed)
    });
    assert_eq!(git(&bare, &["rev-parse", "main"]).trim(), pushed, "jamais de push forcé implicite");
}
