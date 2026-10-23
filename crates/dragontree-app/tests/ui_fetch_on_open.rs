//! Le premier fetch automatique d'un onglet suit l'âge du dernier fetch du dépôt (`FETCH_HEAD`) : un dépôt fetché il y
//! a longtemps l'est dès l'ouverture, et ses branches distantes sont à jour sans attendre l'échéance ; un dépôt fetché
//! à l'instant attend la sienne (pas de rafale de fetchs à chaque lancement).

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use common::*;
use dragontree_app::StartOptions;
use egui_kittest::kittest::Queryable;

const NEW_BRANCH: &str = "nouvelle-sur-le-serveur";

/// Un clone d'un serveur local, puis une branche créée sur le serveur après le clonage ; son dernier fetch date de
/// `fetched_ago`.
fn clone_behind_its_server(base: &Path, fetched_ago: Duration) -> PathBuf {
    let seed = base.join("source");
    init_repo(&seed);
    commit_file(&seed, "a.txt", "1\n", "Premier commit");
    let server = base.join("serveur.git");
    git(base, &["clone", "-q", "--bare", seed.to_str().unwrap(), server.to_str().unwrap()]);
    let repo = base.join("clone");
    git(base, &["clone", "-q", server.to_str().unwrap(), repo.to_str().unwrap()]);
    git(&seed, &["push", "-q", server.to_str().unwrap(), &format!("HEAD:refs/heads/{NEW_BRANCH}")]);
    let fetch_head = repo.join(".git").join("FETCH_HEAD");
    std::fs::write(&fetch_head, "").unwrap();
    let file = std::fs::File::options().write(true).open(&fetch_head).unwrap();
    file.set_modified(SystemTime::now() - fetched_ago).unwrap();
    repo
}

fn open(repo: PathBuf, base: &Path) -> egui_kittest::Harness<'static, dragontree_app::DragonTreeApp> {
    // Fetch automatique par défaut : toutes les 10 minutes.
    harness(StartOptions { open: vec![repo], view: Some("history".into()), ..start(base) })
}

#[test]
fn a_stale_repository_is_fetched_when_opened() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = clone_behind_its_server(tmp.path(), Duration::from_secs(3600));
    let mut h = open(repo, tmp.path());
    wait_until(&mut h, "branche du serveur dans Remotes", |h| h.query_by_label(NEW_BRANCH).is_some());
}

#[test]
fn a_freshly_fetched_repository_waits_for_its_interval() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = clone_behind_its_server(tmp.path(), Duration::ZERO);
    let mut h = open(repo, tmp.path());
    wait_until(&mut h, "branches distantes lues", |h| h.query_by_label("origin").is_some());
    for _ in 0..60 {
        h.step();
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(h.query_by_label(NEW_BRANCH).is_none(), "fetché à l'instant : pas de nouveau fetch à l'ouverture");
}
