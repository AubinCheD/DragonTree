//! Affichage › Tri des commits : par date ou topologique, réglage global qui recharge le journal.

mod common;

use std::path::Path;

use common::*;
use dragontree_app::StartOptions;
use dragontree_app::settings::Settings;
use dragontree_git::Git;
use egui_kittest::kittest::Queryable;

/// Commit daté (auteur et committeur) à `ts` secondes depuis l'époque.
fn commit_at(dir: &Path, file: &str, msg: &str, ts: i64) {
    std::fs::write(dir.join(file), format!("{msg}\n")).unwrap();
    git(dir, &["add", "-A"]);
    run_dated(dir, &["commit", "-q", "-m", msg], ts);
}

fn run_dated(dir: &Path, args: &[&str], ts: i64) {
    let date = format!("@{ts} +0000");
    Git::discover()
        .unwrap()
        .cmd(dir)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .args(args)
        .run()
        .unwrap_or_else(|e| panic!("git {args:?} : {e}"));
}

/// Deux branches aux dates croisées, puis fusionnées : par date, leurs commits s'entrelacent ;
/// en topologique, ceux d'une branche restent groupés.
fn crossed_repo(base: &Path) -> std::path::PathBuf {
    let repo = base.join("northwind");
    init_repo(&repo);
    commit_at(&repo, "base.txt", "Base", 1000);
    git(&repo, &["switch", "-q", "-c", "autre"]);
    commit_at(&repo, "a1.txt", "A1", 2000);
    commit_at(&repo, "a2.txt", "A2", 4000);
    git(&repo, &["switch", "-q", "main"]);
    commit_at(&repo, "m1.txt", "M1", 3000);
    commit_at(&repo, "m2.txt", "M2", 5000);
    run_dated(&repo, &["merge", "-q", "--no-ff", "-m", "Fusion", "autre"], 6000);
    repo
}

/// Résumés des commits affichés, de haut en bas (le panneau de détails, plus bas, ne compte pas).
fn rows(h: &egui_kittest::Harness<'static, dragontree_app::DragonTreeApp>) -> Vec<&'static str> {
    let mut found: Vec<(f32, &'static str)> = ["Base", "A1", "A2", "M1", "M2", "Fusion"]
        .into_iter()
        .filter_map(|name| h.query_all_by_label(name).map(|n| n.rect().min.y).reduce(f32::min).map(|y| (y, name)))
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, name)| name).collect()
}

#[test]
fn the_commit_order_setting_changes_the_rows_and_is_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = crossed_repo(tmp.path());
    let mut h = harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(tmp.path()) });

    // Par défaut : par date, les branches s'entrelacent.
    let by_date = ["Fusion", "M2", "A2", "M1", "A1", "Base"];
    wait_until(&mut h, "historique par date", |h| rows(h) == by_date);

    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Tri des commits");
    wait_until(&mut h, "sous-menu Tri des commits", |h| menu_item(h, "Topologique").is_some());
    assert_eq!(menu_item_checked(&h, "Par date"), Some(true));
    assert_eq!(menu_item_checked(&h, "Topologique"), Some(false));
    click_menu_item(&mut h, "Topologique");

    wait_until(&mut h, "ordre topologique", |h| {
        let r = rows(h);
        r.len() == 6 && r != by_date
    });
    let topo = rows(&h);
    let pos = |name: &str| topo.iter().position(|n| *n == name).unwrap();
    assert_eq!(pos("A2").abs_diff(pos("A1")), 1, "les commits d'une branche restent groupés : {topo:?}");
    let saved = Settings::load_from(&tmp.path().join("profil").join("settings.json"));
    assert!(saved.topological_order);

    open_menu(&mut h, "Affichage");
    open_menu(&mut h, "Tri des commits");
    wait_until(&mut h, "sous-menu Tri des commits", |h| menu_item(h, "Par date").is_some());
    assert_eq!(menu_item_checked(&h, "Topologique"), Some(true));
    click_menu_item(&mut h, "Par date");
    wait_until(&mut h, "retour à l'ordre par date", |h| rows(h) == by_date);
}
