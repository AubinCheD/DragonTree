//! Remotes : modifier l'URL (et l'URL de push distincte), tester la connexion (`ls-remote --heads`).

mod common;

use common::TestRepo;
use dragontree_git::{GitError, NetOptions, Repo};

/// Remote bare contenant `main` et `fonction`.
fn seeded_remote() -> TestRepo {
    let bare = TestRepo::new_bare();
    let seed = TestRepo::new();
    seed.write("a.txt", "1\n");
    seed.commit_all("init");
    seed.run(&["branch", "fonction"]);
    seed.run(&["remote", "add", "origin", &bare.url()]);
    seed.run(&["push", "-q", "origin", "main", "fonction"]);
    bare
}

fn repo_with_origin(url: &str) -> (TestRepo, Repo) {
    let t = TestRepo::new();
    t.run(&["remote", "add", "origin", url]);
    let repo = Repo::open(&t.git, t.path()).unwrap();
    (t, repo)
}

fn origin(repo: &Repo) -> (String, String) {
    let r = repo.remotes().unwrap().into_iter().find(|r| r.name == "origin").unwrap();
    (r.fetch_url, r.push_url)
}

#[test]
fn set_url_changes_the_fetch_url_and_the_push_url_follows() {
    let (_t, repo) = repo_with_origin("https://git.fabrikam.test/ancien.git");
    repo.set_remote_urls("origin", "https://git.fabrikam.test/nouveau.git", None).unwrap();
    assert_eq!(
        origin(&repo),
        ("https://git.fabrikam.test/nouveau.git".into(), "https://git.fabrikam.test/nouveau.git".into())
    );
}

#[test]
fn a_distinct_push_url_is_set_then_removed_when_cleared_or_equal() {
    let (t, repo) = repo_with_origin("https://git.fabrikam.test/depot.git");
    repo.set_remote_urls("origin", "https://git.fabrikam.test/depot.git", Some("ssh://git@fabrikam.test/depot.git"))
        .unwrap();
    assert_eq!(
        origin(&repo),
        ("https://git.fabrikam.test/depot.git".into(), "ssh://git@fabrikam.test/depot.git".into())
    );
    // Vide : plus d'URL de push propre, le push reprend l'URL de lecture.
    repo.set_remote_urls("origin", "https://git.fabrikam.test/depot.git", Some("")).unwrap();
    assert_eq!(origin(&repo).1, "https://git.fabrikam.test/depot.git");
    assert!(t.try_run(&["config", "--get", "remote.origin.pushurl"]).is_err(), "pushurl retirée");
    // Égale à l'URL de lecture : pas de pushurl redondante non plus.
    repo.set_remote_urls("origin", "https://git.fabrikam.test/depot.git", Some("ssh://git@fabrikam.test/x.git"))
        .unwrap();
    repo.set_remote_urls("origin", "https://git.fabrikam.test/depot.git", Some("https://git.fabrikam.test/depot.git"))
        .unwrap();
    assert!(t.try_run(&["config", "--get", "remote.origin.pushurl"]).is_err());
}

#[test]
fn set_url_on_an_unknown_remote_fails() {
    let (_t, repo) = repo_with_origin("https://git.fabrikam.test/depot.git");
    assert!(repo.set_remote_urls("absent", "https://git.fabrikam.test/x.git", None).is_err());
}

#[test]
fn ls_remote_lists_the_heads_of_a_reachable_remote() {
    let bare = seeded_remote();
    let (_t, repo) = repo_with_origin(&bare.url());
    let heads = repo.ls_remote_heads("origin", &NetOptions::default()).unwrap();
    assert_eq!(heads, vec!["fonction".to_string(), "main".to_string()]);
}

#[test]
fn ls_remote_fails_with_git_s_message_when_the_path_is_missing() {
    let missing = tempfile::tempdir().unwrap().path().join("n-existe-pas.git");
    let (_t, repo) = repo_with_origin(&format!("file:///{}", missing.display().to_string().replace('\\', "/")));
    let err = repo.ls_remote_heads("origin", &NetOptions::default()).unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
    assert!(err.to_string().contains("n-existe-pas"), "le message de git est conservé : {err}");
}
