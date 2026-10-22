//! Lecture et résolution des marqueurs de conflit (`<<<<<<<`, `|||||||`, `=======`, `>>>>>>>`).
//!
//! Sur les octets du fichier (L2) : les marqueurs sont en ASCII, et la résolution (`resolve_bytes`,
//! `resolve_partial_bytes`) recopie les octets d'origine de chaque morceau gardé (hors conflit, côtés choisis), quel
//! que soit l'encodage du fichier (UTF-8, Windows-1252…) : rien n'est réencodé. Le texte affiché est décodé ligne à
//! ligne (`decode_conflict_text`) : UTF-8 quand la ligne en est, sinon Windows-1252, un octet par caractère. Les
//! lignes et les marqueurs du texte décodé sont donc ceux des octets, et `parse_conflicts` donne les mêmes régions sur
//! l'un et sur l'autre.

use crate::diff::decode_cp1252;
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
fn marker(line: &[u8], ch: u8) -> Option<&[u8]> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.len() < 7 || !line[..7].iter().all(|&b| b == ch) {
        return None;
    }
    match line.get(7) {
        None => Some(b""),
        Some(b' ') => Some(&line[8..]),
        _ => None,
    }
}

/// Lignes sans leur `\n` (rien après un `\n` final).
fn split_lines(bytes: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = bytes.split(|&b| b == b'\n').collect();
    if bytes.ends_with(b"\n") {
        lines.pop();
    }
    lines
}

/// Lignes avec leur `\n` : les morceaux recopiés par la résolution.
fn split_pieces(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|&b| b == b'\n').collect()
}

/// Texte d'une ligne pour l'affichage : UTF-8 si elle en est, sinon Windows-1252.
fn decode_line(line: &[u8]) -> String {
    match std::str::from_utf8(line) {
        Ok(text) => text.to_string(),
        Err(_) => decode_cp1252(line),
    }
}

/// Texte affiché d'un fichier en conflit, décodé ligne à ligne : UTF-8 quand la ligne en est, sinon Windows-1252
/// (« ANSI » du vieux code .NET français). Mêmes lignes et mêmes marqueurs que les octets.
pub fn decode_conflict_text(bytes: &[u8]) -> String {
    split_pieces(bytes).into_iter().map(decode_line).collect()
}

pub fn parse_conflicts(text: &str) -> Vec<ConflictRegion> {
    // Lignes d'un texte UTF-8 coupé sur `\n` : chacune est de l'UTF-8 valide.
    parse_lines(&split_lines(text.as_bytes()), |line| String::from_utf8_lossy(line).into_owned())
}

/// Régions de conflit des lignes `lines` ; libellés et lignes des côtés décodés par `decode`.
fn parse_lines(lines: &[&[u8]], decode: impl Fn(&[u8]) -> String) -> Vec<ConflictRegion> {
    enum State {
        Outside,
        Ours,
        Base,
        Theirs,
    }
    let mut regions = Vec::new();
    let mut state = State::Outside;
    let mut current: Option<ConflictRegion> = None;

    for (i, &line) in lines.iter().enumerate() {
        match state {
            State::Outside => {
                if let Some(label) = marker(line, b'<') {
                    current = Some(ConflictRegion {
                        start_line: i,
                        base_line: None,
                        sep_line: i,
                        end_line: i,
                        ours_label: decode(label),
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
                } else if marker(line, b'=') == Some(b"") {
                    r.sep_line = i;
                    state = State::Theirs;
                } else if matches!(state, State::Ours) {
                    r.ours.push(decode(line));
                } else if let Some(base) = r.base.as_mut() {
                    base.push(decode(line));
                }
            }
            State::Theirs => {
                let Some(r) = current.as_mut() else { continue };
                if let Some(label) = marker(line, b'>') {
                    r.end_line = i;
                    r.theirs_label = decode(label);
                    regions.extend(current.take());
                    state = State::Outside;
                } else {
                    r.theirs.push(decode(line));
                }
            }
        }
    }
    regions
}

/// Remplace les régions de conflit par les choix (dans l'ordre) ; une région sans choix est
/// conservée telle quelle. Les fins de ligne d'origine sont préservées.
pub fn resolve(text: &str, choices: &[Resolution]) -> String {
    let choices: Vec<Option<Resolution>> = choices.iter().copied().map(Some).collect();
    resolve_partial(text, &choices)
}

/// Comme [`resolve`], avec des choix éventuellement absents : une région sans choix (`None`) est conservée telle
/// quelle, marqueurs compris. Sert à l'aperçu du résultat au fil des choix.
pub fn resolve_partial(text: &str, choices: &[Option<Resolution>]) -> String {
    // Lignes entières d'un texte UTF-8 : le résultat en est aussi.
    match String::from_utf8(resolve_partial_bytes(text.as_bytes(), choices)) {
        Ok(out) => out,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    }
}

/// [`resolve`] sur les octets du fichier : le résultat ne contient que des octets d'origine (voir le module).
pub fn resolve_bytes(bytes: &[u8], choices: &[Resolution]) -> Vec<u8> {
    let choices: Vec<Option<Resolution>> = choices.iter().copied().map(Some).collect();
    resolve_partial_bytes(bytes, &choices)
}

/// [`resolve_partial`] sur les octets du fichier : chaque morceau gardé (hors conflit, côtés choisis, région sans
/// choix) est recopié octet pour octet, fins de ligne comprises.
pub fn resolve_partial_bytes(bytes: &[u8], choices: &[Option<Resolution>]) -> Vec<u8> {
    let pieces = split_pieces(bytes);
    // Seuls les indices des marqueurs servent ici : rien à décoder.
    let regions = parse_lines(&split_lines(bytes), |_| String::new());
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut r = 0;
    while i < pieces.len() {
        let Some(region) = regions.get(r).filter(|reg| reg.start_line == i) else {
            out.extend_from_slice(pieces[i]);
            i += 1;
            continue;
        };
        let ours = &pieces[region.start_line + 1..region.base_line.unwrap_or(region.sep_line)];
        let theirs = &pieces[region.sep_line + 1..region.end_line];
        let chosen: Vec<&[&[u8]]> = match choices.get(r).copied().flatten() {
            None => vec![&pieces[region.start_line..=region.end_line]],
            Some(Resolution::Ours) => vec![ours],
            Some(Resolution::Theirs) => vec![theirs],
            Some(Resolution::Both) => vec![ours, theirs],
            Some(Resolution::BothReversed) => vec![theirs, ours],
        };
        for block in chosen {
            for piece in block {
                out.extend_from_slice(piece);
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

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str =
        "a\n<<<<<<< HEAD\nnous\n=======\neux\n>>>>>>> autre\nb\n<<<<<<< HEAD\nx\n=======\ny\n>>>>>>> autre\nc\n";

    #[test]
    fn resolve_partial_keeps_the_regions_without_a_choice() {
        let out = resolve_partial(TEXT, &[None, Some(Resolution::Theirs)]);
        assert_eq!(out, "a\n<<<<<<< HEAD\nnous\n=======\neux\n>>>>>>> autre\nb\ny\nc\n");
        assert_eq!(resolve_partial(TEXT, &[]), TEXT);
        assert_eq!(resolve(TEXT, &[Resolution::Ours, Resolution::Both]), "a\nnous\nb\nx\ny\nc\n");
    }

    #[test]
    fn resolve_partial_handles_diff3_regions_with_a_base_section() {
        let text = "a\n<<<<<<< HEAD\nnous\n||||||| base\navant\n=======\neux\n>>>>>>> autre\nb\n";
        assert_eq!(resolve_partial(text, &[None]), text, "région sans choix : conservée telle quelle");
        assert_eq!(resolve_partial(text, &[Some(Resolution::Ours)]), "a\nnous\nb\n");
        assert_eq!(resolve_partial(text, &[Some(Resolution::Theirs)]), "a\neux\nb\n");
        assert_eq!(resolve_partial(text, &[Some(Resolution::Both)]), "a\nnous\neux\nb\n");
        assert_eq!(resolve_partial(text, &[Some(Resolution::BothReversed)]), "a\neux\nnous\nb\n");
    }

    /// L2 : octets Windows-1252 (é = E9, € = 80), fins de ligne CRLF et dernière ligne sans `\n` : affichage décodé,
    /// résolution sur les octets d'origine.
    #[test]
    fn cp1252_bytes_are_decoded_for_display_and_copied_back_unchanged() {
        let bytes: &[u8] =
            b"caf\xe9\r\n<<<<<<< HEAD\r\nprix 1 \x80\r\n=======\r\nprix 2 \x80\r\n>>>>>>> autre \xe9t\xe9\r\nfin \xe9";
        let text = decode_conflict_text(bytes);
        assert_eq!(text, "café\r\n<<<<<<< HEAD\r\nprix 1 €\r\n=======\r\nprix 2 €\r\n>>>>>>> autre été\r\nfin é");
        let regions = parse_conflicts(&text);
        assert_eq!(regions.len(), 1);
        assert_eq!(
            (regions[0].ours.as_slice(), regions[0].theirs_label.as_str()),
            (&["prix 1 €\r".to_string()][..], "autre été")
        );
        assert_eq!(resolve_bytes(bytes, &[Resolution::Theirs]), b"caf\xe9\r\nprix 2 \x80\r\nfin \xe9");
        assert_eq!(resolve_partial_bytes(bytes, &[None]), bytes, "sans choix : octets inchangés");
        // Une ligne UTF-8 dans un fichier qui ne l'est pas reste lue en UTF-8.
        assert_eq!(decode_conflict_text("é\n".as_bytes()), "é\n");
        assert_eq!(decode_conflict_text(b"\xe9\n\xc3\xa9\n"), "é\né\n");
    }
}
