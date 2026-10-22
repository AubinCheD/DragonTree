#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    /// Texte, sans le préfixe ni le `\n` final, mais avec un éventuel `\r`.
    pub text: String,
    /// La ligne est suivie de `\ No newline at end of file`.
    pub no_newline: bool,
    /// Octets d'origine quand la ligne n'est pas de l'UTF-8 valide (`text` est alors décodé en
    /// Windows-1252 pour l'affichage). Les patchs sont reconstruits à partir de ces octets.
    pub raw: Option<Box<[u8]>>,
}

impl DiffLine {
    /// Octets exacts de la ligne (pour reconstruire un patch sans réencoder).
    pub fn bytes(&self) -> &[u8] {
        self.raw.as_deref().unwrap_or(self.text.as_bytes())
    }
}

/// Décode des octets Windows-1252 (encodage « ANSI » courant des fichiers français).
pub fn decode_cp1252(bytes: &[u8]) -> String {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => HIGH[(b - 0x80) as usize],
            _ => char::from(b),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// Ligne `@@ … @@` complète.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    /// Lignes d'en-tête (`diff --git`, `index`, `---`, `+++`…) pour reconstruire un patch.
    pub header_lines: Vec<String>,
    pub is_binary: bool,
    pub is_new: bool,
    pub is_deleted: bool,
    pub hunks: Vec<Hunk>,
    /// Diff trop volumineux : hunks non chargés.
    pub truncated: bool,
}

impl FileDiff {
    pub fn display_path(&self) -> &str {
        self.new_path.as_deref().or(self.old_path.as_deref()).unwrap_or("")
    }

    pub fn added_count(&self) -> usize {
        self.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == LineKind::Added).count()
    }

    pub fn removed_count(&self) -> usize {
        self.hunks.iter().flat_map(|h| &h.lines).filter(|l| l.kind == LineKind::Removed).count()
    }

    /// Changement de mode du fichier (`old mode` et `new mode` de l'en-tête) : ancien et nouveau mode.
    pub fn mode_change(&self) -> Option<(&str, &str)> {
        let mode = |prefix: &str| self.header_lines.iter().find_map(|l| l.strip_prefix(prefix)).map(str::trim);
        Some((mode("old mode ")?, mode("new mode ")?))
    }
}

/// Taille maximale d'un diff affiché ; au-delà `truncated` est vrai et les hunks sont vides.
pub const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    pub context: u32,
    pub ignore_whitespace: bool,
}

impl Default for DiffOptions {
    fn default() -> Self {
        DiffOptions { context: 3, ignore_whitespace: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: ChangeKind,
}

/// Parse un diff unifié produit par git (un ou plusieurs fichiers).
pub fn parse_unified_diff(text: &str) -> Vec<FileDiff> {
    parse_unified_diff_bytes(text.as_bytes())
}

/// Parse la sortie brute de git : les lignes qui ne sont pas de l'UTF-8 valide gardent leurs
/// octets d'origine (voir [`DiffLine::raw`]).
pub fn parse_unified_diff_bytes(bytes: &[u8]) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    // Lignes restantes du hunk courant (ancien côté, nouveau côté).
    let mut remaining = (0u32, 0u32);
    let mut cursor = (0u32, 0u32);

    for raw in bytes.split_inclusive(|&b| b == b'\n') {
        let raw_line = raw.strip_suffix(b"\n").unwrap_or(raw);
        let (decoded, not_utf8) = match std::str::from_utf8(raw_line) {
            Ok(s) => (std::borrow::Cow::Borrowed(s), false),
            Err(_) => (std::borrow::Cow::Owned(decode_cp1252(raw_line)), true),
        };
        let line: &str = &decoded;

        // Dans un hunk, les lignes sont identifiées par leur préfixe tant qu'il reste des
        // lignes à lire : « --- x » y est une ligne supprimée, pas un en-tête.
        let in_hunk = remaining != (0, 0);
        if let Some(hunk) = files.last_mut().and_then(|f| f.hunks.last_mut()).filter(|_| in_hunk) {
            let kind = match line.as_bytes().first() {
                Some(b'+') => Some(LineKind::Added),
                Some(b'-') => Some(LineKind::Removed),
                // Une ligne vide sans préfixe est un contexte vide (espace supprimé par un outil).
                Some(b' ') | None => Some(LineKind::Context),
                _ => None,
            };
            if let Some(kind) = kind {
                let body = line.get(1..).unwrap_or("");
                let (old_no, new_no) = match kind {
                    LineKind::Context => {
                        let n = (Some(cursor.0), Some(cursor.1));
                        cursor = (cursor.0 + 1, cursor.1 + 1);
                        remaining = (remaining.0.saturating_sub(1), remaining.1.saturating_sub(1));
                        n
                    }
                    LineKind::Removed => {
                        let n = (Some(cursor.0), None);
                        cursor.0 += 1;
                        remaining.0 = remaining.0.saturating_sub(1);
                        n
                    }
                    LineKind::Added => {
                        let n = (None, Some(cursor.1));
                        cursor.1 += 1;
                        remaining.1 = remaining.1.saturating_sub(1);
                        n
                    }
                };
                let raw = not_utf8.then(|| raw_line.get(1..).unwrap_or_default().to_vec().into_boxed_slice());
                hunk.lines.push(DiffLine { kind, old_no, new_no, text: body.to_string(), no_newline: false, raw });
                continue;
            }
            if !line.starts_with('\\') {
                remaining = (0, 0);
            }
        }

        if line.starts_with('\\') {
            // Marqueur arrivant juste après la dernière ligne du hunk.
            if let Some(last) = files.last_mut().and_then(|f| f.hunks.last_mut()).and_then(|h| h.lines.last_mut()) {
                last.no_newline = true;
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("diff --git ") {
            let (old, new) = split_git_header_paths(rest);
            files.push(FileDiff {
                old_path: old,
                new_path: new,
                header_lines: vec![line.to_string()],
                ..Default::default()
            });
            continue;
        }

        let Some(file) = files.last_mut() else { continue };

        if line.starts_with("@@") {
            if let Some(hunk) = parse_hunk_header(line) {
                remaining = (hunk.old_count, hunk.new_count);
                cursor = (hunk.old_start, hunk.new_start);
                file.hunks.push(hunk);
            }
            continue;
        }

        if !file.hunks.is_empty() {
            continue;
        }
        file.header_lines.push(line.to_string());
        if line.starts_with("new file mode") {
            file.is_new = true;
            file.old_path = None;
        } else if line.starts_with("deleted file mode") {
            file.is_deleted = true;
            file.new_path = None;
        } else if let Some(p) = line.strip_prefix("rename from ").or_else(|| line.strip_prefix("copy from ")) {
            file.old_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("rename to ").or_else(|| line.strip_prefix("copy to ")) {
            file.new_path = Some(unquote(p));
        } else if let Some(p) = line.strip_prefix("--- ") {
            file.old_path = header_path(p, "a/");
        } else if let Some(p) = line.strip_prefix("+++ ") {
            file.new_path = header_path(p, "b/");
        } else if line.starts_with("Binary files ") || line.starts_with("GIT binary patch") {
            file.is_binary = true;
        }
    }
    files
}

fn parse_hunk_header(line: &str) -> Option<Hunk> {
    // @@ -a[,b] +c[,d] @@ contexte
    let body = line.strip_prefix("@@ ")?;
    let end = body.find(" @@")?;
    let mut parts = body[..end].split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let range = |s: &str| -> Option<(u32, u32)> {
        match s.split_once(',') {
            Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
            None => Some((s.parse().ok()?, 1)),
        }
    };
    let (old_start, old_count) = range(old)?;
    let (new_start, new_count) = range(new)?;
    Some(Hunk { old_start, old_count, new_start, new_count, header: line.to_string(), lines: Vec::new() })
}

/// Chemin d'une ligne `---`/`+++` : `/dev/null` → None, préfixe retiré, tabulation finale ignorée.
fn header_path(raw: &str, prefix: &str) -> Option<String> {
    let raw = raw.strip_suffix('\t').unwrap_or(raw);
    if raw == "/dev/null" {
        return None;
    }
    let p = unquote(raw);
    Some(p.strip_prefix(prefix).map(str::to_string).unwrap_or(p))
}

/// Chemins de la ligne `diff --git a/X b/Y` (ambiguë avec des espaces : on privilégie X == Y).
fn split_git_header_paths(rest: &str) -> (Option<String>, Option<String>) {
    if rest.starts_with('"') {
        // Chemins entre guillemets : séparés par l'espace qui suit le guillemet fermant.
        if let Some(close) = find_closing_quote(rest) {
            let a = unquote(&rest[..=close]);
            let b = unquote(rest[close + 1..].trim_start());
            return (a.strip_prefix("a/").map(str::to_string), b.strip_prefix("b/").map(str::to_string));
        }
    }
    let n = rest.len();
    if n % 2 == 1 {
        let mid = n / 2;
        if rest.is_char_boundary(mid) && rest.as_bytes()[mid] == b' ' {
            let (a, b) = (&rest[..mid], &rest[mid + 1..]);
            if let (Some(a), Some(b)) = (a.strip_prefix("a/"), b.strip_prefix("b/"))
                && a == b
            {
                return (Some(a.to_string()), Some(b.to_string()));
            }
        }
    }
    match rest.rfind(" b/") {
        Some(pos) => (rest[..pos].strip_prefix("a/").map(str::to_string), Some(rest[pos + 3..].to_string())),
        None => (None, None),
    }
}

fn find_closing_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Retire les guillemets C de git (`"caf\303\251"` → `café`).
fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let bytes = inner.as_bytes();
    let is_octal = |b: u8| (b'0'..=b'7').contains(&b);
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let c = bytes[i + 1];
            if i + 3 < bytes.len() && bytes[i + 1..=i + 3].iter().all(|&b| is_octal(b)) {
                let v = u32::from(bytes[i + 1] - b'0') * 64
                    + u32::from(bytes[i + 2] - b'0') * 8
                    + u32::from(bytes[i + 3] - b'0');
                out.push((v & 0xff) as u8);
                i += 4;
                continue;
            }
            out.push(match c {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'a' => 7,
                b'b' => 8,
                b'f' => 12,
                b'v' => 11,
                other => other,
            });
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse la sortie de `git diff-tree -r -z --name-status`.
pub fn parse_name_status_z(bytes: &[u8]) -> Vec<FileChange> {
    let text = String::from_utf8_lossy(bytes);
    let mut fields = text.split('\0').filter(|f| !f.is_empty());
    let mut out = Vec::new();
    while let Some(status) = fields.next() {
        let kind = match status.as_bytes().first() {
            Some(b'A') => ChangeKind::Added,
            Some(b'D') => ChangeKind::Deleted,
            Some(b'R') => ChangeKind::Renamed,
            Some(b'C') => ChangeKind::Copied,
            Some(b'T') => ChangeKind::TypeChanged,
            Some(b'U') => ChangeKind::Unmerged,
            _ => ChangeKind::Modified,
        };
        let Some(first) = fields.next() else { break };
        if matches!(kind, ChangeKind::Renamed | ChangeKind::Copied) {
            let Some(second) = fields.next() else { break };
            out.push(FileChange { path: second.to_string(), old_path: Some(first.to_string()), kind });
        } else {
            out.push(FileChange { path: first.to_string(), old_path: None, kind });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> FileDiff {
        let mut v = parse_unified_diff(text);
        assert_eq!(v.len(), 1, "un seul fichier attendu");
        v.remove(0)
    }

    /// Assemble un diff ligne par ligne (les continuations `\` de Rust mangent les espaces de tête).
    fn lines(l: &[&str]) -> String {
        let mut s = l.join("\n");
        s.push('\n');
        s
    }

    #[test]
    fn diff_modified_two_hunks() {
        let d = one(&lines(&[
            "diff --git a/src/a.rs b/src/a.rs",
            "index 1111111..2222222 100644",
            "--- a/src/a.rs",
            "+++ b/src/a.rs",
            "@@ -1,3 +1,3 @@ fn main() {",
            " un",
            "-deux",
            "+DEUX",
            " trois",
            "@@ -10,2 +10,3 @@",
            " dix",
            "+dix bis",
            " onze",
        ]));
        assert_eq!(d.old_path.as_deref(), Some("src/a.rs"));
        assert_eq!(d.new_path.as_deref(), Some("src/a.rs"));
        assert_eq!(d.header_lines.len(), 4);
        assert_eq!(d.hunks.len(), 2);
        let h = &d.hunks[0];
        assert_eq!((h.old_start, h.old_count, h.new_start, h.new_count), (1, 3, 1, 3));
        assert_eq!(h.header, "@@ -1,3 +1,3 @@ fn main() {");
        assert_eq!(h.lines.len(), 4);
        assert_eq!(
            h.lines[1],
            DiffLine {
                kind: LineKind::Removed,
                old_no: Some(2),
                new_no: None,
                text: "deux".into(),
                no_newline: false,
                raw: None
            }
        );
        assert_eq!(
            h.lines[2],
            DiffLine {
                kind: LineKind::Added,
                old_no: None,
                new_no: Some(2),
                text: "DEUX".into(),
                no_newline: false,
                raw: None
            }
        );
        assert_eq!(h.lines[3].old_no, Some(3));
        assert_eq!(h.lines[3].new_no, Some(3));
        let h2 = &d.hunks[1];
        assert_eq!(h2.lines[1].new_no, Some(11));
        assert_eq!(h2.lines[2].old_no, Some(11));
        assert_eq!(h2.lines[2].new_no, Some(12));
        assert_eq!(d.added_count(), 2);
        assert_eq!(d.removed_count(), 1);
    }

    #[test]
    fn diff_new_file() {
        let d = one("diff --git a/n.txt b/n.txt\n\
new file mode 100644\n\
index 0000000..3b18e51\n\
--- /dev/null\n\
+++ b/n.txt\n\
@@ -0,0 +1,2 @@\n\
+hello\n\
+world\n");
        assert!(d.is_new);
        assert_eq!(d.old_path, None);
        assert_eq!(d.new_path.as_deref(), Some("n.txt"));
        let h = &d.hunks[0];
        assert_eq!((h.old_start, h.old_count, h.new_start, h.new_count), (0, 0, 1, 2));
        assert_eq!(h.lines[1].new_no, Some(2));
    }

    #[test]
    fn diff_single_line_hunk_header_defaults_count() {
        let d = one("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n");
        let h = &d.hunks[0];
        assert_eq!((h.old_start, h.old_count, h.new_start, h.new_count), (1, 1, 1, 1));
    }

    #[test]
    fn diff_deleted() {
        let d = one("diff --git a/old.txt b/old.txt\n\
deleted file mode 100644\n\
index 3b18e51..0000000\n\
--- a/old.txt\n\
+++ /dev/null\n\
@@ -1 +0,0 @@\n\
-bye\n");
        assert!(d.is_deleted);
        assert_eq!(d.new_path, None);
        assert_eq!(d.old_path.as_deref(), Some("old.txt"));
        assert_eq!(d.display_path(), "old.txt");
    }

    #[test]
    fn diff_rename_with_changes() {
        let d = one(&lines(&[
            "diff --git a/a.txt b/dir/b.txt",
            "similarity index 80%",
            "rename from a.txt",
            "rename to dir/b.txt",
            "index 1111111..2222222 100644",
            "--- a/a.txt",
            "+++ b/dir/b.txt",
            "@@ -1,2 +1,2 @@",
            " x",
            "-y",
            "+z",
        ]));
        assert_eq!(d.old_path.as_deref(), Some("a.txt"));
        assert_eq!(d.new_path.as_deref(), Some("dir/b.txt"));
        assert_eq!(d.hunks.len(), 1);
        assert_eq!(d.hunks[0].lines.len(), 3);
    }

    #[test]
    fn diff_pure_rename_without_hunks() {
        let d = one("diff --git a/a b.txt b/c d.txt\nsimilarity index 100%\nrename from a b.txt\nrename to c d.txt\n");
        assert_eq!(d.old_path.as_deref(), Some("a b.txt"));
        assert_eq!(d.new_path.as_deref(), Some("c d.txt"));
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn diff_binary() {
        let d = one(
            "diff --git a/img.png b/img.png\nindex 1111111..2222222 100644\nBinary files a/img.png and b/img.png differ\n",
        );
        assert!(d.is_binary);
        assert_eq!(d.display_path(), "img.png");
        assert!(d.hunks.is_empty());
    }

    #[test]
    fn diff_no_newline_marker() {
        let d = one(
            "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n\\ No newline at end of file\n",
        );
        let l = &d.hunks[0].lines;
        assert_eq!(l.len(), 2);
        assert!(l[0].no_newline);
        assert!(l[1].no_newline);
    }

    #[test]
    fn diff_keeps_cr() {
        let d = one("diff --git a/w.cs b/w.cs\n--- a/w.cs\n+++ b/w.cs\n@@ -1 +1 @@\n-abc\r\n+abd\r\n");
        assert_eq!(d.hunks[0].lines[0].text, "abc\r");
        assert_eq!(d.hunks[0].lines[1].text, "abd\r");
    }

    #[test]
    fn diff_content_lines_looking_like_headers() {
        let d = one(
            "diff --git a/q.sql b/q.sql\n--- a/q.sql\n+++ b/q.sql\n@@ -1,2 +1,2 @@\n--- commentaire\n+++ autre\n context\n",
        );
        let l = &d.hunks[0].lines;
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].kind, LineKind::Removed);
        assert_eq!(l[0].text, "-- commentaire");
        assert_eq!(l[1].kind, LineKind::Added);
        assert_eq!(l[1].text, "++ autre");
    }

    #[test]
    fn diff_two_files() {
        let v = parse_unified_diff(
            "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-1\n+2\ndiff --git a/b b/b\n--- a/b\n+++ b/b\n@@ -1 +1 @@\n-3\n+4\n",
        );
        assert_eq!(v.len(), 2);
        assert_eq!(v[1].display_path(), "b");
        assert_eq!(v[1].hunks[0].lines[1].text, "4");
    }

    #[test]
    fn diff_paths_with_spaces_and_trailing_tab() {
        let d = one(
            "diff --git a/dossier x/f y.txt b/dossier x/f y.txt\n--- a/dossier x/f y.txt\t\n+++ b/dossier x/f y.txt\t\n@@ -1 +1 @@\n-a\n+b\n",
        );
        assert_eq!(d.new_path.as_deref(), Some("dossier x/f y.txt"));
    }

    #[test]
    fn diff_quoted_paths() {
        let d = one(
            "diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\n--- \"a/caf\\303\\251.txt\"\n+++ \"b/caf\\303\\251.txt\"\n@@ -1 +1 @@\n-a\n+b\n",
        );
        assert_eq!(d.new_path.as_deref(), Some("café.txt"));
    }

    #[test]
    fn name_status_rename_z() {
        let v = parse_name_status_z(b"R087\0old name\0new name\0M\0x\0A\0n\0D\0d\0T\0t\0");
        assert_eq!(v.len(), 5);
        assert_eq!(
            v[0],
            FileChange { path: "new name".into(), old_path: Some("old name".into()), kind: ChangeKind::Renamed }
        );
        assert_eq!(v[1].kind, ChangeKind::Modified);
        assert_eq!(v[2].kind, ChangeKind::Added);
        assert_eq!(v[3].kind, ChangeKind::Deleted);
        assert_eq!(v[4].kind, ChangeKind::TypeChanged);
    }

    /// Revue finale (C4) : changement de mode lu dans l'en-tête, pour le signaler quand l'option « Espaces » ne
    /// l'indexe pas avec les hunks.
    #[test]
    fn mode_change_is_read_from_the_header() {
        let d = one(&lines(&[
            "diff --git a/run.sh b/run.sh",
            "old mode 100644",
            "new mode 100755",
            "index 1111111..2222222",
            "--- a/run.sh",
            "+++ b/run.sh",
            "@@ -1 +1 @@",
            "-a",
            "+b",
        ]));
        assert_eq!(d.mode_change(), Some(("100644", "100755")));
        let d =
            one(&lines(&["diff --git a/x b/x", "index 1..2 100644", "--- a/x", "+++ b/x", "@@ -1 +1 @@", "-a", "+b"]));
        assert_eq!(d.mode_change(), None);
    }
}
