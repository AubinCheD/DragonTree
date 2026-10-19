//! Surveillance du dossier du dépôt : rafraîchit l'état quand des fichiers changent.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

use super::RepoEvent;
use crate::app::{AppEvent, TabId};
use crate::jobs::EventSender;

pub struct RepoWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
}

/// Effet d'un chemin modifié : (références/index git, copie de travail). Les dossiers de
/// `skip` (bin, obj, node_modules…) sont ignorés : un build ne déclenche pas de `git status`.
pub fn classify(path: &Path, workdir: &Path, git_dir: &Path, skip: &[String]) -> (bool, bool) {
    let in_git = |base: &Path| path.strip_prefix(base).ok().map(Path::to_path_buf);
    let rel: Option<PathBuf> = in_git(git_dir).or_else(|| in_git(&workdir.join(".git")));
    match rel {
        Some(rel) => {
            let first =
                rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default();
            let name = rel.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let ignored = name.ends_with(".lock")
                || matches!(
                    first.as_str(),
                    "objects" | "logs" | "dragontree-rebase" | "hooks" | "info" | "lfs" | "modules" | "worktrees"
                )
                || name == "fetch_head"
                || name == "orig_head";
            (!ignored, false)
        }
        None => {
            let in_skipped_dir = path
                .strip_prefix(workdir)
                .map(|rel| {
                    rel.components()
                        .any(|c| skip.iter().any(|s| s.eq_ignore_ascii_case(&c.as_os_str().to_string_lossy())))
                })
                .unwrap_or(false);
            (false, !in_skipped_dir)
        }
    }
}

impl RepoWatcher {
    pub fn start(
        workdir: &Path,
        git_dir: &Path,
        tab: TabId,
        sender: EventSender,
        skip: Vec<String>,
    ) -> Option<RepoWatcher> {
        let (wd, gd) = (workdir.to_path_buf(), git_dir.to_path_buf());
        let mut debouncer = new_debouncer(Duration::from_millis(250), move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let (mut git, mut work) = (false, false);
            for e in &events {
                let (g, w) = classify(&e.path, &wd, &gd, &skip);
                git |= g;
                work |= w;
            }
            if git || work {
                sender.send(AppEvent::Repo(tab, RepoEvent::FsChanged { git_dir: git, worktree: work }));
            }
        })
        .ok()?;
        debouncer.watcher().watch(workdir, RecursiveMode::Recursive).ok()?;
        // Arbre de travail secondaire : son dossier git est ailleurs.
        if !git_dir.starts_with(workdir) {
            let _ = debouncer.watcher().watch(git_dir, RecursiveMode::Recursive);
        }
        Some(RepoWatcher { _debouncer: debouncer })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_paths() {
        let wd = Path::new(r"C:\r");
        let gd = Path::new(r"C:\r\.git");
        let skip = vec!["bin".to_string(), "node_modules".to_string()];
        assert_eq!(classify(Path::new(r"C:\r\src\a.cs"), wd, gd, &skip), (false, true));
        assert_eq!(classify(Path::new(r"C:\r\.git\index"), wd, gd, &skip), (true, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\refs\heads\main"), wd, gd, &skip), (true, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\HEAD"), wd, gd, &skip), (true, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\index.lock"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\objects\ab\cdef"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\logs\HEAD"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\.git\FETCH_HEAD"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\Web\BIN\Debug\a.dll"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\front\node_modules\x\i.js"), wd, gd, &skip), (false, false));
        assert_eq!(classify(Path::new(r"C:\r\binaire.txt"), wd, gd, &skip), (false, true));
    }
}
