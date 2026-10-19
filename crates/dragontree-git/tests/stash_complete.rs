//! Stashs complets : fichiers choisis, snapshot, renommage, fichiers non suivis d'un stash.

mod common;

use common::TestRepo;
use dragontree_git::Repo;

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

fn base() -> TestRepo {
    let t = TestRepo::new();
    t.write("a.txt", "a\n");
    t.write("b.txt", "b\n");
    t.commit_all("base");
    t
}

#[test]
fn stash_of_chosen_files_leaves_the_others_modified() {
    let t = base();
    t.write("a.txt", "a2\n");
    t.write("b.txt", "b2\n");
    t.write("neuf.txt", "x\n");
    open(&t).stash_push_paths(Some("seulement a"), &["a.txt".into(), "neuf.txt".into()]).unwrap();
    assert_eq!(t.read("a.txt"), "a\n", "fichier choisi remis à zéro");
    assert_eq!(t.read("b.txt"), "b2\n", "fichier non choisi laissé modifié");
    assert!(!t.file("neuf.txt").exists(), "fichier non suivi choisi mis de côté");
    let stashes = open(&t).stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.ends_with("seulement a"));
}

#[test]
fn stash_of_chosen_files_takes_paths_literally() {
    let t = base();
    t.write("[ab].txt", "x\n");
    t.write("a.txt", "a2\n");
    open(&t).stash_push_paths(None, &["[ab].txt".into()]).unwrap();
    assert!(!t.file("[ab].txt").exists());
    assert_eq!(t.read("a.txt"), "a2\n", "le motif n'est pas interprété");
}

#[test]
fn snapshot_creates_a_stash_without_touching_the_worktree() {
    let t = base();
    t.write("a.txt", "a2\n");
    t.run(&["add", "a.txt"]);
    t.write("b.txt", "b2\n");
    let repo = open(&t);
    repo.stash_snapshot(Some("instantané")).unwrap();
    assert_eq!(t.read("a.txt"), "a2\n");
    assert_eq!(t.read("b.txt"), "b2\n");
    assert!(t.run(&["status", "--porcelain"]).contains("M  a.txt"), "l'index est conservé");
    let stashes = repo.stashes().unwrap();
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.ends_with("instantané"), "{}", stashes[0].message);
    // Le stash contient bien les modifications.
    assert_eq!(t.run(&["show", &format!("{}:b.txt", stashes[0].oid.to_hex())]), "b2\n");
}

#[test]
fn snapshot_without_changes_is_refused() {
    let t = base();
    assert!(open(&t).stash_snapshot(None).is_err());
    assert!(open(&t).stashes().unwrap().is_empty());
}

#[test]
fn rename_changes_the_message_keeps_the_content_and_the_position() {
    let t = base();
    for (n, msg) in [("1", "premier"), ("2", "second"), ("3", "troisième")] {
        t.write("a.txt", &format!("a{n}\n"));
        t.run(&["stash", "push", "-q", "-m", msg]);
    }
    let repo = open(&t);
    let before = repo.stashes().unwrap(); // [troisième, second, premier]
    let target = before[1].oid;
    repo.stash_rename_oid(target, "deuxième").unwrap();

    let after = repo.stashes().unwrap();
    assert_eq!(after.len(), 3);
    assert_eq!(after.iter().map(|s| s.oid).collect::<Vec<_>>(), before.iter().map(|s| s.oid).collect::<Vec<_>>());
    assert_eq!(after[1].message, "deuxième");
    assert_eq!(after[0].message, before[0].message);
    assert_eq!(after[2].message, before[2].message);
}

#[test]
fn rename_of_an_unknown_stash_is_refused() {
    let t = base();
    let oid = dragontree_git::Oid::from_hex(&"a".repeat(40)).unwrap();
    assert!(open(&t).stash_rename_oid(oid, "x").is_err());
}

#[test]
fn untracked_files_of_a_stash_are_listed_with_their_commit() {
    let t = base();
    t.write("a.txt", "a2\n");
    t.write("neuf.txt", "x\n");
    t.write("dossier/autre.txt", "y\n");
    t.run(&["stash", "push", "-q", "-u", "-m", "avec non suivis"]);
    let repo = open(&t);
    let oid = repo.stashes().unwrap()[0].oid;
    let (commit, files) = repo.stash_untracked(oid).unwrap().expect("troisième parent");
    let mut paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, ["dossier/autre.txt", "neuf.txt"]);
    // Le diff d'un de ces fichiers se lit depuis ce commit, sans parent.
    let d = repo.diff_commit_file(commit, None, &files[0], &Default::default()).unwrap();
    assert!(d.is_new);
}

#[test]
fn a_stash_without_untracked_files_has_no_third_parent() {
    let t = base();
    t.write("a.txt", "a2\n");
    t.run(&["stash", "push", "-q"]);
    let repo = open(&t);
    assert!(repo.stash_untracked(repo.stashes().unwrap()[0].oid).unwrap().is_none());
}

/// Un hook `reference-transaction` qui refuse la mise à jour de rang `fail_at` (à partir de l'installation).
fn fail_nth_ref_update(t: &TestRepo, fail_at: u32) {
    let hook = t.file(".git/hooks/reference-transaction");
    let script = format!(
        "#!/bin/sh\n[ \"$1\" = prepared ] || exit 0\nn=$(cat .git/cnt 2>/dev/null || echo 0)\nn=$((n+1))\necho $n > .git/cnt\n[ \"$n\" -ge {fail_at} ] && exit 1\nexit 0\n"
    );
    std::fs::write(&hook, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn a_rename_that_fails_midway_loses_no_stash() {
    let t = base();
    for (n, msg) in [("1", "premier"), ("2", "second"), ("3", "troisième")] {
        t.write("a.txt", &format!("a{n}\n"));
        t.run(&["stash", "push", "-q", "-m", msg]);
    }
    let repo = open(&t);
    let before: Vec<_> = repo.stashes().unwrap().iter().map(|s| s.oid).collect();
    // Renomme le plus ancien : trois rangements (lui puis les deux plus récents) ; le troisième échoue.
    fail_nth_ref_update(&t, 3);
    let err = repo.stash_rename_oid(before[2], "renommé").unwrap_err().to_string();
    let after: Vec<_> = repo.stashes().unwrap().iter().map(|s| s.oid).collect();
    for oid in &before {
        assert!(after.contains(oid), "stash {} perdu : {after:?}", oid.to_hex());
        assert!(err.contains(&oid.to_hex()), "le message doit nommer les commits : {err}");
    }
}

#[test]
fn staging_a_rename_then_stashing_both_paths_leaves_nothing_behind() {
    let t = base();
    t.run(&["mv", "a.txt", "renomme.txt"]);
    let repo = open(&t);
    repo.stash_push_paths(Some("renommage"), &["renomme.txt".into(), "a.txt".into()]).unwrap();
    assert_eq!(t.run(&["status", "--porcelain"]), "", "ni ajout ni suppression ne reste indexé");
    assert_eq!(t.read("a.txt"), "a\n");
}
