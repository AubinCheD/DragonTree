use std::sync::Mutex;

use dragontree_git::Git;
use dragontree_git::runner::{CommandRecord, set_command_observer};

static SEEN: Mutex<Vec<CommandRecord>> = Mutex::new(Vec::new());

#[test]
fn observer_receives_every_command() {
    assert!(set_command_observer(|r| SEEN.lock().unwrap().push(r.clone())));
    let dir = tempfile::tempdir().unwrap();
    let git = Git::discover().unwrap();
    git.cmd(dir.path()).args(["init", "-q"]).run().unwrap();
    let _ = git.cmd(dir.path()).read_only().args(["rev-parse", "--verify", "nope"]).run();

    let seen = SEEN.lock().unwrap();
    let init = seen.iter().find(|r| r.display == "git init -q").expect("init enregistré");
    assert_eq!(init.code, Some(0));
    assert!(!init.read_only);
    assert!(!init.cancelled);
    let failed = seen.iter().find(|r| r.display.contains("rev-parse --verify nope")).expect("échec enregistré");
    assert_ne!(failed.code, Some(0));
    assert!(failed.read_only);
    assert!(!failed.stderr.is_empty());
    assert_eq!(failed.cwd, dir.path());
}
