//! Comparaison : fichiers et diff entre deux révisions, et entre une révision et la copie de travail.

mod common;

use common::TestRepo;
use dragontree_git::{ChangeKind, DiffOptions, Repo};

/// Trois révisions : `base`, une branche `fonction` (modifie a.txt, ajoute c.txt) et main (supprime b.txt).
fn fixture() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write("a.txt", "un\n");
    t.write("b.txt", "deux\n");
    t.commit_all("base");
    t.run(&["branch", "fonction"]);
    t.run(&["rm", "-q", "b.txt"]);
    t.commit_all("retire b");
    t.run(&["checkout", "-q", "fonction"]);
    t.write("a.txt", "un\ntrois\n");
    t.write("c.txt", "neuf\n");
    t.commit_all("modifie a, ajoute c");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

#[test]
fn files_between_two_revisions_list_each_change_with_its_kind() {
    let (_t, repo) = fixture();
    let files = repo.compare_files("main", Some("fonction")).unwrap();
    let kinds: Vec<(&str, ChangeKind)> = files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
    assert_eq!(
        kinds,
        vec![("a.txt", ChangeKind::Modified), ("b.txt", ChangeKind::Added), ("c.txt", ChangeKind::Added)]
    );
    // Dans l'autre sens, les ajouts deviennent des suppressions.
    let reverse = repo.compare_files("fonction", Some("main")).unwrap();
    assert!(reverse.iter().any(|f| f.path == "c.txt" && f.kind == ChangeKind::Deleted));
}

#[test]
fn diff_between_two_revisions_shows_the_added_line() {
    let (_t, repo) = fixture();
    let files = repo.compare_files("HEAD~1", Some("HEAD")).unwrap();
    let a = files.iter().find(|f| f.path == "a.txt").unwrap();
    let diff = repo.compare_file_diff("HEAD~1", Some("HEAD"), a, &DiffOptions::default()).unwrap();
    assert_eq!(diff.added_count(), 1);
    assert_eq!(diff.hunks[0].lines.iter().find(|l| l.new_no == Some(2)).unwrap().text, "trois");
}

#[test]
fn files_and_diff_between_a_revision_and_the_working_copy() {
    let (t, repo) = fixture();
    t.write("a.txt", "un\ntrois\nquatre\n");
    let files = repo.compare_files("HEAD", None).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a.txt");
    let diff = repo.compare_file_diff("HEAD", None, &files[0], &DiffOptions::default()).unwrap();
    assert_eq!(diff.added_count(), 1);
    assert!(diff.hunks[0].lines.iter().any(|l| l.text == "quatre"));
    // Contre la révision d'avant, les deux ajouts apparaissent.
    let older = repo.compare_files("HEAD~1", None).unwrap();
    assert!(older.iter().any(|f| f.path == "a.txt"));
}
