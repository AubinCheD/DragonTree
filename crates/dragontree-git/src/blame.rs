use std::collections::HashMap;

use crate::error::GitError;
use crate::oid::Oid;
use crate::repo::Repo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub commit: Oid,
    pub line_no: u32,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlameCommit {
    pub id: Oid,
    pub author: String,
    pub author_time: i64,
    pub summary: String,
}

/// Parse `git blame --porcelain`.
pub fn parse_blame_porcelain(text: &str) -> (Vec<BlameLine>, HashMap<Oid, BlameCommit>) {
    let mut lines = Vec::new();
    let mut commits: HashMap<Oid, BlameCommit> = HashMap::new();
    let mut current: Option<Oid> = None;
    let mut line_no = 0u32;

    for raw in text.split('\n') {
        if let Some(content) = raw.strip_prefix('\t') {
            if let Some(commit) = current {
                lines.push(BlameLine { commit, line_no, text: content.to_string() });
            }
            continue;
        }
        let (key, value) = raw.split_once(' ').unwrap_or((raw, ""));
        if key.len() == 40
            && let Some(oid) = Oid::from_hex(key)
        {
            // <sha> <ligne d'origine> <ligne finale> [<nombre de lignes>]
            line_no = value.split(' ').nth(1).and_then(|n| n.parse().ok()).unwrap_or(0);
            current = Some(oid);
            commits.entry(oid).or_insert_with(|| BlameCommit { id: oid, ..Default::default() });
            continue;
        }
        let Some(commit) = current.and_then(|c| commits.get_mut(&c)) else { continue };
        match key {
            "author" => commit.author = value.to_string(),
            "author-time" => commit.author_time = value.parse().unwrap_or(0),
            "summary" => commit.summary = value.to_string(),
            _ => {}
        }
    }
    (lines, commits)
}

impl Repo {
    /// Blame d'un fichier, à une révision donnée ou dans la copie de travail.
    pub fn blame(&self, path: &str, rev: Option<Oid>) -> Result<(Vec<BlameLine>, HashMap<Oid, BlameCommit>), GitError> {
        let mut cmd = self.read_paths().args(["blame", "--porcelain"]);
        if let Some(rev) = rev {
            cmd = cmd.arg(rev.to_hex());
        }
        let out = cmd.args(["--", path]).run()?;
        Ok(parse_blame_porcelain(&out.stdout_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn blame_porcelain_two_commits() {
        let text = [
            format!("{A} 1 1 2"),
            "author Camille".into(),
            "author-mail <a@x.fr>".into(),
            "author-time 1700000000".into(),
            "author-tz +0100".into(),
            "committer Camille".into(),
            "committer-mail <a@x.fr>".into(),
            "committer-time 1700000000".into(),
            "committer-tz +0100".into(),
            "summary Premier".into(),
            "filename f.txt".into(),
            "\tligne un".into(),
            format!("{A} 2 2"),
            "\tligne deux\r".into(),
            format!("{B} 3 3 1"),
            "author Zoé".into(),
            "author-time 1800000000".into(),
            "summary Second".into(),
            format!("previous {A} f.txt"),
            "filename f.txt".into(),
            "\t".into(),
        ]
        .join("\n");
        let (lines, commits) = parse_blame_porcelain(&text);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], BlameLine { commit: Oid::from_hex(A).unwrap(), line_no: 1, text: "ligne un".into() });
        assert_eq!(lines[1].text, "ligne deux\r");
        assert_eq!(lines[1].line_no, 2);
        assert_eq!(lines[2].commit, Oid::from_hex(B).unwrap());
        assert_eq!(lines[2].text, "");
        let a = &commits[&Oid::from_hex(A).unwrap()];
        assert_eq!((a.author.as_str(), a.author_time, a.summary.as_str()), ("Camille", 1_700_000_000, "Premier"));
        assert_eq!(commits[&Oid::from_hex(B).unwrap()].author, "Zoé");
    }
}
