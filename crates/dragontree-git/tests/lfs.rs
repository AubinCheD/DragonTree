//! Git LFS : détection, installation locale, suivi de motifs (`.gitattributes`), pointeur dans l'objet commité,
//! fichiers LFS d'une liste, prune hors ligne. Les tests s'arrêtent proprement quand `git lfs` est absent. Les verrous
//! demandent un serveur : leur analyse est testée sur des fixtures, dans le module.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, GitError, NetOptions, Repo, lfs_available};

/// Dépôt avec un premier commit, si git LFS est installé ; sinon le test s'arrête avec un message.
fn lfs_repo() -> Option<(TestRepo, Repo)> {
    let t = TestRepo::new();
    if !lfs_available(&t.git) {
        eprintln!("git lfs est absent : test ignoré");
        return None;
    }
    t.write("a.txt", "1\n");
    t.commit_all("Premier commit");
    let repo = Repo::open(&t.git, t.path()).unwrap();
    Some((t, repo))
}

const POINTER_HEADER: &str = "version https://git-lfs.github.com/spec/v1";

#[test]
fn a_missing_git_lfs_is_detected_as_unavailable() {
    // Un exécutable inexistant : ni git ni git lfs.
    let git = dragontree_git::Git::with_exe(std::path::PathBuf::from("dragontree-no-such-git"));
    assert!(!lfs_available(&git));
}

#[test]
fn git_lfs_is_detected_when_installed() {
    let t = TestRepo::new();
    let expected = t.try_run(&["lfs", "version"]).is_ok();
    assert_eq!(lfs_available(&t.git), expected);
    if expected {
        assert!(t.git.lfs_version().unwrap().starts_with("git-lfs/"));
    }
}

#[test]
fn install_local_writes_the_filter_in_the_repository_configuration_only() {
    let Some((t, repo)) = lfs_repo() else { return };
    repo.lfs_install_local().unwrap();
    assert!(t.run(&["config", "--local", "--get", "filter.lfs.clean"]).contains("git-lfs"));
    assert!(t.run(&["config", "--local", "--get", "filter.lfs.required"]).trim() == "true");
    // Les crochets sont ceux du dépôt.
    assert!(t.file(".git/hooks/pre-push").exists());
}

#[test]
fn tracking_adds_a_pattern_to_gitattributes_and_untracking_removes_it() {
    let Some((t, repo)) = lfs_repo() else { return };
    assert!(repo.lfs_patterns().unwrap().is_empty());
    repo.lfs_track("*.bin").unwrap();
    repo.lfs_track("assets/**/*.psd").unwrap();
    assert!(t.read(".gitattributes").contains("*.bin filter=lfs"));
    assert_eq!(repo.lfs_patterns().unwrap(), ["*.bin", "assets/**/*.psd"]);
    repo.lfs_untrack("*.bin").unwrap();
    assert_eq!(repo.lfs_patterns().unwrap(), ["assets/**/*.psd"]);
    assert!(!t.read(".gitattributes").contains("*.bin"));
}

#[test]
fn an_option_looking_pattern_is_refused() {
    let Some((_t, repo)) = lfs_repo() else { return };
    assert!(matches!(repo.lfs_track("--help"), Err(GitError::Refused(_))));
    assert!(matches!(repo.lfs_track("  "), Err(GitError::Refused(_))));
}

#[test]
fn a_tracked_file_is_committed_as_a_pointer_and_stays_whole_in_the_worktree() {
    let Some((t, repo)) = lfs_repo() else { return };
    repo.lfs_install_local().unwrap();
    repo.lfs_track("*.bin").unwrap();
    t.write("data.bin", "contenu volumineux\n");
    t.commit_all("Ajoute un fichier LFS");
    let stored = t.run(&["cat-file", "blob", "HEAD:data.bin"]);
    assert!(stored.starts_with(POINTER_HEADER), "pointeur attendu dans l'objet : {stored}");
    assert!(stored.contains("oid sha256:"));
    assert_eq!(t.read("data.bin"), "contenu volumineux\n");
    // Un fichier non suivi par un motif reste un objet ordinaire.
    t.write("plain.txt", "texte\n");
    t.commit_all("Fichier ordinaire");
    assert_eq!(t.run(&["cat-file", "blob", "HEAD:plain.txt"]), "texte\n");
}

#[test]
fn lfs_files_names_the_paths_handled_by_lfs() {
    let Some((t, repo)) = lfs_repo() else { return };
    // Sans motif LFS : rien, et aucune commande nécessaire.
    assert!(repo.lfs_files(&["a.txt".to_string()]).is_empty());
    repo.lfs_track("*.bin").unwrap();
    t.write("x.bin", "x\n");
    t.write("sub/y.bin", "y\n");
    t.write("z.txt", "z\n");
    let paths: Vec<String> = ["x.bin", "sub/y.bin", "z.txt"].iter().map(|p| p.to_string()).collect();
    let found = repo.lfs_files(&paths);
    assert!(found.contains("x.bin") && found.contains("sub/y.bin"));
    assert!(!found.contains("z.txt"));
    assert!(repo.lfs_files(&[]).is_empty());
}

#[test]
fn prune_runs_offline_without_failing() {
    let Some((_t, repo)) = lfs_repo() else { return };
    repo.lfs_install_local().unwrap();
    let n = NetOptions { cancel: CancelToken::new(), ..Default::default() };
    repo.lfs_prune(&n, &mut |_| {}).unwrap();
}

#[test]
fn a_cancelled_lfs_command_is_reported_as_cancelled() {
    let Some((_t, repo)) = lfs_repo() else { return };
    let cancel = CancelToken::new();
    cancel.cancel();
    let n = NetOptions { cancel, ..Default::default() };
    assert_eq!(repo.lfs_prune(&n, &mut |_| {}).unwrap_err(), GitError::Cancelled);
}
