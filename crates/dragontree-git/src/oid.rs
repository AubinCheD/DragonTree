use std::fmt;

/// Identifiant d'objet git (SHA-1, 20 octets). Copiable et compact : 20 octets au lieu
/// d'une chaîne hexadécimale de 40 caractères allouée sur le tas.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Oid(pub [u8; 20]);

impl Oid {
    pub const ZERO: Oid = Oid([0; 20]);

    /// Parse exactement 40 caractères hexadécimaux (casse indifférente).
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

    #[test]
    fn oid_zero() {
        assert!(Oid::ZERO.is_zero());
        assert!(!Oid::from_hex(HEX).unwrap().is_zero());
    }
}
