//! Version du git installé, pour les tests qui demandent une fonction plus récente que le minimum pris en charge.
//! Fichier commun aux tests du moteur et de l'application (inclus par `#[path]`), comme `global_config` ; module
//! public (`common::git_version::…`) : rien n'est réexporté, pour ne pas laisser d'import inutilisé dans les binaires
//! qui ne s'en servent pas.

#![allow(dead_code)]

use dragontree_git::Git;

/// Le git installé est au moins `major.minor` (`git version 2.38.1.windows.1`).
pub fn git_at_least(git: &Git, major: u32, minor: u32) -> bool {
    let version = git.version().unwrap_or_default();
    let mut numbers = version.trim_start_matches("git version ").split('.').map(|n| n.parse::<u32>().unwrap_or(0));
    (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0)) >= (major, minor)
}

/// Le git installé sait créer un dépôt SHA-256 (git 2.29 et suivants) ; sinon le dit sur la sortie d'erreur, et le
/// test appelant s'arrête là.
pub fn sha256_supported(git: &Git) -> bool {
    let ok = git_at_least(git, 2, 29);
    if !ok {
        let version = git.version().unwrap_or_default();
        eprintln!("{version} ne sait pas créer de dépôt SHA-256 (git 2.29 et suivants) : test ignoré");
    }
    ok
}
