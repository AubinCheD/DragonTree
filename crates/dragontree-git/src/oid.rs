use std::fmt;

use dragontree_i18n::tr;

use crate::GitError;

/// Identifiant d'objet git (SHA-1, 20 octets). Copiable et compact : 20 octets au lieu
/// d'une chaîne hexadécimale de 40 caractères allouée sur le tas.
///
/// Les dépôts SHA-256 (identifiants de 32 octets, 64 caractères) ne sont pas pris en charge (L1) : `Repo::open` et
/// `clone_repo` les refusent avec `GitError::UnsupportedObjectFormat`, et un identifiant de 64 caractères n'est
/// jamais lu, ni tronqué, comme un identifiant SHA-1.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Oid(pub [u8; 20]);

/// Longueur hexadécimale d'un identifiant SHA-256.
const SHA256_HEX_LEN: usize = 64;

impl Oid {
    pub const ZERO: Oid = Oid([0; 20]);

    /// Comme `from_hex`, avec une erreur explicite : un identifiant SHA-256 (64 caractères hexadécimaux) donne
    /// `GitError::UnsupportedObjectFormat`, toute autre chaîne qui n'est pas un identifiant SHA-1, `GitError::Parse`.
    pub fn parse(s: &str) -> Result<Oid, GitError> {
        Oid::from_hex(s).ok_or_else(|| {
            if s.len() == SHA256_HEX_LEN && s.bytes().all(|b| b.is_ascii_hexdigit()) {
                GitError::UnsupportedObjectFormat { format: "sha256".into(), path: None }
            } else {
                GitError::Parse(tr!("identifiant d'objet invalide : {id}", id = format!("{s:?}")))
            }
        })
    }

    /// Parse exactement 40 caractères hexadécimaux (casse indifférente) ; `None` pour toute autre chaîne, un
    /// identifiant SHA-256 de 64 caractères compris.
    pub fn from_hex(s: &str) -> Option<Oid> {
        let bytes = s.as_bytes();
        if bytes.len() != 40 {
            return None;
        }
        let mut out = [0u8; 20];
        for (i, pair) in bytes.as_chunks::<2>().0.iter().enumerate() {
            out[i] = (hex_val(pair[0])? << 4) | hex_val(pair[1])?;
        }
        Some(Oid(out))
    }

    pub fn to_hex(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(40);
        for b in self.0 {
            s.push(DIGITS[(b >> 4) as usize] as char);
            s.push(DIGITS[(b & 0x0f) as usize] as char);
        }
        s
    }

    /// Les 7 premiers caractères hexadécimaux.
    pub fn short(&self) -> String {
        let mut s = self.to_hex();
        s.truncate(7);
        s
    }

    pub fn is_zero(&self) -> bool {
        self.0 == [0; 20]
    }
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

impl fmt::Display for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Oid({})", self.short())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn oid_hex_roundtrip() {
        let oid = Oid::from_hex(HEX).unwrap();
        assert_eq!(oid.to_hex(), HEX);
        assert_eq!(oid.short(), "0123456");
        assert_eq!(oid.to_string(), HEX);
    }

    #[test]
    fn oid_accepts_uppercase() {
        let oid = Oid::from_hex(&HEX.to_uppercase()).unwrap();
        assert_eq!(oid.to_hex(), HEX);
    }

    #[test]
    fn oid_rejects_bad_input() {
        assert!(Oid::from_hex("").is_none());
        assert!(Oid::from_hex(&HEX[..39]).is_none());
        assert!(Oid::from_hex(&format!("{HEX}0")).is_none());
        assert!(Oid::from_hex("g123456789abcdef0123456789abcdef01234567").is_none());
    }

    /// L1 : un identifiant SHA-256 (64 caractères) n'est jamais lu ni tronqué comme un identifiant SHA-1 ; `parse` le
    /// dit explicitement.
    #[test]
    fn oid_refuses_sha256_ids_with_an_explicit_error() {
        let sha256 = format!("{HEX}{}", &HEX[..24]);
        assert_eq!(sha256.len(), 64);
        assert_eq!(Oid::from_hex(&sha256), None);
        let err = Oid::parse(&sha256).unwrap_err();
        assert_eq!(err, GitError::UnsupportedObjectFormat { format: "sha256".into(), path: None });
        assert!(err.to_string().contains("SHA-256"), "{err}");
        assert_eq!(Oid::parse(HEX).unwrap().to_hex(), HEX);
        assert!(matches!(Oid::parse(&HEX[..39]), Err(GitError::Parse(_))));
        assert!(matches!(Oid::parse(&format!("{}g", &sha256[..63])), Err(GitError::Parse(_))));
    }

    #[test]
    fn oid_zero() {
        assert!(Oid::ZERO.is_zero());
        assert!(!Oid::from_hex(HEX).unwrap().is_zero());
    }
}
