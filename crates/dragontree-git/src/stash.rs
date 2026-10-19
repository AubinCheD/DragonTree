use crate::oid::Oid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub oid: Oid,
    pub message: String,
    pub time: i64,
}

pub const STASH_FORMAT: &str = "%H%x1f%gd%x1f%ct%x1f%gs";

/// Parse `git stash list -z --format=STASH_FORMAT`.
pub fn parse_stash_list(out: &str) -> Vec<StashEntry> {
    out.split('\0')
        .map(|r| r.trim_start_matches('\n'))
        .filter(|r| !r.trim().is_empty())
        .filter_map(|r| {
            let f: Vec<&str> = r.splitn(4, '\x1f').collect();
            if f.len() != 4 {
                return None;
            }
            let index = f[1].rsplit_once("@{")?.1.strip_suffix('}')?.parse().ok()?;
            Some(StashEntry {
                index,
                oid: Oid::from_hex(f[0].trim())?,
                message: f[3].trim_end().to_string(),
                time: f[2].parse().unwrap_or(0),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stash_list_parses_entries() {
        let a = "1111111111111111111111111111111111111111";
        let b = "2222222222222222222222222222222222222222";
        let out = format!(
            "{a}\x1fstash@{{0}}\x1f1700000000\x1fOn main: mon message\0{b}\x1fstash@{{1}}\x1f1690000000\x1fWIP on feature/x: abc1234 sujet\0"
        );
        let v = parse_stash_list(&out);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].index, 0);
        assert_eq!(v[0].message, "On main: mon message");
        assert_eq!(v[0].time, 1_700_000_000);
        assert_eq!(v[1].index, 1);
        assert_eq!(v[1].oid, Oid::from_hex(b).unwrap());
    }

    #[test]
    fn stash_list_empty() {
        assert!(parse_stash_list("").is_empty());
    }
}
