//! Aides à la saisie du message de commit : niveau du compteur de longueur (50/72), co-auteurs tirés de l'historique
//! chargé (ligne `Co-authored-by:`) et sujets récents. Logique pure, sans interface : l'affichage est dans
//! `changes.rs`.

use std::collections::HashSet;

use dragontree_git::Commit;

use crate::settings::RecentMessage;

/// Couleur du compteur du résumé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthLevel {
    Normal,
    Warning,
    Error,
}

/// Normal jusqu'à `soft` caractères inclus, avertissement jusqu'à `hard` inclus, erreur au-delà.
pub fn length_level(len: usize, soft: usize, hard: usize) -> LengthLevel {
    if len <= soft {
        LengthLevel::Normal
    } else if len <= hard {
        LengthLevel::Warning
    } else {
        LengthLevel::Error
    }
}

/// Auteur connu de l'historique, proposé comme co-auteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    pub email: String,
}

impl Author {
    /// Vrai si `query` (sans casse) figure dans le nom ou l'e-mail ; une recherche vide les accepte tous.
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.name.to_lowercase().contains(&query) || self.email.to_lowercase().contains(&query)
    }

    /// Ligne de trailer git : `Co-authored-by: Nom <email>`.
    pub fn trailer(&self) -> String {
        format!("Co-authored-by: {} <{}>", self.name, self.email)
    }
}

/// Auteurs des commits, les plus récents d'abord, un seul par e-mail (sans casse ; le premier nom rencontré est gardé).
pub fn known_authors(commits: &[Commit]) -> Vec<Author> {
    let mut seen = HashSet::new();
    commits
        .iter()
        .filter(|c| !c.author_email.trim().is_empty() && seen.insert(c.author_email.to_lowercase()))
        .map(|c| Author { name: c.author.to_string(), email: c.author_email.to_string() })
        .collect()
}

/// Une ligne `Clé: valeur` de trailer (clé sans espace).
fn is_trailer_line(line: &str) -> bool {
    line.split_once(": ").is_some_and(|(key, _)| !key.is_empty() && !key.contains(char::is_whitespace))
}

/// Ajoute le trailer `Co-authored-by` en fin de description, après une ligne vide (ou à la suite du bloc de trailers
/// déjà présent). Sans effet, et `false`, si ce co-auteur (même e-mail, sans casse) y figure déjà.
pub fn add_coauthor(body: &mut String, author: &Author) -> bool {
    let email = format!("<{}>", author.email.to_lowercase());
    let already = body.lines().any(|line| {
        let line = line.trim().to_lowercase();
        line.starts_with("co-authored-by:") && line.contains(&email)
    });
    if already {
        return false;
    }
    // Fins de ligne normalisées : la détection du dernier paragraphe cherche « \n\n ».
    let trimmed = body.replace("\r\n", "\n").trim_end().to_string();
    let last_paragraph = trimmed.rsplit("\n\n").next().unwrap_or("");
    let separator = if trimmed.is_empty() {
        ""
    } else if last_paragraph.lines().all(is_trailer_line) {
        "\n"
    } else {
        "\n\n"
    };
    *body = format!("{trimmed}{separator}{}", author.trailer());
    true
}

/// Résumés des derniers commits de l'historique chargé (sans doublon, ni ceux des messages déjà mémorisés).
pub fn recent_subjects(commits: &[Commit], stored: &[RecentMessage], max: usize) -> Vec<String> {
    let mut seen: HashSet<&str> = stored.iter().map(|m| m.subject.as_str()).collect();
    commits
        .iter()
        .map(|c| &*c.summary)
        .filter(|s| !s.trim().is_empty() && seen.insert(s))
        .take(max)
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dragontree_git::Oid;

    use super::*;

    fn commit(author: &str, email: &str, summary: &str) -> Commit {
        Commit {
            id: Oid::ZERO,
            parents: Box::new([]),
            author: Arc::from(author),
            author_email: Arc::from(email),
            author_time: 0,
            committer: Arc::from(author),
            committer_time: 0,
            summary: summary.into(),
        }
    }

    fn zoe() -> Author {
        Author { name: "Zoé Northwind".into(), email: "zoe@northwind.test".into() }
    }

    #[test]
    fn the_counter_level_follows_the_two_limits() {
        assert_eq!(length_level(0, 50, 72), LengthLevel::Normal);
        assert_eq!(length_level(50, 50, 72), LengthLevel::Normal);
        assert_eq!(length_level(51, 50, 72), LengthLevel::Warning);
        assert_eq!(length_level(72, 50, 72), LengthLevel::Warning);
        assert_eq!(length_level(73, 50, 72), LengthLevel::Error);
        // Limites modifiées dans les réglages.
        assert_eq!(length_level(60, 60, 80), LengthLevel::Normal);
        assert_eq!(length_level(81, 60, 80), LengthLevel::Error);
    }

    #[test]
    fn a_coauthor_is_added_after_a_blank_line_without_duplicates() {
        let mut body = String::new();
        assert!(add_coauthor(&mut body, &zoe()));
        assert_eq!(body, "Co-authored-by: Zoé Northwind <zoe@northwind.test>");

        let mut body = "Explication du changement.\n".to_string();
        assert!(add_coauthor(&mut body, &zoe()));
        assert_eq!(body, "Explication du changement.\n\nCo-authored-by: Zoé Northwind <zoe@northwind.test>");

        // Deuxième co-auteur : à la suite du bloc de trailers, sans nouvelle ligne vide.
        let alice = Author { name: "Alice Contoso".into(), email: "alice@contoso.test".into() };
        assert!(add_coauthor(&mut body, &alice));
        assert_eq!(
            body,
            "Explication du changement.\n\nCo-authored-by: Zoé Northwind <zoe@northwind.test>\nCo-authored-by: Alice Contoso <alice@contoso.test>"
        );

        // Description aux fins de ligne CRLF : même résultat, le trailer rejoint le bloc existant.
        let mut crlf = "Texte.\r\n\r\nSigned-off-by: Alice <alice@contoso.test>\r\n".to_string();
        assert!(add_coauthor(&mut crlf, &zoe()));
        assert_eq!(
            crlf,
            "Texte.\n\nSigned-off-by: Alice <alice@contoso.test>\nCo-authored-by: Zoé Northwind <zoe@northwind.test>"
        );
        let mut crlf = "Texte.\r\n".to_string();
        assert!(add_coauthor(&mut crlf, &zoe()));
        assert_eq!(crlf, "Texte.\n\nCo-authored-by: Zoé Northwind <zoe@northwind.test>");

        // Doublon (même e-mail, casse différente, nom différent) : rien ne change.
        let before = body.clone();
        let again = Author { name: "Zoe".into(), email: "ZOE@northwind.test".into() };
        assert!(!add_coauthor(&mut body, &again));
        assert_eq!(body, before);
    }

    #[test]
    fn authors_are_unique_by_email_most_recent_first_and_searchable() {
        let commits = [
            commit("Zoé Northwind", "zoe@northwind.test", "un"),
            commit("Alice Contoso", "alice@contoso.test", "deux"),
            commit("Zoe N.", "ZOE@northwind.test", "trois"),
            commit("Sans e-mail", "", "quatre"),
        ];
        let authors = known_authors(&commits);
        assert_eq!(authors.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Zoé Northwind", "Alice Contoso"]);
        assert!(authors[1].matches("CONTOSO") && authors[1].matches("ali") && authors[1].matches(""));
        assert!(!authors[1].matches("northwind"));
    }

    #[test]
    fn recent_subjects_skip_blank_duplicates_and_stored_messages() {
        let commits = [
            commit("A", "a@x.test", "feat: a"),
            commit("A", "a@x.test", "feat: a"),
            commit("A", "a@x.test", "  "),
            commit("A", "a@x.test", "fix: b"),
            commit("A", "a@x.test", "chore: c"),
        ];
        let stored = [RecentMessage { subject: "fix: b".into(), body: "détail".into() }];
        assert_eq!(recent_subjects(&commits, &stored, 10), ["feat: a", "chore: c"]);
        assert_eq!(recent_subjects(&commits, &[], 2), ["feat: a", "fix: b"]);
    }
}
