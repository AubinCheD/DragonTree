use std::collections::HashMap;
use std::path::Path;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;

/// État de HEAD.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HeadState {
    /// Sur une branche existante.
    Branch(String),
    /// HEAD détachée sur un commit.
    Detached(Oid),
    /// Branche sans aucun commit (dépôt vide).
    Unborn(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBranch {
    pub name: String,
    pub oid: Oid,
    /// Branche suivie, forme courte (`origin/main`).
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub upstream_gone: bool,
    pub is_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBranch {
    pub remote: String,
    /// Nom sans le préfixe du remote (`feature/x`).
    pub name: String,
    /// Nom complet court (`origin/feature/x`).
    pub full: String,
    pub oid: Oid,
    /// `origin/HEAD` (référence symbolique).
    pub is_symbolic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRef {
    pub name: String,
    /// Commit pointé (tag annoté déréférencé).
    pub oid: Oid,
    pub annotated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    Head,
    Local,
    Remote,
    Tag,
}

/// Étiquette affichée à côté d'un commit dans le graphe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub kind: RefKind,
    pub name: String,
    pub is_head: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefsSnapshot {
    pub head: HeadState,
    pub head_oid: Option<Oid>,
    pub locals: Vec<LocalBranch>,
    pub remotes: Vec<RemoteBranch>,
    pub tags: Vec<TagRef>,
    pub labels: HashMap<Oid, Vec<RefLabel>>,
}

impl RefsSnapshot {
    /// Nom de la branche courante, si HEAD est sur une branche (née ou non).
    pub fn current_branch(&self) -> Option<&str> {
        match &self.head {
            HeadState::Branch(b) | HeadState::Unborn(b) => Some(b),
            HeadState::Detached(_) => None,
        }
    }

    pub fn local(&self, name: &str) -> Option<&LocalBranch> {
        self.locals.iter().find(|b| b.name == name)
    }

    /// Noms des remotes connus d'après les branches distantes.
    pub fn remote_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.remotes.iter().map(|r| r.remote.clone()).collect();
        names.dedup();
        names
    }
}

pub const FOR_EACH_REF_FORMAT: &str = "%(refname)%1f%(objectname)%1f%(*objectname)%1f%(upstream:short)%1f%(upstream:track,nobracket)%1f%(HEAD)%1f%(symref)";

/// Lit `HEAD` directement dans le dossier git (sans lancer de processus).
pub fn read_head(git_dir: &Path) -> Result<HeadState, GitError> {
    let raw = std::fs::read_to_string(git_dir.join("HEAD"))?;
    let raw = raw.trim();
    if let Some(target) = raw.strip_prefix("ref:") {
        let target = target.trim();
        let name = target.strip_prefix("refs/heads/").unwrap_or(target);
        return Ok(HeadState::Branch(name.to_string()));
    }
    Oid::from_hex(raw)
        .map(HeadState::Detached)
        .ok_or_else(|| GitError::Parse(tr!("HEAD illisible : {raw}", raw = format!("{raw:?}"))))
}

/// Parse la sortie de `git for-each-ref --format=FOR_EACH_REF_FORMAT`.
pub fn parse_refs(output: &str, head: HeadState) -> RefsSnapshot {
    let mut locals = Vec::new();
    let mut remotes = Vec::new();
    let mut tags = Vec::new();

    for line in output.lines() {
        let f: Vec<&str> = line.split('\x1f').collect();
        if f.len() < 7 {
            continue;
        }
        let (refname, objectname, peeled, upstream, track, headmark, symref) =
            (f[0], f[1], f[2], f[3], f[4], f[5], f[6]);
        let Some(oid) = Oid::from_hex(objectname) else { continue };

        if let Some(name) = refname.strip_prefix("refs/heads/") {
            let (mut ahead, mut behind) = (0, 0);
            for part in track.split(',') {
                let part = part.trim();
                if let Some(n) = part.strip_prefix("ahead ") {
                    ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    behind = n.parse().unwrap_or(0);
                }
            }
            locals.push(LocalBranch {
                name: name.to_string(),
                oid,
                upstream: (!upstream.is_empty()).then(|| upstream.to_string()),
                ahead,
                behind,
                upstream_gone: track == "gone",
                is_head: headmark == "*",
            });
        } else if let Some(full) = refname.strip_prefix("refs/remotes/") {
            let (remote, name) = full.split_once('/').unwrap_or((full, ""));
            remotes.push(RemoteBranch {
                remote: remote.to_string(),
                name: name.to_string(),
                full: full.to_string(),
                oid,
                is_symbolic: !symref.is_empty(),
            });
        } else if let Some(name) = refname.strip_prefix("refs/tags/") {
            let peeled_oid = Oid::from_hex(peeled);
            tags.push(TagRef {
                name: name.to_string(),
                oid: peeled_oid.unwrap_or(oid),
                annotated: peeled_oid.is_some(),
            });
        }
    }

    locals.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    remotes.sort_by(|a, b| natural_cmp(&a.full, &b.full));
    tags.sort_by(|a, b| natural_cmp(&a.name, &b.name));

    let (head, head_oid) = match head {
        HeadState::Branch(name) | HeadState::Unborn(name) => match locals.iter().find(|b| b.name == name) {
            Some(b) => {
                let oid = b.oid;
                (HeadState::Branch(name), Some(oid))
            }
            None => (HeadState::Unborn(name), None),
        },
        HeadState::Detached(oid) => (HeadState::Detached(oid), Some(oid)),
    };

    let mut labels: HashMap<Oid, Vec<RefLabel>> = HashMap::new();
    if let HeadState::Detached(oid) = head {
        labels.entry(oid).or_default().push(RefLabel { kind: RefKind::Head, name: "HEAD".into(), is_head: true });
    }
    for b in &locals {
        let is_head = matches!(&head, HeadState::Branch(n) if *n == b.name);
        labels.entry(b.oid).or_default().push(RefLabel { kind: RefKind::Local, name: b.name.clone(), is_head });
    }
    for r in remotes.iter().filter(|r| !r.is_symbolic) {
        labels.entry(r.oid).or_default().push(RefLabel { kind: RefKind::Remote, name: r.full.clone(), is_head: false });
    }
    for t in &tags {
        labels.entry(t.oid).or_default().push(RefLabel { kind: RefKind::Tag, name: t.name.clone(), is_head: false });
    }

    RefsSnapshot { head, head_oid, locals, remotes, tags, labels }
}

/// Compare deux noms en traitant les suites de chiffres comme des nombres (`v1.9` < `v1.10`).
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ia, mut ib) = (a.char_indices().peekable(), b.char_indices().peekable());
    loop {
        match (ia.peek().copied(), ib.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((pa, ca)), Some((pb, cb))) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let ea = a[pa..].find(|c: char| !c.is_ascii_digit()).map_or(a.len(), |e| pa + e);
                    let eb = b[pb..].find(|c: char| !c.is_ascii_digit()).map_or(b.len(), |e| pb + e);
                    let (na, nb) = (a[pa..ea].trim_start_matches('0'), b[pb..eb].trim_start_matches('0'));
                    let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    while ia.peek().is_some_and(|&(p, _)| p < ea) {
                        ia.next();
                    }
                    while ib.peek().is_some_and(|&(p, _)| p < eb) {
                        ib.next();
                    }
                } else {
                    let ord = ca.to_lowercase().cmp(cb.to_lowercase());
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    ia.next();
                    ib.next();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";
    const C: &str = "3333333333333333333333333333333333333333";

    fn line(fields: [&str; 7]) -> String {
        fields.join("\x1f")
    }

    fn oid(s: &str) -> Oid {
        Oid::from_hex(s).unwrap()
    }

    #[test]
    fn refs_local_with_tracking() {
        let out = line(["refs/heads/main", A, "", "origin/main", "ahead 2, behind 1", "*", ""]);
        let r = parse_refs(&out, HeadState::Branch("main".into()));
        let b = &r.locals[0];
        assert_eq!(b.name, "main");
        assert_eq!(b.oid, oid(A));
        assert_eq!(b.upstream.as_deref(), Some("origin/main"));
        assert_eq!((b.ahead, b.behind), (2, 1));
        assert!(b.is_head);
        assert!(!b.upstream_gone);
        assert_eq!(r.head_oid, Some(oid(A)));
    }

    #[test]
    fn refs_only_ahead_or_behind() {
        let out = [
            line(["refs/heads/a", A, "", "origin/a", "ahead 3", " ", ""]),
            line(["refs/heads/b", A, "", "origin/b", "behind 4", " ", ""]),
        ]
        .join("\n");
        let r = parse_refs(&out, HeadState::Branch("a".into()));
        assert_eq!((r.locals[0].ahead, r.locals[0].behind), (3, 0));
        assert_eq!((r.locals[1].ahead, r.locals[1].behind), (0, 4));
    }

    #[test]
    fn refs_gone_upstream() {
        let out = line(["refs/heads/old", A, "", "origin/old", "gone", " ", ""]);
        let r = parse_refs(&out, HeadState::Branch("main".into()));
        assert!(r.locals[0].upstream_gone);
        assert!(!r.locals[0].is_head);
    }

    #[test]
    fn refs_annotated_tag_peeled() {
        let out = [line(["refs/tags/v1.0", B, A, "", "", " ", ""]), line(["refs/tags/light", C, "", "", "", " ", ""])]
            .join("\n");
        let r = parse_refs(&out, HeadState::Detached(oid(A)));
        assert_eq!(r.tags[0].name, "light");
        assert_eq!(r.tags[0].oid, oid(C));
        assert!(!r.tags[0].annotated);
        assert_eq!(r.tags[1].name, "v1.0");
        assert_eq!(r.tags[1].oid, oid(A));
        assert!(r.tags[1].annotated);
    }

    #[test]
    fn refs_remote_head_symbolic() {
        let out = [
            line(["refs/remotes/origin/HEAD", A, "", "", "", " ", "refs/remotes/origin/main"]),
            line(["refs/remotes/origin/feature/x", B, "", "", "", " ", ""]),
        ]
        .join("\n");
        let r = parse_refs(&out, HeadState::Branch("main".into()));
        let head = r.remotes.iter().find(|b| b.name == "HEAD").unwrap();
        assert!(head.is_symbolic);
        let fx = r.remotes.iter().find(|b| b.name == "feature/x").unwrap();
        assert_eq!(fx.remote, "origin");
        assert_eq!(fx.full, "origin/feature/x");
        assert!(!fx.is_symbolic);
        assert_eq!(r.remote_names(), vec!["origin".to_string()]);
    }

    #[test]
    fn refs_labels_grouped_by_oid() {
        let out = [
            line(["refs/heads/main", A, "", "", "", "*", ""]),
            line(["refs/remotes/origin/main", A, "", "", "", " ", ""]),
            line(["refs/remotes/origin/HEAD", A, "", "", "", " ", "refs/remotes/origin/main"]),
            line(["refs/tags/v1", B, A, "", "", " ", ""]),
        ]
        .join("\n");
        let r = parse_refs(&out, HeadState::Branch("main".into()));
        let labels = &r.labels[&oid(A)];
        let names: Vec<&str> = labels.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, vec!["main", "origin/main", "v1"], "origin/HEAD n'est pas affiché");
        assert!(labels[0].is_head);
        assert_eq!(labels[0].kind, RefKind::Local);
        assert_eq!(labels[1].kind, RefKind::Remote);
        assert_eq!(labels[2].kind, RefKind::Tag);
    }

    #[test]
    fn refs_detached_head_label() {
        let out = line(["refs/heads/main", A, "", "", "", " ", ""]);
        let r = parse_refs(&out, HeadState::Detached(oid(B)));
        assert_eq!(r.head_oid, Some(oid(B)));
        assert_eq!(r.labels[&oid(B)][0].kind, RefKind::Head);
        assert_eq!(r.current_branch(), None);
    }

    #[test]
    fn refs_unborn() {
        let r = parse_refs("", HeadState::Branch("main".into()));
        assert_eq!(r.head, HeadState::Unborn("main".into()));
        assert_eq!(r.head_oid, None);
        assert_eq!(r.current_branch(), Some("main"));
    }

    #[test]
    fn read_head_variants() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("HEAD"), "ref: refs/heads/feature/é\n").unwrap();
        assert_eq!(read_head(dir.path()).unwrap(), HeadState::Branch("feature/é".into()));
        std::fs::write(dir.path().join("HEAD"), format!("{A}\n")).unwrap();
        assert_eq!(read_head(dir.path()).unwrap(), HeadState::Detached(oid(A)));
        std::fs::write(dir.path().join("HEAD"), "garbage").unwrap();
        assert!(read_head(dir.path()).is_err());
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["v1.10", "v1.9", "v1.2", "alpha", "v10"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["alpha", "v1.2", "v1.9", "v1.10", "v10"]);
    }
}
