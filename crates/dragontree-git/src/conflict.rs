//! Lecture et résolution des marqueurs de conflit (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`).

use crate::error::GitError;
use crate::repo::Repo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRegion {
    /// Index (0-based) de la ligne `<<<<<<<`.
    pub start_line: usize,
    /// Index de la ligne `|||||||` (style diff3), si présente.
    pub base_line: Option<usize>,
    /// Index de la ligne `=======`.
    pub sep_line: usize,
    /// Index de la ligne `>>>>>>>`.
    pub end_line: usize,
    pub ours_label: String,
    pub theirs_label: String,
    /// Lignes sans leur `\n` final (un éventuel `\r` est conservé).
    pub ours: Vec<String>,
    pub base: Option<Vec<String>>,
    pub theirs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Ours,
    Theirs,
    /// Les deux : d'abord la nôtre, puis la leur.
    Both,
    /// Les deux : d'abord la leur, puis la nôtre.
    BothReversed,
}

/// Libellé d'une ligne marqueur de 7 caractères `ch` (exactement 7, puis espace ou fin).
fn marker(line: &str, ch: u8) -> Option<&str> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let bytes = line.as_bytes();
    if bytes.len() < 7 || !bytes[..7].iter().all(|&b| b == ch) {
        return None;
    }
    match bytes.get(7) {
        None => Some(""),
        Some(b' ') => Some(&line[8..]),
        _ => None,
    }
}

fn split_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
}

pub fn parse_conflicts(text: &str) -> Vec<ConflictRegion> {
    enum State {
        Outside,
        Ours,
        Base,
        Theirs,
    }
    let mut regions = Vec::new();
    let mut state = State::Outside;
    let mut current: Option<ConflictRegion> = None;

    for (i, line) in split_lines(text).into_iter().enumerate() {
        match state {
            State::Outside => {
                if let Some(label) = marker(line, b'<') {
                    current = Some(ConflictRegion {
                        start_line: i,
                        base_line: None,
                        sep_line: i,
                        end_line: i,
                        ours_label: label.to_string(),
                        theirs_label: String::new(),
                        ours: Vec::new(),
                        base: None,
                        theirs: Vec::new(),
                    });
                    state = State::Ours;
                }
            }
            State::Ours | State::Base => {
                let Some(r) = current.as_mut() else { continue };
                if marker(line, b'|').is_some() && matches!(state, State::Ours) {
                    r.base_line = Some(i);
                    r.base = Some(Vec::new());
                    state = State::Base;
                } else if marker(line, b'=') == Some("") {
                    r.sep_line = i;
                    state = State::Theirs;
                } else if matches!(state, State::Ours) {
                    r.ours.push(line.to_string());
                } else if let Some(base) = r.base.as_mut() {
                    base.push(line.to_string());
                }
            }
            State::Theirs => {
                let Some(r) = current.as_mut() else { continue };
                if let Some(label) = marker(line, b'>') {
                    r.end_line = i;
                    r.theirs_label = label.to_string();
                    regions.extend(current.take());
                    state = State::Outside;
                } else {
                    r.theirs.push(line.to_string());
                }
            }
        }
    }
    regions
}

/// Remplace les régions de conflit par les choix (dans l'ordre) ; une région sans choix est
/// conservée telle quelle. Les fins de ligne d'origine sont préservées.
pub fn resolve(text: &str, choices: &[Resolution]) -> String {
    let pieces: Vec<&str> = text.split_inclusive('\n').collect();
    let regions = parse_conflicts(text);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut r = 0;
    while i < pieces.len() {
        let Some(region) = regions.get(r).filter(|reg| reg.start_line == i) else {
            out.push_str(pieces[i]);
            i += 1;
            continue;
        };
        let ours = &pieces[region.start_line + 1..region.base_line.unwrap_or(region.sep_line)];
        let theirs = &pieces[region.sep_line + 1..region.end_line];
        let chosen: Vec<&[&str]> = match choices.get(r) {
            None => vec![&pieces[region.start_line..=region.end_line]],
            Some(Resolution::Ours) => vec![ours],
            Some(Resolution::Theirs) => vec![theirs],
            Some(Resolution::Both) => vec![ours, theirs],
            Some(Resolution::BothReversed) => vec![theirs, ours],
        };
        for block in chosen {
            for piece in block {
                out.push_str(piece);
            }
        }
        i = region.end_line + 1;
        r += 1;
    }
    out
}

impl Repo {
    /// Lance l'outil de merge configuré (`git mergetool --no-prompt`) sur un fichier.
    pub fn run_mergetool(&self, path: &str) -> Result<(), GitError> {
        self.write_paths().args(["mergetool", "--no-prompt", "--", path]).run().map(|_| ())
    }

    /// Remplace un fichier en conflit par notre version (`ours`) ou la leur.
    pub fn checkout_side(&self, path: &str, ours: bool) -> Result<(), GitError> {
        let side = if ours { "--ours" } else { "--theirs" };
        self.write_paths().args(["checkout", side, "--", path]).run().map(|_| ())
    }
}
