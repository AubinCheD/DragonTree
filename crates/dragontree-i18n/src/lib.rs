//! Traduction de l'interface de DragonTree.
//!
//! Le texte français écrit dans le code sert de clé. Chaque autre langue a un catalogue
//! `locales/<code>.json` : un objet JSON « texte français » → « traduction ». Un texte absent d'un
//! catalogue reste en français ; le test `tests/catalog.rs` vérifie qu'il n'en manque aucun.
//!
//! - `tr!("Fermer l'onglet")` donne un `&'static str`.
//! - `tr!("Supprimer « {name} » ?", name = branch)` (ou `tr!("… {name} …", name)` pour une variable du
//!   même nom) donne une `String`. Les paramètres s'écrivent `{nom}` dans le texte et ses traductions.
//! - `trn!(count, "{n} fichier", "{n} fichiers")` choisit la forme selon le nombre et la langue
//!   (`{n}` est fourni d'office).
//! - `tr_mark!("Fetch")` marque un texte rangé dans une constante, traduit à l'affichage par [`tr_str`].
//!
//! La langue est globale au processus ([`set_lang`]) : un changement s'applique à l'image suivante.
//!
//! Ajouter une langue : une variante de [`Lang`] (avec son code et son nom), son catalogue
//! `locales/<code>.json` et une entrée dans [`CATALOGS`].

use std::collections::HashMap;
use std::fmt::{Display, Write as _};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};

/// Langue de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    Fr,
    En,
}

/// Catalogues des langues autres que le français (langue des clés).
pub const CATALOGS: &[(Lang, &str)] = &[(Lang::En, include_str!("../locales/en.json"))];

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Fr, Lang::En];

    /// Code ISO 639-1 (`fr`, `en`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Fr => "fr",
            Lang::En => "en",
        }
    }

    /// Nom de la langue dans cette langue, affiché tel quel dans le choix de la langue.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
        }
    }

    /// Langue d'après un nom de locale (`fr`, `fr-FR`, `en_US.UTF-8`…) ; `None` si elle n'est pas gérée.
    pub fn from_locale(name: &str) -> Option<Lang> {
        let primary = name.split(['-', '_', '.']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == primary)
    }

    /// Langue d'affichage du système si elle est gérée, l'anglais sinon.
    pub fn system() -> Lang {
        system_locale().and_then(|name| Lang::from_locale(&name)).unwrap_or(Lang::En)
    }
}

#[cfg(windows)]
fn system_locale() -> Option<String> {
    use windows_sys::Win32::Globalization::{GetUserDefaultUILanguage, LCIDToLocaleName};
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    // SAFETY : le tampon et sa taille sont cohérents ; la fonction écrit au plus `buf.len()` caractères.
    let len = unsafe { LCIDToLocaleName(u32::from(GetUserDefaultUILanguage()), buf.as_mut_ptr(), buf.len() as i32, 0) };
    // `len` compte le zéro final ; 0 signale une erreur.
    (len > 1).then(|| String::from_utf16_lossy(&buf[..len as usize - 1]))
}

#[cfg(not(windows))]
fn system_locale() -> Option<String> {
    ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Change la langue de l'interface pour tout le processus.
pub fn set_lang(lang: Lang) {
    let index = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(0);
    CURRENT.store(index as u8, Ordering::Relaxed);
}

/// Langue courante de l'interface.
pub fn lang() -> Lang {
    Lang::ALL.get(CURRENT.load(Ordering::Relaxed) as usize).copied().unwrap_or_default()
}

/// Analyse un catalogue ; un catalogue illisible est vide (le test des catalogues le signale).
pub fn parse_catalog(json: &str) -> HashMap<String, String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn catalog(lang: Lang) -> Option<&'static HashMap<String, String>> {
    static PARSED: OnceLock<Vec<(Lang, HashMap<String, String>)>> = OnceLock::new();
    let parsed = PARSED.get_or_init(|| CATALOGS.iter().map(|(l, json)| (*l, parse_catalog(json))).collect());
    parsed.iter().find(|(l, _)| *l == lang).map(|(_, map)| map)
}

/// Traduction d'un texte français dans `lang` (le texte lui-même s'il n'a pas de traduction).
pub fn translate(lang: Lang, fr: &'static str) -> &'static str {
    match catalog(lang) {
        Some(map) => map.get(fr).map_or(fr, String::as_str),
        None => fr,
    }
}

/// Traduction d'un texte français dans la langue courante. Pour un texte écrit dans le code, préférer
/// `tr!` ; cette fonction sert aux textes marqués par `tr_mark!`.
pub fn tr_str(fr: &'static str) -> &'static str {
    translate(lang(), fr)
}

/// Forme à employer pour `n` : 0 pour le singulier, 1 pour le pluriel.
pub fn plural_form(lang: Lang, n: u64) -> usize {
    match lang {
        Lang::Fr => usize::from(n > 1),
        Lang::En => usize::from(n != 1),
    }
}

/// Remplace les paramètres `{nom}` de `template`. Un paramètre inconnu est laissé tel quel.
#[doc(hidden)]
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = &after[..close];
        match args.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => {
                let _ = write!(out, "{value}");
            }
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Texte traduit : `tr!("Texte")` → `&'static str` ; `tr!("… {nom} …", nom = valeur)` → `String`.
#[macro_export]
macro_rules! tr {
    ($fr:literal $(,)?) => {
        $crate::tr_str($fr)
    };
    ($fr:literal, $($args:tt)+) => {
        $crate::fill($crate::tr_str($fr), &$crate::__tr_args!([] $($args)+))
    };
}

/// Texte traduit selon un nombre : `trn!(n, "{n} fichier", "{n} fichiers")` → `String`.
#[macro_export]
macro_rules! trn {
    ($n:expr, $one:literal, $other:literal $(, $($args:tt)+)? $(,)?) => {{
        let __n = $n;
        let __lang = $crate::lang();
        let __fr = if $crate::plural_form(__lang, __n as u64) == 0 { $one } else { $other };
        $crate::fill(
            $crate::translate(__lang, __fr),
            &$crate::__tr_args!([("n", &__n as &dyn ::std::fmt::Display),] $($($args)+)?),
        )
    }};
}

/// Marque un texte à traduire rangé dans une constante ; le traduire à l'affichage avec [`tr_str`].
#[macro_export]
macro_rules! tr_mark {
    ($fr:literal) => {
        $fr
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __tr_args {
    ([$($done:tt)*]) => {
        [$($done)*]
    };
    ([$($done:tt)*] $k:ident = $v:expr $(, $($rest:tt)*)?) => {
        $crate::__tr_args!([$($done)* (stringify!($k), &$v as &dyn ::std::fmt::Display),] $($($rest)*)?)
    };
    ([$($done:tt)*] $k:ident $(, $($rest:tt)*)?) => {
        $crate::__tr_args!([$($done)* (stringify!($k), &$k as &dyn ::std::fmt::Display),] $($($rest)*)?)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_names_map_to_languages() {
        assert_eq!(Lang::from_locale("fr"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("fr-FR"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("fr-CA"), Some(Lang::Fr));
        assert_eq!(Lang::from_locale("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(Lang::from_locale("EN-gb"), Some(Lang::En));
        assert_eq!(Lang::from_locale("de-DE"), None);
        assert_eq!(Lang::from_locale(""), None);
    }

    #[test]
    fn language_is_saved_as_its_code() {
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"fr\"").unwrap(), Lang::Fr);
    }

    #[test]
    fn fill_replaces_named_parameters() {
        let branch = "main";
        let n = 3;
        assert_eq!(fill("Fusionner « {b} » ({n})", &[("b", &branch), ("n", &n)]), "Fusionner « main » (3)");
    }

    #[test]
    fn fill_keeps_unknown_parameters_and_lone_braces() {
        assert_eq!(fill("stash@{0} {x", &[]), "stash@{0} {x");
        assert_eq!(fill("{a}{a}", &[("a", &1)]), "11");
    }

    #[test]
    fn french_plural_includes_zero_and_one() {
        assert_eq!(plural_form(Lang::Fr, 0), 0);
        assert_eq!(plural_form(Lang::Fr, 1), 0);
        assert_eq!(plural_form(Lang::Fr, 2), 1);
        assert_eq!(plural_form(Lang::En, 0), 1);
        assert_eq!(plural_form(Lang::En, 1), 0);
        assert_eq!(plural_form(Lang::En, 2), 1);
    }

    #[test]
    fn french_text_is_its_own_translation() {
        assert_eq!(translate(Lang::Fr, "Texte sans traduction"), "Texte sans traduction");
        assert_eq!(translate(Lang::En, "Texte absent du catalogue"), "Texte absent du catalogue");
    }

    #[test]
    fn parse_catalog_reads_a_json_object() {
        let map = parse_catalog(r#"{ "Fermer l'onglet": "Close tab", "« {name} »": "\"{name}\"" }"#);
        assert_eq!(map["Fermer l'onglet"], "Close tab");
        assert_eq!(map["« {name} »"], "\"{name}\"");
        assert!(parse_catalog("pas du json").is_empty());
    }

    #[test]
    fn macros_build_texts() {
        // Langue courante par défaut (français) : aucun test de ce binaire ne la change.
        let name = "main";
        assert_eq!(tr!("Texte simple"), "Texte simple");
        assert_eq!(tr!("Branche « {name} »", name), "Branche « main »");
        assert_eq!(tr!("{a} et {b}", a = 1, b = format!("x{}", 2),), "1 et x2");
        assert_eq!(trn!(1usize, "{n} fichier", "{n} fichiers"), "1 fichier");
        assert_eq!(trn!(4u32, "{n} fichier de {who}", "{n} fichiers de {who}", who = name), "4 fichiers de main");
        const MARKED: &str = tr_mark!("Texte marqué");
        assert_eq!(tr_str(MARKED), "Texte marqué");
    }
}
