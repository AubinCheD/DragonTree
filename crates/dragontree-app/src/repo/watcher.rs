//! Surveillance du dossier du dépôt : rafraîchit l'état quand des fichiers changent.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify_debouncer_mini::notify::{Config, RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

use super::RepoEvent;
use crate::app::{AppEvent, TabId};
use crate::jobs::EventSender;

pub struct RepoWatcher {
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    /// Dossiers surveillés, rendus à la fermeture (voir `Drop`).
    watched: Vec<PathBuf>,
    /// L'application se termine : rien n'est rendu ni attendu (voir `Drop`).
    exiting: bool,
}

/// Attente maximale, à la fermeture, de la fin de la surveillance : au-delà, le thread de l'interface reprend la main
/// et la fin se poursuit en arrière-plan.
const RELEASE_WAIT: Duration = Duration::from_secs(1);

impl Drop for RepoWatcher {
    /// Ferme les dossiers surveillés avant de rendre la main, en attendant au plus `RELEASE_WAIT`. Seul, `notify`
    /// arrête sa surveillance plus tard, sur son thread : un dossier supprimé entre-temps (worktree retiré, dossier
    /// temporaire d'un test) pendant qu'il se réarme peut bloquer pour toujours la suppression et ce thread (vu sous
    /// Windows). Sur un thread d'aide : les `unwatch`, puis `configure`, qui attend la réponse du thread de `notify`
    /// (il a d'abord traité les `unwatch` : les poignées des dossiers sont alors fermées). Si `notify` ne répond
    /// jamais, l'interface ne reste pas figée. À la sortie de l'application (`on_app_exit`), rien n'est attendu.
    fn drop(&mut self) {
        let Some(mut debouncer) = self.debouncer.take() else { return };
        if self.exiting {
            std::mem::forget(debouncer);
            return;
        }
        let watched = std::mem::take(&mut self.watched);
        let released = run_bounded("watch-release", RELEASE_WAIT, move || {
            let watcher = debouncer.watcher();
            for dir in &watched {
                let _ = watcher.unwatch(dir);
            }
            let _ = watcher.configure(Config::default());
            drop(debouncer);
        });
        if !released {
            eprintln!("DragonTree : surveillance du dépôt pas arrêtée après {RELEASE_WAIT:?}, fin en arrière-plan");
        }
    }
}

/// Lance `f` sur un thread et attend sa fin au plus `wait` ; vrai si elle est finie. Au-delà, `f` continue seule.
fn run_bounded(name: &str, wait: Duration, f: impl FnOnce() + Send + 'static) -> bool {
    let (done_tx, done_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name(format!("dragontree-{name}")).spawn(move || {
        f();
        let _ = done_tx.send(());
    });
    spawned.is_ok() && done_rx.recv_timeout(wait).is_ok()
}

/// Effet d'un chemin modifié : (références/index git, copie de travail, liste des worktrees). Les dossiers de
/// `skip` (bin, obj, node_modules…) sont ignorés : un build ne déclenche pas de `git status`.
pub fn classify(path: &Path, workdir: &Path, git_dir: &Path, common_dir: &Path, skip: &[String]) -> (bool, bool, bool) {
    let in_git = |base: &Path| path.strip_prefix(base).ok().map(Path::to_path_buf);
    let rel: Option<PathBuf> = in_git(git_dir).or_else(|| in_git(&workdir.join(".git")));
    let first_of = |rel: &Path| {
        rel.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).unwrap_or_default()
    };
    // `<common>/worktrees` décrit les worktrees liés (un dossier chacun, avec son HEAD). Celui d'un worktree lié
    // ouvert dans l'onglet est son propre dossier git : traité plus bas.
    if let Ok(other) = path.strip_prefix(common_dir.join("worktrees"))
        && rel.as_deref().is_none_or(|r| first_of(r) == "worktrees")
    {
        let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let noise = name.ends_with(".lock") || name == "index" || other.components().any(|c| c.as_os_str() == "logs");
        return (false, false, !noise);
    }
    match rel {
        Some(rel) => {
            let first = first_of(&rel);
            let name = rel.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let ignored = name.ends_with(".lock")
                || matches!(
                    first.as_str(),
                    "objects" | "logs" | "dragontree-rebase" | "hooks" | "info" | "lfs" | "modules" | "worktrees"
                )
                || name == "fetch_head"
                || name == "orig_head";
            (!ignored, false, false)
        }
        None => {
            let in_skipped_dir = path
                .strip_prefix(workdir)
                .map(|rel| {
                    rel.components()
                        .any(|c| skip.iter().any(|s| s.eq_ignore_ascii_case(&c.as_os_str().to_string_lossy())))
                })
                .unwrap_or(false);
            (false, !in_skipped_dir, false)
        }
    }
}

impl RepoWatcher {
    pub fn start(
        workdir: &Path,
        git_dir: &Path,
        common_dir: &Path,
        tab: TabId,
        sender: EventSender,
        skip: Vec<String>,
    ) -> Option<RepoWatcher> {
        let (wd, gd, cd) = (workdir.to_path_buf(), git_dir.to_path_buf(), common_dir.to_path_buf());
        let mut debouncer = new_debouncer(Duration::from_millis(250), move |res: DebounceEventResult| {
            let Ok(events) = res else { return };
            let (mut git, mut work, mut worktrees) = (false, false, false);
            for e in &events {
                let (g, w, t) = classify(&e.path, &wd, &gd, &cd, &skip);
                git |= g;
                work |= w;
                worktrees |= t;
            }
            if git || work || worktrees {
                sender.send(AppEvent::Repo(tab, RepoEvent::FsChanged { git_dir: git, worktree: work, worktrees }));
            }
        })
        .ok()?;
        debouncer.watcher().watch(workdir, RecursiveMode::Recursive).ok()?;
        let mut watched = vec![workdir.to_path_buf()];
        // Arbre de travail secondaire : son dossier git est ailleurs.
        if !git_dir.starts_with(workdir) && debouncer.watcher().watch(git_dir, RecursiveMode::Recursive).is_ok() {
            watched.push(git_dir.to_path_buf());
        }
        // Liste des worktrees liés, hors de la copie de travail pour un worktree lié (créée au premier ajout sinon).
        let worktrees = common_dir.join("worktrees");
        if !worktrees.starts_with(workdir)
            && !worktrees.starts_with(git_dir)
            && worktrees.is_dir()
            && debouncer.watcher().watch(&worktrees, RecursiveMode::Recursive).is_ok()
        {
            watched.push(worktrees);
        }
        Some(RepoWatcher { debouncer: Some(debouncer), watched, exiting: false })
    }

    /// L'application se termine : la surveillance n'est pas arrêtée proprement, Windows ferme ses poignées avec le
    /// processus. Sinon chaque onglet attendrait jusqu'à `RELEASE_WAIT` à la sortie.
    pub fn on_app_exit(&mut self) {
        self.exiting = true;
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
        let c = |p: &str| classify(Path::new(p), wd, gd, gd, &skip);
        assert_eq!(c(r"C:\r\src\a.cs"), (false, true, false));
        assert_eq!(c(r"C:\r\.git\index"), (true, false, false));
        assert_eq!(c(r"C:\r\.git\refs\heads\main"), (true, false, false));
        assert_eq!(c(r"C:\r\.git\HEAD"), (true, false, false));
        assert_eq!(c(r"C:\r\.git\index.lock"), (false, false, false));
        assert_eq!(c(r"C:\r\.git\objects\ab\cdef"), (false, false, false));
        assert_eq!(c(r"C:\r\.git\logs\HEAD"), (false, false, false));
        assert_eq!(c(r"C:\r\.git\FETCH_HEAD"), (false, false, false));
        assert_eq!(c(r"C:\r\Web\BIN\Debug\a.dll"), (false, false, false));
        assert_eq!(c(r"C:\r\front\node_modules\x\i.js"), (false, false, false));
        assert_eq!(c(r"C:\r\binaire.txt"), (false, true, false));
    }

    #[test]
    fn worktree_list_changes_refresh_only_the_worktrees() {
        let skip = Vec::new();
        // Worktree principal : `.git/worktrees` décrit les autres.
        let (wd, gd) = (Path::new(r"C:\r"), Path::new(r"C:\r\.git"));
        let c = |p: &str| classify(Path::new(p), wd, gd, gd, &skip);
        assert_eq!(c(r"C:\r\.git\worktrees"), (false, false, true));
        assert_eq!(c(r"C:\r\.git\worktrees\r-x"), (false, false, true));
        assert_eq!(c(r"C:\r\.git\worktrees\r-x\HEAD"), (false, false, true));
        assert_eq!(c(r"C:\r\.git\worktrees\r-x\index"), (false, false, false));
        assert_eq!(c(r"C:\r\.git\worktrees\r-x\index.lock"), (false, false, false));
        assert_eq!(c(r"C:\r\.git\worktrees\r-x\logs\HEAD"), (false, false, false));

        // Worktree lié : son dossier git est dans `<common>/worktrees`, les autres à côté.
        let (wd, gd, cd) = (Path::new(r"C:\r-x"), Path::new(r"C:\r\.git\worktrees\r-x"), Path::new(r"C:\r\.git"));
        let c = |p: &str| classify(Path::new(p), wd, gd, cd, &skip);
        assert_eq!(c(r"C:\r\.git\worktrees\r-x\HEAD"), (true, false, false), "son propre HEAD");
        assert_eq!(c(r"C:\r\.git\worktrees\r-y\HEAD"), (false, false, true), "celui d'un autre worktree");
        assert_eq!(c(r"C:\r\.git\worktrees\r-z"), (false, false, true), "un worktree ajouté");
        assert_eq!(c(r"C:\r-x\src\a.cs"), (false, true, false));
    }

    /// Dossier de travail surveillé, avec son dossier `.git` (la surveillance ne lance pas git) et un fichier.
    fn watched_folder() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let wd = tmp.path().join("depot");
        std::fs::create_dir_all(wd.join(".git")).unwrap();
        std::fs::write(wd.join("a.txt"), "a\n").unwrap();
        (tmp, wd)
    }

    fn start(wd: &Path) -> (RepoWatcher, mpsc::Receiver<AppEvent>) {
        let (jobs, events) = crate::jobs::Jobs::new(egui::Context::default());
        let gd = wd.join(".git");
        let watcher = RepoWatcher::start(wd, &gd, &gd, 1, jobs.sender(), Vec::new()).expect("surveillance démarrée");
        (watcher, events)
    }

    /// Supprime `dir` sur un thread, attendu au plus 30 s : un blocage fait échouer le test au lieu de le figer.
    fn remove_within_bound(dir: PathBuf) {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(std::fs::remove_dir_all(&dir));
        });
        let removed = rx.recv_timeout(Duration::from_secs(30)).expect("suppression du dossier surveillé bloquée");
        removed.expect("dossier surveillé supprimé");
    }

    #[test]
    fn the_watched_folder_is_removed_right_after_the_watcher_is_dropped() {
        let (_tmp, wd) = watched_folder();
        let (watcher, _events) = start(&wd);
        drop(watcher);
        remove_within_bound(wd);
    }

    #[test]
    fn a_watcher_dropped_while_files_change_releases_its_folder() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let (_tmp, wd) = watched_folder();
        let (watcher, events) = start(&wd);
        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let (wd, stop) = (wd.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut i = 0u32;
                while !stop.load(Ordering::Relaxed) {
                    let _ = std::fs::write(wd.join(format!("f{}.txt", i % 20)), i.to_string());
                    i += 1;
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
        };
        // Les changements sont signalés : la surveillance tourne quand on l'arrête.
        let changed = events.recv_timeout(Duration::from_secs(30)).expect("changement signalé");
        assert!(matches!(changed, AppEvent::Repo(1, RepoEvent::FsChanged { worktree: true, .. })));
        drop(watcher);
        stop.store(true, Ordering::Relaxed);
        writer.join().unwrap();
        remove_within_bound(wd);
    }

    #[test]
    fn the_release_wait_is_bounded_even_if_it_never_ends() {
        assert!(run_bounded("test", Duration::from_secs(30), || {}), "une fin rapide est attendue");
        // Une fin qui ne vient jamais (thread de notify bloqué) : on rend la main après le délai.
        let (never_tx, never_rx) = mpsc::channel::<()>();
        let start = std::time::Instant::now();
        let done = run_bounded("test", Duration::from_millis(200), move || {
            let _ = never_rx.recv();
        });
        assert!(!done);
        assert!(start.elapsed() < Duration::from_secs(20));
        drop(never_tx);
    }
}
