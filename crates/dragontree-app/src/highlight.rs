//! Coloration syntaxique des diffs (syntect, chargé à la première utilisation).
//!
//! Les deux côtés du diff sont reconstitués (ancien : contexte + supprimées ; nouveau :
//! contexte + ajoutées) puis colorés ligne à ligne, ce qui conserve l'état du parseur.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::OnceLock;

use dragontree_git::{FileDiff, LineKind};
use egui::Color32;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

use crate::repo::diff_view::display_text;

/// Au-delà, pas de coloration (le diff resterait fluide mais le calcul serait trop long).
const MAX_LINES: usize = 3000;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme(dark: bool) -> &'static Theme {
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let set = THEMES.get_or_init(ThemeSet::load_defaults);
    let name = if dark { "base16-ocean.dark" } else { "InspiredGitHub" };
    set.themes.get(name).or_else(|| set.themes.values().next()).expect("thèmes syntect par défaut")
}

type Spans = Vec<(Range<usize>, Color32)>;

#[derive(Default)]
pub struct Highlighter {
    key: Option<(String, usize, bool)>,
    lines: HashMap<(usize, usize), Spans>,
    dark: bool,
}

impl Highlighter {
    pub fn new(dark: bool) -> Highlighter {
        Highlighter { dark, ..Default::default() }
    }

    pub fn is_dark(&self) -> bool {
        self.dark
    }

    /// Calcule les couleurs si le diff a changé.
    pub fn prepare(&mut self, diff: &FileDiff) {
        let total: usize = diff.hunks.iter().map(|h| h.lines.len()).sum();
        let key = (diff.display_path().to_string(), total, self.dark);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        self.lines.clear();
        if total == 0 || total > MAX_LINES {
            return;
        }
        let ss = syntaxes();
        let path = diff.display_path();
        let ext =
            std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let syntax = match ext.as_str() {
            "csproj" | "props" | "targets" | "config" | "resx" | "xaml" | "nuspec" => {
                ss.find_syntax_by_extension("xml")
            }
            "ts" | "tsx" | "mjs" | "cjs" => ss.find_syntax_by_extension("js"),
            "cshtml" | "razor" => ss.find_syntax_by_extension("html"),
            _ => ss.find_syntax_by_extension(&ext),
        };
        let Some(syntax) = syntax else { return };
        let theme = theme(self.dark);

        for side_old in [true, false] {
            let mut h = HighlightLines::new(syntax, theme);
            for (hi, hunk) in diff.hunks.iter().enumerate() {
                for (li, line) in hunk.lines.iter().enumerate() {
                    let on_side = match line.kind {
                        LineKind::Context => !side_old,
                        LineKind::Removed => side_old,
                        LineKind::Added => !side_old,
                    };
                    // Le contexte fait avancer l'état du parseur des deux côtés.
                    if !on_side && !(line.kind == LineKind::Context && side_old) {
                        continue;
                    }
                    let text = display_text(&line.text);
                    let with_nl = format!("{text}\n");
                    let Ok(ranges) = h.highlight_line(&with_nl, ss) else { continue };
                    if !on_side {
                        continue;
                    }
                    let mut spans = Vec::with_capacity(ranges.len());
                    let mut offset = 0;
                    for (style, piece) in ranges {
                        let end = (offset + piece.len()).min(text.len());
                        if offset < end {
                            let c = style.foreground;
                            spans.push((offset..end, Color32::from_rgb(c.r, c.g, c.b)));
                        }
                        offset += piece.len();
                    }
                    self.lines.insert((hi, li), spans);
                }
            }
        }
    }

    pub fn colors(&mut self, _diff: &FileDiff, h: usize, l: usize, _text: &str, _dark: bool) -> Option<Spans> {
        self.lines.get(&(h, l)).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dragontree_git::diff::parse_unified_diff;

    #[test]
    fn rust_code_gets_several_colors() {
        let text = [
            "diff --git a/x.rs b/x.rs",
            "--- a/x.rs",
            "+++ b/x.rs",
            "@@ -1 +1 @@",
            "-fn main() {}",
            "+fn main() { let x = 1; }",
        ]
        .join("\n")
            + "\n";
        let diff = parse_unified_diff(&text).remove(0);
        let mut h = Highlighter::new(false);
        h.prepare(&diff);
        let spans = h.colors(&diff, 0, 1, "", false).expect("ligne ajoutée colorée");
        let distinct: std::collections::HashSet<_> = spans.iter().map(|(_, c)| *c).collect();
        assert!(distinct.len() >= 2, "{spans:?}");
        assert_eq!(spans.last().unwrap().0.end, "fn main() { let x = 1; }".len());
    }

    #[test]
    fn unknown_extension_is_not_colored() {
        let text =
            ["diff --git a/x.zzz b/x.zzz", "--- a/x.zzz", "+++ b/x.zzz", "@@ -1 +1 @@", "-a", "+b"].join("\n") + "\n";
        let diff = parse_unified_diff(&text).remove(0);
        let mut h = Highlighter::new(false);
        h.prepare(&diff);
        assert!(h.colors(&diff, 0, 1, "", false).is_none());
    }
}
