//! Files d'opérations des dépôts, partagées par leurs onglets.
//!
//! - **Écritures** : une file par dossier git (`git_dir`, propre à chaque worktree comme son index et son
//!   `index.lock`), commune à tous les onglets ouverts sur ce dépôt (« Ouvrir dans un nouvel onglet ») : les
//!   opérations s'y exécutent dans l'ordre des clics, quel que soit l'onglet, sans collision sur `index.lock`.
//! - **Réseau silencieux** (fetch automatique) : une file à part par dépôt (`common_dir`, commun à ses worktrees, qui
//!   partagent les refs) : un fetch lent ne fait plus attendre une indexation.
//!
//! Entre les deux, le fetch automatique ne tourne jamais en même temps qu'une opération du dépôt qui écrit ses refs
//! distantes : opération réseau visible (fetch, pull, push, test de connexion…) ou opération locale qui réécrit
//! `refs/remotes` (suppression d'un remote), toutes dites « réseau » ici ([`PendingOp::network`]) :
//! - un fetch automatique n'est pas lancé quand une telle opération attend ou tourne sur une file d'écriture du dépôt
//!   ([`OpQueues::network_busy`]), ni quand la file silencieuse n'est pas vide (un autre onglet du même dépôt fetche
//!   déjà) : il attend l'échéance suivante ;
//! - une telle opération, à son tour, annule le fetch automatique qui n'a pas encore commencé (git n'est pas lancé)
//!   mais attend la fin de celui qui tourne, sans l'arrêter ([`OpQueue::make_way`]) : git tué pendant l'écriture des
//!   refs (transaction, réécriture de `packed-refs`) laisserait des fichiers `*.lock`, et le `git maintenance --auto`
//!   qu'il lance serait tué avec lui. Seule la fermeture de l'application l'arrête (décision RV4a : c'est le choix
//!   de l'utilisateur).
//!
//! Limite : deux opérations réseau visibles lancées depuis deux worktrees du même dépôt sont sur deux files
//! d'écriture et peuvent tourner ensemble ; elles écrivent alors les mêmes refs distantes. Les verrous de git
//! (`*.lock`) font échouer l'une des deux (« cannot lock ref »), sans dommage ; l'utilisateur a lancé les deux.
//!
//! Chaque file tient la liste de ses opérations en attente ou en cours ([`OpQueue::pending`],
//! [`OpQueues::pending`]) : de quoi les retrouver toutes, et les annuler, à la fermeture de l'application.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use dragontree_git::{CancelToken, Repo};

use crate::app::TabId;
use crate::jobs::SerialWorker;
use crate::settings::path_key;

/// Intervalle auquel [`OpQueue::make_way`] revérifie l'annulation de l'opération qui attend.
const WAKE_UP: Duration = Duration::from_millis(50);

/// Opération en attente ou en cours sur une file.
#[derive(Clone, Debug)]
pub struct PendingOp {
    /// Onglet qui l'a lancée et son numéro dans cet onglet.
    pub tab: TabId,
    pub id: u64,
    pub title: String,
    /// Opération réseau, ou locale qui réécrit les refs distantes (suppression d'un remote) : le fetch automatique ne
    /// tourne pas en même temps (voir le module).
    pub network: bool,
    pub silent: bool,
    pub cancel: CancelToken,
}

/// Opération de la liste d'une file, et si sa tâche a commencé.
struct Entry {
    op: PendingOp,
    started: bool,
}

struct QueueInner {
    worker: SerialWorker,
    pending: Mutex<Vec<Entry>>,
    /// Signalée quand une opération quitte la liste.
    changed: Condvar,
    /// Dépôt de la file (dossier git commun, voir [`path_key`]) : regroupe les files d'écriture de ses worktrees.
    group: String,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// File d'exécution séquentielle (un thread) qui tient la liste de ses opérations. Les clones désignent la même file.
#[derive(Clone)]
pub struct OpQueue {
    inner: Arc<QueueInner>,
}

/// Retire l'opération de la liste à la fin de sa tâche, même si elle panique.
struct Done {
    inner: Arc<QueueInner>,
    key: (TabId, u64),
}

impl Drop for Done {
    fn drop(&mut self) {
        lock(&self.inner.pending).retain(|e| (e.op.tab, e.op.id) != self.key);
        self.inner.changed.notify_all();
    }
}

impl OpQueue {
    fn new(name: &str, group: String) -> OpQueue {
        OpQueue {
            inner: Arc::new(QueueInner {
                worker: SerialWorker::new(name),
                pending: Mutex::new(Vec::new()),
                changed: Condvar::new(),
                group,
            }),
        }
    }

    /// Ajoute `op` à la liste et `f` à la file : `f` s'exécute après ce qui y est déjà, puis `op` quitte la liste.
    pub fn submit(&self, op: PendingOp, f: impl FnOnce() + Send + 'static) {
        let done = Done { inner: Arc::clone(&self.inner), key: (op.tab, op.id) };
        lock(&self.inner.pending).push(Entry { op, started: false });
        self.inner.worker.submit(move || {
            // Marquée sous le verrou : `make_way` l'annule avant (sa tâche ne lancera pas git) ou l'attend après.
            if let Some(entry) = lock(&done.inner.pending).iter_mut().find(|e| (e.op.tab, e.op.id) == done.key) {
                entry.started = true;
            }
            let _done = done;
            f();
        });
    }

    /// Tâche sans suivi (une lecture qui doit passer après les écritures déjà en file).
    pub fn then(&self, f: impl FnOnce() + Send + 'static) {
        self.inner.worker.submit(f);
    }

    /// Opérations en attente ou en cours, dans l'ordre de la file.
    pub fn pending(&self) -> Vec<PendingOp> {
        lock(&self.inner.pending).iter().map(|e| e.op.clone()).collect()
    }

    pub fn is_idle(&self) -> bool {
        lock(&self.inner.pending).is_empty()
    }

    /// Une opération réseau attend ou tourne.
    pub fn has_network_op(&self) -> bool {
        lock(&self.inner.pending).iter().any(|e| e.op.network)
    }

    /// Laisse la place à une opération qui ne doit pas tourner en même temps que celles de cette file (une opération
    /// réseau visible face au fetch automatique) : annule celles qui n'ont pas commencé (leur tâche ne lancera pas
    /// git), puis attend la fin de celles qui tournent, sans les annuler. `on_wait` est appelé une fois, si l'attente
    /// commence. Renvoie faux si `stop` (l'annulation de l'opération qui attend) est déclenché pendant l'attente.
    /// Jamais depuis une tâche de cette même file (elle s'attendrait elle-même).
    pub fn make_way(&self, stop: &CancelToken, on_wait: impl FnOnce()) -> bool {
        let mut on_wait = Some(on_wait);
        let mut pending = lock(&self.inner.pending);
        loop {
            for entry in pending.iter().filter(|e| !e.started) {
                entry.op.cancel.cancel();
            }
            if !pending.iter().any(|e| e.started) {
                return true;
            }
            if stop.is_cancelled() {
                return false;
            }
            if let Some(f) = on_wait.take() {
                // Sans le verrou : `f` prévient l'interface, qui lit cette liste à chaque image.
                drop(pending);
                f();
                pending = lock(&self.inner.pending);
                continue;
            }
            // Réveillée à chaque fin d'opération, et régulièrement pour voir l'annulation de `stop`.
            pending = self.inner.changed.wait_timeout(pending, WAKE_UP).unwrap_or_else(PoisonError::into_inner).0;
        }
    }

    /// Annule les opérations en attente ou en cours et attend que la file soit vide, au plus `timeout` ; renvoie vrai
    /// si elle l'est (tests : vider une file entre deux étapes). Jamais depuis une tâche de cette même file.
    #[cfg(test)]
    pub fn cancel_and_wait(&self, timeout: Duration) -> bool {
        use std::time::Instant;
        let deadline = Instant::now() + timeout;
        let mut pending = lock(&self.inner.pending);
        for entry in pending.iter() {
            entry.op.cancel.cancel();
        }
        while !pending.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            pending = self.inner.changed.wait_timeout(pending, left).unwrap_or_else(PoisonError::into_inner).0;
        }
        true
    }

    /// Même file (même dépôt) que `other`.
    pub fn same_as(&self, other: &OpQueue) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Files d'un dépôt pour un onglet.
#[derive(Clone)]
pub struct RepoQueues {
    /// Écritures et opérations réseau visibles, dans l'ordre des clics (dossier git de l'onglet).
    pub writes: OpQueue,
    /// Opérations réseau silencieuses (dépôt commun aux worktrees).
    pub background: OpQueue,
}

impl RepoQueues {
    /// Dépôt des files (dossier git commun) : à passer à [`OpQueues::network_busy`].
    pub fn group(&self) -> &str {
        &self.writes.inner.group
    }
}

/// Registre des files de tous les dépôts ouverts. Une file vit tant qu'un onglet la tient ou qu'une opération y
/// attend ; rouvrir le dépôt pendant ce temps redonne la même.
#[derive(Default)]
pub struct OpQueues {
    writes: Mutex<HashMap<String, Weak<QueueInner>>>,
    background: Mutex<HashMap<String, Weak<QueueInner>>>,
}

impl OpQueues {
    /// Files de `repo` : celle des écritures de son dossier git et celle du réseau silencieux de son dépôt commun.
    pub fn for_repo(&self, repo: &Repo) -> RepoQueues {
        let group = path_key(&repo.common_dir);
        RepoQueues {
            writes: Self::get(&self.writes, path_key(&repo.git_dir), &group, "repo"),
            background: Self::get(&self.background, group.clone(), &group, "repo-reseau"),
        }
    }

    fn get(map: &Mutex<HashMap<String, Weak<QueueInner>>>, key: String, group: &str, name: &str) -> OpQueue {
        let mut map = lock(map);
        map.retain(|_, queue| queue.strong_count() > 0);
        if let Some(inner) = map.get(&key).and_then(Weak::upgrade) {
            return OpQueue { inner };
        }
        let queue = OpQueue::new(name, group.to_string());
        map.insert(key, Arc::downgrade(&queue.inner));
        queue
    }

    /// Une opération réseau attend ou tourne sur une file d'écriture du dépôt `group` (un de ses worktrees).
    pub fn network_busy(&self, group: &str) -> bool {
        Self::live(&self.writes).iter().any(|q| q.inner.group == group && q.has_network_op())
    }

    /// Opérations en attente ou en cours sur toutes les files, écritures puis réseau silencieux.
    pub fn pending(&self) -> Vec<PendingOp> {
        Self::live(&self.writes).iter().chain(&Self::live(&self.background)).flat_map(OpQueue::pending).collect()
    }

    fn live(map: &Mutex<HashMap<String, Weak<QueueInner>>>) -> Vec<OpQueue> {
        lock(map).values().filter_map(Weak::upgrade).map(|inner| OpQueue { inner }).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::Instant;

    use dragontree_git::Git;

    use super::*;

    fn repo_at(workdir: &str, git_dir: &str, common_dir: &str) -> Repo {
        Repo {
            git: Git::with_exe(PathBuf::from("git")),
            workdir: PathBuf::from(workdir),
            git_dir: PathBuf::from(git_dir),
            common_dir: PathBuf::from(common_dir),
        }
    }

    fn op(tab: TabId, id: u64, network: bool, silent: bool) -> PendingOp {
        PendingOp { tab, id, title: format!("op {id}"), network, silent, cancel: CancelToken::new() }
    }

    #[test]
    fn tabs_on_the_same_repository_share_its_queues_and_worktrees_share_the_silent_one() {
        let queues = OpQueues::default();
        let main = repo_at("C:/depots/a", "C:/depots/a/.git", "C:/depots/a/.git");
        // Même dépôt ouvert depuis un sous-dossier, chemin écrit autrement.
        let again = repo_at("C:/depots/a", "c:\\Depots\\A\\.git\\", "c:\\Depots\\A\\.git");
        let worktree = repo_at("C:/depots/a-wt", "C:/depots/a/.git/worktrees/a-wt", "C:/depots/a/.git");
        let other = repo_at("C:/depots/b", "C:/depots/b/.git", "C:/depots/b/.git");
        let (first, second) = (queues.for_repo(&main), queues.for_repo(&again));
        assert!(first.writes.same_as(&second.writes), "une seule file d'écriture par dépôt");
        assert!(first.background.same_as(&second.background));
        let wt = queues.for_repo(&worktree);
        assert!(!wt.writes.same_as(&first.writes), "un worktree a son propre index");
        assert!(wt.background.same_as(&first.background), "mais partage les refs de son dépôt");
        assert_eq!(wt.group(), first.group());
        assert!(!queues.for_repo(&other).writes.same_as(&first.writes));
    }

    #[test]
    fn a_queue_lives_while_a_tab_holds_it_or_an_operation_waits() {
        let queues = OpQueues::default();
        let repo = repo_at("C:/d", "C:/d/.git", "C:/d/.git");
        let held = queues.for_repo(&repo);
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        held.writes.submit(op(1, 1, false, false), move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        drop(held);
        // L'onglet est fermé, l'opération attend encore : elle reste visible, et le dépôt rouvert retrouve sa file.
        assert_eq!(queues.pending().len(), 1);
        let reopened = queues.for_repo(&repo);
        assert_eq!(reopened.writes.pending().len(), 1, "même file : l'opération y attend toujours");
        gate_tx.send(()).unwrap();
        assert!(reopened.writes.cancel_and_wait(Duration::from_secs(10)));
    }

    #[test]
    fn network_operations_are_seen_across_the_write_queues_of_a_repository() {
        let queues = OpQueues::default();
        let main = queues.for_repo(&repo_at("C:/a", "C:/a/.git", "C:/a/.git"));
        let wt = queues.for_repo(&repo_at("C:/a-wt", "C:/a/.git/worktrees/a-wt", "C:/a/.git"));
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        assert!(!queues.network_busy(main.group()));
        wt.writes.submit(op(2, 1, true, false), move || {
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        assert!(queues.network_busy(main.group()), "pull en cours dans un worktree du dépôt");
        assert!(!queues.network_busy("c:/autre/.git"));
        gate_tx.send(()).unwrap();
        assert!(wt.writes.cancel_and_wait(Duration::from_secs(10)));
        assert!(!queues.network_busy(main.group()));
    }

    #[test]
    fn cancel_and_wait_cancels_the_running_operations_and_waits_for_them() {
        let queue = OpQueue::new("test", String::new());
        let running = op(1, 1, true, true);
        let cancel = running.cancel.clone();
        let (started_tx, started_rx) = mpsc::channel();
        queue.submit(running, move || {
            started_tx.send(()).unwrap();
            // Opération qui ne s'arrête que si on l'annule (un fetch bloqué).
            let deadline = Instant::now() + Duration::from_secs(10);
            while !cancel.is_cancelled() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let queued = op(1, 2, true, true);
        let queued_cancel = queued.cancel.clone();
        queue.submit(queued, || {});
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(queue.pending().iter().map(|o| o.id).collect::<Vec<_>>(), [1, 2]);
        assert!(queue.cancel_and_wait(Duration::from_secs(10)), "file vidée");
        assert!(queue.is_idle() && queued_cancel.is_cancelled());
    }

    #[test]
    fn making_way_cancels_what_has_not_started_and_waits_for_what_runs() {
        let queue = OpQueue::new("test", String::new());
        let running = op(1, 1, true, true);
        let running_cancel = running.cancel.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        queue.submit(running, move || {
            started_tx.send(()).unwrap();
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        let queued = op(1, 2, true, true);
        let queued_cancel = queued.cancel.clone();
        queue.submit(queued, || {});
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

        let (waiting_tx, waiting_rx) = mpsc::channel();
        let waiter = std::thread::spawn({
            let queue = queue.clone();
            move || queue.make_way(&CancelToken::new(), move || waiting_tx.send(()).unwrap())
        });
        waiting_rx.recv_timeout(Duration::from_secs(10)).expect("attente signalée");
        assert!(queued_cancel.is_cancelled(), "l'opération qui n'a pas commencé est annulée");
        assert!(!running_cancel.is_cancelled(), "celle qui tourne n'est pas arrêtée");
        assert!(!waiter.is_finished(), "elle est attendue");
        gate_tx.send(()).unwrap();
        assert!(waiter.join().unwrap(), "place faite à sa fin");
        assert!(!running_cancel.is_cancelled());

        // File vide : rien à attendre, `on_wait` n'est pas appelé.
        assert!(queue.make_way(&CancelToken::new(), || panic!("pas d'attente")));
    }

    #[test]
    fn making_way_stops_waiting_when_the_waiting_operation_is_cancelled() {
        let queue = OpQueue::new("test", String::new());
        let (started_tx, started_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        queue.submit(op(1, 1, true, true), move || {
            started_tx.send(()).unwrap();
            let _ = gate_rx.recv_timeout(Duration::from_secs(10));
        });
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let stop = CancelToken::new();
        let canceller = stop.clone();
        assert!(!queue.make_way(&stop, move || canceller.cancel()), "l'attente s'arrête à l'annulation");
        gate_tx.send(()).unwrap();
    }

    #[test]
    fn an_operation_that_panics_leaves_the_list() {
        let queue = OpQueue::new("panique", String::new());
        queue.submit(op(1, 1, false, false), || panic!("tâche en échec"));
        assert!(queue.cancel_and_wait(Duration::from_secs(10)));
    }
}
