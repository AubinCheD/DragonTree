//! Remettre un fichier dans son état à un commit donné.

mod common;

use common::TestRepo;
use dragontree_git::{FileState, Repo};

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

/// Trois commits sur `f.txt` (v1, v2, v3) puis un quatrième qui supprime `gone.txt` (créé au deuxième).
fn history() -> (TestRepo, Vec<String>) {
    let t = TestRepo::new();
    t.write("f.txt", "v1\n");
    let c1 = t.commit_all("un");
    t.write("f.txt", "v2\n");
    t.write("gone.txt", "bientôt supprimé\n");
    let c2 = t.commit_all("deux");
    t.write("f.txt", "v3\n");
    let c3 = t.commit_all("trois");
    t.run(&["rm", "-q", "gone.txt"]);
    let c4 = t.commit_all("suppression");
    (t, vec![c1, c2, c3, c4])
}

#[test]
fn restores_a_file_to_its_state_at_a_commit_and_stages_it() {
    let (t, c) = history();
    let repo = open(&t);
    repo.restore_file(&c[0], "f.txt").unwrap();
    assert_eq!(t.read("f.txt"), "v1\n");
    let status = repo.status().unwrap();
    let entry = status.entries.iter().find(|e| e.path == "f.txt").unwrap();
    assert_eq!(entry.index, FileState::Modified, "le résultat est indexé, comme git checkout <sha> -- <chemin>");
    assert_eq!(entry.worktree, FileState::Unmodified);
}

#[test]
fn restores_the_version_before_a_commit_with_the_parent_revision() {
    let (t, c) = history();
    let repo = open(&t);
    // Avant le commit « trois » : le contenu du commit « deux ».
    repo.restore_file(&format!("{}^", c[2]), "f.txt").unwrap();
    assert_eq!(t.read("f.txt"), "v2\n");
}

#[test]
fn restores_a_file_deleted_by_a_commit_from_before_that_commit() {
    let (t, c) = history();
    let repo = open(&t);
    assert!(!t.file("gone.txt").exists());
    // À la version du commit qui le supprime, il n'existe pas : git refuse.
    assert!(repo.restore_file(&c[3], "gone.txt").is_err());
    repo.restore_file(&format!("{}^", c[3]), "gone.txt").unwrap();
    assert_eq!(t.read("gone.txt"), "bientôt supprimé\n");
}

#[test]
fn restoring_overwrites_local_changes_and_takes_literal_paths() {
    let t = TestRepo::new();
    t.write("a [1].txt", "ancien\n");
    let c1 = t.commit_all("un");
    t.write("a [1].txt", "nouveau\n");
    t.write("a 1.txt", "autre\n");
    t.commit_all("deux");
    t.write("a [1].txt", "modif locale\n");
    let repo = open(&t);
    repo.restore_file(&c1, "a [1].txt").unwrap();
    assert_eq!(t.read("a [1].txt"), "ancien\n");
    assert_eq!(t.read("a 1.txt"), "autre\n", "le chemin est pris littéralement, pas comme un motif");
}

#[test]
fn a_root_commit_has_no_version_before_it() {
    let (t, c) = history();
    let repo = open(&t);
    assert!(repo.restore_file(&format!("{}^", c[0]), "f.txt").is_err());
}

#[test]
fn local_changes_include_modified_untracked_and_ignored_files() {
    let t = TestRepo::new();
    t.write(".env", "secret\n");
    t.write("f.txt", "v1\n");
    t.commit_all("un");
    // `.env` n'est plus suivi mais reste sur le disque, désormais ignoré.
    t.run(&["rm", "-q", "--cached", ".env"]);
    t.write(".gitignore", ".env\n");
    t.commit_all("ignore .env");
    let repo = open(&t);
    assert!(!repo.file_has_local_changes("f.txt").unwrap(), "fichier propre");
    assert!(repo.file_has_local_changes(".env").unwrap(), "fichier ignoré");
    t.write("f.txt", "modifié\n");
    assert!(repo.file_has_local_changes("f.txt").unwrap(), "fichier modifié");
    t.write("autre.txt", "x\n");
    assert!(repo.file_has_local_changes("autre.txt").unwrap(), "fichier non suivi");
}
