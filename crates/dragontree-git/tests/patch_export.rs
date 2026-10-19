//! Enregistrer comme patch : commit, plage de commits, stash, fichiers ; chaque patch produit s'applique à sa base.

mod common;

use common::TestRepo;
use dragontree_git::apply::ApplyTarget;
use dragontree_git::{Oid, Repo};

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

fn oid(t: &TestRepo, rev: &str) -> Oid {
    Oid::from_hex(t.run(&["rev-parse", rev]).trim()).unwrap()
}

/// Trois commits linéaires sur `a.txt` : « un », « deux », « trois ».
fn history() -> TestRepo {
    let t = TestRepo::new();
    for (n, msg) in [("1", "un"), ("2", "deux"), ("3", "trois")] {
        t.write("a.txt", &format!("{n}\n"));
        t.write(&format!("f{n}.txt"), "x\n");
        t.commit_all(msg);
    }
    t
}

#[test]
fn a_commit_patch_is_a_mail_patch_that_applies_to_its_parent() {
    let t = history();
    let repo = open(&t);
    let patch = repo.commit_patch(oid(&t, "HEAD")).unwrap();
    let text = String::from_utf8_lossy(&patch);
    assert!(text.starts_with("From "), "{text}");
    assert!(text.contains("Subject: [PATCH] trois"));
    t.run(&["checkout", "-q", "--detach", "HEAD~1"]);
    repo.patch_check(&patch, ApplyTarget::Worktree).unwrap();
    repo.am_patch(&patch).unwrap();
    assert_eq!(t.read("a.txt"), "3\n");
}

#[test]
fn a_merge_commit_has_no_patch() {
    let t = history();
    t.run(&["checkout", "-q", "-b", "autre", "HEAD~2"]);
    t.write("g.txt", "g\n");
    t.commit_all("autre");
    t.run(&["checkout", "-q", "main"]);
    t.run(&["merge", "-q", "--no-ff", "-m", "fusion", "autre"]);
    assert!(open(&t).commit_patch(oid(&t, "HEAD")).is_err());
}

#[test]
fn the_commits_from_a_commit_to_head_are_listed_oldest_first() {
    let t = history();
    let repo = open(&t);
    let list = repo.commits_to_head(oid(&t, "HEAD~1")).unwrap();
    assert_eq!(list, [oid(&t, "HEAD~1"), oid(&t, "HEAD")]);
    // Depuis la racine : tout l'historique.
    assert_eq!(repo.commits_to_head(oid(&t, "HEAD~2")).unwrap().len(), 3);
    assert_eq!(repo.commits_to_head(oid(&t, "HEAD")).unwrap(), [oid(&t, "HEAD")]);
}

#[test]
fn a_commit_that_is_not_an_ancestor_of_head_has_no_range() {
    let t = history();
    t.run(&["checkout", "-q", "-b", "autre", "HEAD~1"]);
    t.write("g.txt", "g\n");
    t.commit_all("autre");
    let other = oid(&t, "HEAD");
    t.run(&["checkout", "-q", "main"]);
    assert!(open(&t).commits_to_head(other).is_err());
}

#[test]
fn a_range_is_written_as_one_numbered_file_per_commit() {
    let t = history();
    let repo = open(&t);
    let out = tempfile::tempdir().unwrap();
    let list = repo.commits_to_head(oid(&t, "HEAD~1")).unwrap();
    let files = repo.commit_patches_to_dir(&list, out.path()).unwrap();
    let names: Vec<_> = files.iter().map(|f| f.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names, ["0001-deux.patch", "0002-trois.patch"]);
    // Les fichiers, appliqués dans l'ordre sur la base, refont l'historique.
    t.run(&["reset", "-q", "--hard", "HEAD~2"]);
    for f in &files {
        repo.am_patch(&std::fs::read(f).unwrap()).unwrap();
    }
    assert_eq!(t.read("a.txt"), "3\n");
    assert_eq!(t.run(&["log", "--format=%s"]).lines().collect::<Vec<_>>(), ["trois", "deux", "un"]);
}

#[test]
fn a_stash_patch_includes_the_untracked_files_and_applies_to_the_base() {
    let t = history();
    t.write("a.txt", "modifié\n");
    t.write("neuf.txt", "non suivi\n");
    t.run(&["stash", "push", "-q", "-u", "-m", "mon stash"]);
    let repo = open(&t);
    let patch = repo.stash_patch(repo.stashes().unwrap()[0].oid).unwrap();
    let text = String::from_utf8_lossy(&patch);
    assert!(text.contains("a.txt") && text.contains("neuf.txt"), "{text}");
    repo.patch_check(&patch, ApplyTarget::Worktree).unwrap();
    repo.apply_patch_to(&patch, ApplyTarget::Worktree).unwrap();
    assert_eq!(t.read("a.txt"), "modifié\n");
    assert_eq!(t.read("neuf.txt"), "non suivi\n");
}

#[test]
fn a_file_patch_covers_unstaged_staged_and_untracked_files() {
    let t = history();
    t.write("a.txt", "modifié\n");
    t.write("f1.txt", "indexé\n");
    t.run(&["add", "f1.txt"]);
    t.write("neuf.txt", "non suivi\n");
    let repo = open(&t);

    let unstaged = repo.files_patch(&["a.txt".into()], &["neuf.txt".into()], false).unwrap();
    let text = String::from_utf8_lossy(&unstaged);
    assert!(text.contains("+modifié") && text.contains("+non suivi"), "{text}");
    assert!(!text.contains("indexé"), "seulement les fichiers demandés");

    let staged = repo.files_patch(&["f1.txt".into()], &[], true).unwrap();
    assert!(String::from_utf8_lossy(&staged).contains("+indexé"));

    // Retour à la base : les deux patchs s'appliquent.
    t.run(&["reset", "-q", "--hard"]);
    std::fs::remove_file(t.file("neuf.txt")).unwrap();
    repo.patch_check(&unstaged, ApplyTarget::Worktree).unwrap();
    repo.patch_check(&staged, ApplyTarget::Worktree).unwrap();
}

#[test]
fn a_file_patch_takes_paths_literally_and_keeps_binary_files() {
    let t = history();
    t.write("[ab].txt", "x\n");
    t.write("a.txt", "modifié\n");
    std::fs::write(t.file("img.bin"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    t.run(&["add", "-N", "img.bin"]);
    let repo = open(&t);
    let patch = repo.files_patch(&["img.bin".into()], &["[ab].txt".into()], false).unwrap();
    let text = String::from_utf8_lossy(&patch);
    assert!(text.contains("[ab].txt") && text.contains("GIT binary patch"), "{text}");
    assert!(!text.contains("modifié"), "a.txt n'est pas demandé");
}

#[test]
fn patch_file_names_follow_the_subject() {
    use dragontree_git::export::patch_file_name;
    assert_eq!(patch_file_name("Corrige l'accueil : retour"), "corrige-l-accueil-retour.patch");
    assert_eq!(patch_file_name("   "), "commit.patch");
}

#[test]
fn a_merge_in_a_range_is_skipped_not_replaced_by_another_commit() {
    let t = history();
    t.run(&["checkout", "-q", "-b", "autre", "HEAD~2"]);
    t.write("g.txt", "g\n");
    t.commit_all("autre");
    t.run(&["checkout", "-q", "main"]);
    t.run(&["merge", "-q", "--no-ff", "-m", "fusion", "autre"]);
    let out = tempfile::tempdir().unwrap();
    let files = open(&t).commit_patches_to_dir(&[oid(&t, "HEAD")], out.path()).unwrap();
    assert!(files.is_empty(), "{files:?}");
}

#[test]
fn a_range_with_a_merge_keeps_the_side_branch_changes_like_format_patch() {
    // R ← A ← B ← M (fusion de S) ; S part de R. Depuis A : A, B et S, pas M.
    let t = TestRepo::new();
    t.write("r.txt", "r\n");
    let r = t.commit_all("racine");
    t.run(&["checkout", "-q", "-b", "cote"]);
    t.write("s.txt", "s\n");
    t.commit_all("cote");
    t.run(&["checkout", "-q", "main"]);
    t.write("a.txt", "a\n");
    let a = t.commit_all("a");
    t.write("b.txt", "b\n");
    t.commit_all("b");
    t.run(&["merge", "-q", "--no-ff", "-m", "fusion", "cote"]);
    let repo = open(&t);
    let list = repo.commits_to_head(oid(&t, &a)).unwrap();
    let subjects: std::collections::BTreeSet<_> =
        list.iter().map(|o| t.run(&["log", "-1", "--format=%s", &o.to_hex()]).trim().to_string()).collect();
    assert_eq!(subjects.into_iter().collect::<Vec<_>>(), ["a", "b", "cote"]);
    // La série s'applique avec am sur le parent du premier commit.
    let out = tempfile::tempdir().unwrap();
    let files = repo.commit_patches_to_dir(&list, out.path()).unwrap();
    assert_eq!(files.len(), 3);
    t.run(&["checkout", "-q", "--detach", &r]);
    for f in &files {
        repo.am_patch(&std::fs::read(f).unwrap()).unwrap();
    }
    for f in ["a.txt", "b.txt", "s.txt"] {
        assert!(t.file(f).exists(), "{f} manque après am");
    }
}

#[test]
fn a_staged_rename_needs_both_names_in_the_file_patch() {
    let t = history();
    t.run(&["mv", "f1.txt", "renomme.txt"]);
    let repo = open(&t);
    let only_new = repo.files_patch(&["renomme.txt".into()], &[], true).unwrap();
    assert!(!String::from_utf8_lossy(&only_new).contains("deleted file mode"));
    let both = repo.files_patch(&["renomme.txt".into(), "f1.txt".into()], &[], true).unwrap();
    let text = String::from_utf8_lossy(&both);
    assert!(text.contains("deleted file mode") && text.contains("new file mode"), "{text}");
    t.run(&["reset", "-q", "--hard"]);
    repo.apply_patch_to(&both, ApplyTarget::Worktree).unwrap();
    assert!(!t.file("f1.txt").exists() && t.file("renomme.txt").exists());
}
