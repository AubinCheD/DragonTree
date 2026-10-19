//! Jetons d'accès dans le Gestionnaire d'identifiants Windows (service « DragonTree »,
//! utilisateur « account:<id> »). Les jetons ne sont jamais écrits ailleurs.

use dragontree_i18n::tr;

use crate::model::ForgeError;

/// Nom du service dans le Gestionnaire d'identifiants.
const SERVICE: &str = "DragonTree";

/// Enregistre (ou remplace) le jeton du compte.
pub fn store_token(account_id: &str, token: &str) -> Result<(), ForgeError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ForgeError::InvalidInput(tr!("le jeton est vide").into()));
    }
    entry(account_id)?.set_password(token).map_err(secret_error)
}

/// Jeton du compte, `None` s'il n'y en a pas.
pub fn load_token(account_id: &str) -> Result<Option<String>, ForgeError> {
    match entry(account_id)?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(secret_error(e)),
    }
}

/// Supprime le jeton du compte (sans erreur s'il n'existe pas).
pub fn delete_token(account_id: &str) -> Result<(), ForgeError> {
    match entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(secret_error(e)),
    }
}

fn entry(account_id: &str) -> Result<keyring::Entry, ForgeError> {
    let id = account_id.trim();
    if id.is_empty() {
        return Err(ForgeError::InvalidInput(tr!("identifiant de compte vide").into()));
    }
    keyring::Entry::new(SERVICE, &format!("account:{id}")).map_err(secret_error)
}

/// Seul le message (`Display`) est conservé : il ne contient jamais le secret.
fn secret_error(e: keyring::Error) -> ForgeError {
    ForgeError::Secret(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_reject_empty_values_before_touching_the_store() {
        assert!(matches!(store_token("", "jeton"), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(store_token("0123456789abcdef", " \n"), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(load_token("  "), Err(ForgeError::InvalidInput(_))));
        assert!(matches!(delete_token(""), Err(ForgeError::InvalidInput(_))));
    }
}
