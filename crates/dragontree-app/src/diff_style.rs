//! Apparence du texte des diffs (Réglages › Diff et commit) : police, taille, largeur de tabulation, espaces visibles.
//!
//! Le même style sert aux vues de diff (unifiée, côte à côte), au blame et à l'éditeur de conflits. Les marqueurs
//! d'espaces (`·` et `→`) ne sont qu'un habillage d'affichage : le texte copié est toujours l'original.

use std::collections::BTreeSet;

use dragontree_git::FileDiff;
use dragontree_i18n::tr;
use egui::{FontFamily, FontId};
use serde::{Deserialize, Serialize};

use crate::settings::Settings;

/// Famille egui de la police du diff : toujours déclarée (voir `theme::install_fonts`), même si le fichier de police
/// choisi est absent (elle retombe alors sur la chasse fixe intégrée).
pub const DIFF_FAMILY: &str = "diff";

/// Largeurs de tabulation proposées.
pub const TAB_WIDTHS: [usize; 3] = [2, 4, 8];
pub const FONT_SIZE_MIN: f32 = 9.0;
pub const FONT_SIZE_MAX: f32 = 24.0;

/// Police du diff : la chasse fixe intégrée ou une police monospace système (dossier `Fonts` de Windows).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffFont {
    #[default]
    Builtin,
    Consolas,
    CascadiaMono,
    CourierNew,
}

impl DiffFont {
    pub const ALL: [DiffFont; 4] =
        [DiffFont::Builtin, DiffFont::Consolas, DiffFont::CascadiaMono, DiffFont::CourierNew];

    /// Nom affiché (les noms de polices ne se traduisent pas).
    pub fn label(self) -> &'static str {
        match self {
            DiffFont::Builtin => tr!("Chasse fixe intégrée"),
            DiffFont::Consolas => "Consolas",
            DiffFont::CascadiaMono => "Cascadia Mono",
            DiffFont::CourierNew => "Courier New",
        }
    }

    /// Fichiers candidats dans `C:\Windows\Fonts` (`None` : police intégrée).
    pub fn file_names(self) -> &'static [&'static str] {
        match self {
            DiffFont::Builtin => &[],
            DiffFont::Consolas => &["consola.ttf"],
            DiffFont::CascadiaMono => &["CascadiaMono.ttf", "CascadiaMonoPL.ttf"],
            DiffFont::CourierNew => &["cour.ttf"],
        }
    }

    /// Contenu du fichier de police ; `None` pour la police intégrée ou un fichier absent (repli sur l'intégrée).
    pub fn load(self) -> Option<Vec<u8>> {
        let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from)?.join("Fonts");
        self.file_names().iter().find_map(|n| std::fs::read(dir.join(n)).ok())
    }

    /// Choix proposés dans les réglages : l'intégrée et les polices système trouvées.
    pub fn available() -> Vec<DiffFont> {
        let dir = std::env::var_os("WINDIR").map(|w| std::path::PathBuf::from(w).join("Fonts"));
        DiffFont::ALL
            .into_iter()
            .filter(|f| {
                *f == DiffFont::Builtin
                    || dir.as_ref().is_some_and(|d| f.file_names().iter().any(|n| d.join(n).is_file()))
            })
            .collect()
    }
}

/// Style du texte d'un diff, tiré des réglages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffStyle {
    pub font_size: f32,
    pub tab_width: usize,
    pub show_whitespace: bool,
}

impl Default for DiffStyle {
    fn default() -> Self {
        DiffStyle { font_size: 12.5, tab_width: 4, show_whitespace: false }
    }
}

impl DiffStyle {
    pub fn from_settings(s: &Settings) -> DiffStyle {
        DiffStyle {
            font_size: s.diff_font_size.clamp(FONT_SIZE_MIN, FONT_SIZE_MAX),
            tab_width: usize::from(s.diff_tab_width).max(1),
            show_whitespace: s.diff_show_whitespace,
        }
    }

    pub fn font_id(&self) -> FontId {
        FontId::new(self.font_size, FontFamily::Name(DIFF_FAMILY.into()))
    }

    /// Police des numéros de ligne et des en-têtes de hunk, un peu plus petite.
    pub fn small_font_id(&self, delta: f32) -> FontId {
        FontId::new((self.font_size - delta).max(8.0), FontFamily::Name(DIFF_FAMILY.into()))
    }

    /// Texte tout prêt à afficher d'un seul tenant (blame, conflits) : tabulations développées, marqueurs si demandés.
    pub fn render(&self, raw: &str) -> String {
        if self.show_whitespace { whitespace_markers(raw, self.tab_width) } else { expand_tabs(raw, self.tab_width) }
    }
}

/// Texte affiché : tabulations développées selon la colonne (une tabulation avance jusqu'au prochain multiple de
/// `tab_width`), `\r` final retiré.
pub fn expand_tabs(raw: &str, tab_width: usize) -> String {
    let raw = raw.strip_suffix('\r').unwrap_or(raw);
    if !raw.contains('\t') {
        return raw.to_string();
    }
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(raw.len() + 8);
    let mut col = 0;
    for c in raw.chars() {
        if c == '\t' {
            let n = tab_width - col % tab_width;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Même texte que [`expand_tabs`] (même nombre de caractères, colonne par colonne) avec les espaces visibles :
/// `·` pour un espace, `→` au début de chaque tabulation (complétée d'espaces jusqu'à la colonne suivante).
pub fn whitespace_markers(raw: &str, tab_width: usize) -> String {
    let raw = raw.strip_suffix('\r').unwrap_or(raw);
    let tab_width = tab_width.max(1);
    let mut out = String::with_capacity(raw.len() + 8);
    let mut col = 0;
    for c in raw.chars() {
        match c {
            '\t' => {
                let n = tab_width - col % tab_width;
                out.push('→');
                out.extend(std::iter::repeat_n(' ', n - 1));
                col += n;
            }
            ' ' => {
                out.push('·');
                col += 1;
            }
            c => {
                out.push(c);
                col += 1;
            }
        }
    }
    out
}

/// Lignes à copier pour un clic droit sur `clicked` : la sélection si elle contient la ligne, sinon la ligne seule.
pub fn lines_to_copy(selected: &BTreeSet<(usize, usize)>, clicked: (usize, usize)) -> Vec<(usize, usize)> {
    if selected.contains(&clicked) { selected.iter().copied().collect() } else { vec![clicked] }
}

/// Texte copié : les lignes d'origine du fichier, une par ligne, sans marqueur (`+`, `-`, `·`, `→`) ni tabulation
/// développée.
pub fn copy_text(diff: &FileDiff, lines: &[(usize, usize)]) -> String {
    let mut out = String::new();
    for &(h, l) in lines {
        if let Some(line) = diff.hunks.get(h).and_then(|hk| hk.lines.get(l)) {
            out.push_str(line.text.strip_suffix('\r').unwrap_or(&line.text));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dragontree_git::diff::parse_unified_diff;

    #[test]
    fn tabs_follow_the_column() {
        assert_eq!(expand_tabs("\tx\r", 4), "    x");
        assert_eq!(expand_tabs("ab\tc", 4), "ab  c");
        assert_eq!(expand_tabs("abc\td", 4), "abc d");
        assert_eq!(expand_tabs("abcd\te", 4), "abcd    e");
        assert_eq!(expand_tabs("a\tb", 8), "a       b");
        assert_eq!(expand_tabs("a\tb", 2), "a b");
        assert_eq!(expand_tabs("\t\tx", 2), "    x");
        // Largeur nulle : repli sur 1 colonne, jamais de division par zéro.
        assert_eq!(expand_tabs("a\tb", 0), "a b");
    }

    #[test]
    fn whitespace_markers_keep_the_columns() {
        assert_eq!(whitespace_markers("a b", 4), "a·b");
        assert_eq!(whitespace_markers("\tx", 4), "→   x");
        assert_eq!(whitespace_markers("ab\tc", 4), "ab→ c");
        for raw in ["\tx y\t", "  \t z", "plain"] {
            assert_eq!(whitespace_markers(raw, 4).chars().count(), expand_tabs(raw, 4).chars().count(), "{raw:?}");
        }
    }

    fn sample() -> FileDiff {
        let text = ["diff --git a/f b/f", "--- a/f", "+++ b/f", "@@ -1,2 +1,2 @@", " a b", "-\told", "+\tnew x"]
            .join("\n")
            + "\n";
        parse_unified_diff(&text).remove(0)
    }

    #[test]
    fn copied_text_has_no_markers() {
        let diff = sample();
        let selected: BTreeSet<_> = [(0, 1), (0, 2)].into_iter().collect();
        let text = copy_text(&diff, &lines_to_copy(&selected, (0, 2)));
        assert_eq!(text, "\told\n\tnew x\n");
        // Clic sur une ligne hors sélection : seule cette ligne est copiée.
        assert_eq!(copy_text(&diff, &lines_to_copy(&selected, (0, 0))), "a b\n");
        assert!(!text.contains('·') && !text.contains('→') && !text.starts_with(['+', '-']));
    }

    #[test]
    fn style_reads_settings_with_bounds() {
        let s = Settings { diff_font_size: 99.0, diff_tab_width: 0, diff_show_whitespace: true, ..Settings::default() };
        let style = DiffStyle::from_settings(&s);
        assert_eq!((style.font_size, style.tab_width, style.show_whitespace), (FONT_SIZE_MAX, 1, true));
        assert_eq!(DiffStyle::default().render("\ta"), "    a");
        assert_eq!(style.render(" a"), "·a");
    }
}
