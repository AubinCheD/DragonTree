//! Worktrees : liste, création (branche locale, branche distante suivie, commit), suppression, nettoyage, et
//! refus de sortir une branche déjà sortie dans un autre worktree.

mod common;

use std::path::{Path, PathBuf};

use common::TestRepo;
use dragontree_git::{GitError, Repo, WorktreeStart};

/// Dépôt avec deux commits sur main et une branche `fonction`.
fn repo_with_branch() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write("a.txt", "1\n");
    t.commit_all("premier");
    t.run(&["branch", "fonction"]);
    t.write("a.txt", "2\n");
    t.commit_all("second");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

/// Même dossier, quelle que soit l'écriture (séparateurs, casse, chemin court de Windows).
fn same(a: &Path, b: &Path) -> bool {
    let key = |p: &Path| {
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
    };
    key(a) == key(b)
}

fn sibling(dir: &tempfile::TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

#[test]
fn the_list_starts_with_the_main_worktree_marked_current() {
    let (t, repo) = repo_with_branch();
    let list = repo.worktrees().unwrap();
    assert_eq!(list.len(), 1);
    let main = &list[0];
    assert!(main.is_main && main.is_current);
    assert!(same(&main.path, t.path()), "{:?}", main.path);
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert!(!main.detached && !main.locked && !main.prunable);
    assert_eq!(main.head.map(|o| o.to_hex()), Some(t.run(&["rev-parse", "HEAD"]).trim().to_string()));
}

#[test]
fn add_from_a_local_branch_lists_it_and_checks_the_branch_out() {
    let (_t, repo) = repo_with_branch();
    let dir = tempfile::tempdir().unwrap();
    let path = sibling(&dir, "depot-fonction");
    repo.add_worktree(&path, &WorktreeStart::Branch("fonction".into())).unwrap();

    assert_eq!(std::fs::read_to_string(path.join("a.txt")).unwrap(), "1\n");
    let list = repo.worktrees().unwrap();
    assert_eq!(list.len(), 2);
    let wt = list.iter().find(|w| same(&w.path, &path)).expect("worktree listé");
    assert_eq!(wt.branch.as_deref(), Some("fonction"));
    assert!(!wt.is_main && !wt.is_current && !wt.detached);

    // Vu depuis le worktree lui-même : c'est lui le courant.
    let other = Repo::open(&repo.git, &path).unwrap();
    let seen = other.worktrees().unwrap();
    assert!(seen.iter().find(|w| same(&w.path, &path)).unwrap().is_current);
    assert!(!seen[0].is_current && seen[0].is_main);
}

#[test]
fn add_from_a_commit_gives_a_detached_head() {
    let (t, repo) = repo_with_branch();
    let first = t.run(&["rev-parse", "HEAD~1"]).trim().to_string();
    let dir = tempfile::tempdir().unwrap();
    let path = sibling(&dir, "depot-detache");
    repo.add_worktree(&path, &WorktreeStart::Detached(first.clone())).unwrap();

    let list = repo.worktrees().unwrap();
    let wt = list.iter().find(|w| same(&w.path, &path)).unwrap();
    assert!(wt.detached);
    assert_eq!(wt.branch, None);
    assert_eq!(wt.head.map(|o| o.to_hex()), Some(first));
}

#[test]
fn add_from_a_remote_branch_creates_a_tracking_branch() {
    let bare = TestRepo::new_bare();
    let seed = TestRepo::new();
    seed.write("a.txt", "1\n");
    seed.commit_all("init");
    seed.run(&["remote", "add", "origin", &bare.url()]);
    seed.run(&["push", "-q", "origin", "main"]);
    seed.run(&["checkout", "-q", "-b", "distante"]);
    seed.write("d.txt", "d\n");
    seed.commit_all("sur distante");
    seed.run(&["push", "-q", "origin", "distante"]);

    let work = tempfile::tempdir().unwrap();
    let clone = work.path().join("clone");
    seed.git.cmd(work.path()).args(["clone", "-q", &bare.url(), clone.to_str().unwrap()]).run().unwrap();
    let repo = Repo::open(&seed.git, &clone).unwrap();
    let path = work.path().join("clone-distante");
    let start = WorktreeStart::Track { remote_branch: "origin/distante".into(), local: "distante".into() };
    repo.add_worktree(&path, &start).unwrap();

    assert!(path.join("d.txt").exists());
    let refs = repo.refs().unwrap();
    let local = refs.locals.iter().find(|b| b.name == "distante").expect("branche locale créée");
    assert_eq!(local.upstream.as_deref(), Some("origin/distante"));
    let list = repo.worktrees().unwrap();
    assert_eq!(list.iter().find(|w| same(&w.path, &path)).unwrap().branch.as_deref(), Some("distante"));
}

#[test]
fn a_branch_checked_out_elsewhere_is_refused() {
    let (t, repo) = repo_with_branch();
    let dir = tempfile::tempdir().unwrap();
    let path = sibling(&dir, "depot-fonction");
    repo.add_worktree(&path, &WorktreeStart::Branch("fonction".into())).unwrap();

    // Un second worktree sur la même branche : git refuse.
    let again = repo.add_worktree(&sibling(&dir, "encore"), &WorktreeStart::Branch("fonction".into()));
    assert!(again.is_err());
    assert!(!sibling(&dir, "encore").join("a.txt").exists());

    // Checkout dans le worktree principal : refusé, avec le worktree qui la détient.
    match repo.checkout("fonction") {
        Err(GitError::CheckedOutElsewhere { branch, path: at }) => {
            assert_eq!(branch, "fonction");
            assert!(same(&at, &path), "{at:?}");
        }
        other => panic!("attendu CheckedOutElsewhere, obtenu {other:?}"),
    }
    assert_eq!(t.run(&["branch", "--show-current"]).trim(), "main");

    // Un échec pour une autre raison reste une erreur git ordinaire.
    assert!(matches!(repo.checkout("inconnue"), Err(GitError::Failed { .. })));
}

#[test]
fn remove_needs_force_for_a_dirty_worktree() {
    let (_t, repo) = repo_with_branch();
    let dir = tempfile::tempdir().unwrap();
    let path = sibling(&dir, "depot-fonction");
    repo.add_worktree(&path, &WorktreeStart::Branch("fonction".into())).unwrap();
    std::fs::write(path.join("a.txt"), "modifié\n").unwrap();

    assert!(repo.remove_worktree(&path, false).is_err(), "modifications locales : refus sans --force");
    assert!(path.join("a.txt").exists());
    repo.remove_worktree(&path, true).unwrap();
    assert!(!path.exists());
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}

#[test]
fn remove_a_clean_worktree_and_prune_a_vanished_one() {
    let (_t, repo) = repo_with_branch();
    let dir = tempfile::tempdir().unwrap();
    let clean = sibling(&dir, "propre");
    repo.add_worktree(&clean, &WorktreeStart::Branch("fonction".into())).unwrap();
    repo.remove_worktree(&clean, false).unwrap();
    assert!(!clean.exists());

    // Dossier supprimé à la main : le worktree reste listé, « à nettoyer », jusqu'au prune.
    let gone = sibling(&dir, "disparu");
    repo.add_worktree(&gone, &WorktreeStart::Detached("HEAD".into())).unwrap();
    std::fs::remove_dir_all(&gone).unwrap();
    let list = repo.worktrees().unwrap();
    assert!(list.iter().find(|w| same(&w.path, &gone) || w.path.ends_with("disparu")).unwrap().prunable);
    repo.prune_worktrees().unwrap();
    assert_eq!(repo.worktrees().unwrap().len(), 1);
}
