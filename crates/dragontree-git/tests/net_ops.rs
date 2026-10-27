mod common;

use common::TestRepo;
use dragontree_git::{GitError, NetOptions, Progress, Repo, clone_repo, init_repo};

fn clone_into(remote: &TestRepo, parent: &std::path::Path, name: &str) -> Repo {
    let dest = parent.join(name);
    let mut events: Vec<Progress> = Vec::new();
    clone_repo(&remote.git, &remote.url(), &dest, None, false, &NetOptions::default(), &mut |p| events.push(p))
        .unwrap();
    let repo = Repo::open(&remote.git, &dest).unwrap();
    for (k, v) in
        [("user.name", "Testeur"), ("user.email", "t@e.com"), ("core.autocrlf", "false"), ("commit.gpgsign", "false")]
    {
        remote.git.cmd(&dest).args(["config", k, v]).run().unwrap();
    }
    repo
}

/// Remote bare contenant un premier commit sur main.
fn seeded_remote() -> TestRepo {
    let bare = TestRepo::new_bare();
    let seed = TestRepo::new();
    seed.write("a.txt", "1\n");
    seed.commit_all("init");
    seed.run(&["remote", "add", "origin", &bare.url()]);
    seed.run(&["push", "-q", "origin", "main"]);
    bare
}

#[test]
fn clone_fetch_push_pull() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice dossier");
    let bob = clone_into(&remote, work.path(), "bob");

    std::fs::write(alice.workdir.join("b.txt"), "b\n").unwrap();
    alice.stage(&["b.txt".into()]).unwrap();
    alice.commit("ajout de b", false).unwrap();
    alice.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();

    bob.fetch(None, true, &NetOptions::default(), &mut |_| {}).unwrap();
    let main = bob.refs().unwrap().locals.into_iter().find(|b| b.name == "main").unwrap();
    assert_eq!(main.behind, 1);

    bob.pull("origin", "main", false, &NetOptions::default(), &mut |_| {}).unwrap();
    assert!(bob.workdir.join("b.txt").exists());

    // Nouvelle branche poussée avec suivi.
    bob.create_branch("feature/z", "main", true).unwrap();
    bob.push("origin", "feature/z", "feature/z", true, false, &NetOptions::default(), &mut |_| {}).unwrap();
    let refs = bob.refs().unwrap();
    let fz = refs.locals.iter().find(|b| b.name == "feature/z").unwrap();
    assert_eq!(fz.upstream.as_deref(), Some("origin/feature/z"));

    bob.delete_remote_branch("origin", "feature/z", &NetOptions::default(), &mut |_| {}).unwrap();
    bob.fetch(Some("origin"), true, &NetOptions::default(), &mut |_| {}).unwrap();
    assert!(!bob.refs().unwrap().remotes.iter().any(|r| r.name == "feature/z"));

    bob.create_tag("v1", "HEAD", None).unwrap();
    bob.push_tag("origin", "v1", &NetOptions::default(), &mut |_| {}).unwrap();
    alice.fetch(None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    assert!(alice.refs().unwrap().tags.iter().any(|t| t.name == "v1"));
}

#[test]
fn push_rejected_returns_stderr() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");

    std::fs::write(alice.workdir.join("x"), "a\n").unwrap();
    alice.stage(&["x".into()]).unwrap();
    alice.commit("alice", false).unwrap();
    alice.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();

    std::fs::write(bob.workdir.join("y"), "b\n").unwrap();
    bob.stage(&["y".into()]).unwrap();
    bob.commit("bob", false).unwrap();
    let err = bob.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap_err();
    match &err {
        GitError::Failed { stderr, .. } => assert!(stderr.contains("rejected"), "{stderr}"),
        other => panic!("attendu Failed, obtenu {other:?}"),
    }
    assert!(err.stderr_summary().contains("rejected"));
}

#[test]
fn clone_reports_progress_and_init_works() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let dest = work.path().join("c");
    let mut phases = Vec::new();
    let mut clone = |recurse| {
        clone_repo(&remote.git, &remote.url(), &dest, Some("main"), recurse, &NetOptions::default(), &mut |p| {
            phases.push(p.phase)
        })
    };
    // Avec les sous-modules, seulement si le git installé n'a pas la faille CVE-2024-32002.
    if dragontree_git::net::submodule_clone_is_safe(&remote.git.version().unwrap()) {
        clone(true).unwrap();
    } else {
        assert!(matches!(clone(true), Err(GitError::Refused(_))));
        assert!(!dest.exists(), "rien n'est cloné");
        clone(false).unwrap();
    }
    assert!(dest.join("a.txt").exists());

    let fresh = work.path().join("nouveau dépôt");
    init_repo(&remote.git, &fresh).unwrap();
    assert!(Repo::open(&remote.git, &fresh).is_ok());
}

#[test]
fn clone_bad_url_fails_without_hanging() {
    let git = common::git();
    let work = tempfile::tempdir().unwrap();
    let err = clone_repo(
        &git,
        "file:///Z:/nexiste/pas",
        &work.path().join("x"),
        None,
        false,
        &NetOptions::default(),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
}

#[test]
fn push_all_tags_pushes_every_tag() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    alice.create_tag("t1", "HEAD", None).unwrap();
    alice.create_tag("t2", "HEAD", Some("annoté")).unwrap();
    alice.push_all_tags("origin", &NetOptions::default(), &mut |_| {}).unwrap();
    let tags = remote.run(&["tag", "--list"]);
    assert!(tags.contains("t1") && tags.contains("t2"), "{tags}");
}

#[test]
fn fast_forward_branch_moves_non_checked_out_branch() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");
    std::fs::write(alice.workdir.join("n"), "n\n").unwrap();
    alice.stage(&["n".into()]).unwrap();
    alice.commit("nouveau", false).unwrap();
    alice.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();

    bob.create_branch("autre", "main", true).unwrap();
    bob.fetch(None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    bob.fast_forward_branch("main", "origin/main").unwrap();
    let refs = bob.refs().unwrap();
    let main = refs.locals.iter().find(|b| b.name == "main").unwrap();
    let origin_main = refs.remotes.iter().find(|b| b.full == "origin/main").unwrap();
    assert_eq!(main.oid, origin_main.oid);
    assert_eq!(refs.current_branch(), Some("autre"));
}

#[test]
fn update_submodules_on_repo_without_submodules_is_ok() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    alice.update_submodules(&NetOptions::default(), &mut |_| {}).unwrap();
}

#[test]
fn force_with_lease_refuses_after_background_fetch() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");
    std::fs::write(alice.workdir.join("a"), "a\n").unwrap();
    alice.stage(&["a".into()]).unwrap();
    alice.commit("travail d'alice", false).unwrap();
    alice.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();

    std::fs::write(bob.workdir.join("b"), "b\n").unwrap();
    bob.stage(&["b".into()]).unwrap();
    bob.commit("travail de bob", false).unwrap();
    // Fetch automatique en arrière-plan : origin/main contient maintenant le commit d'alice.
    bob.fetch(None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    let err = bob.push("origin", "main", "main", false, true, &NetOptions::default(), &mut |_| {});
    assert!(err.is_err(), "le push forcé ne doit pas écraser un commit jamais intégré");
    let top = remote.git.cmd(remote.path()).args(["log", "-1", "--format=%s", "main"]).run().unwrap().stdout_str();
    assert_eq!(top.trim(), "travail d'alice");
}

/// Identifiant de `rev` dans le dépôt `dir`.
fn rev_of(remote: &TestRepo, dir: &std::path::Path, rev: &str) -> String {
    remote.git.cmd(dir).args(["rev-parse", rev]).run().unwrap().stdout_str().trim().to_string()
}

/// Ajoute `file` au dernier commit (amend).
fn amend_with(repo: &Repo, file: &str) {
    std::fs::write(repo.workdir.join(file), "modifié\n").unwrap();
    repo.stage(&[file.into()]).unwrap();
    repo.commit("init modifié", true).unwrap();
}

/// Amend d'un commit déjà poussé (L5) : `head_if_pushed`, lu avant l'amend, donne le commit remplacé ; le push
/// forcé dont le bail est fixé à cette valeur remplace le commit sur le remote.
#[test]
fn an_amended_pushed_commit_is_replaced_by_a_push_with_a_pinned_lease() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let replaced = alice.head_if_pushed().unwrap().expect("HEAD est origin/main après le clonage");
    assert_eq!(replaced.to_hex(), rev_of(&remote, remote.path(), "main"));

    amend_with(&alice, "b.txt");
    assert_eq!(alice.head_if_pushed().unwrap(), None, "le commit modifié diverge de origin/main");
    let amended = alice.head_commit().unwrap();
    assert_eq!(amended.to_hex(), rev_of(&remote, &alice.workdir, "HEAD"));
    alice.push_with_lease("origin", amended, "main", replaced, &NetOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(rev_of(&remote, remote.path(), "main"), rev_of(&remote, &alice.workdir, "HEAD"));
    assert_eq!(rev_of(&remote, &alice.workdir, "origin/main"), amended.to_hex(), "référence de suivi à jour");
}

/// Revue finale (B1) : c'est le commit modifié qui est poussé, pas la branche locale, qui a pu bouger depuis l'amend
/// (ici un commit de plus) : la branche distante reçoit le commit modifié, et le commit suivant reste local.
#[test]
fn a_push_with_a_pinned_lease_pushes_the_amended_commit_even_if_the_branch_moved() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let replaced = alice.head_if_pushed().unwrap().unwrap();
    amend_with(&alice, "b.txt");
    let amended = alice.head_commit().unwrap();
    std::fs::write(alice.workdir.join("c.txt"), "c\n").unwrap();
    alice.stage(&["c.txt".into()]).unwrap();
    alice.commit("après l'amend", false).unwrap();

    alice.push_with_lease("origin", amended, "main", replaced, &NetOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(rev_of(&remote, remote.path(), "main"), amended.to_hex());
    assert_ne!(rev_of(&remote, &alice.workdir, "HEAD"), amended.to_hex(), "le commit suivant n'est pas poussé");
}

/// Le bail porte sur la valeur remplacée, pas sur la référence de suivi : un autre clone a poussé, puis un fetch
/// (automatique) a avancé origin/main ; le push forcé est quand même refusé, et le commit de l'autre clone reste.
#[test]
fn a_push_with_a_pinned_lease_is_refused_when_the_remote_moved_even_after_a_fetch() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");
    let replaced = alice.head_if_pushed().unwrap().unwrap();
    amend_with(&alice, "b.txt");
    let amended = alice.head_commit().unwrap();

    std::fs::write(bob.workdir.join("c.txt"), "c\n").unwrap();
    bob.stage(&["c.txt".into()]).unwrap();
    bob.commit("travail de bob", false).unwrap();
    bob.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();
    let bob_head = rev_of(&remote, &bob.workdir, "HEAD");
    alice.fetch(None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(rev_of(&remote, &alice.workdir, "origin/main"), bob_head);

    let err =
        alice.push_with_lease("origin", amended, "main", replaced, &NetOptions::default(), &mut |_| {}).unwrap_err();
    match &err {
        GitError::Failed { stderr, .. } => assert!(stderr.contains("stale info"), "{stderr}"),
        other => panic!("attendu Failed, obtenu {other:?}"),
    }
    assert_eq!(rev_of(&remote, remote.path(), "main"), bob_head, "rien n'est écrasé");
}

/// Pas de valeur à remplacer quand HEAD n'est pas exactement la branche distante suivie.
#[test]
fn head_if_pushed_is_none_unless_head_is_its_upstream() {
    let remote = seeded_remote();
    let work = tempfile::tempdir().unwrap();
    let alice = clone_into(&remote, work.path(), "alice");
    let bob = clone_into(&remote, work.path(), "bob");

    // En avance d'un commit.
    std::fs::write(alice.workdir.join("n"), "n\n").unwrap();
    alice.stage(&["n".into()]).unwrap();
    alice.commit("nouveau", false).unwrap();
    assert_eq!(alice.head_if_pushed().unwrap(), None);
    alice.push("origin", "main", "main", false, false, &NetOptions::default(), &mut |_| {}).unwrap();
    assert!(alice.head_if_pushed().unwrap().is_some());

    // En retard : le commit d'alice est arrivé par un fetch.
    bob.fetch(None, false, &NetOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(bob.head_if_pushed().unwrap(), None);

    // Branche sans branche suivie, puis HEAD détachée.
    bob.create_branch("locale", "main", true).unwrap();
    assert_eq!(bob.head_if_pushed().unwrap(), None);
    remote.git.cmd(&bob.workdir).args(["checkout", "-q", "--detach"]).run().unwrap();
    assert_eq!(bob.head_if_pushed().unwrap(), None);
}

/// Environnement que reçoit le transport d'un fetch (ssh simulé par `core.sshCommand`, qui le note puis échoue) : en
/// arrière-plan, git parle anglais (son message d'authentification refusée se reconnaît) et aucune invite
/// d'identifiants n'est possible ; un fetch visible garde la langue de l'utilisateur.
#[test]
fn a_background_fetch_runs_git_in_english_without_any_credential_prompt() {
    let work = TestRepo::new();
    let slashes = |p: std::path::PathBuf| p.display().to_string().replace('\\', "/");
    let (script, marker) = (slashes(work.file("ssh.sh")), work.file("env.txt"));
    let report = "LC_ALL=$LC_ALL GIT_ASKPASS=[$GIT_ASKPASS] SSH_ASKPASS=[$SSH_ASKPASS] \
                  SSH_ASKPASS_REQUIRE=$SSH_ASKPASS_REQUIRE GCM_INTERACTIVE=$GCM_INTERACTIVE";
    let body = format!("#!/bin/sh\necho \"{report}\" > '{}'\nexit 1\n", slashes(marker.clone()));
    std::fs::write(work.file("ssh.sh"), body).unwrap();
    work.run(&["config", "core.sshCommand", &format!("sh '{script}'")]);
    work.run(&["remote", "add", "origin", "ssh://git@forge.contoso.test/depot.git"]);
    let repo = Repo::open(&work.git, work.path()).unwrap();
    // Langue de l'utilisateur (simulée).
    let fetch = |non_interactive| {
        let env = vec![("LC_ALL".to_string(), "fr_FR.UTF-8".to_string())];
        let n = NetOptions { env, non_interactive, ..NetOptions::default() };
        assert!(repo.fetch(None, true, &n, &mut |_| {}).is_err(), "le transport simulé échoue");
        std::fs::read_to_string(&marker).unwrap()
    };
    let background = fetch(true);
    assert!(background.starts_with("LC_ALL=C "), "{background}");
    let no_prompt = "GIT_ASKPASS=[] SSH_ASKPASS=[] SSH_ASKPASS_REQUIRE=never GCM_INTERACTIVE=never";
    assert!(background.contains(no_prompt), "{background}");
    let visible = fetch(false);
    assert!(visible.starts_with("LC_ALL=fr_FR.UTF-8 "), "{visible}");
}
