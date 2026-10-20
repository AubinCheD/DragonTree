//! Statistiques d'un dépôt : nombre de commits, contributeurs (alias fusionnés par e-mail), commits par mois, fichiers
//! suivis, taille de la base d'objets, branches et tags. Les lectures sont des commandes git ordinaires
//! (`shortlog`, `rev-list --count`, `log --format=%at`, `count-objects -vH`, `ls-files`) ; l'agrégation est faite par
//! des fonctions pures, testées seules.

use std::collections::HashMap;

use crate::error::GitError;
use crate::repo::Repo;
use crate::runner::CancelToken;

/// Un contributeur : tous les noms et e-mails qui désignent la même adresse sont réunis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contributor {
    /// Nom le plus utilisé pour cette adresse (le premier rencontré à égalité).
    pub name: String,
    /// E-mail tel qu'écrit le plus souvent (la comparaison ignore la casse).
    pub email: String,
    pub commits: u64,
    /// Autres noms utilisés avec la même adresse.
    pub aliases: Vec<String>,
}

/// Commits d'un mois civil (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthCount {
    pub year: i32,
    /// 1 à 12.
    pub month: u32,
    pub commits: u64,
}

/// Base d'objets (`git count-objects -vH`) : les tailles sont celles que git affiche (« 4.52 MiB »).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectStats {
    /// Objets « en vrac ».
    pub loose_objects: u64,
    pub loose_size: String,
    /// Objets dans les packs.
    pub packed_objects: u64,
    pub packs: u64,
    pub pack_size: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoStats {
    /// Commits atteignables depuis HEAD (0 sans commit).
    pub commits_head: u64,
    /// Commits atteignables depuis toutes les références.
    pub commits_all: u64,
    /// Contributeurs de toutes les branches, du plus actif au moins actif.
    pub contributors: Vec<Contributor>,
    /// Commits de toutes les branches par mois, du plus ancien au plus récent, mois vides compris.
    pub monthly: Vec<MonthCount>,
    /// Date (secondes Unix) du premier et du dernier commit, selon la date d'auteur.
    pub first_commit: Option<i64>,
    pub last_commit: Option<i64>,
    pub tracked_files: u64,
    pub objects: ObjectStats,
    pub local_branches: u64,
    pub remote_branches: u64,
    pub tags: u64,
}

/// Une ligne de `git shortlog -sne` : (commits, nom, e-mail).
type ShortlogEntry = (u64, String, String);

/// Lignes de `git shortlog -sne` (`  12\tNom <adresse>`) ; une ligne illisible est ignorée.
pub fn parse_shortlog(text: &str) -> Vec<ShortlogEntry> {
    text.lines()
        .filter_map(|line| {
            let (count, who) = line.trim_start().split_once('\t')?;
            let count = count.trim().parse().ok()?;
            let who = who.trim();
            Some(match who.rsplit_once('<') {
                Some((name, rest)) => (count, name.trim().to_string(), rest.trim_end_matches('>').trim().to_string()),
                None => (count, who.to_string(), String::new()),
            })
        })
        .collect()
}

/// Réunit les lignes d'un même e-mail (casse ignorée ; sans e-mail, d'un même nom) : le total des commits, le nom le
/// plus utilisé, les autres noms en alias. Trié par commits décroissants, puis par nom.
pub fn merge_contributors(entries: &[ShortlogEntry]) -> Vec<Contributor> {
    // Clé de regroupement -> (nom, e-mail, commits) de chaque ligne, dans l'ordre de lecture.
    let mut groups: Vec<(String, Vec<&ShortlogEntry>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for entry in entries {
        let (_, name, email) = entry;
        let key = if email.trim().is_empty() { format!("name:{}", name.to_lowercase()) } else { email.to_lowercase() };
        let slot = *index.entry(key.clone()).or_insert_with(|| {
            groups.push((key, Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.push(entry);
    }
    let mut merged: Vec<Contributor> = groups
        .into_iter()
        .map(|(_, rows)| {
            let commits = rows.iter().map(|r| r.0).sum();
            // Le nom qui porte le plus de commits ; à égalité, le premier lu.
            let main = rows.iter().fold(rows[0], |best, row| if row.0 > best.0 { row } else { best });
            let mut aliases: Vec<String> = Vec::new();
            for row in &rows {
                if row.1 != main.1 && !aliases.contains(&row.1) {
                    aliases.push(row.1.clone());
                }
            }
            Contributor { name: main.1.clone(), email: main.2.clone(), commits, aliases }
        })
        .collect();
    merged.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    merged
}

/// Année, mois (1 à 12) et jour du jour numéro `days` depuis le 1er janvier 1970 (calendrier grégorien).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400) as i32 + i32::from(month <= 2);
    (year, month, day)
}

/// Commits par mois civil (UTC), du plus ancien au plus récent ; les mois sans commit entre les deux sont à zéro.
pub fn monthly_histogram(timestamps: &[i64]) -> Vec<MonthCount> {
    let month_of = |t: i64| {
        let (year, month, _) = civil_from_days(t.div_euclid(86_400));
        (year, month)
    };
    let mut counts: HashMap<(i32, u32), u64> = HashMap::new();
    for &t in timestamps {
        *counts.entry(month_of(t)).or_default() += 1;
    }
    let (Some(first), Some(last)) = (counts.keys().min().copied(), counts.keys().max().copied()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let (mut year, mut month) = first;
    loop {
        out.push(MonthCount { year, month, commits: counts.get(&(year, month)).copied().unwrap_or(0) });
        if (year, month) == last {
            return out;
        }
        if month == 12 {
            (year, month) = (year + 1, 1);
        } else {
            month += 1;
        }
    }
}

/// Sortie de `git count-objects -vH`.
pub fn parse_count_objects(text: &str) -> ObjectStats {
    let mut stats = ObjectStats::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "count" => stats.loose_objects = value.parse().unwrap_or(0),
            "size" => stats.loose_size = value.to_string(),
            "in-pack" => stats.packed_objects = value.parse().unwrap_or(0),
            "packs" => stats.packs = value.parse().unwrap_or(0),
            "size-pack" => stats.pack_size = value.to_string(),
            _ => {}
        }
    }
    stats
}

impl Repo {
    /// Calcule les statistiques du dépôt. Chaque lecture suit `cancel` : annulé, le calcul s'arrête avec
    /// `GitError::Cancelled`. Un dépôt sans commit donne des comptes à zéro.
    pub fn stats(&self, cancel: &CancelToken) -> Result<RepoStats, GitError> {
        let count = |revs: &[&str]| -> Result<u64, GitError> {
            let out = self.read().args(["rev-list", "--count"]).args(revs).cancel(cancel.clone()).run()?;
            Ok(out.stdout_str().trim().parse().unwrap_or(0))
        };
        let head_exists = self.head_exists();
        let commits_head = if head_exists { count(&["HEAD"])? } else { 0 };
        // « Toutes les branches » : branches, remotes, tags et HEAD, pas `--all` qui suit aussi le stash, les notes et
        // les références sauvegardées par filter-branch (leurs commits gonfleraient les comptes).
        let mut scope = vec!["--branches", "--remotes", "--tags"];
        if head_exists {
            scope.push("HEAD");
        }
        let commits_all = count(&scope)?;
        let shortlog = self.read().args(["shortlog", "-sne"]).args(&scope).cancel(cancel.clone()).run()?;
        let contributors = merge_contributors(&parse_shortlog(&shortlog.stdout_str()));
        let times = self.read().args(["log", "--format=%at"]).args(&scope).cancel(cancel.clone()).run()?;
        let timestamps: Vec<i64> = times.stdout_str().lines().filter_map(|l| l.trim().parse().ok()).collect();
        let objects = self.read().args(["count-objects", "-vH"]).cancel(cancel.clone()).run()?;
        let files = self.read().args(["ls-files", "-z", "--deduplicate"]).cancel(cancel.clone()).run()?;
        let refs = self
            .read()
            .args(["for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes", "refs/tags"])
            .cancel(cancel.clone())
            .run()?;
        let refs = refs.stdout_str();
        let in_namespace = |prefix: &str| refs.lines().filter(|r| r.starts_with(prefix)).count() as u64;
        Ok(RepoStats {
            commits_head,
            commits_all,
            contributors,
            monthly: monthly_histogram(&timestamps),
            first_commit: timestamps.iter().min().copied(),
            last_commit: timestamps.iter().max().copied(),
            tracked_files: files.stdout.iter().filter(|b| **b == 0).count() as u64,
            objects: parse_count_objects(&objects.stdout_str()),
            local_branches: in_namespace("refs/heads/"),
            // `refs/remotes/origin/HEAD` n'est pas une branche.
            remote_branches: refs.lines().filter(|r| r.starts_with("refs/remotes/") && !r.ends_with("/HEAD")).count()
                as u64,
            tags: in_namespace("refs/tags/"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(commits: u64, name: &str, email: &str) -> ShortlogEntry {
        (commits, name.to_string(), email.to_string())
    }

    #[test]
    fn shortlog_lines_are_parsed() {
        let text =
            "    12\tAlice Martin <alice@contoso.test>\n     3\tbob <BOB@fabrikam.test>\n     1\tsans adresse\nbruit\n";
        assert_eq!(
            parse_shortlog(text),
            vec![
                entry(12, "Alice Martin", "alice@contoso.test"),
                entry(3, "bob", "BOB@fabrikam.test"),
                entry(1, "sans adresse", ""),
            ]
        );
    }

    #[test]
    fn contributors_with_the_same_email_are_merged() {
        let merged = merge_contributors(&[
            entry(2, "Bob", "bob@fabrikam.test"),
            entry(5, "Alice M.", "alice@contoso.test"),
            entry(4, "Robert", "BOB@fabrikam.test"),
            entry(1, "Alice Martin", "alice@contoso.test"),
            entry(1, "Solo", ""),
            entry(2, "solo", ""),
        ]);
        assert_eq!(
            merged,
            vec![
                Contributor {
                    name: "Alice M.".into(),
                    email: "alice@contoso.test".into(),
                    commits: 6,
                    aliases: vec!["Alice Martin".into()]
                },
                Contributor {
                    name: "Robert".into(),
                    email: "BOB@fabrikam.test".into(),
                    commits: 6,
                    aliases: vec!["Bob".into()]
                },
                Contributor { name: "solo".into(), email: String::new(), commits: 3, aliases: vec!["Solo".into()] },
            ],
            "à égalité de commits, l'ordre alphabétique (casse ignorée) départage"
        );
    }

    #[test]
    fn commits_are_counted_per_month_with_empty_months_filled() {
        // 2024-01-31 23:59:59 UTC, 2024-02-01 00:00:00 UTC, 2024-02-15, puis 2024-05-10 : mars et avril sont vides.
        let stamps = [1_706_745_599, 1_706_745_600, 1_707_998_400, 1_715_342_400];
        let months = |list: &[MonthCount]| list.iter().map(|m| (m.year, m.month, m.commits)).collect::<Vec<_>>();
        assert_eq!(
            months(&monthly_histogram(&stamps)),
            vec![(2024, 1, 1), (2024, 2, 2), (2024, 3, 0), (2024, 4, 0), (2024, 5, 1)]
        );
        assert!(monthly_histogram(&[]).is_empty());
        // Passage d'année, et dates avant 1970.
        assert_eq!(months(&monthly_histogram(&[1_703_980_800, 1_704_153_600])), vec![(2023, 12, 1), (2024, 1, 1)]);
        assert_eq!(months(&monthly_histogram(&[-86_400])), vec![(1969, 12, 1)]);
    }

    #[test]
    fn count_objects_output_is_parsed() {
        let text = "count: 12\nsize: 48.00 KiB\nin-pack: 125\npacks: 2\nsize-pack: 523.21 KiB\nprune-packable: 0\n";
        assert_eq!(
            parse_count_objects(text),
            ObjectStats {
                loose_objects: 12,
                loose_size: "48.00 KiB".into(),
                packed_objects: 125,
                packs: 2,
                pack_size: "523.21 KiB".into()
            }
        );
    }
}
