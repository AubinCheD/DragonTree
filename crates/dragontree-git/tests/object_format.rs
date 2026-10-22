//! L1 : dépôts SHA-256 (`extensions.objectFormat=sha256`). DragonTree ne gère que des identifiants SHA-1 de
//! 20 octets : ouvrir ou cloner un tel dépôt donne une erreur explicite et traduite, sans panique, et un identifiant
//! de 64 caractères n'est jamais lu (ni tronqué) comme un identifiant SHA-1.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, GitError, LogOptions, NetOptions, Oid, Repo, clone_repo};

/// Dépôt SHA-256 avec un commit sur `main`, ou `None` si le git installé ne sait pas en créer (message).
fn sha256_repo() -> Option<TestRepo> {
    let t = TestRepo { dir: tempfile::tempdir().unwrap(), git: common::git() };
    if !common::git_version::sha256_supported(&t.git) {
        return None;
    }
    t.run(&["init", "-q", "-b", "main", "--object-format=sha256"]);
    t.configure();
    t.write("a.txt", "premier\n");
    t.commit_all("Premier commit");
    Some(t)
}

fn assert_unsupported(err: GitError, path: &std::path::Path) {
    match &err {
        GitError::UnsupportedObjectFormat { format, path: Some(p) } => {
            assert_eq!(format, "sha256");
            assert!(std::path::Path::new(p).ends_with(path.file_name().unwrap()), "{p}");
        }
        other => panic!("attendu UnsupportedObjectFormat, obtenu {other:?}"),
    }
    let message = err.to_string();
    assert!(message.contains("SHA-256") && message.contains("SHA-1"), "message explicite : {message}");
}

#[test]
fn opening_a_sha256_repository_gives_a_clear_error() {
    let Some(t) = sha256_repo() else { return };
    assert_eq!(t.run(&["rev-parse", "--show-object-format"]).trim(), "sha256", "dépôt de test SHA-256");
    let err = Repo::open(&t.git, t.path()).expect_err("dépôt SHA-256 refusé");
    assert_unsupported(err, t.path());
    // Depuis un sous-dossier aussi.
    t.write("sous/b.txt", "b\n");
    assert_unsupported(Repo::open(&t.git, &t.file("sous")).unwrap_err(), t.path());
}

#[test]
fn cloning_a_sha256_repository_gives_the_same_clear_error() {
    let Some(source) = sha256_repo() else { return };
    let work = tempfile::tempdir().unwrap();
    let dest = work.path().join("copie");
    let err = clone_repo(&source.git, &source.url(), &dest, None, false, &NetOptions::default(), &mut |_| {})
        .expect_err("clonage d'un dépôt SHA-256 refusé");
    assert_unsupported(err, &dest);
    assert!(dest.join(".git").exists(), "le moteur laisse le clone sur le disque (l'appelant décide)");
}

/// Revue de L1 : la vérification du format après le clonage ne fait jamais échouer un clonage réussi pour une autre
/// raison. Cas principal : dossier d'un autre propriétaire (partage réseau, disque FAT ou exFAT, dossier d'un autre
/// compte), simulé par `GIT_TEST_ASSUME_DIFFERENT_OWNER` dans l'environnement du clonage (pas celui du processus).
/// Le clone reste en place ; c'est son ouverture qui signale le propriétaire, et l'onglet propose alors « Faire
/// confiance à ce dépôt ».
#[test]
fn a_clone_in_a_folder_of_another_owner_is_kept() {
    let source = TestRepo::new();
    source.write("a.txt", "a\n");
    source.commit_all("Premier commit");
    let work = tempfile::tempdir().unwrap();
    let dest = work.path().join("copie");
    let other_owner = [("GIT_TEST_ASSUME_DIFFERENT_OWNER".to_string(), "1".to_string())];
    let net = NetOptions { env: other_owner.to_vec(), ..NetOptions::default() };

    let cloned = clone_repo(&source.git, source.path().to_str().unwrap(), &dest, None, false, &net, &mut |_| {});
    assert_eq!(cloned, Ok(()), "clonage réussi, gardé");
    assert!(dest.join("a.txt").exists() && dest.join(".git").exists());
    // L'ouverture, elle, voit le propriétaire (même environnement) : erreur qui propose la confiance.
    let opened = source.git.cmd(&dest).envs(&other_owner).args(["rev-parse", "--show-toplevel"]).run();
    assert!(matches!(opened, Err(GitError::DubiousOwnership { .. })), "{opened:?}");
}

/// Les identifiants de 64 caractères ne sont jamais lus comme des identifiants SHA-1 : `Oid::from_hex` les refuse,
/// et les lectures du moteur sur un dépôt SHA-256 (ouvert sans passer par `Repo::open`) échouent ou les ignorent,
/// sans panique.
#[test]
fn sha256_ids_are_refused_by_every_parser_without_panicking() {
    let Some(t) = sha256_repo() else { return };
    let head = t.run(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(head.len(), 64);
    assert_eq!(Oid::from_hex(&head), None, "identifiant SHA-256 refusé");
    assert_eq!(Oid::from_hex(&head[..40]).map(|o| o.to_hex()), Some(head[..40].to_string()), "40 premiers seulement");

    assert!(matches!(Oid::parse(&head), Err(GitError::UnsupportedObjectFormat { path: None, .. })));

    let repo =
        Repo { git: t.git.clone(), workdir: t.path().into(), git_dir: t.file(".git"), common_dir: t.file(".git") };
    // Lectures qui renvoient un résultat : l'identifiant SHA-256 donne l'erreur explicite.
    let log = repo.log(&LogOptions::default(), &CancelToken::new());
    assert!(matches!(log, Err(GitError::UnsupportedObjectFormat { .. })), "{log:?}");
    // Lectures qui ignorent les lignes illisibles : rien n'est lu, et surtout pas un identifiant tronqué.
    let refs = repo.refs().unwrap();
    assert!(refs.locals.is_empty(), "branche lue avec un identifiant tronqué : {:?}", refs.locals);
    let status = repo.status().unwrap();
    assert_eq!(status.head_oid, None);
    let _ = repo.stashes();
    let _ = repo.worktrees();
    // HEAD détachée : son identifiant est lu dans le fichier HEAD, erreur explicite.
    t.run(&["checkout", "-q", "--detach"]);
    assert!(matches!(repo.refs(), Err(GitError::UnsupportedObjectFormat { .. })));
}
