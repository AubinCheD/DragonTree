use std::collections::HashMap;
use std::sync::Arc;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;

/// Commit tel qu'affiché dans l'historique (le message complet est chargé à la demande).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub id: Oid,
    pub parents: Box<[Oid]>,
    pub author: Arc<str>,
    pub author_email: Arc<str>,
    pub author_time: i64,
    pub committer: Arc<str>,
    pub committer_time: i64,
    pub summary: Box<str>,
}

/// Quelles références parcourir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogScope {
    /// Branches locales, distantes, tags et HEAD (exclut refs/stash).
    All,
    /// Seulement HEAD.
    Head,
    /// Révisions explicites.
    Revs(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOrder {
    Date,
    Topo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogOptions {
    pub scope: LogScope,
    pub order: LogOrder,
    pub max_count: Option<usize>,
    pub paths: Vec<String>,
    pub follow: bool,
}

impl Default for LogOptions {
    fn default() -> Self {
        LogOptions { scope: LogScope::All, order: LogOrder::Date, max_count: None, paths: Vec::new(), follow: false }
    }
}

pub const LOG_FORMAT: &str = "%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%cn%x1f%ct%x1f%s";

/// Arguments de `git log` correspondant aux options (sortie lisible par [`LogParser`]).
pub fn log_args(opts: &LogOptions) -> Vec<String> {
    let mut args: Vec<String> = vec!["log".into(), "-z".into(), format!("--format={LOG_FORMAT}")];
    args.push(match opts.order {
        LogOrder::Date => "--date-order".into(),
        LogOrder::Topo => "--topo-order".into(),
    });
    if let Some(n) = opts.max_count {
        args.push(format!("--max-count={n}"));
    }
    if opts.follow {
        args.push("--follow".into());
    }
    match &opts.scope {
        LogScope::All => args.extend(["--branches", "--remotes", "--tags", "HEAD"].map(String::from)),
        LogScope::Head => args.push("HEAD".into()),
        LogScope::Revs(revs) => args.extend(revs.iter().cloned()),
    }
    args.push("--".into());
    args.extend(opts.paths.iter().cloned());
    args
}

/// Parseur incrémental de la sortie de `git log -z --format=LOG_FORMAT`.
#[derive(Default)]
pub struct LogParser {
    buf: Vec<u8>,
    names: HashMap<Box<str>, Arc<str>>,
}

impl LogParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Ajoute un morceau de sortie ; les commits complets sont poussés dans `out`.
    pub fn feed(&mut self, chunk: &[u8], out: &mut Vec<Commit>) -> Result<(), GitError> {
        let start = self.buf.len();
        self.buf.extend_from_slice(chunk);
        let Some(last_nul) = self.buf[start..].iter().rposition(|&b| b == 0).map(|p| p + start) else {
            return Ok(());
        };
        let complete = std::mem::take(&mut self.buf);
        for record in complete[..last_nul].split(|&b| b == 0) {
            if !record.is_empty() {
                out.push(self.parse_record(record)?);
            }
        }
        self.buf.extend_from_slice(&complete[last_nul + 1..]);
        Ok(())
    }

    /// Termine le flux (dernier enregistrement éventuellement sans séparateur final).
    pub fn finish(&mut self, out: &mut Vec<Commit>) -> Result<(), GitError> {
        let rest = std::mem::take(&mut self.buf);
        let trimmed = rest.strip_suffix(b"\n").unwrap_or(&rest);
        if !trimmed.is_empty() {
            out.push(self.parse_record(trimmed)?);
        }
        Ok(())
    }

    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(a) = self.names.get(s) {
            return a.clone();
        }
        let a: Arc<str> = Arc::from(s);
        self.names.insert(s.into(), a.clone());
        a
    }

    fn parse_record(&mut self, record: &[u8]) -> Result<Commit, GitError> {
        let text = String::from_utf8_lossy(record);
        // Un `\n` peut précéder l'enregistrement selon les versions de git.
        let text = text.trim_start_matches('\n');
        let f: Vec<&str> = text.splitn(8, '\x1f').collect();
        let bad = || {
            GitError::Parse(tr!(
                "commit illisible dans le log : {record}",
                record = format!("{:?}", truncate(text, 120))
            ))
        };
        if f.len() != 8 {
            return Err(bad());
        }
        let id = Oid::from_hex(f[0]).ok_or_else(bad)?;
        let parents = f[1]
            .split_whitespace()
            .map(|p| Oid::from_hex(p).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?
            .into_boxed_slice();
        Ok(Commit {
            id,
            parents,
            author: self.intern(f[2]),
            author_email: self.intern(f[3]),
            author_time: f[4].parse().unwrap_or(0),
            committer: self.intern(f[5]),
            committer_time: f[6].parse().unwrap_or(0),
            summary: f[7].trim_end_matches('\n').into(),
        })
    }
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";
    const C: &str = "3333333333333333333333333333333333333333";

    fn record(id: &str, parents: &str, author: &str, subject: &str) -> String {
        format!(
            "{id}\x1f{parents}\x1f{author}\x1f{author}@x.fr\x1f1700000000\x1fCommitteur\x1f1700000100\x1f{subject}\0"
        )
    }

    fn sample() -> String {
        [
            record(A, &format!("{B} {C}"), "Camille Lefèvre", "Merge branch 'feature/é'"),
            record(B, C, "Camille Lefèvre", "Deuxième"),
            record(C, "", "Zoé", "Racine : init"),
        ]
        .concat()
    }

    fn parse_all(input: &[u8]) -> Vec<Commit> {
        let mut p = LogParser::new();
        let mut out = Vec::new();
        p.feed(input, &mut out).unwrap();
        p.finish(&mut out).unwrap();
        out
    }

    #[test]
    fn log_parses_merge_and_root() {
        let commits = parse_all(sample().as_bytes());
        assert_eq!(commits.len(), 3);
        assert_eq!(commits[0].id, Oid::from_hex(A).unwrap());
        assert_eq!(&*commits[0].parents, &[Oid::from_hex(B).unwrap(), Oid::from_hex(C).unwrap()]);
        assert_eq!(&*commits[0].summary, "Merge branch 'feature/é'");
        assert_eq!(&*commits[0].author, "Camille Lefèvre");
        assert_eq!(&*commits[0].author_email, "Camille Lefèvre@x.fr");
        assert_eq!(commits[0].author_time, 1_700_000_000);
        assert_eq!(commits[0].committer_time, 1_700_000_100);
        assert!(commits[2].parents.is_empty());
        assert_eq!(&*commits[2].summary, "Racine : init");
    }

    #[test]
    fn log_feed_split_anywhere() {
        let input = sample().into_bytes();
        let expected = parse_all(&input);
        for size in 1..=7 {
            let mut p = LogParser::new();
            let mut out = Vec::new();
            for chunk in input.chunks(size) {
                p.feed(chunk, &mut out).unwrap();
            }
            p.finish(&mut out).unwrap();
            assert_eq!(out, expected, "taille de morceau {size}");
        }
    }

    #[test]
    fn log_interns_authors() {
        let commits = parse_all(sample().as_bytes());
        assert!(Arc::ptr_eq(&commits[0].author, &commits[1].author));
    }

    #[test]
    fn log_finish_accepts_missing_trailing_nul() {
        let mut s = sample();
        s.pop();
        assert_eq!(parse_all(s.as_bytes()).len(), 3);
    }

    #[test]
    fn log_rejects_truncated_record() {
        let mut p = LogParser::new();
        let mut out = Vec::new();
        assert!(p.feed(format!("{A}\x1f\x1fx\0").as_bytes(), &mut out).is_err());
    }

    #[test]
    fn log_args_all_scope() {
        let args = log_args(&LogOptions::default());
        for expected in ["log", "-z", "--date-order", "--branches", "--remotes", "--tags", "HEAD"] {
            assert!(args.iter().any(|a| a == expected), "{expected} manquant dans {args:?}");
        }
        assert!(args.iter().any(|a| a == &format!("--format={LOG_FORMAT}")));
        assert!(!args.iter().any(|a| a == "--all"), "--all inclurait refs/stash");
    }

    #[test]
    fn log_args_file_history() {
        let args = log_args(&LogOptions {
            scope: LogScope::Head,
            order: LogOrder::Topo,
            max_count: Some(500),
            paths: vec!["src/a b.rs".into()],
            follow: true,
        });
        assert!(args.iter().any(|a| a == "--topo-order"));
        assert!(args.iter().any(|a| a == "--max-count=500"));
        assert!(args.iter().any(|a| a == "--follow"));
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dd + 1], "src/a b.rs");
        assert_eq!(args.last().unwrap(), "src/a b.rs");
    }
}
