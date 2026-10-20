//! Textes lus par l'IA : diff indexé et revue d'une branche (diff depuis le `merge-base`).

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, Repo};

fn fixture() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write("a.txt", "un\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "fonction"]);
    t.write("a.txt", "un\ndeux\n");
    t.commit_all("ajoute deux");
    t.write("b.txt", "trois\n");
    t.commit_all("ajoute b");
    // main avance de son côté : ses changements ne font pas partie de la revue.
    t.run(&["checkout", "-q", "main"]);
    t.write("c.txt", "ailleurs\n");
    t.commit_all("avance main");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

#[test]
fn a_branch_review_reads_the_diff_from_the_merge_base_only() {
    let (t, repo) = fixture();
    let review = repo.branch_review("fonction", "main", &CancelToken::new()).unwrap();
    assert_eq!(review.merge_base, t.run(&["rev-parse", "main~1"]).trim());
    assert_eq!(review.commits, ["ajoute b", "ajoute deux"]);
    assert!(review.diff.contains("+deux") && review.diff.contains("+++ b/b.txt"), "{}", review.diff);
    assert!(!review.diff.contains("ailleurs"), "le travail de main n'est pas dans la revue : {}", review.diff);
}

#[test]
fn a_branch_that_adds_nothing_has_an_empty_review() {
    let (_t, repo) = fixture();
    let review = repo.branch_review("main", "main", &CancelToken::new()).unwrap();
    assert!(review.diff.is_empty() && review.commits.is_empty());
}

#[test]
fn the_staged_diff_text_holds_only_the_index() {
    let t = TestRepo::new();
    t.write("a.txt", "un\n");
    t.commit_all("base");
    t.write("a.txt", "un\nindexé\n");
    t.run(&["add", "a.txt"]);
    t.write("a.txt", "un\nindexé\nnon indexé\n");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let text = repo.staged_diff_text(&CancelToken::new()).unwrap();
    assert!(text.contains("+indexé") && !text.contains("non indexé"), "{text}");
}

#[test]
fn an_unknown_branch_is_an_error() {
    let (_t, repo) = fixture();
    assert!(repo.branch_review("absente", "main", &CancelToken::new()).is_err());
}
