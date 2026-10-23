//! Les essais n'approchent jamais le coffre de Windows (jetons de la suite et ancien coffre de DragonTree) : le
//! harnais commun les remplace par des coffres en mémoire, pour tout le processus.

mod common;

#[test]
fn tests_never_use_the_windows_vaults() {
    let tmp = tempfile::tempdir().unwrap();
    let _ = common::start(tmp.path());
    assert!(!dragontree_forges::secrets::uses_windows_vaults());
}
