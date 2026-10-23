//! Les remotes de la barre latérale : tout déplier ou replier d'un coup (un dépôt Azure DevOps range ses centaines de
//! branches dans des dizaines de dossiers), et l'âge du dernier fetch dans l'infobulle.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

const BRANCHES: [&str; 3] = ["feature/ada/premiere", "feature/bob/seconde", "fix/troisieme"];

/// Un dépôt avec un remote `origin` et des branches distantes rangées dans des dossiers.
fn repo_with_remote_folders(base: &Path) -> PathBuf {
    let repo = demo_repo(base);
    git(&repo, &["remote", "add", "origin", "https://example.invalid/depot.git"]);
    for b in BRANCHES {
        git(&repo, &["update-ref", &format!("refs/remotes/origin/{b}"), "HEAD"]);
    }
    repo
}

fn open(repo: PathBuf, base: &Path) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    // Sans fetch automatique : le remote n'existe pas.
    let mut h = harness(StartOptions {
        open: vec![repo],
        view: Some("history".into()),
        auto_fetch_every: Some(std::time::Duration::from_secs(3600)),
        ..start(base)
    });
    wait_until(&mut h, "remote origin", |h| h.query_by_label("origin").is_some());
    h
}

#[test]
fn unfolding_all_shows_every_branch_of_the_remote() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_remote_folders(tmp.path());
    // Fetché à l'instant : pas de fetch à l'ouverture (le remote n'existe pas).
    std::fs::write(repo.join(".git").join("FETCH_HEAD"), "").unwrap();
    let mut h = open(repo, tmp.path());
    assert!(h.query_by_label(BRANCHES[0]).is_none(), "dossiers repliés au départ");
    right_click_label(&mut h, "origin");
    click_label_containing(&mut h, "Tout déplier");
    wait_until(&mut h, "toutes les branches", |h| BRANCHES.iter().all(|b| h.query_by_label(b).is_some()));
    right_click_label(&mut h, "origin");
    click_label_containing(&mut h, "Tout replier");
    wait_until(&mut h, "repliées", |h| BRANCHES.iter().all(|b| h.query_by_label(b).is_none()));
}

#[test]
fn remote_tooltip_tells_the_last_fetch() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = repo_with_remote_folders(tmp.path());
    std::fs::write(repo.join(".git").join("FETCH_HEAD"), "").unwrap();
    let mut h = open(repo.clone(), tmp.path());
    h.get_by_label("origin").hover();
    wait_until(&mut h, "infobulle du remote", |h| h.query_by_label_contains("Dernier fetch").is_some());

    // Jamais fetché : l'infobulle le dit.
    let tmp = tempfile::tempdir().unwrap();
    let mut h = open(repo_with_remote_folders(tmp.path()), tmp.path());
    h.get_by_label("origin").hover();
    wait_until(&mut h, "jamais fetché", |h| h.query_by_label_contains("Jamais fetché").is_some());
}
