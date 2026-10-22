use std::time::{Duration, Instant};

mod common;

use common::git;
use dragontree_git::{CancelToken, GitError};

#[test]
fn runner_reports_version() {
    let v = git().version().unwrap();
    assert!(v.starts_with("git version"), "{v}");
}

#[test]
fn runner_failure_has_stderr() {
    let dir = tempfile::tempdir().unwrap();
    git().cmd(dir.path()).args(["init", "-q"]).run().unwrap();
    let err = git().cmd(dir.path()).args(["rev-parse", "--verify", "nope"]).run().unwrap_err();
    match err {
        GitError::Failed { code, stderr, cmd } => {
            assert_ne!(code, 0);
            assert!(!stderr.trim().is_empty());
            assert!(cmd.contains("rev-parse --verify nope"), "{cmd}");
        }
        other => panic!("attendu Failed, obtenu {other:?}"),
    }
}

#[test]
fn runner_allows_extra_codes() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
    let out = git()
        .cmd(dir.path())
        .args(["diff", "--no-index", "--no-color", "a.txt", "b.txt"])
        .allow_codes(&[1])
        .run()
        .unwrap();
    assert_eq!(out.code, 1);
    assert!(out.stdout_str().contains("+b"));
}

#[test]
fn runner_passes_stdin() {
    let dir = tempfile::tempdir().unwrap();
    git().cmd(dir.path()).args(["init", "-q"]).run().unwrap();
    let out = git().cmd(dir.path()).args(["hash-object", "--stdin"]).stdin(b"hello\n".to_vec()).run().unwrap();
    assert_eq!(out.stdout_str().trim(), "ce013625030ba8dba906f756967f9e9ca394464a");
}

#[test]
fn runner_streams_stdout_chunks() {
    let dir = tempfile::tempdir().unwrap();
    git().cmd(dir.path()).args(["init", "-q"]).run().unwrap();
    let mut seen = Vec::new();
    let mut lines = Vec::new();
    let out = git()
        .cmd(dir.path())
        .args(["rev-parse", "--git-dir"])
        .run_streaming(&mut |c| seen.extend_from_slice(c), &mut |l| lines.push(l.to_string()))
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&seen).trim(), ".git");
    assert!(out.stdout.is_empty(), "le streaming ne doit pas dupliquer stdout");
}

#[test]
fn runner_cancel_kills_process_tree() {
    let dir = tempfile::tempdir().unwrap();
    let token = CancelToken::new();
    let t2 = token.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        t2.cancel();
    });
    let start = Instant::now();
    // L'alias lance `sh -c "sleep 30"` : un petit-enfant qui garde les tubes ouverts.
    let err = git().cmd(dir.path()).args(["-c", "alias.zz=!sleep 30", "zz"]).cancel(token).run().unwrap_err();
    assert_eq!(err, GitError::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(5), "annulation trop lente : {:?}", start.elapsed());
}

/// RV4a : la limite du job (processus tués à sa fermeture) ne vaut que pendant la commande. Ce que git laisse tourner
/// en partant (démon fsmonitor, outil lancé par `git mergetool`, navigateur du Gestionnaire d'identifiants) lui
/// survit ; seuls l'annulation, la limite de sortie et la fin de DragonTree arrêtent une commande en cours.
#[test]
fn a_program_git_leaves_running_outlives_the_command() {
    let dir = tempfile::tempdir().unwrap();
    // L'alias lance un sous-shell en arrière-plan, sans les tubes de sortie, puis rend la main aussitôt.
    let alias = "alias.bg=!(sleep 1; echo fini > marque.txt) > /dev/null 2>&1 &";
    let start = Instant::now();
    git().cmd(dir.path()).args(["-c", alias, "bg"]).run().unwrap();
    assert!(start.elapsed() < Duration::from_secs(10), "la commande n'attend pas son programme");
    let marker = dir.path().join("marque.txt");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !marker.exists() {
        assert!(Instant::now() < deadline, "le programme laissé par git n'a pas survécu à la commande");
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn a_bounded_output_stops_reading_and_stops_git_at_the_limit() {
    const LIMIT: usize = 1024 * 1024;
    let dir = tempfile::tempdir().unwrap();
    // `yes` écrit sans fin (lancé par `sh`, un petit-enfant) : la commande ne se termine que si la lecture s'arrête et
    // que les processus sont arrêtés avec elle.
    let (tx, rx) = std::sync::mpsc::channel();
    let cwd = dir.path().to_path_buf();
    std::thread::spawn(move || {
        let run = git().cmd(&cwd).args(["-c", "alias.endless=!yes dragontree", "endless"]).stdout_limit(LIMIT).run();
        let _ = tx.send(run);
    });
    let out = rx.recv_timeout(Duration::from_secs(60)).expect("sortie sans fin jamais arrêtée").unwrap();
    assert!(out.stdout_truncated, "sortie coupée");
    assert_eq!(out.stdout.len(), LIMIT, "jamais plus que la limite");
    assert!(out.stdout.starts_with(b"dragontree\ndragontree\n"));

    // Sous la limite : sortie entière, rien de coupé.
    let out = git().cmd(dir.path()).arg("--version").stdout_limit(LIMIT).run().unwrap();
    assert!(!out.stdout_truncated && out.stdout_str().starts_with("git version"));
    let exact = git().cmd(dir.path()).arg("--version").run().unwrap().stdout.len();
    let out = git().cmd(dir.path()).arg("--version").stdout_limit(exact).run().unwrap();
    assert!(!out.stdout_truncated, "exactement la limite : rien de coupé");
    let out = git().cmd(dir.path()).arg("--version").stdout_limit(exact - 1).run().unwrap();
    assert!(out.stdout_truncated && out.stdout.len() == exact - 1, "un octet de trop");
}

#[test]
fn display_quotes_args_with_spaces() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = git().cmd(dir.path()).args(["log", "--grep", "two words"]);
    assert_eq!(cmd.display(), "git log --grep \"two words\"");
}
