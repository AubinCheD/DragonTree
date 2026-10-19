//! Actions groupées sur les branches : mise à jour, push, branches périmées.

mod common;

use common::TestRepo;
use dragontree_git::{GitError, MergeMode, NetOptions, Repo, StaleReason, clone_repo};

fn clone_into(remote: &TestRepo, parent: &std::path::Path, name: &str) -> Repo {
    let dest = parent.join(name);
    clone_repo(&remote.git, &remote.url(), &dest, None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    for (k, v) in
        [("user.name", "Testeur"), ("user.email", "t@e.com"), ("core.autocrlf", "false"), ("commit.gpgsign", "false")]
    {
        remote.git.cmd(&dest).args(["config", k, v]).run().unwrap();
    }
    Repo::open(&remote.git, &dest).unwrap()
}

fn seeded_remote() -> TestRepo {
    let bare = TestRepo::new_bare();
    let seed = TestRepo::new();
    seed.write("a.txt", "1\n");
    seed.commit_all("init");
    seed.run(&["remote", "add", "origin", &bare.url()]);
    seed.run(&["push", "-q", "origin", "main"]);
    bare
}

fn commit(repo: &Repo, file: &str, msg: &str) {
    std::fs::write(repo.workdir.join(file), msg).unwrap();
    repo.stage(&[file.to_string()]).unwrap();
    repo.commit(msg, false).unwrap();
}

fn push(repo: &Repo, branch: &str) {
    repo.push("origin", branch, branch, true, false, &NetOptions::default(), &mut |_| {}).unwrap();
}

fn fetch(repo: &Repo) {
    repo.fetch(None, true, &NetOptions::default(), &mut |_| {}).unwrap();
}

#[test]
fn updatable_branches_are_behind_and_not_ahead_and_updated_together() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    alice.create_branch("dev", "main", true).unwrap();
    commit(&alice, "d.txt", "dev 1");
    push(&alice, "dev");
    alice.create_branch("diverge", "main", true).unwrap();
    commit(&alice, "x.txt", "alice diverge");
    push(&alice, "diverge");
    alice.checkout("main").unwrap();

    let bob = clone_into(&remote, work.path(), "bob");
    bob.create_branch("dev", "origin/dev", false).unwrap();
    bob.create_branch("diverge", "origin/diverge", true).unwrap();
    commit(&bob, "y.txt", "bob diverge");

    commit(&alice, "m.txt", "main 2");
    push(&alice, "main");
    alice.checkout("dev").unwrap();
    commit(&alice, "d2.txt", "dev 2");
    push(&alice, "dev");
    alice.checkout("diverge").unwrap();
    commit(&alice, "x2.txt", "alice diverge 2");
    push(&alice, "diverge");

    bob.checkout("main").unwrap();
    fetch(&bob);

    let refs = bob.refs().unwrap();
    let names: Vec<&str> = refs.updatable_branches().iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["dev", "main"], "diverge n'est pas fast-forwardable");

    let plan: Vec<(String, String)> =
        refs.updatable_branches().iter().map(|b| (b.name.clone(), b.upstream.clone().unwrap())).collect();
    let outcomes = bob.update_branches(&plan, refs.current_branch());
    assert!(outcomes.iter().all(|o| o.result.is_ok()), "{outcomes:?}");
    let after = bob.refs().unwrap();
    for name in ["main", "dev"] {
        let local = after.local(name).unwrap();
        let remote_oid = after.remotes.iter().find(|r| r.full == format!("origin/{name}")).unwrap().oid;
        assert_eq!(local.oid, remote_oid, "{name}");
    }
    assert_eq!(after.current_branch(), Some("main"));
    assert!(bob.workdir.join("m.txt").exists(), "la branche courante est avancée par merge --ff-only");
    let diverge = after.local("diverge").unwrap();
    assert_eq!((diverge.ahead, diverge.behind), (1, 1), "diverge n'a pas bougé");
}

#[test]
fn update_never_moves_a_branch_that_is_not_a_fast_forward() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");
    commit(&alice, "m.txt", "alice");
    push(&alice, "main");
    bob.create_branch("autre", "main", true).unwrap();
    bob.checkout("main").unwrap();
    commit(&bob, "b.txt", "bob");
    bob.checkout("autre").unwrap();
    fetch(&bob);
    let before = bob.refs().unwrap().local("main").unwrap().oid;
    let outcomes = bob.update_branches(&[("main".into(), "origin/main".into())], Some("autre"));
    assert!(outcomes[0].result.is_err());
    assert_eq!(bob.refs().unwrap().local("main").unwrap().oid, before);
}

#[test]
fn pushable_branches_are_ahead_and_pushed_together() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let bob = clone_into(&remote, work.path(), "bob");
    bob.create_branch("f1", "main", true).unwrap();
    commit(&bob, "f1.txt", "f1");
    push(&bob, "f1");
    commit(&bob, "f1b.txt", "f1 suite");
    bob.checkout("main").unwrap();
    commit(&bob, "m.txt", "main suite");

    let refs = bob.refs().unwrap();
    let names: Vec<&str> = refs.pushable_branches().iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["f1", "main"]);

    for b in refs.pushable_branches() {
        let (r, rb) = refs.upstream_parts(b.upstream.as_deref().unwrap()).unwrap();
        assert_eq!(r, "origin");
        bob.push(&r, &b.name, &rb, false, false, &NetOptions::default(), &mut |_| {}).unwrap();
    }
    assert_eq!(remote.run(&["log", "-1", "--format=%s", "f1"]).trim(), "f1 suite");
    assert_eq!(remote.run(&["log", "-1", "--format=%s", "main"]).trim(), "main suite");
}

#[test]
fn stale_branches_are_gone_upstream_or_merged_into_the_default_branch() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let bob = clone_into(&remote, work.path(), "bob");

    // `gone` : poussée puis supprimée sur le remote, avec un commit propre (non fusionnée).
    bob.create_branch("gone", "main", true).unwrap();
    commit(&bob, "g.txt", "gone");
    push(&bob, "gone");
    // `merged` : fusionnée dans main, sans amont.
    bob.checkout("main").unwrap();
    bob.create_branch("merged", "main", true).unwrap();
    commit(&bob, "m.txt", "merged");
    bob.checkout("main").unwrap();
    bob.merge("merged", MergeMode::FfOnly).unwrap();
    // `wip` : non fusionnée, amont présent : jamais proposée.
    bob.create_branch("wip", "main", true).unwrap();
    commit(&bob, "w.txt", "wip");
    push(&bob, "wip");
    bob.checkout("main").unwrap();
    bob.delete_remote_branch("origin", "gone", &NetOptions::default(), &mut |_| {}).unwrap();
    fetch(&bob);

    let refs = bob.refs().unwrap();
    assert_eq!(bob.default_branch(&refs).as_deref(), Some("main"));
    let stale = bob.stale_branches(&refs).unwrap();
    let got: Vec<(&str, StaleReason)> = stale.iter().map(|s| (s.name.as_str(), s.reason)).collect();
    assert_eq!(got, [("gone", StaleReason::Gone), ("merged", StaleReason::Merged)]);

    // Suppression : `-d` refuse la branche gone non fusionnée, accepte la fusionnée ; `-D` la supprime.
    let outcomes = bob.delete_branches(&["gone".into(), "merged".into()], false);
    assert!(matches!(outcomes[0].result, Err(GitError::Failed { .. })), "{outcomes:?}");
    assert!(outcomes[1].result.is_ok());
    let outcomes = bob.delete_branches(&["gone".into()], true);
    assert!(outcomes[0].result.is_ok());
    let names: Vec<String> = bob.refs().unwrap().locals.iter().map(|b| b.name.clone()).collect();
    assert_eq!(names, ["main", "wip"]);
}

#[test]
fn the_current_branch_is_never_stale() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let bob = clone_into(&remote, work.path(), "bob");
    bob.create_branch("topic", "main", true).unwrap();
    commit(&bob, "t.txt", "topic");
    push(&bob, "topic");
    bob.delete_remote_branch("origin", "topic", &NetOptions::default(), &mut |_| {}).unwrap();
    fetch(&bob);
    // `topic` est la branche courante et son amont a disparu : elle reste.
    let refs = bob.refs().unwrap();
    assert!(refs.local("topic").unwrap().upstream_gone);
    assert!(bob.stale_branches(&refs).unwrap().is_empty());
}

#[test]
fn a_branch_checked_out_in_another_worktree_is_neither_moved_nor_listed_stale() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");
    commit(&alice, "m.txt", "alice");
    push(&alice, "main");
    bob.create_branch("ailleurs", "main", false).unwrap();
    bob.create_branch("tenue", "main", false).unwrap();
    push(&bob, "tenue");
    bob.delete_remote_branch("origin", "tenue", &NetOptions::default(), &mut |_| {}).unwrap();
    let wt = work.path().join("wt");
    remote.git.cmd(&bob.workdir).args(["worktree", "add", "-q", wt.to_str().unwrap(), "tenue"]).run().unwrap();
    fetch(&bob);
    let before = bob.refs().unwrap().local("tenue").unwrap().oid;
    // `main` est tenue par un autre worktree : refus, la branche ne bouge pas.
    remote.git.cmd(&bob.workdir).args(["checkout", "-q", "ailleurs"]).run().unwrap();
    remote.git.cmd(&wt).args(["checkout", "-q", "main"]).run().unwrap();
    let outcomes = bob.update_branches(&[("main".into(), "origin/main".into())], Some("ailleurs"));
    assert!(matches!(outcomes[0].result, Err(GitError::Refused(_))), "{outcomes:?}");
    assert_ne!(
        bob.refs().unwrap().local("main").unwrap().oid,
        bob.refs().unwrap().remotes.iter().find(|r| r.full == "origin/main").unwrap().oid
    );
    // `tenue` (amont disparu) est sortie dans `wt` : pas proposée à la suppression.
    remote.git.cmd(&wt).args(["checkout", "-q", "tenue"]).run().unwrap();
    let refs = bob.refs().unwrap();
    assert_eq!(refs.local("tenue").unwrap().oid, before);
    let names: Vec<String> = bob.stale_branches(&refs).unwrap().into_iter().map(|s| s.name).collect();
    assert!(!names.contains(&"tenue".to_string()), "{names:?}");
}
