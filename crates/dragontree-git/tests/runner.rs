use std::time::{Duration, Instant};

use dragontree_git::{CancelToken, Git, GitError};

fn git() -> Git {
    Git::discover().expect("git doit être installé pour les tests")
}

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

#[test]
fn display_quotes_args_with_spaces() {
    let dir = tempfile::tempdir().unwrap();
    let cmd = git().cmd(dir.path()).args(["log", "--grep", "two words"]);
    assert_eq!(cmd.display(), "git log --grep \"two words\"");
}
