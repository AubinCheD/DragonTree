//! Découverte des dépôts git sous des dossiers racines (barre latérale droite).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::{WalkBuilder, WalkState};
use serde::{Deserialize, Serialize};

use crate::refs::{HeadState, read_head};
use crate::runner::CancelToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepoKind {
    Normal,
    /// Arbre de travail supplémentaire (`git worktree add`).
    Worktree,
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundRepo {
    pub path: PathBuf,
    pub name: String,
    /// Dossier racine sous lequel le dépôt a été trouvé.
    pub root: PathBuf,
    pub kind: RepoKind,
}

impl FoundRepo {
    /// Chemin du dossier parent relatif à la racine (`""` si le dépôt est directement dessous).
    pub fn relative_parent(&self) -> String {
        self.path
            .parent()
            .and_then(|p| p.strip_prefix(&self.root).ok())
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanOptions {
    pub max_depth: usize,
    /// Noms de dossiers jamais parcourus (comparaison insensible à la casse).
    pub skip_dirs: Vec<String>,
    /// Parcourt aussi l'intérieur des dépôts (sous-modules et dépôts imbriqués). Plus lent.
    pub include_submodules: bool,
}

pub const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "bin",
    "obj",
    "packages",
    ".vs",
    ".idea",
    ".vscode",
    ".gradle",
    ".venv",
    "venv",
    "__pycache__",
    ".nuget",
    "dist",
    ".next",
    ".cache",
    "$recycle.bin",
    "system volume information",
];

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            max_depth: 8,
            skip_dirs: DEFAULT_SKIP_DIRS.iter().map(|s| s.to_string()).collect(),
            include_submodules: false,
        }
    }
}

/// Parcourt les racines en parallèle. `on_found` est appelé dès qu'un dépôt est trouvé
/// (depuis les threads du parcours). Résultat trié par chemin, sans doublon.
pub fn scan(
    roots: &[PathBuf],
    opts: &ScanOptions,
    cancel: &CancelToken,
    on_found: &(dyn Fn(FoundRepo) + Sync),
) -> Vec<FoundRepo> {
    let skip: Arc<HashSet<String>> = Arc::new(opts.skip_dirs.iter().map(|s| s.to_lowercase()).collect());
    let results = Mutex::new(Vec::new());
    let seen = Mutex::new(HashSet::new());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(16));

    for root in roots {
        if cancel.is_cancelled() {
            break;
        }
        if !root.is_dir() {
            continue;
        }
        let skip = skip.clone();
        let mut builder = WalkBuilder::new(root);
        builder
            .standard_filters(false)
            .hidden(false)
            .follow_links(false)
            .max_depth(Some(opts.max_depth))
            .threads(threads)
            .filter_entry(move |e| {
                e.file_type().is_some_and(|t| t.is_dir())
                    && e.file_name() != ".git"
                    && !skip.contains(&e.file_name().to_string_lossy().to_lowercase())
            });

        let (results, seen) = (&results, &seen);
        builder.build_parallel().run(|| {
            Box::new(move |entry| {
                if cancel.is_cancelled() {
                    return WalkState::Quit;
                }
                let Ok(entry) = entry else { return WalkState::Continue };
                let dir = entry.path();
                let Some(kind) = detect(dir) else { return WalkState::Continue };
                let fresh = seen.lock().unwrap().insert(dir.to_string_lossy().to_lowercase());
                if fresh && (kind != RepoKind::Submodule || opts.include_submodules) {
                    let repo = FoundRepo {
                        path: dir.to_path_buf(),
                        name: dir
                            .file_name()
                            .map_or_else(|| dir.display().to_string(), |n| n.to_string_lossy().into_owned()),
                        root: root.clone(),
                        kind,
                    };
                    on_found(repo.clone());
                    results.lock().unwrap().push(repo);
                }
                // Sans recherche profonde, on ne descend pas dans un dépôt : c'est l'essentiel du coût.
                if opts.include_submodules { WalkState::Continue } else { WalkState::Skip }
            })
        });
    }

    let mut found = results.into_inner().unwrap();
    // Tri composant par composant : « Survey/x » avant « Survey2/x ».
    found.sort_by_cached_key(|r| {
        r.path.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect::<Vec<_>>()
    });
    found
}

/// Nature du dépôt dont `dir` est la racine, s'il y en a un.
fn detect(dir: &Path) -> Option<RepoKind> {
    let dotgit = dir.join(".git");
    let meta = std::fs::symlink_metadata(&dotgit).ok()?;
    if meta.is_dir() {
        return dotgit.join("HEAD").is_file().then_some(RepoKind::Normal);
    }
    let gitdir = read_gitdir_file(&dotgit)?.replace('\\', "/");
    Some(if gitdir.contains("/worktrees/") {
        RepoKind::Worktree
    } else if gitdir.contains("/modules/") {
        RepoKind::Submodule
    } else {
        RepoKind::Normal
    })
}

fn read_gitdir_file(dotgit: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dotgit).ok()?;
    content.lines().find_map(|l| l.strip_prefix("gitdir:")).map(|g| g.trim().to_string())
}

/// État de HEAD d'un dépôt trouvé, lu directement sur disque (suit un fichier `.git` `gitdir:`).
pub fn repo_head(repo_path: &Path) -> Option<HeadState> {
    let dotgit = repo_path.join(".git");
    let meta = std::fs::symlink_metadata(&dotgit).ok()?;
    let git_dir = if meta.is_dir() {
        dotgit
    } else {
        let gd = PathBuf::from(read_gitdir_file(&dotgit)?);
        if gd.is_absolute() { gd } else { repo_path.join(gd) }
    };
    read_head(&git_dir).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkrepo(base: &Path, rel: &str) -> PathBuf {
        let p = base.join(rel);
        std::fs::create_dir_all(p.join(".git")).unwrap();
        std::fs::write(p.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        p
    }

    fn mkdotgit_file(base: &Path, rel: &str, gitdir: &str) -> PathBuf {
        let p = base.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
        p
    }

    fn names(v: &[FoundRepo]) -> Vec<String> {
        v.iter().map(|r| r.name.clone()).collect()
    }

    fn run(root: &Path, opts: &ScanOptions) -> Vec<FoundRepo> {
        scan(&[root.to_path_buf()], opts, &CancelToken::new(), &|_| {})
    }

    #[test]
    fn discovery_finds_repos_at_various_depths() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "A");
        mkrepo(d.path(), "C/D/E");
        std::fs::create_dir_all(d.path().join("vide/rien")).unwrap();
        let found = run(d.path(), &ScanOptions::default());
        assert_eq!(names(&found), vec!["A", "E"]);
        assert_eq!(found[1].relative_parent(), "C/D");
        assert_eq!(found[0].relative_parent(), "");
        assert!(found.iter().all(|r| r.kind == RepoKind::Normal && r.root == d.path()));
    }

    #[test]
    fn discovery_does_not_descend_into_repos_by_default() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "A");
        mkrepo(d.path(), "A/libs/B");
        assert_eq!(names(&run(d.path(), &ScanOptions::default())), vec!["A"]);
        let deep = ScanOptions { include_submodules: true, ..ScanOptions::default() };
        assert_eq!(names(&run(d.path(), &deep)), vec!["A", "B"]);
    }

    #[test]
    fn discovery_skips_node_modules() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "app");
        mkrepo(d.path(), "Front/node_modules/lib");
        mkrepo(d.path(), "Back/BIN/x");
        assert_eq!(names(&run(d.path(), &ScanOptions::default())), vec!["app"]);
    }

    #[test]
    fn discovery_classifies_worktree_and_submodule() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "main");
        mkdotgit_file(d.path(), "wt", "../main/.git/worktrees/wt");
        mkrepo(d.path(), "parent");
        mkdotgit_file(d.path(), "parent/sub", "../.git/modules/sub");
        let found = run(d.path(), &ScanOptions { include_submodules: true, ..ScanOptions::default() });
        let kind = |n: &str| found.iter().find(|r| r.name == n).map(|r| r.kind);
        assert_eq!(kind("main"), Some(RepoKind::Normal));
        assert_eq!(kind("wt"), Some(RepoKind::Worktree));
        assert_eq!(kind("sub"), Some(RepoKind::Submodule));
        let shallow = run(d.path(), &ScanOptions::default());
        assert!(shallow.iter().all(|r| r.name != "sub"));
        assert!(shallow.iter().any(|r| r.name == "wt"));
    }

    #[test]
    fn discovery_respects_max_depth() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "a/b/c/d/profond");
        mkrepo(d.path(), "proche");
        let found = run(d.path(), &ScanOptions { max_depth: 3, ..ScanOptions::default() });
        assert_eq!(names(&found), vec!["proche"]);
    }

    #[test]
    fn discovery_handles_spaces_and_root_itself() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "Peppol - Copie");
        assert_eq!(names(&run(d.path(), &ScanOptions::default())), vec!["Peppol - Copie"]);
        let root_repo = mkrepo(d.path(), "seul");
        let found = run(&root_repo, &ScanOptions::default());
        assert_eq!(names(&found), vec!["seul"]);
    }

    #[test]
    fn discovery_sorts_by_path_components() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "Survey2/Projet");
        mkrepo(d.path(), "Survey/Projet");
        mkrepo(d.path(), "alpha");
        let found = run(d.path(), &ScanOptions::default());
        let parents: Vec<String> = found.iter().map(|r| r.relative_parent()).collect();
        assert_eq!(parents, vec!["", "Survey", "Survey2"]);
    }

    #[test]
    fn discovery_dedupes_overlapping_roots() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "x/r");
        let roots = vec![d.path().to_path_buf(), d.path().join("x")];
        let found = scan(&roots, &ScanOptions::default(), &CancelToken::new(), &|_| {});
        assert_eq!(names(&found), vec!["r"]);
    }

    #[test]
    fn discovery_reports_progressively() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "a");
        mkrepo(d.path(), "b");
        let seen = std::sync::Mutex::new(Vec::new());
        scan(&[d.path().to_path_buf()], &ScanOptions::default(), &CancelToken::new(), &|r| {
            seen.lock().unwrap().push(r.name)
        });
        let mut seen = seen.into_inner().unwrap();
        seen.sort();
        assert_eq!(seen, vec!["a", "b"]);
    }

    #[test]
    fn discovery_cancelled_returns_quickly() {
        let d = tempfile::tempdir().unwrap();
        mkrepo(d.path(), "a");
        let token = CancelToken::new();
        token.cancel();
        let found = scan(&[d.path().to_path_buf()], &ScanOptions::default(), &token, &|_| {});
        assert!(found.len() <= 1);
    }

    #[test]
    fn repo_head_follows_gitdir_file() {
        let d = tempfile::tempdir().unwrap();
        let main = mkrepo(d.path(), "main");
        std::fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
        std::fs::write(main.join(".git/worktrees/wt/HEAD"), "ref: refs/heads/feature/é\n").unwrap();
        let wt = mkdotgit_file(d.path(), "wt", "../main/.git/worktrees/wt");
        assert_eq!(repo_head(&main), Some(HeadState::Branch("main".into())));
        assert_eq!(repo_head(&wt), Some(HeadState::Branch("feature/é".into())));
        assert_eq!(repo_head(d.path()), None);
    }
}
