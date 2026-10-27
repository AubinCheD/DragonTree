//! Opérations demandées à distance par un hôte (le téléphone de la suite) : lancées par la file de l'onglet comme ses
//! boutons, avec leurs notifications et leurs résultats ; une fusion arrêtée sur un conflit s'affiche dans le bandeau
//! de l'onglet, se résout et se poursuit à distance.

mod common;

use std::path::Path;

use common::{commit_file, git, harness, start, try_git, wait_until};
use dragontree_app::DragonTreeApp;
use dragontree_app::repo::{Refresh, RemoteOp, RepoTab};
use dragontree_git::OpState;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type H = Harness<'static, DragonTreeApp>;

fn configure(dir: &Path) {
    for (k, v) in [
        ("user.name", "Testeur"),
        ("user.email", "t@example.com"),
        ("core.autocrlf", "false"),
        ("commit.gpgsign", "false"),
    ] {
        git(dir, &["config", k, v]);
    }
}

fn tab<'h>(h: &'h H, repo: &Path) -> &'h RepoTab {
    h.state().repo_tab(repo).expect("onglet ouvert")
}

/// Fait tourner l'interface jusqu'à la fin de l'opération `id` ; rend ce qu'elle a donné.
fn outcome(h: &mut H, repo: &Path, id: u64) -> Result<Option<String>, String> {
    wait_until(h, "fin de l'opération", |h| tab(h, repo).outcome(id).is_some());
    tab(h, repo).outcome(id).unwrap().result.clone()
}

fn shows_containing(h: &H, part: &str) -> bool {
    h.query_all_by(|n| common::text_of(n).is_some_and(|t| t.contains(part))).next().is_some()
}

#[test]
fn remote_operations_run_through_the_tab_queue() {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let url = format!("file:///{}", bare.display().to_string().replace('\\', "/"));
    let work = tmp.path().join("travail");
    git(tmp.path(), &["clone", "-q", &url, work.to_str().unwrap()]);
    configure(&work);
    commit_file(&work, "a.txt", "un\n", "premier");
    git(&work, &["push", "-q", "-u", "origin", "main"]);

    let mut h = harness(start(tmp.path()));
    h.step();
    // Ouvert dans un onglet de fond : l'onglet affiché ne change pas.
    h.state_mut().open_repo_tab(&work);
    assert!(h.state().active_repo_dir().is_none(), "l'accueil reste affiché");
    assert!(h.state_mut().queue_operation(&work, RemoteOp::Fetch).is_err(), "le dépôt s'ouvre encore");
    wait_until(&mut h, "dépôt ouvert", |h| tab(h, &work).status.is_some() && tab(h, &work).refs.is_some());
    assert!(h.state_mut().queue_operation(&tmp.path().join("ailleurs"), RemoteOp::Fetch).is_err());

    // Indexer, valider, pousser : par la file de l'onglet, comme les boutons.
    std::fs::write(work.join("a.txt"), "un\ndeux\n").unwrap();
    h.state_mut().refresh_repo(&work, Refresh::ALL);
    let stage = h.state_mut().queue_operation(&work, RemoteOp::Stage(vec!["a.txt".into()])).unwrap();
    assert!(tab(&h, &work).running.iter().any(|o| o.id == stage), "en cours dans l'onglet");
    assert!(outcome(&mut h, &work, stage).is_ok());
    let commit = RemoteOp::Commit { message: "deuxième".into(), amend: false };
    let id = h.state_mut().queue_operation(&work, commit).unwrap();
    assert_eq!(outcome(&mut h, &work, id), Ok(Some("Commit créé".into())));
    assert_eq!(git(&work, &["log", "-1", "--format=%s"]).trim(), "deuxième");
    let id = h.state_mut().queue_operation(&work, RemoteOp::Push { force: false }).unwrap();
    assert!(outcome(&mut h, &work, id).is_ok());
    assert!(try_git(&bare, &["log", "-1", "--format=%s", "main"]).is_some_and(|s| s.trim() == "deuxième"));
    assert!(shows_containing(&h, "main poussée vers origin/main"), "la notification est sur le PC");

    // Une fusion en conflit : elle échoue, et le conflit est celui de DragonTree.
    git(&work, &["checkout", "-q", "-b", "feature"]);
    commit_file(&work, "a.txt", "un\ntrois\n", "côté feature");
    git(&work, &["checkout", "-q", "main"]);
    commit_file(&work, "a.txt", "un\nquatre\n", "côté main");
    h.state_mut().refresh_repo(&work, Refresh::ALL);
    wait_until(&mut h, "branche feature vue", |h| {
        tab(h, &work).refs.as_ref().is_some_and(|r| r.locals.iter().any(|b| b.name == "feature"))
    });
    let id = h.state_mut().queue_operation(&work, RemoteOp::Merge("feature".into())).unwrap();
    assert!(outcome(&mut h, &work, id).is_err(), "conflit");
    wait_until(&mut h, "fusion en cours, un conflit", |h| {
        let t = tab(h, &work);
        t.op_state == OpState::Merge && t.status.as_ref().is_some_and(|s| s.conflicted().len() == 1)
    });
    h.state_mut().show_repo(&work);
    wait_until(&mut h, "bandeau de la fusion", |h| shows_containing(h, "en conflit"));
    assert!(h.state_mut().queue_operation(&work, RemoteOp::Skip).is_err(), "pas un rebase");

    // Résolue à distance (version de main), puis poursuivie : le commit de fusion est créé.
    let resolve = RemoteOp::Resolve { paths: vec!["a.txt".into()], ours: true };
    let id = h.state_mut().queue_operation(&work, resolve).unwrap();
    assert!(outcome(&mut h, &work, id).is_ok());
    wait_until(&mut h, "plus de conflit", |h| tab(h, &work).status.as_ref().is_some_and(|s| s.conflicted().is_empty()));
    let id = h.state_mut().queue_operation(&work, RemoteOp::Continue).unwrap();
    assert!(outcome(&mut h, &work, id).is_ok());
    wait_until(&mut h, "fusion terminée", |h| tab(h, &work).op_state == OpState::None);
    assert_eq!(git(&work, &["log", "-1", "--format=%p"]).split_whitespace().count(), 2, "commit de fusion");
    assert_eq!(std::fs::read_to_string(work.join("a.txt")).unwrap(), "un\nquatre\n");
    assert!(h.state_mut().queue_operation(&work, RemoteOp::Continue).is_err(), "plus rien en cours");
    assert!(!h.state().cancel_operation(&work, 999), "rien à annuler");
}
