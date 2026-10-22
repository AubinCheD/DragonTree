//! L1 : dépôts SHA-256 (`extensions.objectFormat=sha256`). DragonTree ne gère que des identifiants SHA-1 de
//! 20 octets : ouvrir ou cloner un tel dépôt donne une erreur explicite et traduite, sans panique, et un identifiant
//! de 64 caractères n'est jamais lu (ni tronqué) comme un identifiant SHA-1.

mod common;

use common::TestRepo;
use dragontree_git::{CancelToken, GitError, LogOptions, NetOptions, Oid, Repo, clone_repo};

/// Le git installé sait créer un dépôt SHA-256 (git 2.29 et suivants) ; sinon le test s'arrête en le disant.
fn sha256_supported(t: &TestRepo) -> bool {
    let version = t.git.version().unwrap();
    let mut numbers = version.trim_start_matches("git version ").split('.').map(|n| n.parse::<u32>().unwrap_or(0));
    let ok = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0)) >= (2, 29);
    if !ok {
        eprintln!("{version} ne sait pas créer de dépôt SHA-256 (git 2.29 et suivants) : test ignoré");
    }
    ok
}

/// Dépôt SHA-256 avec un commit sur `main`, ou `None` si le git installé ne sait pas en créer.
fn sha256_repo() -> Option<TestRepo> {
    let t = TestRepo { dir: tempfile::tempdir().unwrap(), git: common::git() };
    if !sha256_supported(&t) {
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
