//! Bisect : session complète sur un historique linéaire de 8 commits dont le fichier est « cassé » à partir du 5e,
//! démarrage implicite par un marquage, passer un commit, terminer, détection de l'état.

mod common;

use common::TestRepo;
use dragontree_git::{BisectMark, Oid, Repo};

/// Huit commits `c1`…`c8` : `f.txt` contient `broken` à partir de `c5`. Renvoie aussi leurs identifiants.
fn history() -> (TestRepo, Repo, Vec<Oid>) {
    let t = TestRepo::new();
    let mut ids = Vec::new();
    for i in 1..=8 {
        t.write("f.txt", if i >= 5 { "broken\n" } else { "ok\n" });
        t.write("n.txt", &format!("{i}\n"));
        ids.push(Oid::from_hex(&t.commit_all(&format!("c{i}"))).unwrap());
    }
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo, ids)
}

fn is_broken(t: &TestRepo) -> bool {
    t.read("f.txt").trim() == "broken"
}

#[test]
fn without_a_session_there_is_no_state() {
    let (_t, repo, _) = history();
    assert!(repo.bisect_state().is_none());
}

#[test]
fn starting_marks_head_bad_and_records_where_the_session_began() {
    let (_t, repo, ids) = history();
    repo.bisect_start().unwrap();
    let state = repo.bisect_state().expect("session détectée");
    assert_eq!(state.start.as_deref(), Some("main"));
    assert_eq!(state.bad, Some(ids[7]));
    assert!(state.good.is_empty());
    assert_eq!(state.remaining, None, "pas de commit bon : rien à compter");
    assert_eq!(state.found, None);
}

#[test]
fn marking_a_commit_good_gives_the_remaining_revisions_from_git() {
    let (_t, repo, ids) = history();
    repo.bisect_start().unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())).unwrap();
    let state = repo.bisect_state().unwrap();
    assert_eq!(state.good, [ids[0]]);
    assert_eq!(state.remaining, Some(3));
    assert_eq!(state.steps, Some(2));
}

#[test]
fn a_complete_session_finds_the_first_broken_commit() {
    let (t, repo, ids) = history();
    repo.bisect_start().unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())).unwrap();
    let mut answers = 0;
    let found = loop {
        let state = repo.bisect_state().unwrap();
        if let Some(found) = state.found {
            break found;
        }
        assert!(answers < 8, "la session doit se terminer");
        answers += 1;
        let mark = if is_broken(&t) { BisectMark::Bad } else { BisectMark::Good };
        repo.bisect_mark(mark, None).unwrap();
    };
    assert_eq!(found, ids[4], "le 5e commit est le premier cassé");
    assert!(answers <= 3, "bisection : {answers} réponses");
}

#[test]
fn marking_without_a_session_starts_one_with_head_bad() {
    let (_t, repo, ids) = history();
    repo.bisect_mark(BisectMark::Good, Some(&ids[1].to_hex())).unwrap();
    let state = repo.bisect_state().unwrap();
    assert_eq!(state.bad, Some(ids[7]));
    assert_eq!(state.good, [ids[1]]);
    assert!(state.remaining.is_some());
}

#[test]
fn marking_a_given_commit_bad_starts_the_session_with_that_commit() {
    let (_t, repo, ids) = history();
    repo.bisect_mark(BisectMark::Bad, Some(&ids[5].to_hex())).unwrap();
    let state = repo.bisect_state().unwrap();
    assert_eq!(state.bad, Some(ids[5]));
    assert!(state.good.is_empty());
}

#[test]
fn skipping_records_the_commit_and_moves_on() {
    let (t, repo, ids) = history();
    repo.bisect_start().unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())).unwrap();
    let before = Oid::from_hex(t.run(&["rev-parse", "HEAD"]).trim()).unwrap();
    repo.bisect_mark(BisectMark::Skip, None).unwrap();
    let state = repo.bisect_state().unwrap();
    assert_eq!(state.skipped, [before]);
    let after = Oid::from_hex(t.run(&["rev-parse", "HEAD"]).trim()).unwrap();
    assert_ne!(after, before, "un autre commit est proposé");
    assert!(!ids[..1].contains(&after));
}

#[test]
fn reset_ends_the_session_and_returns_to_the_branch() {
    let (t, repo, ids) = history();
    repo.bisect_start().unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())).unwrap();
    assert_ne!(t.run(&["rev-parse", "HEAD"]).trim(), ids[7].to_hex());
    repo.bisect_reset().unwrap();
    assert!(repo.bisect_state().is_none());
    assert_eq!(t.run(&["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "main");
    assert_eq!(t.run(&["rev-parse", "HEAD"]).trim(), ids[7].to_hex());
    assert!(t.run(&["for-each-ref", "refs/bisect"]).trim().is_empty());
}

#[test]
fn reset_without_a_session_is_harmless() {
    let (_t, repo, _) = history();
    repo.bisect_reset().unwrap();
    assert!(repo.bisect_state().is_none());
}

#[test]
fn a_session_is_refused_during_an_operation_in_progress() {
    let (t, repo, ids) = history();
    std::fs::write(t.path().join(".git/MERGE_HEAD"), format!("{}\n", ids[0].to_hex())).unwrap();
    assert!(matches!(repo.bisect_start(), Err(dragontree_git::GitError::Refused(_))));
    assert!(matches!(
        repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())),
        Err(dragontree_git::GitError::Refused(_))
    ));
    assert!(repo.bisect_state().is_none());
    assert!(!t.path().join(".git/BISECT_START").exists());
}

#[test]
fn two_answers_about_the_same_commit_do_not_mark_the_next_one() {
    let (t, repo, ids) = history();
    repo.bisect_start().unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&ids[0].to_hex())).unwrap();
    // Commit affiché au moment du clic : les deux réponses (double clic) portent dessus.
    let shown = t.run(&["rev-parse", "HEAD"]).trim().to_string();
    repo.bisect_mark(BisectMark::Good, Some(&shown)).unwrap();
    repo.bisect_mark(BisectMark::Good, Some(&shown)).unwrap();
    let state = repo.bisect_state().unwrap();
    let mut good: Vec<String> = state.good.iter().map(|o| o.to_hex()).collect();
    good.sort();
    let mut expected = vec![ids[0].to_hex(), shown];
    expected.sort();
    assert_eq!(good, expected, "seul le commit affiché est marqué, pas celui qui suit");
}
