//! Cohérence des catalogues avec le code : chaque texte passé à `tr!`, `trn!` ou `tr_mark!` dans les
//! crates du workspace a une traduction dans chaque catalogue, aucune traduction n'est orpheline et les
//! paramètres `{nom}` sont les mêmes de part et d'autre.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dragontree_i18n::{CATALOGS, Lang};

/// Textes à traduire trouvés dans le code : texte → premier emplacement (`fichier:ligne`).
fn keys_in_sources() -> (BTreeMap<String, String>, Vec<String>) {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap() {
        let dir = entry.unwrap().path();
        // Le crate de traduction ne contient que des exemples dans sa documentation.
        if dir.file_name().is_some_and(|n| n == "dragontree-i18n") {
            continue;
        }
        collect_rs(&dir.join("src"), &mut files);
    }
    let mut keys = BTreeMap::new();
    let mut errors = Vec::new();
    for file in files {
        let text = normalize_newlines(&std::fs::read_to_string(&file).unwrap());
        let shown = file.strip_prefix(crates).unwrap_or(&file).display().to_string();
        scan(&text, &shown, &mut keys, &mut errors);
    }
    (keys, errors)
}

/// Fins de ligne CRLF ramenées à LF : avec `core.autocrlf=true` le checkout écrit les sources en CRLF, alors que
/// rustc normalise les littéraux multilignes en LF.
fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Repère `tr!(`, `trn!(` et `tr_mark!(` hors commentaires et chaînes, et lit leurs textes littéraux.
fn scan(text: &str, file: &str, keys: &mut BTreeMap<String, String>, errors: &mut Vec<String>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ident_before = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        // Commentaires, chaînes, chaînes brutes et caractères : sautés pour ne pas y voir de macro.
        if bytes[i..].starts_with(b"//") {
            i = text[i..].find('\n').map_or(bytes.len(), |n| i + n);
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i = text[i..].find("*/").map_or(bytes.len(), |n| i + n + 2);
            continue;
        }
        if bytes[i] == b'"' {
            i = read_literal(text, i).map_or(bytes.len(), |(_, end)| end);
            continue;
        }
        if bytes[i] == b'r' && !ident_before {
            let hashes = bytes[i + 1..].iter().take_while(|b| **b == b'#').count();
            if bytes.get(i + 1 + hashes) == Some(&b'"') {
                let close = format!("\"{}", "#".repeat(hashes));
                let body = i + 2 + hashes;
                i = text[body..].find(&close).map_or(bytes.len(), |n| body + n + close.len());
                continue;
            }
        }
        if bytes[i] == b'\'' {
            i += 1 + char_literal_len(&text[i + 1..]).unwrap_or(0);
            continue;
        }
        let macro_name = ["tr_mark!(", "trn!(", "tr!("].into_iter().find(|m| bytes[i..].starts_with(m.as_bytes()));
        let Some(name) = macro_name.filter(|_| !ident_before) else {
            i += 1;
            continue;
        };
        let place = format!("{file}:{}", text[..i].matches('\n').count() + 1);
        let mut pos = i + name.len();
        let wanted = if name == "trn!(" { 2 } else { 1 };
        if name == "trn!(" {
            // Premier argument : le nombre, jusqu'à la première virgule.
            match text[pos..].find(',') {
                Some(comma) => pos += comma + 1,
                None => {
                    errors.push(format!("{place} : trn! sans texte"));
                    return;
                }
            }
        }
        for _ in 0..wanted {
            while pos < bytes.len() && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b',') {
                pos += 1;
            }
            match read_literal(text, pos) {
                Some((value, end)) => {
                    keys.entry(value).or_insert_with(|| place.clone());
                    pos = end;
                }
                None => {
                    errors.push(format!("{place} : {name}…) doit recevoir un texte littéral entre guillemets"));
                    break;
                }
            }
        }
        i = pos.max(i + 1);
    }
}

/// Longueur, après l'apostrophe ouvrante, d'un littéral caractère (`'"'`, `'\''`) ; `None` pour une durée
/// de vie (`'a`).
fn char_literal_len(rest: &str) -> Option<usize> {
    let first = rest.chars().next()?;
    if first == '\\' {
        let escaped = rest[1..].chars().next()?.len_utf8();
        return rest[1 + escaped..].find('\'').map(|q| 1 + escaped + q + 1);
    }
    let len = first.len_utf8();
    rest[len..].starts_with('\'').then_some(len + 1)
}

/// Lit le littéral `"…"` qui commence en `start` ; renvoie sa valeur et la position qui le suit.
fn read_literal(text: &str, start: usize) -> Option<(String, usize)> {
    let mut chars = text[start..].char_indices().peekable();
    if chars.next()?.1 != '"' {
        return None;
    }
    let mut value = String::new();
    while let Some((at, c)) = chars.next() {
        match c {
            '"' => return Some((value, start + at + 1)),
            '\\' => match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                'r' => value.push('\r'),
                '0' => value.push('\0'),
                '\\' => value.push('\\'),
                '"' => value.push('"'),
                '\'' => value.push('\''),
                'u' => {
                    chars.next(); // {
                    let hex: String = chars.by_ref().map(|(_, c)| c).take_while(|c| *c != '}').collect();
                    value.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                '\n' | '\r' => {
                    // Continuation de ligne : les blancs du début de la ligne suivante sont ignorés.
                    while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                        chars.next();
                    }
                }
                other => {
                    value.push('\\');
                    value.push(other);
                }
            },
            c => value.push(c),
        }
    }
    None
}

/// Noms des paramètres `{nom}` d'un texte.
fn params(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('}') {
            let name = &rest[..close];
            let mut chars = name.chars();
            if chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
            {
                out.insert(name.to_string());
            }
        }
    }
    out
}

#[test]
fn every_text_has_a_translation_and_every_translation_is_used() {
    let (keys, mut problems) = keys_in_sources();
    for (lang, json) in CATALOGS {
        let catalog: BTreeMap<String, String> = match serde_json::from_str(json) {
            Ok(c) => c,
            Err(e) => panic!("catalogue {} illisible : {e}", lang.code()),
        };
        let code = lang.code();
        for (key, place) in &keys {
            match catalog.get(key) {
                None => problems.push(format!("[{code}] manquant ({place}) : {key:?}")),
                Some(t) if t.trim().is_empty() => problems.push(format!("[{code}] traduction vide : {key:?}")),
                Some(t) if params(t) != params(key) => {
                    problems.push(format!("[{code}] paramètres différents ({place}) : {key:?} → {t:?}"))
                }
                Some(_) => {}
            }
        }
        for key in catalog.keys().filter(|k| !keys.contains_key(*k)) {
            problems.push(format!("[{code}] traduction orpheline : {key:?}"));
        }
    }
    assert!(problems.is_empty(), "{} problème(s) :\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn every_language_but_french_has_a_catalog() {
    for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::Fr) {
        assert!(CATALOGS.iter().any(|(l, _)| *l == lang), "pas de catalogue pour {}", lang.code());
    }
}

#[test]
fn scanner_reads_the_three_macros() {
    let src = r##"
        // tr!("commentaire ignoré")
        /* tr!("bloc ignoré") */
        let a = tr!("Simple");
        let b = tr!(
            "Avec « {name} »",
            name = x
        );
        let c = trn!(items.len(), "{n} fichier", "{n} fichiers");
        const D: &str = tr_mark!("Marqué");
        let e = "tr!(\"dans une chaîne\")";
        let f = __tr_args!(x);
        let g = tr!("Échappé \"guillemets\"\n");
        let h = r#"tr!("dans une chaîne brute")"#;
        let i = ('"', '\'', b'"');
        let j: &'static str = tr!("Après des caractères");
        let k = tr!("Suite \
                     de ligne");
    "##;
    let mut keys = BTreeMap::new();
    let mut errors = Vec::new();
    scan(src, "x.rs", &mut keys, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let found: Vec<&str> = keys.keys().map(String::as_str).collect();
    assert_eq!(
        found,
        [
            "Après des caractères",
            "Avec « {name} »",
            "Marqué",
            "Simple",
            "Suite de ligne",
            "{n} fichier",
            "{n} fichiers",
            "Échappé \"guillemets\"\n",
        ]
    );
}

#[test]
fn scanner_is_insensitive_to_crlf_line_endings() {
    let src = "let b = tr!(\"Multi\r\nligne\");\r\nlet c = tr!(\"Coupée \\\r\n   ici\");\r\n";
    let mut keys = BTreeMap::new();
    let mut errors = Vec::new();
    scan(&normalize_newlines(src), "x.rs", &mut keys, &mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let found: Vec<&str> = keys.keys().map(String::as_str).collect();
    assert_eq!(found, ["Coupée ici", "Multi\nligne"]);
    // Sans normalisation, la clé multiligne garderait son \r.
    let mut raw = BTreeMap::new();
    scan(src, "x.rs", &mut raw, &mut errors);
    assert!(raw.contains_key("Multi\r\nligne"));
}

#[test]
fn scanner_reports_non_literal_texts() {
    let mut keys = BTreeMap::new();
    let mut errors = Vec::new();
    scan("let a = tr!(label);", "x.rs", &mut keys, &mut errors);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("x.rs:1"), "{errors:?}");
}

#[test]
fn params_are_named_placeholders_only() {
    assert_eq!(params("stash@{0} « {name} » {n}"), BTreeSet::from(["n".to_string(), "name".to_string()]));
}

/// Clés d'un objet JSON plat, doublons compris (serde_json garde silencieusement la dernière valeur).
fn duplicate_keys(json: &str) -> Vec<String> {
    use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};

    struct Keys(Vec<String>);
    impl<'de> Visitor<'de> for Keys {
        type Value = Vec<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("un objet JSON")
        }
        fn visit_map<A: MapAccess<'de>>(mut self, mut map: A) -> Result<Self::Value, A::Error> {
            while let Some((key, IgnoredAny)) = map.next_entry::<String, IgnoredAny>()? {
                self.0.push(key);
            }
            Ok(self.0)
        }
    }
    let all = serde_json::Deserializer::from_str(json).deserialize_map(Keys(Vec::new())).unwrap();
    let mut seen = BTreeSet::new();
    all.into_iter().filter(|k| !seen.insert(k.clone())).collect()
}

#[test]
fn catalogs_have_no_duplicate_keys() {
    for (lang, json) in CATALOGS {
        let dups = duplicate_keys(json);
        assert!(dups.is_empty(), "[{}] clés en double : {dups:?}", lang.code());
    }
    assert_eq!(duplicate_keys(r#"{"a": "1", "b": "2", "a": "3"}"#), ["a"]);
}
