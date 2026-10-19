//! Disposition du graphe des commits en « voies » (colonnes) fixes.
//!
//! Chaque lien enfant → parent est une [`Edge`] qui occupe une voie de la ligne de l'enfant
//! jusqu'à celle du parent. Mémoire linéaire en nombre de liens (~16 octets par lien), même
//! avec des centaines de branches actives.

use std::collections::HashMap;

use smallvec::SmallVec;

use crate::oid::Oid;

/// Nombre de couleurs de la palette des voies.
pub const PALETTE_SIZE: u8 = 10;

/// Ligne de parent pas encore connue (parent absent du log chargé).
pub const UNKNOWN_ROW: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphRow {
    /// Voie du nœud du commit.
    pub lane: u16,
    pub color: u8,
    pub is_merge: bool,
}

/// Lien d'un commit (ligne `child_row`) vers un parent (ligne `parent_row`).
///
/// Dessin : du nœud de l'enfant vers `lane` en bas de la ligne de l'enfant, vertical sur `lane`,
/// puis de `lane` en haut de la ligne du parent vers le nœud du parent (`parent_lane`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub child_row: u32,
    pub parent_row: u32,
    pub child_lane: u16,
    pub lane: u16,
    pub parent_lane: u16,
    pub color: u8,
}

#[derive(Debug, Clone, Default)]
pub struct GraphLayout {
    pub rows: Vec<GraphRow>,
    pub edges: Vec<Edge>,
    /// Plus grand indice de voie utilisé.
    pub max_lane: u16,
}

impl GraphLayout {
    /// Liens qui croisent les lignes `first..=last`.
    pub fn edges_in(&self, first: u32, last: u32) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.child_row <= last && e.parent_row >= first)
    }
}

/// Construit la disposition commit par commit, dans l'ordre d'affichage (enfants d'abord).
#[derive(Default)]
pub struct GraphBuilder {
    lanes: Vec<Option<Oid>>,
    lane_color: Vec<u8>,
    pending: HashMap<Oid, SmallVec<[u32; 2]>>,
    next_color: u8,
    rows: Vec<GraphRow>,
    edges: Vec<Edge>,
    max_lane: u16,
}

impl GraphBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: Oid, parents: &[Oid]) {
        let row = self.rows.len() as u32;
        let pending = self.pending.remove(&id).unwrap_or_default();

        // Voies qui attendent ce commit : elles convergent vers son nœud.
        let mut matching: SmallVec<[u16; 4]> = pending.iter().map(|&e| self.edges[e as usize].lane).collect();
        matching.sort_unstable();
        matching.dedup();
        let (node_lane, color) = match matching.first() {
            Some(&lane) => (lane, self.lane_color[lane as usize]),
            None => (self.free_lane(), self.new_color()),
        };
        for &e in &pending {
            let edge = &mut self.edges[e as usize];
            edge.parent_row = row;
            edge.parent_lane = node_lane;
        }
        for &lane in &matching {
            self.lanes[lane as usize] = None;
        }

        for (k, &parent) in parents.iter().enumerate() {
            let (lane, edge_color) = if k == 0 {
                // Le premier parent continue la voie du commit, même s'il est déjà attendu
                // ailleurs : les voies convergeront sur le parent (point de bifurcation visible).
                self.occupy(node_lane, parent, color);
                (node_lane, color)
            } else if let Some(existing) = self.pending.get(&parent).and_then(|v| v.first()) {
                let lane = self.edges[*existing as usize].lane;
                (lane, self.lane_color[lane as usize])
            } else {
                let lane = self.free_lane();
                let c = self.new_color();
                self.occupy(lane, parent, c);
                (lane, c)
            };
            let idx = self.edges.len() as u32;
            self.edges.push(Edge {
                child_row: row,
                parent_row: UNKNOWN_ROW,
                child_lane: node_lane,
                lane,
                parent_lane: lane,
                color: edge_color,
            });
            self.pending.entry(parent).or_default().push(idx);
        }

        self.rows.push(GraphRow { lane: node_lane, color, is_merge: parents.len() > 1 });
        self.max_lane = self.max_lane.max(node_lane).max(self.lanes.len().saturating_sub(1) as u16);
        while self.lanes.last().is_some_and(Option::is_none) {
            self.lanes.pop();
        }
    }

    pub fn finish(self) -> GraphLayout {
        GraphLayout { rows: self.rows, edges: self.edges, max_lane: self.max_lane }
    }

    fn free_lane(&mut self) -> u16 {
        match self.lanes.iter().position(Option::is_none) {
            Some(i) => i as u16,
            None => {
                self.lanes.push(None);
                self.lane_color.resize(self.lanes.len().max(self.lane_color.len()), 0);
                (self.lanes.len() - 1) as u16
            }
        }
    }

    fn occupy(&mut self, lane: u16, expected: Oid, color: u8) {
        let i = lane as usize;
        if self.lanes.len() <= i {
            self.lanes.resize(i + 1, None);
        }
        if self.lane_color.len() <= i {
            self.lane_color.resize(i + 1, 0);
        }
        self.lanes[i] = Some(expected);
        self.lane_color[i] = color;
    }

    fn new_color(&mut self) -> u8 {
        let c = self.next_color;
        self.next_color = (c + 1) % PALETTE_SIZE;
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(n: u32) -> Oid {
        let mut b = [0u8; 20];
        b[..4].copy_from_slice(&n.to_be_bytes());
        b[19] = 1;
        Oid(b)
    }

    fn build(commits: &[(u32, &[u32])]) -> GraphLayout {
        let mut g = GraphBuilder::new();
        for (id, parents) in commits {
            let ps: Vec<Oid> = parents.iter().map(|p| oid(*p)).collect();
            g.push(oid(*id), &ps);
        }
        g.finish()
    }

    #[test]
    fn graph_linear() {
        let g = build(&[(1, &[2]), (2, &[3]), (3, &[])]);
        assert!(g.rows.iter().all(|r| r.lane == 0));
        assert_eq!(g.edges.len(), 2);
        assert_eq!((g.edges[0].child_row, g.edges[0].parent_row), (0, 1));
        assert_eq!((g.edges[1].child_row, g.edges[1].parent_row), (1, 2));
        assert_eq!(g.max_lane, 0);
    }

    #[test]
    fn graph_branch_merge() {
        // M(A, B), A(C), B(C), C
        let g = build(&[(10, &[11, 12]), (11, &[13]), (12, &[13]), (13, &[])]);
        assert!(g.rows[0].is_merge);
        assert_eq!(g.rows[1].lane, 0);
        assert_eq!(g.rows[2].lane, 1);
        assert_eq!(g.rows[3].lane, 0);
        assert_ne!(g.rows[1].color, g.rows[2].color);
        let into_c: Vec<&Edge> = g.edges.iter().filter(|e| e.parent_row == 3).collect();
        assert_eq!(into_c.len(), 2);
        assert!(into_c.iter().all(|e| e.parent_lane == 0));
        let mut lanes: Vec<u16> = into_c.iter().map(|e| e.lane).collect();
        lanes.sort();
        assert_eq!(lanes, vec![0, 1]);
        assert_eq!(g.max_lane, 1);
    }

    #[test]
    fn graph_fork_converges_at_parent() {
        // X(P), Y(P), P
        let g = build(&[(1, &[3]), (2, &[3]), (3, &[])]);
        assert_eq!(g.rows[0].lane, 0);
        assert_eq!(g.rows[1].lane, 1);
        assert_eq!(g.edges.len(), 2);
        assert!(g.edges.iter().all(|e| e.parent_row == 2 && e.parent_lane == 0));
        assert_ne!(g.edges[0].lane, g.edges[1].lane);
    }

    #[test]
    fn graph_merge_parent_joins_existing_lane() {
        // A(B), M(B2, B) : le 2e parent de M est déjà attendu sur la voie 0.
        let g = build(&[(1, &[3]), (2, &[4, 3]), (4, &[3]), (3, &[])]);
        let m_to_b = g.edges.iter().find(|e| e.child_row == 1 && e.parent_row == 3).unwrap();
        assert_eq!(m_to_b.lane, 0, "rejoint la voie qui attend déjà B");
        assert_eq!(m_to_b.color, g.rows[0].color);
    }

    #[test]
    fn graph_unknown_parent_open_edge() {
        let g = build(&[(1, &[99])]);
        assert_eq!(g.edges[0].parent_row, UNKNOWN_ROW);
        assert_eq!(g.edges_in(5, 10).count(), 1, "un lien ouvert traverse toutes les lignes suivantes");
    }

    #[test]
    fn graph_lane_reuse() {
        // X(P) voie 0, Y(P) voie 1, P(Q) voie 0 (libère la voie 1), Z(Q) réutilise la voie 1.
        let g = build(&[(1, &[3]), (2, &[3]), (3, &[5]), (4, &[5]), (5, &[])]);
        assert_eq!(g.rows[3].lane, 1);
        assert_eq!(g.max_lane, 1);
    }

    #[test]
    fn graph_edges_in_window() {
        let g = build(&[(1, &[2]), (2, &[3]), (3, &[4]), (4, &[])]);
        let rows: Vec<(u32, u32)> = g.edges_in(2, 3).map(|e| (e.child_row, e.parent_row)).collect();
        assert_eq!(rows, vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn graph_memory_is_linear() {
        // 50 chaînes de 2000 commits, fusions régulières entre chaînes voisines.
        let (chains, depth) = (50u32, 2000u32);
        let id = |c: u32, k: u32| c * 1_000_000 + k + 1;
        let mut g = GraphBuilder::new();
        let mut n = 0usize;
        for k in 0..depth {
            for c in 0..chains {
                let mut ps = Vec::new();
                if k + 1 < depth {
                    ps.push(oid(id(c, k + 1)));
                    if k % 100 == 0 && c > 0 {
                        ps.push(oid(id(c - 1, k + 1)));
                    }
                }
                g.push(oid(id(c, k)), &ps);
                n += 1;
            }
        }
        let layout = g.finish();
        assert_eq!(layout.rows.len(), n);
        assert!(layout.edges.len() < n * 13 / 10, "{} liens pour {n} commits", layout.edges.len());
        assert!(layout.max_lane < 60, "max_lane = {}", layout.max_lane);
        assert!(std::mem::size_of::<Edge>() <= 16);
    }
}
