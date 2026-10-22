use std::sync::Mutex;

mod common;

use common::git;
use dragontree_git::runner::{CommandRecord, set_command_observer};

static SEEN: Mutex<Vec<CommandRecord>> = Mutex::new(Vec::new());

#[test]
fn observer_receives_every_command() {
    assert!(set_command_observer(|r| SEEN.lock().unwrap().push(r.clone())));
    let dir = tempfile::tempdir().unwrap();
    let git = git();
    git.cmd(dir.path()).args(["init", "-q"]).run().unwrap();
    let _ = git.cmd(dir.path()).read_only().args(["rev-parse", "--verify", "nope"]).run();
    let cut = git.cmd(dir.path()).read_only().arg("--version").stdout_limit(4).run().unwrap();
    assert!(cut.stdout_truncated);

    let seen = SEEN.lock().unwrap();
    // Arrêtée à la limite de sa sortie : notée comme une annulation, avec la raison.
    // (`Git::discover` lance aussi `git --version` : la dernière est la nôtre.)
    let stopped = seen.iter().rfind(|r| r.display == "git --version").expect("commande coupée enregistrée");
    assert!(stopped.cancelled && stopped.code.is_none());
    assert!(stopped.stderr.contains("4 octets"), "{}", stopped.stderr);
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
