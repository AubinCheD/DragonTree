//! Liens vers les tickets : découpe un message de commit en segments de texte et en liens selon les règles
//! du dépôt (`Settings::ticket_links`).

use regex::Regex;

use crate::settings::TicketRule;

/// Morceau d'un message : texte simple, ou texte devenu lien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Link { text: String, url: String },
}

/// Seuls les liens web s'ouvrent : un motif mal écrit ne doit pas lancer `file:` ou une autre commande.
fn is_web_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// Découpe `message` : chaque occurrence d'un motif devient un lien (`$1`, `${nom}` de l'URL remplacés par les
/// groupes capturés). Un motif invalide ou vide est ignoré ; quand plusieurs occurrences se chevauchent, la
/// première (puis la première règle) l'emporte ; une occurrence vide ou dont l'URL n'est pas http(s) reste du texte.
pub fn link_segments(message: &str, rules: &[TicketRule]) -> Vec<Segment> {
    // (début, fin, index de règle, URL) de chaque occurrence retenue.
    let mut found: Vec<(usize, usize, usize, String)> = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        if rule.pattern.trim().is_empty() {
            continue;
        }
        let Ok(re) = Regex::new(&rule.pattern) else { continue };
        for caps in re.captures_iter(message) {
            let whole = caps.get(0).expect("le groupe 0 existe toujours");
            if whole.is_empty() {
                continue;
            }
            let mut url = String::new();
            caps.expand(&rule.url, &mut url);
            if is_web_url(&url) {
                found.push((whole.start(), whole.end(), index, url));
            }
        }
    }
    found.sort_by_key(|(start, _, index, _)| (*start, *index));

    let mut segments = Vec::new();
    let mut pos = 0;
    for (start, end, _, url) in found {
        if start < pos {
            continue;
        }
        if start > pos {
            segments.push(Segment::Text(message[pos..start].to_string()));
        }
        segments.push(Segment::Link { text: message[start..end].to_string(), url });
        pos = end;
    }
    if pos < message.len() {
        segments.push(Segment::Text(message[pos..].to_string()));
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(pattern: &str, url: &str) -> TicketRule {
        TicketRule { pattern: pattern.into(), url: url.into() }
    }

    fn azure() -> TicketRule {
        rule(r"#(\d+)", "https://dev.azure.com/contoso/projet/_workitems/edit/$1")
    }

    fn text(s: &str) -> Segment {
        Segment::Text(s.into())
    }

    fn link(t: &str, url: &str) -> Segment {
        Segment::Link { text: t.into(), url: url.into() }
    }

    #[test]
    fn a_message_without_rules_or_matches_is_one_text_segment() {
        assert_eq!(link_segments("Corrige le bug", &[]), vec![text("Corrige le bug")]);
        assert_eq!(link_segments("Corrige le bug", &[azure()]), vec![text("Corrige le bug")]);
        assert_eq!(link_segments("", &[azure()]), Vec::<Segment>::new());
    }

    #[test]
    fn each_match_becomes_a_link_with_its_captured_groups() {
        let got = link_segments("Corrige #12 et #345.", &[azure()]);
        assert_eq!(
            got,
            vec![
                text("Corrige "),
                link("#12", "https://dev.azure.com/contoso/projet/_workitems/edit/12"),
                text(" et "),
                link("#345", "https://dev.azure.com/contoso/projet/_workitems/edit/345"),
                text("."),
            ]
        );
    }

    #[test]
    fn named_groups_and_several_rules_work_together() {
        let rules = [rule(r"(?P<key>FAB-\d+)", "https://tickets.fabrikam.test/browse/${key}"), azure()];
        let got = link_segments("FAB-7 puis #8", &rules);
        assert_eq!(
            got,
            vec![
                link("FAB-7", "https://tickets.fabrikam.test/browse/FAB-7"),
                text(" puis "),
                link("#8", "https://dev.azure.com/contoso/projet/_workitems/edit/8"),
            ]
        );
    }

    #[test]
    fn an_invalid_or_empty_pattern_is_ignored_without_panicking() {
        let rules =
            [rule("(", "https://x.test/$1"), rule("", "https://x.test"), rule("[a-", "https://x.test"), azure()];
        let got = link_segments("voir #3", &rules);
        assert_eq!(got, vec![text("voir "), link("#3", "https://dev.azure.com/contoso/projet/_workitems/edit/3")]);
    }

    #[test]
    fn overlapping_matches_keep_the_first_and_empty_matches_are_skipped() {
        let rules = [rule(r"abc", "https://a.test/1"), rule(r"bcd", "https://b.test/2"), rule(r"x*", "https://c.test")];
        assert_eq!(link_segments("abcd", &rules), vec![link("abc", "https://a.test/1"), text("d")]);
    }

    #[test]
    fn only_web_urls_become_links() {
        let rules = [rule(r"#(\d+)", "file:///C:/Windows/$1"), rule(r"!(\d+)", "HTTPS://fabrikam.test/$1")];
        assert_eq!(link_segments("#1 !2", &rules), vec![text("#1 "), link("!2", "HTTPS://fabrikam.test/2")]);
    }

    #[test]
    fn accented_text_keeps_its_boundaries() {
        let got = link_segments("Réglé — #9 ✓", &[azure()]);
        assert_eq!(
            got,
            vec![text("Réglé — "), link("#9", "https://dev.azure.com/contoso/projet/_workitems/edit/9"), text(" ✓"),]
        );
    }
}
