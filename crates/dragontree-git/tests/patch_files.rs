//! Application d'un patch complet : aperçu (numstat), vérification, copie de travail, index, `git am`.

mod common;

use common::TestRepo;
use dragontree_git::apply::{ApplyTarget, is_mail_patch};
use dragontree_git::{OpState, Repo};

fn open(t: &TestRepo) -> Repo {
    Repo::open(&t.git, t.path()).unwrap()
}

/// Dépôt avec `a.txt` (3 lignes) et `b.txt` ; renvoie le dépôt et un diff qui modifie a.txt et crée c.txt.
fn setup() -> (TestRepo, Vec<u8>) {
    let t = TestRepo::new();
    t.write("a.txt", "1\n2\n3\n");
    t.write("b.txt", "b\n");
    t.commit_all("base");
    t.write("a.txt", "1\nDEUX\n3\nquatre\n");
    t.write("c.txt", "neuf\n");
    t.run(&["add", "-N", "c.txt"]);
    let patch = t.run(&["diff"]).into_bytes();
    t.run(&["checkout", "--", "a.txt"]);
    t.run(&["rm", "-f", "-q", "--cached", "c.txt"]);
    std::fs::remove_file(t.file("c.txt")).unwrap();
    (t, patch)
}

#[test]
fn the_preview_lists_the_touched_files_with_their_counts() {
    let (t, patch) = setup();
    let files = open(&t).patch_numstat(&patch).unwrap();
    let list: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.added, f.removed)).collect();
    assert_eq!(list, [("a.txt", Some(2), Some(1)), ("c.txt", Some(1), Some(0))]);
}

#[test]
fn a_patch_is_checked_before_being_applied() {
    let (t, patch) = setup();
    let repo = open(&t);
    repo.patch_check(&patch, ApplyTarget::Worktree).unwrap();
    repo.patch_check(&patch, ApplyTarget::Index).unwrap();
    // Aucun effet de bord.
    assert_eq!(t.read("a.txt"), "1\n2\n3\n");
    assert!(!t.file("c.txt").exists());
}

#[test]
fn a_patch_that_does_not_apply_is_refused_with_git_s_explanation() {
    let (t, patch) = setup();
    t.write("a.txt", "tout autre contenu\n");
    t.run(&["add", "a.txt"]);
    t.run(&["commit", "-q", "-m", "autre"]);
    let repo = open(&t);
    let err = repo.patch_check(&patch, ApplyTarget::Worktree).unwrap_err();
    assert!(err.to_string().contains("a.txt"), "{err}");
    let err = repo.apply_patch_to(&patch, ApplyTarget::Worktree).unwrap_err();
    assert!(err.to_string().contains("a.txt"), "{err}");
    assert_eq!(t.read("a.txt"), "tout autre contenu\n", "rien n'est appliqué à moitié");
    assert!(!t.file("c.txt").exists());
}

#[test]
fn a_patch_applies_to_the_working_copy_only() {
    let (t, patch) = setup();
    open(&t).apply_patch_to(&patch, ApplyTarget::Worktree).unwrap();
    assert_eq!(t.read("a.txt"), "1\nDEUX\n3\nquatre\n");
    assert_eq!(t.read("c.txt"), "neuf\n");
    assert_eq!(t.run(&["diff", "--cached", "--name-only"]), "", "l'index n'est pas touché");
}

#[test]
fn a_patch_applies_to_the_index_only() {
    let (t, patch) = setup();
    open(&t).apply_patch_to(&patch, ApplyTarget::Index).unwrap();
    assert_eq!(t.run(&["show", ":a.txt"]), "1\nDEUX\n3\nquatre\n");
    assert_eq!(t.read("a.txt"), "1\n2\n3\n", "la copie de travail n'est pas touchée");
    assert!(!t.file("c.txt").exists());
}

/// Patch au format mail (`format-patch`) d'un commit de `autre`, le dépôt étant revenu avant ce commit.
fn mail_setup() -> (TestRepo, Vec<u8>) {
    let t = TestRepo::new();
    t.write("a.txt", "1\n2\n3\n");
    t.commit_all("base");
    t.write("a.txt", "1\n2\n3\nquatre\n");
    t.commit_all("Ajoute quatre");
    let patch = t.run(&["format-patch", "-1", "--stdout"]).into_bytes();
    t.run(&["reset", "-q", "--hard", "HEAD~1"]);
    (t, patch)
}

#[test]
fn a_format_patch_is_recognised_and_applied_as_a_commit_with_am() {
    let (t, patch) = mail_setup();
    assert!(is_mail_patch(&patch));
    open(&t).am_patch(&patch).unwrap();
    assert_eq!(t.run(&["log", "-1", "--format=%s"]).trim(), "Ajoute quatre");
    assert_eq!(t.read("a.txt"), "1\n2\n3\nquatre\n");
    assert_eq!(t.run(&["status", "--porcelain"]), "");
}

#[test]
fn a_plain_diff_is_not_a_mail_patch() {
    let (_t, patch) = setup();
    assert!(!is_mail_patch(&patch));
    assert!(!is_mail_patch(b""));
}

#[test]
fn am_that_fails_leaves_no_operation_in_progress() {
    let (t, patch) = mail_setup();
    t.write("a.txt", "tout autre contenu\n");
    t.commit_all("autre");
    let repo = open(&t);
    let head = t.run(&["rev-parse", "HEAD"]);
    assert!(repo.am_patch(&patch).is_err());
    assert_eq!(repo.operation_state(), OpState::None, "am abandonné proprement");
    assert_eq!(t.run(&["rev-parse", "HEAD"]), head);
}

/// Session `git am` de l'utilisateur : un commit local, puis `am` de deux patchs dont le second est en conflit.
/// Renvoie le dépôt, le contenu d'un patch à part et le HEAD au milieu de la session.
fn am_session() -> (TestRepo, Vec<u8>, String) {
    let t = TestRepo::new();
    t.write("a.txt", "1\n2\n3\n");
    t.write("b.txt", "b\n");
    t.commit_all("base");
    t.write("a.txt", "x\n2\n3\n");
    t.commit_all("un");
    t.write("b.txt", "y\n");
    t.commit_all("deux");
    let dir = tempfile::tempdir().unwrap();
    t.run(&["format-patch", "-2", "-o", dir.path().to_str().unwrap()]);
    let mut files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    t.run(&["reset", "-q", "--hard", "HEAD~2"]);
    t.write("b.txt", "z\n");
    t.commit_all("local");
    let other = std::fs::read(&files[0]).unwrap();
    // Le premier patch s'applique, le second entre en conflit sur b.txt : la session reste ouverte.
    assert!(t.try_run(&["am", files[0].to_str().unwrap(), files[1].to_str().unwrap()]).is_err());
    let head = t.run(&["rev-parse", "HEAD"]);
    (t, other, head)
}

#[test]
fn am_refuses_while_the_user_has_an_am_session_and_leaves_it_alone() {
    let (t, patch, head) = am_session();
    let repo = open(&t);
    assert_eq!(repo.operation_state(), OpState::Rebase { interactive: false });
    assert!(repo.am_patch(&patch).is_err());
    assert_eq!(t.run(&["rev-parse", "HEAD"]), head, "HEAD n'a pas bougé");
    assert!(t.file(".git/rebase-apply").is_dir(), "la session de l'utilisateur est intacte");
}

#[test]
fn the_am_check_needs_the_working_copy_to_match_the_index() {
    let (t, patch) = mail_setup();
    // Modification non indexée du fichier touché : l'index accepte le patch, pas `am`.
    t.write("a.txt", "1\n2\nlocal\n");
    let repo = open(&t);
    repo.patch_check(&patch, ApplyTarget::Index).unwrap();
    assert!(repo.patch_check(&patch, ApplyTarget::IndexAndWorktree).is_err());
    assert!(repo.am_patch(&patch).is_err());
}
