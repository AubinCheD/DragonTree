//! Ignorer un motif : aperçu des fichiers concernés (non suivis qui seraient ignorés, suivis qui correspondent) et
//! ajout dans le `.gitignore` racine, celui d'un dossier ou `.git/info/exclude`.

mod common;

use common::TestRepo;
use dragontree_git::{IgnoreTarget, Repo};

/// Dépôt avec `keep.txt` et `suivi.log` suivis ; `a.log`, `sub/b.log` et `sub/c.txt` non suivis.
fn repo() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write("keep.txt", "1\n");
    t.write("suivi.log", "1\n");
    t.commit_all("premier");
    t.write("a.log", "x\n");
    t.write("sub/b.log", "x\n");
    t.write("sub/c.txt", "x\n");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

#[test]
fn the_preview_lists_untracked_files_the_pattern_would_ignore() {
    let (_t, repo) = repo();
    let p = repo.ignore_preview("*.log", &IgnoreTarget::Root).unwrap();
    assert_eq!(p.untracked, ["a.log", "sub/b.log"]);
    assert_eq!(p.untracked_total, 2);
}

#[test]
fn tracked_files_that_match_are_reported_separately() {
    let (_t, repo) = repo();
    let p = repo.ignore_preview("*.log", &IgnoreTarget::Root).unwrap();
    assert_eq!(p.tracked, ["suivi.log"]);
    assert!(!p.untracked.contains(&"suivi.log".to_string()));
}

#[test]
fn files_already_ignored_are_not_listed_again() {
    let (t, repo) = repo();
    t.write(".gitignore", "a.log\n");
    let p = repo.ignore_preview("*.log", &IgnoreTarget::Root).unwrap();
    assert_eq!(p.untracked, ["sub/b.log"]);
}

#[test]
fn a_folder_pattern_matches_the_files_inside() {
    let (_t, repo) = repo();
    let p = repo.ignore_preview("sub/", &IgnoreTarget::Root).unwrap();
    assert_eq!(p.untracked, ["sub/b.log", "sub/c.txt"]);
}

#[test]
fn a_folder_target_only_reaches_that_folder_and_anchors_there() {
    let (_t, repo) = repo();
    let dir = IgnoreTarget::Dir("sub".into());
    assert_eq!(repo.ignore_preview("*.log", &dir).unwrap().untracked, ["sub/b.log"]);
    // `/c.txt` est ancré au dossier du `.gitignore` : sub/c.txt, pas un c.txt à la racine.
    assert_eq!(repo.ignore_preview("/c.txt", &dir).unwrap().untracked, ["sub/c.txt"]);
}

#[test]
fn negations_comments_and_empty_patterns_have_an_empty_preview() {
    let (_t, repo) = repo();
    for pattern in ["!a.log", "# commentaire", "   "] {
        let p = repo.ignore_preview(pattern, &IgnoreTarget::Root).unwrap();
        assert!(p.untracked.is_empty() && p.tracked.is_empty(), "{pattern}");
    }
}

#[test]
fn the_lists_are_capped_but_the_totals_are_exact() {
    let t = TestRepo::new();
    t.write("keep.txt", "1\n");
    t.commit_all("premier");
    for i in 0..250 {
        t.write(&format!("cache/f{i:03}.tmp"), "x\n");
    }
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let p = repo.ignore_preview("*.tmp", &IgnoreTarget::Root).unwrap();
    assert_eq!(p.untracked_total, 250);
    assert_eq!(p.untracked.len(), 200);
}

#[test]
fn adding_to_the_root_gitignore_keeps_existing_bytes() {
    let (t, repo) = repo();
    std::fs::write(t.file(".gitignore"), b"# g\xe9n\xe9r\xe9\nbin/").unwrap();
    repo.add_ignore_pattern("*.log", &IgnoreTarget::Root).unwrap();
    assert_eq!(std::fs::read(t.file(".gitignore")).unwrap(), b"# g\xe9n\xe9r\xe9\nbin/\n*.log\n");
}

#[test]
fn adding_to_a_folder_gitignore_creates_it_there() {
    let (t, repo) = repo();
    repo.add_ignore_pattern("*.log", &IgnoreTarget::Dir("sub".into())).unwrap();
    assert_eq!(t.read("sub/.gitignore"), "*.log\n");
    assert!(!t.file(".gitignore").exists());
    // Git applique bien ce `.gitignore` : seul sub/b.log est ignoré.
    let ignored = t.run(&["ls-files", "--others", "--ignored", "--exclude-standard"]);
    assert_eq!(ignored.trim(), "sub/b.log");
}

#[test]
fn adding_to_info_exclude_creates_the_file_and_leaves_the_worktree_alone() {
    let (t, repo) = repo();
    let _ = std::fs::remove_file(t.path().join(".git/info/exclude"));
    let _ = std::fs::remove_dir(t.path().join(".git/info"));
    repo.add_ignore_pattern("*.log", &IgnoreTarget::InfoExclude).unwrap();
    assert_eq!(std::fs::read_to_string(t.path().join(".git/info/exclude")).unwrap(), "*.log\n");
    assert!(!t.file(".gitignore").exists());
    let ignored = t.run(&["ls-files", "--others", "--ignored", "--exclude-standard"]);
    assert_eq!(ignored.lines().collect::<Vec<_>>(), ["a.log", "sub/b.log"]);
}

#[test]
fn a_pattern_on_several_lines_or_an_escaping_folder_is_refused() {
    let (t, repo) = repo();
    assert!(repo.add_ignore_pattern("a.log\nb.log", &IgnoreTarget::Root).is_err());
    assert!(repo.add_ignore_pattern("", &IgnoreTarget::Root).is_err());
    assert!(repo.add_ignore_pattern("x", &IgnoreTarget::Dir("../dehors".into())).is_err());
    assert!(repo.add_ignore_pattern("x", &IgnoreTarget::Dir("n-existe-pas".into())).is_err());
    assert!(!t.file(".gitignore").exists());
}

#[test]
fn a_folder_preview_equals_what_the_real_folder_gitignore_ignores() {
    let t = TestRepo::new();
    t.write("keep.txt", "1\n");
    t.commit_all("premier");
    for f in ["sub/b.log", "sub/c.txt", "sub/sx.txt", "sub/deep/sy.txt", "sa.txt"] {
        t.write(f, "x\n");
    }
    let repo = Repo::open(&t.git, t.path()).unwrap();
    // `s*` correspond aussi au dossier `sub` lui-même : le `.gitignore` de `sub` ne s'applique qu'à l'intérieur.
    let preview = repo.ignore_preview("s*", &IgnoreTarget::Dir("sub".into())).unwrap();
    std::fs::write(t.file("sub/.gitignore"), "s*\n").unwrap();
    let real = t.run(&["ls-files", "--others", "--ignored", "--exclude-standard"]);
    let real: Vec<&str> = real.lines().collect();
    assert_eq!(preview.untracked, real);
    assert_eq!(real, ["sub/deep/sy.txt", "sub/sx.txt"]);
}

#[test]
fn an_anchored_folder_pattern_with_a_leading_slash_starts_at_the_folder() {
    let (_t, repo) = repo();
    let dir = IgnoreTarget::Dir("sub".into());
    assert_eq!(repo.ignore_preview("/*.txt", &dir).unwrap().untracked, ["sub/c.txt"]);
}
