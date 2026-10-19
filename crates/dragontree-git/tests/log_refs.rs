//! Sélection des références parcourues par `git log` (`RefSelection`) sur de vrais dépôts.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, LogOptions, LogScope, RefSelection, Repo};

fn summaries(t: &TestRepo, selection: RefSelection) -> Vec<String> {
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let opts = LogOptions { scope: LogScope::Refs(selection), ..LogOptions::default() };
    let mut found: Vec<String> =
        repo.log(&opts, &CancelToken::new()).unwrap().iter().map(|c| c.summary.to_string()).collect();
    found.sort();
    found
}

/// Dépôt sur `main` (commits « base » puis « tip ») avec, hors de HEAD : un commit atteignable seulement par un
/// tag, un par `feature/a`, un par `feature/b`, un par `solo` et un par la branche distante `origin/dev`.
fn sample() -> TestRepo {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    for (branch, summary) in [("feature/a", "propre a"), ("feature/b", "propre b"), ("solo", "propre solo")] {
        t.run(&["checkout", "-q", "-b", branch, "main"]);
        t.write("g", summary);
        t.commit_all(summary);
    }
    t.run(&["checkout", "-q", "-b", "pour-tag", "main"]);
    t.write("g", "propre tag");
    t.commit_all("propre tag");
    t.run(&["tag", "v1"]);
    t.run(&["checkout", "-q", "-b", "pour-remote", "main"]);
    t.write("g", "propre remote");
    let remote_tip = t.commit_all("propre remote");
    t.run(&["update-ref", "refs/remotes/origin/dev", &remote_tip]);
    t.run(&["checkout", "-q", "main"]);
    t.run(&["branch", "-q", "-D", "pour-tag", "pour-remote"]);
    t.write("f", "1\n");
    t.commit_all("tip");
    t
}

fn all_except(excluded: &[&str]) -> Vec<String> {
    let mut all: Vec<String> = ["base", "tip", "propre a", "propre b", "propre solo", "propre tag", "propre remote"]
        .map(String::from)
        .to_vec();
    all.retain(|s| !excluded.contains(&s.as_str()));
    all.sort();
    all
}

#[test]
fn default_selection_walks_everything() {
    assert_eq!(summaries(&sample(), RefSelection::default()), all_except(&[]));
}

#[test]
fn commit_only_reachable_by_a_tag_disappears_when_tags_are_hidden() {
    let t = sample();
    assert_eq!(summaries(&t, RefSelection { tags: false, ..RefSelection::default() }), all_except(&["propre tag"]));
}

#[test]
fn commit_only_reachable_by_a_remote_branch_disappears_when_remotes_are_hidden() {
    let t = sample();
    let selection = RefSelection { remotes: false, ..RefSelection::default() };
    assert_eq!(summaries(&t, selection), all_except(&["propre remote"]));
}

#[test]
fn excluded_branch_folder_and_remote_hide_their_own_commits() {
    let t = sample();
    let exclude = |patterns: &[&str]| RefSelection {
        exclude: patterns.iter().map(|p| p.to_string()).collect(),
        ..RefSelection::default()
    };
    assert_eq!(summaries(&t, exclude(&["refs/heads/solo"])), all_except(&["propre solo"]));
    assert_eq!(summaries(&t, exclude(&["refs/heads/feature/*"])), all_except(&["propre a", "propre b"]));
    assert_eq!(summaries(&t, exclude(&["refs/remotes/origin/*"])), all_except(&["propre remote"]));
    assert_eq!(summaries(&t, exclude(&["refs/tags/v1"])), all_except(&["propre tag"]));
}

#[test]
fn head_stays_walked_when_its_branch_is_excluded() {
    let t = sample();
    let selection = RefSelection { exclude: vec!["refs/heads/main".into()], ..RefSelection::default() };
    assert!(summaries(&t, selection).contains(&"tip".to_string()));
}

#[test]
fn only_walks_the_chosen_refs_and_globs() {
    let t = sample();
    let only = |picks: &[&str]| RefSelection {
        head: false,
        only: picks.iter().map(|p| p.to_string()).collect(),
        ..RefSelection::default()
    };
    assert_eq!(summaries(&t, only(&["refs/heads/solo"])), ["base", "propre solo"]);
    assert_eq!(
        summaries(&t, only(&["refs/heads/feature/*", "refs/remotes/origin/*"])),
        ["base", "propre a", "propre b", "propre remote"]
    );
    assert_eq!(summaries(&t, only(&["refs/tags/v1"])), ["base", "propre tag"]);
}

#[test]
fn excluded_ref_is_not_walked_even_if_chosen() {
    let t = sample();
    let selection = RefSelection {
        head: false,
        only: vec!["refs/heads/feature/*".into(), "refs/heads/solo".into()],
        exclude: vec!["refs/heads/feature/a".into(), "refs/heads/solo".into()],
        ..RefSelection::default()
    };
    assert_eq!(summaries(&t, selection), ["base", "propre b"]);
}

#[test]
fn unborn_head_keeps_the_other_refs() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    let oid = t.commit_all("racine");
    t.run(&["symbolic-ref", "HEAD", "refs/heads/vide"]);
    t.run(&["tag", "v0", &oid]);
    assert_eq!(summaries(&t, RefSelection::default()), ["racine"]);
}
