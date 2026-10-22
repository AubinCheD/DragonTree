use std::collections::{HashMap, HashSet};
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
    /// Branches locales, distantes, tags et HEAD (exclut refs/stash) : `Refs(RefSelection::default())`.
    All,
    /// Seulement HEAD.
    Head,
    /// Révisions explicites.
    Revs(Vec<String>),
    /// Références choisies : tout sauf certaines, ou seulement certaines (voir [`RefSelection`]).
    Refs(RefSelection),
}

/// Sélecteur structuré des références parcourues par le graphe. Les options de l'interface (tags, branches
/// distantes, éléments masqués, filtre sur des branches…) le remplissent ; [`selection_args`] le traduit en
/// arguments de `git log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefSelection {
    /// Parcourir aussi HEAD (même si sa branche est exclue ; sans effet sur `extra`).
    pub head: bool,
    /// Parcourir les tags (hors mode `only`).
    pub tags: bool,
    /// Parcourir les branches distantes (hors mode `only`).
    pub remotes: bool,
    /// Références à ne pas parcourir : noms complets ou motifs (`refs/heads/x`, `refs/heads/feature/*`,
    /// `refs/remotes/origin/*`, `refs/tags/v1`).
    pub exclude: Vec<String>,
    /// Si non vide : seulement ces références (nom complet, ou motif contenant `*`, `?` ou `[`), à la place de
    /// toutes les branches, branches distantes et tags.
    pub only: Vec<String>,
    /// Révisions supplémentaires (extrémités des commits du reflog…), transmises à git par l'entrée standard
    /// (`--stdin`, voir [`log_stdin`]) : leur nombre n'est pas borné et la ligne de commande Windows l'est.
    pub extra: Vec<String>,
}

impl Default for RefSelection {
    fn default() -> Self {
        RefSelection { head: true, tags: true, remotes: true, exclude: Vec::new(), only: Vec::new(), extra: Vec::new() }
    }
}

/// Familles de références que `--branches`, `--remotes` et `--tags` parcourent : (préfixe, option).
const FAMILIES: [(&str, &str); 3] =
    [("refs/heads/", "--branches"), ("refs/remotes/", "--remotes"), ("refs/tags/", "--tags")];

/// `--exclude` ne vaut que pour l'option `--branches`/`--remotes`/`--tags`/`--glob` qui le suit, puis il est
/// oublié : il est donc répété devant chacune. Avec `--branches`, `--remotes` et `--tags`, le motif est relatif
/// à la famille (`feature/*`, pas `refs/heads/feature/*`) ; avec `--glob`, il est complet.
fn push_excludes(args: &mut Vec<String>, exclude: &[String], family: Option<&str>) {
    for pattern in exclude {
        match family {
            Some(prefix) => {
                if let Some(relative) = pattern.strip_prefix(prefix) {
                    args.push(format!("--exclude={relative}"));
                }
            }
            None => args.push(format!("--exclude={pattern}")),
        }
    }
}

fn is_glob(pattern: &str) -> bool {
    pattern.contains(['*', '?', '['])
}

/// Arguments de révisions (avant `--`) qui parcourent les références choisies.
pub fn selection_args(sel: &RefSelection) -> Vec<String> {
    let mut args = Vec::new();
    if sel.only.is_empty() {
        for (prefix, option) in FAMILIES {
            let wanted = match option {
                "--remotes" => sel.remotes,
                "--tags" => sel.tags,
                _ => true,
            };
            if wanted {
                push_excludes(&mut args, &sel.exclude, Some(prefix));
                if option == "--remotes" {
                    // `origin/HEAD` (symbolique) suit `origin/main` : parcourue, elle ferait fuir une branche masquée.
                    args.push("--exclude=*/HEAD".into());
                }
                args.push(option.into());
            }
        }
    } else {
        for pick in &sel.only {
            if is_glob(pick) {
                push_excludes(&mut args, &sel.exclude, None);
                if pick.starts_with("refs/remotes/") {
                    args.push("--exclude=refs/remotes/*/HEAD".into());
                }
                args.push(format!("--glob={pick}"));
            } else {
                // Un choix précis l'emporte sur le masquage : les exclusions ne retirent que ce que les motifs
                // (`--glob`) parcourent.
                args.push(pick.clone());
            }
        }
    }
    if sel.head {
        args.push("HEAD".into());
    }
    args
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

/// Arguments de révisions (avant `--`) d'une portée.
fn scope_args(scope: &LogScope) -> Vec<String> {
    match scope {
        LogScope::All => selection_args(&RefSelection::default()),
        LogScope::Head => vec!["HEAD".into()],
        LogScope::Revs(revs) => revs.clone(),
        LogScope::Refs(selection) => selection_args(selection),
    }
}

/// Recherche de commits par ce que leurs modifications contiennent ou touchent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitSearch {
    /// Commits dont le diff ajoute ou retire une ligne qui correspond à l'expression régulière (`git log -G`).
    Content(String),
    /// Commits qui touchent un de ces chemins ou motifs (`git log -- <motif>…`) ; les jokers de git valent.
    Paths(Vec<String>),
}

/// Arguments de `git log` qui listent (un oid par ligne) les commits de la portée de `opts` que `search` trouve.
/// Le texte cherché est collé à `-G` (jamais pris pour une option) et les chemins viennent après `--`.
pub fn search_args(opts: &LogOptions, search: &CommitSearch) -> Vec<String> {
    let mut args: Vec<String> = ["log", "--no-show-signature", "--format=%H"].map(String::from).to_vec();
    if let CommitSearch::Content(text) = search {
        args.push(format!("-G{text}"));
    }
    args.extend(scope_args(&opts.scope));
    if log_stdin(opts).is_some() {
        args.push("--stdin".into());
    }
    args.push("--".into());
    if let CommitSearch::Paths(paths) = search {
        args.extend(paths.iter().cloned());
    }
    args
}

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
    args.extend(scope_args(&opts.scope));
    if log_stdin(opts).is_some() {
        args.push("--stdin".into());
    }
    args.push("--".into());
    args.extend(opts.paths.iter().cloned());
    args
}

/// Révisions supplémentaires du sélecteur, une par ligne, à envoyer sur l'entrée standard de `git log --stdin`
/// (`None` : rien à envoyer, `--stdin` n'est alors pas demandé).
pub fn log_stdin(opts: &LogOptions) -> Option<Vec<u8>> {
    let LogScope::Refs(selection) = &opts.scope else { return None };
    if selection.extra.is_empty() {
        return None;
    }
    Some(selection.extra.iter().flat_map(|rev| [rev.as_bytes(), b"\n"]).flatten().copied().collect())
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
        // Un identifiant SHA-256 donne son erreur explicite (L1), tout autre identifiant illisible celle du commit.
        let oid = |s: &str| {
            Oid::parse(s).map_err(|e| if matches!(e, GitError::UnsupportedObjectFormat { .. }) { e } else { bad() })
        };
        let id = oid(f[0])?;
        let parents = f[1].split_whitespace().map(oid).collect::<Result<Vec<_>, _>>()?.into_boxed_slice();
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

/// Commits atteignables seulement par le reflog (ex. après un `reset --hard` ou un `commit --amend`) : ni
/// branche, ni branche distante, ni tag, ni HEAD ne les contient.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LostCommits {
    /// Tous les commits perdus.
    pub all: HashSet<Oid>,
    /// Les extrémités : commits perdus qui n'ont aucun descendant perdu. Les passer à `git log` suffit à tout
    /// parcourir.
    pub tips: Vec<Oid>,
}

/// Arguments de `git rev-list` listant les commits perdus : tout ce que les reflogs atteignent, sauf ce qu'une
/// référence atteint. `--all` couvre branches, tags, remotes, `refs/stash`, `refs/notes`, `refs/original`, et le
/// HEAD de chaque worktree. Les commits des stashs plus anciens que `refs/stash` ne sont pas atteints par elle :
/// leurs oid sont exclus par [`lost_stdin`].
pub fn lost_args() -> Vec<String> {
    ["rev-list", "--parents", "--reflog", "--not", "--all", "--stdin"].map(String::from).to_vec()
}

/// Entrée standard de [`lost_args`] : `^oid` pour chaque stash (une ligne lue sur stdin n'hérite pas de `--not`).
pub fn lost_stdin(stashes: &[Oid]) -> Vec<u8> {
    stashes.iter().flat_map(|oid| format!("^{}\n", oid.to_hex()).into_bytes()).collect()
}

/// Analyse la sortie de `git rev-list --parents` (« oid parent… » par ligne).
pub fn parse_lost(out: &str) -> LostCommits {
    let rows: Vec<(Oid, Vec<Oid>)> = out
        .lines()
        .filter_map(|line| {
            let mut ids = line.split_whitespace().map(Oid::from_hex);
            let id = ids.next()??;
            Some((id, ids.flatten().collect()))
        })
        .collect();
    let all: HashSet<Oid> = rows.iter().map(|(id, _)| *id).collect();
    let parents: HashSet<Oid> = rows.iter().flat_map(|(_, p)| p.iter().copied()).collect();
    let tips = rows.iter().map(|(id, _)| *id).filter(|id| !parents.contains(id)).collect();
    LostCommits { all, tips }
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

    #[test]
    fn search_args_keep_text_and_paths_out_of_option_position() {
        let opts = LogOptions { scope: LogScope::Head, ..LogOptions::default() };
        let content = search_args(&opts, &CommitSearch::Content("--output=x".into()));
        assert!(content.contains(&"-G--output=x".to_string()));
        assert_eq!(content.last().map(String::as_str), Some("--"));
        let paths = search_args(&opts, &CommitSearch::Paths(vec!["--all".into(), "src/*.rs".into()]));
        let dashes = paths.iter().position(|a| a == "--").unwrap();
        assert_eq!(&paths[dashes..], ["--", "--all", "src/*.rs"]);
        assert!(!paths[..dashes].contains(&"--all".to_string()));
    }

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

    fn tail(selection: RefSelection) -> Vec<String> {
        let args = log_args(&LogOptions { scope: LogScope::Refs(selection), ..LogOptions::default() });
        let from = args.iter().position(|a| a.starts_with("--date-order")).unwrap() + 1;
        args[from..args.iter().position(|a| a == "--").unwrap()].to_vec()
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn default_selection_is_the_all_scope() {
        assert_eq!(
            log_args(&LogOptions { scope: LogScope::Refs(RefSelection::default()), ..LogOptions::default() }),
            log_args(&LogOptions::default())
        );
        assert_eq!(tail(RefSelection::default()), ["--branches", "--exclude=*/HEAD", "--remotes", "--tags", "HEAD"]);
    }

    #[test]
    fn hidden_tags_and_remotes_are_not_walked() {
        let args = tail(RefSelection { tags: false, remotes: false, ..RefSelection::default() });
        assert_eq!(args, ["--branches", "HEAD"]);
        let args = tail(RefSelection { tags: false, ..RefSelection::default() });
        assert_eq!(args, ["--branches", "--exclude=*/HEAD", "--remotes", "HEAD"]);
    }

    #[test]
    fn excludes_precede_the_family_they_apply_to_and_are_relative_to_it() {
        let args = tail(RefSelection {
            exclude: strings(&["refs/remotes/origin/*", "refs/heads/feature/*", "refs/heads/x", "refs/tags/v1"]),
            ..RefSelection::default()
        });
        assert_eq!(
            args,
            [
                "--exclude=feature/*",
                "--exclude=x",
                "--branches",
                "--exclude=origin/*",
                "--exclude=*/HEAD",
                "--remotes",
                "--exclude=v1",
                "--tags",
                "HEAD"
            ]
        );
    }

    #[test]
    fn excludes_of_a_skipped_family_are_dropped() {
        let args = tail(RefSelection {
            tags: false,
            exclude: strings(&["refs/tags/v1", "refs/heads/x"]),
            ..RefSelection::default()
        });
        assert_eq!(args, ["--exclude=x", "--branches", "--exclude=*/HEAD", "--remotes", "HEAD"]);
    }

    #[test]
    fn only_lists_explicit_refs_and_globs_with_full_excludes() {
        let args = tail(RefSelection {
            head: false,
            only: strings(&["refs/heads/main", "refs/heads/feature/*", "refs/remotes/origin/*"]),
            exclude: strings(&["refs/heads/feature/b"]),
            ..RefSelection::default()
        });
        assert_eq!(
            args,
            [
                "refs/heads/main",
                "--exclude=refs/heads/feature/b",
                "--glob=refs/heads/feature/*",
                "--exclude=refs/heads/feature/b",
                "--exclude=refs/remotes/*/HEAD",
                "--glob=refs/remotes/origin/*"
            ]
        );
        let args = tail(RefSelection {
            head: false,
            only: strings(&["refs/heads/a", "refs/heads/b"]),
            exclude: strings(&["refs/heads/a"]),
            ..RefSelection::default()
        });
        // Un choix précis l'emporte sur le masquage.
        assert_eq!(args, ["refs/heads/a", "refs/heads/b"]);
    }

    #[test]
    fn extra_revisions_go_through_stdin_not_the_command_line() {
        let sel = RefSelection { extra: strings(&["abc123", "def456"]), ..RefSelection::default() };
        let opts = LogOptions { scope: LogScope::Refs(sel), ..LogOptions::default() };
        let args = log_args(&opts);
        assert!(args.contains(&"--stdin".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("abc") || a.starts_with("def")), "{args:?}");
        assert_eq!(log_stdin(&opts).unwrap(), b"abc123\ndef456\n");
        // Sans révision supplémentaire : ni --stdin ni entrée.
        assert!(!log_args(&LogOptions::default()).contains(&"--stdin".to_string()));
        assert_eq!(log_stdin(&LogOptions::default()), None);
    }

    #[test]
    fn lost_args_walk_the_reflog_but_not_any_reference_or_stash() {
        let args = lost_args();
        assert_eq!(args[..3], ["rev-list", "--parents", "--reflog"]);
        let not = args.iter().position(|a| a == "--not").unwrap();
        assert_eq!(args[not + 1..], ["--all", "--stdin"]);
        assert_eq!(lost_stdin(&[Oid::from_hex(A).unwrap()]), format!("^{A}\n").into_bytes());
        assert!(lost_stdin(&[]).is_empty());
    }

    #[test]
    fn lost_parsing_keeps_only_the_tips_of_the_lost_chain() {
        // C et son parent B sont perdus ; A, atteint par une branche, n'est pas listé.
        let out = format!("{C} {B}\n{B} {A}\n");
        let lost = parse_lost(&out);
        assert_eq!(lost.tips, [Oid::from_hex(C).unwrap()]);
        assert_eq!(lost.all.len(), 2);
        assert!(lost.all.contains(&Oid::from_hex(B).unwrap()));
        assert_eq!(parse_lost(""), LostCommits::default());
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
