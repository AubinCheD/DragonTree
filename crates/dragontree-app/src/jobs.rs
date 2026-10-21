//! Travail en arrière-plan : aucune commande git ne tourne sur le thread de l'interface.
//!
//! - [`Jobs::spawn`] : un thread par tâche de lecture (chargements parallèles).
//! - [`SerialWorker`] : file d'exécution séquentielle, la base des files d'opérations des dépôts (`op_queue` : une
//!   file d'écriture par dépôt, partagée par ses onglets, et une file à part pour le réseau silencieux).

use std::sync::mpsc::{self, Receiver, Sender};

use dragontree_git::GitError;
use dragontree_i18n::tr;

use crate::app::AppEvent;

/// Envoie des événements à l'interface et la réveille.
#[derive(Clone)]
pub struct EventSender {
    tx: Sender<AppEvent>,
    ctx: egui::Context,
}

impl EventSender {
    pub fn send(&self, ev: AppEvent) {
        if self.tx.send(ev).is_ok() {
            self.ctx.request_repaint();
        }
    }
}

pub struct Jobs {
    sender: EventSender,
}

impl Jobs {
    pub fn new(ctx: egui::Context) -> (Jobs, Receiver<AppEvent>) {
        let (tx, rx) = mpsc::channel();
        (Jobs { sender: EventSender { tx, ctx } }, rx)
    }

    pub fn sender(&self) -> EventSender {
        self.sender.clone()
    }

    /// Lance `f` sur un thread ; son éventuel événement est envoyé à l'interface.
    pub fn spawn(&self, name: &str, f: impl FnOnce() -> Option<AppEvent> + Send + 'static) {
        let sender = self.sender.clone();
        let _ = std::thread::Builder::new().name(format!("dragontree-{name}")).spawn(move || {
            if let Some(ev) = f() {
                sender.send(ev);
            }
        });
    }
}

/// Exécute `f` ; une panique devient une erreur au lieu de tuer le thread appelant.
pub fn guarded<T>(f: impl FnOnce() -> Result<T, GitError>) -> Result<T, GitError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .unwrap_or_else(|payload| Err(GitError::Internal(panic_message(payload.as_ref()))))
}

pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    match (payload.downcast_ref::<&str>(), payload.downcast_ref::<String>()) {
        (Some(s), _) => s.to_string(),
        (_, Some(s)) => s.clone(),
        _ => tr!("panique sans message").into(),
    }
}

type Task = Box<dyn FnOnce() + Send>;

/// File d'exécution séquentielle (un thread). Le thread s'arrête quand la file est abandonnée.
pub struct SerialWorker {
    tx: Sender<Task>,
}

impl SerialWorker {
    pub fn new(name: &str) -> SerialWorker {
        let (tx, rx) = mpsc::channel::<Task>();
        let _ = std::thread::Builder::new().name(format!("dragontree-serial-{name}")).spawn(move || {
            while let Ok(task) = rx.recv() {
                // Une tâche qui panique ne doit pas arrêter la file (les suivantes seraient perdues).
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
            }
        });
        SerialWorker { tx }
    }

    pub fn submit(&self, f: impl FnOnce() + Send + 'static) {
        let _ = self.tx.send(Box::new(f));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    #[test]
    fn serial_worker_preserves_order() {
        let worker = SerialWorker::new("test");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (done_tx, done_rx) = mpsc::channel();
        for i in 0..100 {
            let seen = seen.clone();
            worker.submit(move || {
                if i % 7 == 0 {
                    std::thread::sleep(Duration::from_millis(1));
                }
                seen.lock().unwrap().push(i);
            });
        }
        worker.submit(move || done_tx.send(()).unwrap());
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(*seen.lock().unwrap(), (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn serial_worker_survives_a_panicking_task() {
        let worker = SerialWorker::new("panique");
        let (tx, rx) = mpsc::channel();
        worker.submit(|| panic!("tâche en échec"));
        worker.submit(move || tx.send(()).unwrap());
        rx.recv_timeout(Duration::from_secs(10)).expect("la file continue après une panique");
    }

    #[test]
    fn guarded_turns_a_panic_into_an_error() {
        let res: Result<(), GitError> = guarded(|| panic!("boum"));
        match res {
            Err(GitError::Internal(msg)) => assert!(msg.contains("boum"), "{msg}"),
            other => panic!("erreur attendue, obtenu {other:?}"),
        }
        assert_eq!(guarded(|| Ok::<_, GitError>(3)), Ok(3));
    }
}
