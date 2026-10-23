//! Jetons d'accès dans le Gestionnaire d'identifiants Windows : ceux de toute la suite EyrieSuite (service
//! « EyrieSuite », utilisateur « account:<id> »), communs à Eyrie, DragonTree et Owlery. L'ancien service « DragonTree »
//! est encore relu, pour un compte pas encore repris. Les jetons ne sont jamais écrits ailleurs.

use std::sync::{Arc, PoisonError, RwLock};

use dragontree_i18n::tr;
use suite_accounts::{OsVault, Vault};

use crate::model::ForgeError;

/// Service des jetons de DragonTree avant les comptes communs de la suite.
const LEGACY_SERVICE: &str = "DragonTree";

/// Coffres qui remplacent ceux de Windows pour tout le processus (`use_vaults`) : celui de la suite, puis l'ancien.
type Vaults = (Arc<dyn Vault>, Arc<dyn Vault>);
static REPLACED: RwLock<Option<Vaults>> = RwLock::new(None);

/// Remplace les coffres de Windows (celui de la suite et l'ancien de DragonTree) pour tout le processus : les essais
/// n'y touchent jamais.
pub fn use_vaults(suite: Arc<dyn Vault>, legacy: Arc<dyn Vault>) {
    *REPLACED.write().unwrap_or_else(PoisonError::into_inner) = Some((suite, legacy));
}

/// Les coffres de Windows sont ceux qui servent (pas de `use_vaults`).
pub fn uses_windows_vaults() -> bool {
    REPLACED.read().unwrap_or_else(PoisonError::into_inner).is_none()
}

/// Le coffre de la suite : celui de Windows (service « EyrieSuite »), sauf remplacement (`use_vaults`).
pub fn suite_vault() -> Arc<dyn Vault> {
    match &*REPLACED.read().unwrap_or_else(PoisonError::into_inner) {
        Some((suite, _)) => suite.clone(),
        None => Arc::new(OsVault::suite()),
    }
}

fn legacy_vault() -> Arc<dyn Vault> {
    match &*REPLACED.read().unwrap_or_else(PoisonError::into_inner) {
        Some((_, legacy)) => legacy.clone(),
        None => Arc::new(OsVault::new(LEGACY_SERVICE)),
    }
}

/// Enregistre (ou remplace) le jeton du compte.
pub fn store_token(account_id: &str, token: &str) -> Result<(), ForgeError> {
    store_in(&*suite_vault(), account_id, token)
}

/// Jeton du compte, `None` s'il n'y en a pas.
pub fn load_token(account_id: &str) -> Result<Option<String>, ForgeError> {
    load_from(&*suite_vault(), &*legacy_vault(), account_id)
}

/// Supprime le jeton du compte (sans erreur s'il n'existe pas), dans les deux coffres.
pub fn delete_token(account_id: &str) -> Result<(), ForgeError> {
    delete_from(&*suite_vault(), &*legacy_vault(), account_id)
}

/// Jeton d'un compte dans l'ancien coffre de DragonTree, pour la reprise de ses comptes par la suite.
pub fn legacy_token(account_id: &str) -> Result<Option<String>, ForgeError> {
    legacy_vault().get(checked(account_id)?).map_err(ForgeError::Secret)
}

fn checked(account_id: &str) -> Result<&str, ForgeError> {
    let id = account_id.trim();
    if id.is_empty() {
        return Err(ForgeError::InvalidInput(tr!("identifiant de compte vide").into()));
    }
    Ok(id)
}

fn store_in(suite: &dyn Vault, account_id: &str, token: &str) -> Result<(), ForgeError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ForgeError::InvalidInput(tr!("le jeton est vide").into()));
    }
    suite.set(checked(account_id)?, token).map_err(ForgeError::Secret)
}

fn load_from(suite: &dyn Vault, legacy: &dyn Vault, account_id: &str) -> Result<Option<String>, ForgeError> {
    let id = checked(account_id)?;
    match suite.get(id).map_err(ForgeError::Secret)? {
        Some(token) => Ok(Some(token)),
        None => legacy.get(id).map_err(ForgeError::Secret),
    }
}

fn delete_from(suite: &dyn Vault, legacy: &dyn Vault, account_id: &str) -> Result<(), ForgeError> {
    let id = checked(account_id)?;
    suite.delete(id).map_err(ForgeError::Secret)?;
    legacy.delete(id).map_err(ForgeError::Secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_tokens_are_still_read_and_new_ones_go_to_the_suite() {
        use suite_accounts::{MemoryVault, Vault};
        let (suite, legacy) = (MemoryVault::default(), MemoryVault::default());
        legacy.set("0123456789abcdef", "ancien").unwrap();
        assert_eq!(load_from(&suite, &legacy, "0123456789abcdef").unwrap().as_deref(), Some("ancien"));
        store_in(&suite, "0123456789abcdef", " nouveau ").unwrap();
        assert_eq!(suite.get("0123456789abcdef").unwrap().as_deref(), Some("nouveau"));
        assert_eq!(load_from(&suite, &legacy, "0123456789abcdef").unwrap().as_deref(), Some("nouveau"));
        delete_from(&suite, &legacy, "0123456789abcdef").unwrap();
        assert_eq!(load_from(&suite, &legacy, "0123456789abcdef").unwrap(), None, "les deux coffres sont vidés");
    }

    #[test]
    fn secrets_reject_empty_values_before_touching_the_store() {
        assert!(matches!(store_token("", "jeton"), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(store_token("0123456789abcdef", " \n"), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(load_token("  "), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(delete_token(""), Err(ForgeError::InvalidInput(_))));
    }
}
