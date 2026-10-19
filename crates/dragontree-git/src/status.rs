use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;

/// État d'un fichier dans l'index ou dans la copie de travail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileState {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
}

/// Nature d'un conflit de fusion (codes XY de `git status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictKind {
    BothModified,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
    BothAdded,
    BothDeleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub orig_path: Option<String>,
    pub index: FileState,
    pub worktree: FileState,
    pub conflict: Option<ConflictKind>,
    pub is_submodule: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub branch: Option<String>,
    pub head_oid: Option<Oid>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<StatusEntry>,
}

impl StatusSnapshot {
    /// Fichiers ayant des modifications indexées (hors conflits).
    pub fn staged(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|e| {
                e.conflict.is_none()
                    && !matches!(e.index, FileState::Unmodified | FileState::Untracked | FileState::Ignored)
            })
            .collect()
    }

    /// Fichiers ayant des modifications non indexées, y compris les non suivis (hors conflits).
    pub fn unstaged(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|e| e.conflict.is_none() && !matches!(e.worktree, FileState::Unmodified | FileState::Ignored))
            .collect()
    }

    /// Fichiers ignorés par git (présents seulement si le statut les a demandés) ; jamais indexés par défaut.
    pub fn ignored(&self) -> Vec<&StatusEntry> {
        self.entries.iter().filter(|e| e.worktree == FileState::Ignored).collect()
    }

    pub fn conflicted(&self) -> Vec<&StatusEntry> {
        self.entries.iter().filter(|e| e.conflict.is_some()).collect()
    }

    pub fn is_clean(&self) -> bool {
        self.entries.iter().all(|e| e.worktree == FileState::Ignored)
    }
}

fn state_from(c: u8) -> FileState {
    match c {
        b'M' => FileState::Modified,
        b'A' => FileState::Added,
        b'D' => FileState::Deleted,
        b'R' => FileState::Renamed,
        b'C' => FileState::Copied,
        b'T' => FileState::TypeChanged,
        _ => FileState::Unmodified,
    }
}

fn conflict_from(xy: &str) -> ConflictKind {
    match xy {
        "AU" => ConflictKind::AddedByUs,
        "UA" => ConflictKind::AddedByThem,
        "DU" => ConflictKind::DeletedByUs,
        "UD" => ConflictKind::DeletedByThem,
        "AA" => ConflictKind::BothAdded,
        "DD" => ConflictKind::BothDeleted,
        _ => ConflictKind::BothModified,
    }
}

/// Arguments de la commande dont [`parse_status_v2`] lit la sortie.
pub const STATUS_ARGS: &[&str] = &["status", "--porcelain=v2", "-z", "--branch", "--untracked-files=all"];

/// Option ajoutée pour lister aussi les fichiers ignorés : un dossier ignoré n'est qu'une entrée (`target/`).
pub const STATUS_IGNORED_ARG: &str = "--ignored=matching";

/// Parse la sortie de `git status --porcelain=v2 -z --branch`.
pub fn parse_status_v2(bytes: &[u8]) -> Result<StatusSnapshot, GitError> {
    let text = String::from_utf8_lossy(bytes);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut snap = StatusSnapshot::default();
    let bad = |rec: &str| GitError::Parse(tr!("enregistrement de status invalide : {rec}", rec = format!("{rec:?}")));

    while let Some(rec) = fields.next() {
        if let Some(header) = rec.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" => snap.head_oid = Oid::from_hex(value),
                "branch.head" if value != "(detached)" => snap.branch = Some(value.to_string()),
                "branch.upstream" => snap.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split_whitespace() {
                        if let Some(n) = part.strip_prefix('+') {
                            snap.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            snap.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }

        let (kind, rest) = rec.split_at(rec.find(' ').ok_or_else(|| bad(rec))?);
        let rest = &rest[1..];
        let entry = match kind {
            "1" | "2" | "u" => {
                // Nombre de champs avant le chemin (le chemin peut contenir des espaces).
                let n = match kind {
                    "1" => 7,
                    "2" => 8,
                    _ => 9,
                };
                let parts: Vec<&str> = rest.splitn(n + 1, ' ').collect();
                if parts.len() != n + 1 || parts[0].len() != 2 || parts[1].len() != 4 {
                    return Err(bad(rec));
                }
                let xy = parts[0].as_bytes();
                let path = parts[n].to_string();
                let is_submodule = parts[1].starts_with('S');
                if kind == "u" {
                    StatusEntry {
                        path,
                        orig_path: None,
                        index: FileState::Modified,
                        worktree: FileState::Modified,
                        conflict: Some(conflict_from(parts[0])),
                        is_submodule,
                    }
                } else {
                    let orig_path =
                        if kind == "2" { Some(fields.next().ok_or_else(|| bad(rec))?.to_string()) } else { None };
                    StatusEntry {
                        path,
                        orig_path,
                        index: state_from(xy[0]),
                        worktree: state_from(xy[1]),
                        conflict: None,
                        is_submodule,
                    }
                }
            }
            "?" | "!" => {
                let state = if kind == "?" { FileState::Untracked } else { FileState::Ignored };
                StatusEntry {
                    path: rest.to_string(),
                    orig_path: None,
                    index: state,
                    worktree: state,
                    conflict: None,
                    is_submodule: false,
                }
            }
            _ => return Err(bad(rec)),
        };
        snap.entries.push(entry);
    }
    Ok(snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391";

    fn parse(s: &str) -> StatusSnapshot {
        parse_status_v2(s.as_bytes()).unwrap()
    }

    #[test]
    fn status_ordinary() {
        let s = parse(&format!("1 .M N... 100644 100644 100644 {H} {H} src/a.rs\0"));
        assert_eq!(s.entries.len(), 1);
        let e = &s.entries[0];
        assert_eq!(e.path, "src/a.rs");
        assert_eq!(e.index, FileState::Unmodified);
        assert_eq!(e.worktree, FileState::Modified);
        assert_eq!(e.conflict, None);
        assert!(!e.is_submodule);
        assert_eq!(s.unstaged().len(), 1);
        assert!(s.staged().is_empty());
    }

    #[test]
    fn status_staged_and_unstaged_same_file() {
        let s = parse(&format!("1 MM N... 100644 100644 100644 {H} {H} b.txt\0"));
        assert_eq!(s.staged().len(), 1);
        assert_eq!(s.unstaged().len(), 1);
    }

    #[test]
    fn status_rename() {
        let s = parse(&format!("2 R. N... 100644 100644 100644 {H} {H} R100 new.rs\0old.rs\0"));
        let e = &s.entries[0];
        assert_eq!(e.path, "new.rs");
        assert_eq!(e.orig_path.as_deref(), Some("old.rs"));
        assert_eq!(e.index, FileState::Renamed);
        assert_eq!(e.worktree, FileState::Unmodified);
    }

    #[test]
    fn status_unmerged() {
        let s = parse(&format!("u UU N... 100644 100644 100644 100644 {H} {H} {H} conflit.txt\0"));
        let e = &s.entries[0];
        assert_eq!(e.path, "conflit.txt");
        assert_eq!(e.conflict, Some(ConflictKind::BothModified));
        assert_eq!(s.conflicted().len(), 1);
        assert!(s.staged().is_empty());
        assert!(s.unstaged().is_empty());
    }

    #[test]
    fn status_conflict_kinds() {
        let cases = [
            ("AU", ConflictKind::AddedByUs),
            ("UA", ConflictKind::AddedByThem),
            ("DU", ConflictKind::DeletedByUs),
            ("UD", ConflictKind::DeletedByThem),
            ("AA", ConflictKind::BothAdded),
            ("DD", ConflictKind::BothDeleted),
        ];
        for (xy, kind) in cases {
            let s = parse(&format!("u {xy} N... 100644 100644 100644 100644 {H} {H} {H} f\0"));
            assert_eq!(s.entries[0].conflict, Some(kind), "{xy}");
        }
    }

    #[test]
    fn status_untracked() {
        let s = parse("? dir/n.txt\0");
        let e = &s.entries[0];
        assert_eq!(e.path, "dir/n.txt");
        assert_eq!(e.worktree, FileState::Untracked);
        assert_eq!(s.unstaged().len(), 1);
        assert!(s.staged().is_empty());
    }

    #[test]
    fn status_submodule_flag() {
        let s = parse(&format!("1 .M SC.. 160000 160000 160000 {H} {H} libs/sub\0"));
        assert!(s.entries[0].is_submodule);
    }

    #[test]
    fn status_branch_headers() {
        let s = parse("# branch.oid (initial)\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0");
        assert_eq!(s.head_oid, None);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!(s.ahead, 2);
        assert_eq!(s.behind, 1);
        assert!(s.is_clean());
    }

    #[test]
    fn status_detached_head() {
        let s = parse(&format!("# branch.oid {H}\0# branch.head (detached)\0"));
        assert_eq!(s.branch, None);
        assert_eq!(s.head_oid, Oid::from_hex(H));
    }

    #[test]
    fn status_paths_with_spaces_and_unicode() {
        let s = parse(&format!("1 .M N... 100644 100644 100644 {H} {H} Peppol - Copie/é tè.cs\0? dossier à/x y.txt\0"));
        assert_eq!(s.entries[0].path, "Peppol - Copie/é tè.cs");
        assert_eq!(s.entries[1].path, "dossier à/x y.txt");
    }

    #[test]
    fn status_ignores_ignored_entries_for_cleanliness() {
        let s = parse("! target/\0");
        assert!(s.is_clean());
        assert!(s.unstaged().is_empty());
    }

    #[test]
    fn status_rejects_garbage() {
        assert!(parse_status_v2(b"1 broken\0").is_err());
    }
}
