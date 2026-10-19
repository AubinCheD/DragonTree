//! Statut avec et sans fichiers ignorés, et indexation forcée d'un fichier ignoré.

mod common;

use common::TestRepo;
use dragontree_git::{FileState, Repo};

fn repo_with_ignored() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    let repo = Repo::open(&t.git, t.path()).unwrap();
    t.write(".gitignore", "target/\n*.log\n");
    t.commit_all("Base");
    t.write("target/debug/a.bin", "x");
    t.write("trace.log", "l");
    t.write("nouveau.txt", "n");
    (t, repo)
}

#[test]
fn status_lists_ignored_files_only_on_request() {
    let (_t, repo) = repo_with_ignored();

    let plain = repo.status().unwrap();
    assert!(plain.ignored().is_empty());
    assert!(plain.entries.iter().all(|e| e.worktree != FileState::Ignored));

    let with = repo.status_with_ignored(true).unwrap();
    let mut paths: Vec<&str> = with.ignored().iter().map(|e| e.path.as_str()).collect();
    paths.sort();
    // Un dossier ignoré est une seule entrée.
    assert_eq!(paths, ["target/", "trace.log"]);
    // Les ignorés n'ajoutent rien aux fichiers à indexer ni à l'état « propre ».
    assert_eq!(with.unstaged().len(), 1);
    assert!(with.staged().is_empty());
    assert!(!with.is_clean());
}

#[test]
fn ignored_files_alone_leave_the_repository_clean() {
    let t = TestRepo::new();
    let repo = Repo::open(&t.git, t.path()).unwrap();
    t.write(".gitignore", "*.log\n");
    t.commit_all("Base");
    t.write("trace.log", "l");
    let s = repo.status_with_ignored(true).unwrap();
    assert_eq!(s.ignored().len(), 1);
    assert!(s.is_clean());
}

#[test]
fn stage_force_adds_an_ignored_file_but_stage_does_not() {
    let (_t, repo) = repo_with_ignored();
    let paths = vec!["trace.log".to_string()];
    assert!(repo.stage(&paths).is_err());
    assert!(repo.status().unwrap().staged().is_empty());

    repo.stage_force(&paths).unwrap();
    let s = repo.status_with_ignored(true).unwrap();
    assert_eq!(s.staged().len(), 1);
    assert_eq!(s.staged()[0].path, "trace.log");
}
