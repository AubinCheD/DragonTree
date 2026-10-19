//! Pile des onglets de dépôt fermés, pour « Rouvrir l'onglet fermé » (Ctrl+Maj+T). Mémoire de la session
//! seulement : rien n'est enregistré.

use std::path::PathBuf;

use crate::settings::path_key;

/// Nombre maximal d'onglets fermés retenus : les plus anciens sont oubliés.
const CAPACITY: usize = 20;

/// Onglet fermé : le dépôt et sa position dans la barre au moment de la fermeture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedTab {
    pub path: PathBuf,
    pub index: usize,
}

#[derive(Debug, Default)]
pub struct ClosedTabs {
    /// Du plus ancien au plus récent.
    stack: Vec<ClosedTab>,
}

impl ClosedTabs {
    /// Empile un onglet fermé ; sans effet si le même dépôt vient d'être empilé (pas de doublon consécutif).
    pub fn push(&mut self, path: PathBuf, index: usize) {
        if self.stack.last().is_some_and(|last| path_key(&last.path) == path_key(&path)) {
            return;
        }
        if self.stack.len() == CAPACITY {
            self.stack.remove(0);
        }
        self.stack.push(ClosedTab { path, index });
    }

    /// Empile plusieurs onglets fermés d'un coup, dans n'importe quel ordre : les plus à droite sont empilés d'abord,
    /// si bien que les réouvertures successives (le plus à gauche d'abord) reconstituent l'ordre d'origine.
    pub fn push_all(&mut self, tabs: impl IntoIterator<Item = (PathBuf, usize)>) {
        let mut tabs: Vec<_> = tabs.into_iter().collect();
        tabs.sort_by_key(|(_, index)| std::cmp::Reverse(*index));
        for (path, index) in tabs {
            self.push(path, index);
        }
    }

    /// Dépile le plus récent.
    pub fn pop(&mut self) -> Option<ClosedTab> {
        self.stack.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(name: &str) -> PathBuf {
        PathBuf::from(format!("repos/{name}"))
    }

    #[test]
    fn pops_most_recent_first_with_its_position() {
        let mut closed = ClosedTabs::default();
        assert!(closed.is_empty());
        closed.push(path("a"), 0);
        closed.push(path("b"), 3);
        assert!(!closed.is_empty());
        assert_eq!(closed.pop(), Some(ClosedTab { path: path("b"), index: 3 }));
        assert_eq!(closed.pop(), Some(ClosedTab { path: path("a"), index: 0 }));
        assert_eq!(closed.pop(), None);
        assert!(closed.is_empty());
    }

    #[test]
    fn ignores_a_consecutive_duplicate_but_not_a_repeat_after_another_repo() {
        let mut closed = ClosedTabs::default();
        closed.push(path("a"), 0);
        closed.push(path("a"), 1);
        closed.push(path("b"), 1);
        closed.push(path("a"), 2);
        let order: Vec<_> = std::iter::from_fn(|| closed.pop()).map(|t| t.path).collect();
        assert_eq!(order, [path("a"), path("b"), path("a")]);
    }

    /// Rouvre tout ce qui est empilé comme `reopen_closed_tab` : à la position d'origine, ou à la fin.
    fn reopen_all(mut tabs: Vec<String>, closed: &mut ClosedTabs) -> Vec<String> {
        while let Some(tab) = closed.pop() {
            let at = tab.index.min(tabs.len());
            tabs.insert(at, tab.path.file_name().unwrap().to_string_lossy().into_owned());
        }
        tabs
    }

    #[test]
    fn close_others_then_reopening_restores_the_original_order() {
        let mut closed = ClosedTabs::default();
        // [A, B, C, D], « Fermer les autres » sur C.
        closed.push_all([(path("A"), 0), (path("B"), 1), (path("D"), 3)]);
        assert_eq!(reopen_all(vec!["C".into()], &mut closed), ["A", "B", "C", "D"]);
    }

    #[test]
    fn close_all_then_reopening_restores_the_original_order() {
        let mut closed = ClosedTabs::default();
        // [A, B, C], « Tout fermer » : il ne reste que l'onglet d'accueil, remplacé à la première réouverture.
        closed.push_all([(path("A"), 0), (path("B"), 1), (path("C"), 2)]);
        assert_eq!(reopen_all(Vec::new(), &mut closed), ["A", "B", "C"]);
    }

    #[test]
    fn keeps_only_the_most_recent_twenty() {
        let mut closed = ClosedTabs::default();
        for i in 0..(CAPACITY + 5) {
            closed.push(path(&i.to_string()), i);
        }
        let all: Vec<_> = std::iter::from_fn(|| closed.pop()).collect();
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all.first().unwrap().index, CAPACITY + 4);
        assert_eq!(all.last().unwrap().index, 5);
    }
}
