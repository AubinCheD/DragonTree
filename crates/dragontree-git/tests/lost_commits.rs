//! Commits perdus (atteignables seulement par le reflog) sur de vrais dépôts.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, LogOptions, LogScope, Oid, RefSelection, Repo};

fn lost(t: &TestRepo) -> dragontree_git::LostCommits {
    Repo::open(&t.git, t.path()).unwrap().lost_commits(&CancelToken::new()).unwrap()
}

fn oid(hex: &str) -> Oid {
    Oid::from_hex(hex).unwrap()
}

#[test]
fn nothing_is_lost_in_a_plain_history() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.write("f", "1\n");
    t.commit_all("suite");
    assert!(lost(&t).all.is_empty());
}

#[test]
fn the_commit_dropped_by_a_hard_reset_is_lost() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.write("f", "1\n");
    let dropped = t.commit_all("abandonné");
    t.run(&["reset", "-q", "--hard", "HEAD~1"]);
    let found = lost(&t);
    assert_eq!(found.tips, [oid(&dropped)]);
    assert_eq!(found.all.len(), 1, "« base » est atteint par main");
}

#[test]
fn a_lost_chain_has_a_single_tip_and_a_new_branch_recovers_it() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.write("f", "1\n");
    t.commit_all("perdu 1");
    t.write("f", "2\n");
    let tip = t.commit_all("perdu 2");
    t.run(&["reset", "-q", "--hard", "HEAD~2"]);
    let found = lost(&t);
    assert_eq!((found.tips.len(), found.all.len()), (1, 2));
    t.run(&["branch", "secours", &tip]);
    assert!(lost(&t).all.is_empty(), "une branche le rattrape");
}

#[test]
fn the_log_walks_the_lost_tips_passed_as_extra_revisions() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.write("f", "1\n");
    t.commit_all("abandonné");
    t.run(&["reset", "-q", "--hard", "HEAD~1"]);
    let repo = Repo::open(&t.git, t.path()).unwrap();
    let summaries = |extra: Vec<String>| -> Vec<String> {
        let opts = LogOptions {
            scope: LogScope::Refs(RefSelection { extra, ..RefSelection::default() }),
            ..Default::default()
        };
        repo.log(&opts, &CancelToken::new()).unwrap().iter().map(|c| c.summary.to_string()).collect()
    };
    assert_eq!(summaries(Vec::new()), ["base"]);
    let tips = repo.lost_commits(&CancelToken::new()).unwrap().tips.iter().map(Oid::to_hex).collect();
    assert_eq!(summaries(tips), ["abandonné", "base"]);
}

#[test]
fn stashes_are_not_lost_commits() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    for n in 1..=2 {
        t.write("f", &format!("{n}\n"));
        t.write("neuf", "x\n");
        t.run(&["stash", "push", "-q", "-u", "-m", &format!("stash {n}")]);
    }
    assert!(lost(&t).all.is_empty(), "{:?}", lost(&t));
}

#[test]
fn an_unborn_head_has_nothing_lost() {
    let t = TestRepo::new();
    assert!(lost(&t).all.is_empty());
}

#[test]
fn notes_commits_are_not_lost() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    t.run(&["notes", "add", "-m", "une note"]);
    t.run(&["notes", "add", "-f", "-m", "une autre note"]);
    assert!(lost(&t).all.is_empty(), "refs/notes est une référence");
}

#[test]
fn a_detached_commit_of_another_worktree_is_not_lost() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    t.commit_all("base");
    let other = tempfile::tempdir().unwrap();
    let wt = other.path().join("second");
    t.run(&["worktree", "add", "-q", "--detach", wt.to_str().unwrap()]);
    let run_in_wt = |args: &[&str]| t.git.cmd(&wt).args(args).run().unwrap();
    std::fs::write(wt.join("g"), "1\n").unwrap();
    run_in_wt(&["add", "-A"]);
    run_in_wt(&["commit", "-q", "-m", "sur HEAD détaché du second worktree"]);
    assert!(lost(&t).all.is_empty(), "le HEAD de l'autre worktree le garde");
}

#[test]
fn thousands_of_extra_revisions_do_not_hit_the_command_line_limit() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    let head = t.commit_all("base");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    // 3 000 x 41 caractères dépasseraient largement les 32 767 de CreateProcess s'ils étaient des arguments.
    let selection = RefSelection { extra: vec![head; 3000], ..RefSelection::default() };
    let opts = LogOptions { scope: LogScope::Refs(selection), ..LogOptions::default() };
    assert_eq!(repo.log(&opts, &CancelToken::new()).unwrap().len(), 1);
}
