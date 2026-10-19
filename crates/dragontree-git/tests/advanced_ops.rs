mod common;

use common::TestRepo;
use dragontree_git::conflict::{Resolution, parse_conflicts, resolve};
use dragontree_git::rebase::{RebaseAction, RebaseItem};
use dragontree_git::{CancelToken, LogOptions, MergeMode, Oid, OpState, Repo};

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

#[test]
fn rebase_candidates_lists_oldest_first() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    let base = t.commit_all("base");
    for i in 1..=3 {
        t.write("f", &format!("{i}\n"));
        t.commit_all(&format!("c{i}"));
    }
    let repo = open(&t);
    let c = repo.rebase_candidates(&base).unwrap();
    let subjects: Vec<&str> = c.iter().map(|c| &*c.summary).collect();
    assert_eq!(subjects, vec!["c1", "c2", "c3"]);
}

#[test]
fn interactive_rebase_squash_reword_and_drop() {
    let t = TestRepo::new();
    t.write("f", "0\n");
    let base = t.commit_all("base");
    t.write("a", "a\n");
    t.commit_all("c1 ajoute a");
    t.write("b", "b\n");
    t.commit_all("c2 ajoute b");
    t.write("c", "c\n");
    t.commit_all("c3 ajoute c");
    t.write("d", "d\n");
    t.commit_all("c4 ajoute d");
    let repo = open(&t);
    let c = repo.rebase_candidates(&base).unwrap();
    let items = vec![
        RebaseItem { id: c[0].id, action: RebaseAction::Pick, summary: c[0].summary.to_string() },
        RebaseItem { id: c[1].id, action: RebaseAction::Fixup, summary: c[1].summary.to_string() },
        RebaseItem { id: c[2].id, action: RebaseAction::Drop, summary: c[2].summary.to_string() },
        RebaseItem {
            id: c[3].id,
            action: RebaseAction::Reword("c4 renommé\n\nAvec un corps".into()),
            summary: c[3].summary.to_string(),
        },
    ];
    repo.interactive_rebase(&base, &items).unwrap();
    assert_eq!(repo.operation_state(), OpState::None);
    let log = repo
        .log(&LogOptions { scope: dragontree_git::LogScope::Head, ..LogOptions::default() }, &CancelToken::new())
        .unwrap();
    let subjects: Vec<&str> = log.iter().map(|c| &*c.summary).collect();
    assert_eq!(subjects, vec!["c4 renommé", "c1 ajoute a", "base"]);
    assert_eq!(repo.last_commit_message().unwrap(), "c4 renommé\n\nAvec un corps");
    assert!(t.file("a").exists() && t.file("b").exists() && t.file("d").exists());
    assert!(!t.file("c").exists(), "c3 a été supprimé");
}

fn conflicted_repo() -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.write("f", "base\n");
    t.commit_all("base");
    t.run(&["checkout", "-q", "-b", "other"]);
    t.write("f", "theirs\n");
    t.commit_all("theirs");
    t.run(&["checkout", "-q", "main"]);
    t.write("f", "ours\n");
    t.commit_all("ours");
    let repo = open(&t);
    assert!(repo.merge("other", MergeMode::Default).is_err());
    (t, repo)
}

#[test]
fn checkout_side_takes_ours_or_theirs() {
    let (t, repo) = conflicted_repo();
    repo.checkout_side("f", false).unwrap();
    assert_eq!(t.read("f"), "theirs\n");
    repo.checkout_side("f", true).unwrap();
    assert_eq!(t.read("f"), "ours\n");
    repo.stage(&["f".into()]).unwrap();
    assert!(repo.status().unwrap().conflicted().is_empty());
    repo.continue_op(OpState::Merge).unwrap();
    assert_eq!(repo.operation_state(), OpState::None);
}

#[test]
fn resolve_markers_in_real_conflict() {
    let (t, repo) = conflicted_repo();
    let text = t.read("f");
    let regions = parse_conflicts(&text);
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].ours, vec!["ours"]);
    assert_eq!(regions[0].theirs, vec!["theirs"]);
    t.write("f", &resolve(&text, &[Resolution::Both]));
    assert_eq!(t.read("f"), "ours\ntheirs\n");
    repo.stage(&["f".into()]).unwrap();
    repo.continue_op(OpState::Merge).unwrap();
}

#[test]
fn blame_on_real_file() {
    let t = TestRepo::new();
    t.write("f", "un\n");
    let c1 = t.commit_all("premier");
    t.write("f", "un\ndeux\n");
    let c2 = t.commit_all("second");
    let repo = open(&t);
    let (lines, commits) = repo.blame("f", None).unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].commit, Oid::from_hex(&c1).unwrap());
    assert_eq!(lines[1].commit, Oid::from_hex(&c2).unwrap());
    assert_eq!(commits[&lines[1].commit].summary, "second");
    let (old, _) = repo.blame("f", Some(Oid::from_hex(&c1).unwrap())).unwrap();
    assert_eq!(old.len(), 1);
}

#[test]
fn mergetool_resolves_with_configured_tool() {
    let (t, repo) = conflicted_repo();
    // Outil factice : prend la version « theirs ».
    t.run(&["config", "merge.tool", "fake"]);
    t.run(&["config", "mergetool.fake.cmd", "cp \"$REMOTE\" \"$MERGED\""]);
    t.run(&["config", "mergetool.fake.trustExitCode", "true"]);
    t.run(&["config", "mergetool.keepBackup", "false"]);
    repo.run_mergetool("f").unwrap();
    assert_eq!(t.read("f"), "theirs\n");
    assert!(repo.status().unwrap().conflicted().is_empty());
}
