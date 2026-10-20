//! Statistiques du dépôt : comptes exacts sur un dépôt de test connu.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, GitError, MonthCount, Repo};

/// Commit daté (auteur et validation), signé par `name <email>`.
fn commit_as(t: &TestRepo, file: &str, content: &str, name: &str, email: &str, date: &str) {
    t.write(file, content);
    t.run(&["add", "-A"]);
    let author = format!("{name} <{email}>");
    let args = ["commit", "-q", "-m", file, "--author", author.as_str(), "--date", date];
    t.git
        .cmd(t.path())
        .args(args)
        .env("GIT_COMMITTER_DATE", date)
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email)
        .run()
        .unwrap();
}

/// main : 3 commits (janvier, février, avril 2024) ; `fonction` : 2 commits de plus (février 2024), dont un d'un alias.
fn fixture() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    commit_as(&t, "a.txt", "1\n", "Alice", "alice@contoso.test", "2024-01-15T12:00:00Z");
    commit_as(&t, "b.txt", "2\n", "Bob", "bob@fabrikam.test", "2024-02-10T12:00:00Z");
    t.run(&["branch", "fonction"]);
    commit_as(&t, "c.txt", "3\n", "Alice", "alice@contoso.test", "2024-04-20T12:00:00Z");
    t.run(&["checkout", "-q", "fonction"]);
    commit_as(&t, "d.txt", "4\n", "Robert", "BOB@fabrikam.test", "2024-02-20T12:00:00Z");
    commit_as(&t, "e.txt", "5\n", "Alice", "alice@contoso.test", "2024-02-25T12:00:00Z");
    t.run(&["checkout", "-q", "main"]);
    t.run(&["tag", "v1"]);
    t.run(&["tag", "-a", "v2", "-m", "deux"]);
    // Un stash et une note ajoutent des commits à refs/stash et refs/notes, que « toutes les branches » ne suit pas.
    t.write(
        "a.txt",
        "modifié
",
    );
    t.run(&["stash", "-q"]);
    t.run(&["notes", "add", "-m", "une note", "HEAD"]);
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

#[test]
fn stats_of_a_known_repository_are_exact() {
    let (_t, repo) = fixture();
    let stats = repo.stats(&CancelToken::new()).unwrap();
    assert_eq!((stats.commits_head, stats.commits_all), (3, 5));
    // « Robert » et « Bob » partagent une adresse (la casse ne compte pas) : un seul contributeur, de deux commits.
    let who: Vec<_> = stats.contributors.iter().map(|c| (c.name.as_str(), c.commits)).collect();
    assert_eq!(who, [("Alice", 3), ("Bob", 2)]);
    assert_eq!(stats.contributors[1].aliases, ["Robert"]);
    let months: Vec<_> = stats.monthly.iter().map(|m| (m.year, m.month, m.commits)).collect();
    assert_eq!(months, [(2024, 1, 1), (2024, 2, 3), (2024, 3, 0), (2024, 4, 1)]);
    assert_eq!(stats.monthly[0], MonthCount { year: 2024, month: 1, commits: 1 });
    assert_eq!((stats.first_commit, stats.last_commit), (Some(1_705_320_000), Some(1_713_614_400)));
    assert_eq!(stats.tracked_files, 3, "a, b et c sont suivis sur main");
    assert_eq!((stats.local_branches, stats.remote_branches, stats.tags), (2, 0, 2));
    assert!(stats.objects.loose_objects + stats.objects.packed_objects >= 15, "{:?}", stats.objects);
    assert!(!stats.objects.loose_size.is_empty());
}

#[test]
fn unmerged_files_are_counted_once() {
    let t = TestRepo::new();
    t.write(
        "f", "base
",
    );
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "other"]);
    t.write(
        "f", "theirs
",
    );
    t.commit_all("theirs");
    t.run(&["checkout", "-q", "main"]);
    t.write(
        "f", "ours
",
    );
    t.commit_all("ours");
    assert!(t.try_run(&["merge", "other"]).is_err(), "conflit attendu");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    assert_eq!(repo.stats(&CancelToken::new()).unwrap().tracked_files, 1, "trois étapes dans l'index, un seul fichier");
}

#[test]
fn a_repository_without_commit_has_empty_stats() {
    let t = TestRepo::new();
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let stats = repo.stats(&CancelToken::new()).unwrap();
    assert_eq!((stats.commits_head, stats.commits_all, stats.tracked_files), (0, 0, 0));
    assert!(stats.contributors.is_empty() && stats.monthly.is_empty());
    assert_eq!((stats.first_commit, stats.last_commit), (None, None));
}

#[test]
fn remote_branches_are_counted_without_the_remote_head() {
    let (t, _) = fixture();
    let bare = TestRepo::new_bare();
    t.run(&["remote", "add", "origin", &bare.url()]);
    t.run(&["push", "-q", "origin", "main", "fonction"]);
    t.run(&["fetch", "-q", "origin"]);
    t.run(&["remote", "set-head", "origin", "main"]);
    let repo = Repo::open(&t.git, t.path()).unwrap();
    assert_eq!(repo.stats(&CancelToken::new()).unwrap().remote_branches, 2);
}

#[test]
fn a_cancelled_calculation_stops_with_the_cancelled_error() {
    let (_t, repo) = fixture();
    let cancel = CancelToken::new();
    cancel.cancel();
    assert_eq!(repo.stats(&cancel), Err(GitError::Cancelled));
}
