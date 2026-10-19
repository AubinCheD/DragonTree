//! Bisect (`git bisect`) : état d'une session, démarrage, marquage (bon, mauvais, passer) et fin.
//!
//! L'état se lit sans interpréter les messages de git (traduits) : `.git/BISECT_START` signale la session, les
//! références `refs/bisect/*` donnent les commits marqués, `git rev-list --bisect-vars` le nombre de révisions
//! restantes.

use std::collections::HashMap;

use dragontree_i18n::tr;

use crate::error::GitError;
use crate::oid::Oid;
use crate::repo::{OpState, Repo};

/// Réponse à donner sur le commit en cours d'examen (ou sur un commit choisi).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BisectMark {
    Good,
    Bad,
    Skip,
}

impl BisectMark {
    fn arg(self) -> &'static str {
        match self {
            BisectMark::Good => "good",
            BisectMark::Bad => "bad",
            BisectMark::Skip => "skip",
        }
    }
}

/// Session de bisect en cours.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BisectState {
    /// Branche (ou commit) où la session a commencé, celle que `bisect reset` restaure.
    pub start: Option<String>,
    /// Commit marqué mauvais (le plus récent des mauvais connus).
    pub bad: Option<Oid>,
    pub good: Vec<Oid>,
    pub skipped: Vec<Oid>,
    /// Révisions restant à tester après le commit en cours (`bisect_nr` de git) ; `None` tant qu'il manque un
    /// commit bon ou mauvais.
    pub remaining: Option<usize>,
    /// Nombre d'étapes estimé par git (`bisect_steps`).
    pub steps: Option<usize>,
    /// Premier mauvais commit, une fois la recherche terminée.
    pub found: Option<Oid>,
}

/// Lit les lignes `clé='valeur'` de `git rev-list --bisect-vars`.
fn parse_bisect_vars(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('\'').to_string()))
        .collect()
}

impl Repo {
    /// Refuse de démarrer un bisect pendant une fusion, un rebase, un cherry-pick, un revert ou une session `am` :
    /// il déplacerait HEAD sous leurs pieds.
    fn refuse_during_operation(&self) -> Result<(), GitError> {
        if self.operation_state() != OpState::None {
            return Err(GitError::Refused(
                tr!("Une opération (fusion, rebase, cherry-pick, revert, am) est en cours : terminez-la avant de lancer un bisect.")
                    .into(),
            ));
        }
        Ok(())
    }

    /// État de la session de bisect, `None` hors session. Lit un fichier et lance deux commandes de lecture :
    /// à appeler depuis une tâche de fond.
    pub fn bisect_state(&self) -> Option<BisectState> {
        let start = std::fs::read_to_string(self.git_dir.join("BISECT_START")).ok()?;
        let mut state =
            BisectState { start: Some(start.trim().to_string()).filter(|s| !s.is_empty()), ..Default::default() };
        let refs = self
            .read()
            .args(["for-each-ref", "--format=%(refname) %(objectname)", "refs/bisect"])
            .run()
            .map(|o| o.stdout_str())
            .unwrap_or_default();
        for line in refs.lines() {
            let Some((name, sha)) = line.split_once(' ') else { continue };
            let Some(oid) = Oid::from_hex(sha) else { continue };
            match name.strip_prefix("refs/bisect/") {
                Some("bad") => state.bad = Some(oid),
                Some(n) if n.starts_with("good-") => state.good.push(oid),
                Some(n) if n.starts_with("skip-") => state.skipped.push(oid),
                _ => {}
            }
        }
        if let (Some(bad), false) = (state.bad, state.good.is_empty()) {
            let mut cmd = self.read().args(["rev-list", "--bisect-vars", &bad.to_hex(), "--not"]);
            for good in &state.good {
                cmd = cmd.arg(good.to_hex());
            }
            if let Ok(out) = cmd.run() {
                let vars = parse_bisect_vars(&out.stdout_str());
                state.remaining = vars.get("bisect_nr").and_then(|v| v.parse().ok());
                state.steps = vars.get("bisect_steps").and_then(|v| v.parse().ok());
                // Il ne reste que le commit mauvais dans l'intervalle : c'est le premier mauvais.
                if vars.get("bisect_all").and_then(|v| v.parse::<usize>().ok()) == Some(1) {
                    state.found = Some(bad);
                }
            }
        }
        Some(state)
    }

    /// Démarre une session avec `HEAD` mauvais (`git bisect start`, `git bisect bad HEAD`).
    pub fn bisect_start(&self) -> Result<(), GitError> {
        self.refuse_during_operation()?;
        self.write().args(["bisect", "start"]).run()?;
        self.write().args(["bisect", "bad", "HEAD"]).run().map(|_| ())
    }

    /// Marque `rev` (`None` : le commit en cours d'examen, HEAD). Sans session, en démarre une : `HEAD` est
    /// d'abord marqué mauvais, sauf si c'est `rev` qu'on marque mauvais.
    pub fn bisect_mark(&self, mark: BisectMark, rev: Option<&str>) -> Result<(), GitError> {
        if self.bisect_state().is_none() {
            self.refuse_during_operation()?;
            self.write().args(["bisect", "start"]).run()?;
            if mark != BisectMark::Bad {
                self.write().args(["bisect", "bad", "HEAD"]).run()?;
            }
        }
        let cmd = self.write().args(["bisect", mark.arg()]);
        match rev {
            Some(rev) => cmd.arg(rev).run().map(|_| ()),
            None => cmd.run().map(|_| ()),
        }
    }

    /// Termine la session (`git bisect reset`) : retour à la branche de départ. Sans effet hors session.
    pub fn bisect_reset(&self) -> Result<(), GitError> {
        if self.bisect_state().is_none() {
            return Ok(());
        }
        self.write().args(["bisect", "reset"]).run().map(|_| ())
    }
}
