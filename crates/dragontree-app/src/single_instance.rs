//! Instance unique par profil (RV4b) : deux instances sur le même profil écriraient le même `settings.json`, la
//! dernière écriture l'emportant.
//!
//! - Au démarrage (`main.rs`, jamais dans `DragonTreeApp::new` : les tests d'interface créent autant d'applications
//!   qu'ils veulent), un mutex nommé `Local\DragonTree-<clé>` désigne la première instance du profil ; la clé est une
//!   empreinte stable du dossier du profil (`profile_key`) : `--profile <dossier>` donne une instance distincte.
//! - La première instance écoute sur un tube nommé `\\.\pipe\DragonTree-<clé>` (thread de fond). Une instance
//!   suivante lui envoie les dépôts de sa ligne de commande (dossiers en arguments et `--open`, en chemins absolus :
//!   son dossier courant n'est pas celui de la première), lui permet de passer au premier plan
//!   (`AllowSetForegroundWindow` : Windows refuse sinon qu'une application en arrière-plan prenne le focus), puis
//!   quitte. Les autres options de la seconde instance sont ignorées.
//! - La première instance ouvre ces dépôts (`AppAction::OpenRepo`), ou seulement passe au premier plan si aucun n'est
//!   donné.
//!
//! Le tube garde la sécurité par défaut de Windows : seuls l'utilisateur qui l'a créé, les administrateurs et le
//! système peuvent y écrire ; les clients distants sont refusés.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::settings::path_key;

/// Taille maximale d'un message (quelques chemins).
const MAX_MESSAGE: usize = 1024 * 1024;
/// Version du format des messages : une instance d'une autre version est ignorée.
const VERSION: u32 = 1;

/// Message d'une instance suivante à la première : les dépôts à ouvrir, en chemins absolus ; aucun : passer au
/// premier plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Forwarded {
    pub open: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    version: u32,
    open: Vec<String>,
}

impl Forwarded {
    /// Message transmis (JSON). Les arguments de la ligne de commande sont toujours de l'Unicode valide (`env::args`).
    pub fn encode(&self) -> Vec<u8> {
        let open = self.open.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        serde_json::to_vec(&Wire { version: VERSION, open }).unwrap_or_default()
    }

    /// Message reçu ; `None` s'il est illisible, trop gros ou d'une autre version.
    pub fn decode(bytes: &[u8]) -> Option<Forwarded> {
        if bytes.len() > MAX_MESSAGE {
            return None;
        }
        let wire: Wire = serde_json::from_slice(bytes).ok()?;
        (wire.version == VERSION).then(|| Forwarded { open: wire.open.into_iter().map(PathBuf::from).collect() })
    }
}

/// Clé d'un profil : empreinte FNV-1a sur 64 bits de son dossier normalisé (`path_key` : casse et séparateurs), en
/// hexadécimal. Stable d'une exécution et d'une version à l'autre, contrairement au hachage de la bibliothèque
/// standard : deux versions de DragonTree sur le même profil se reconnaissent.
pub fn profile_key(dir: &Path) -> String {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    format!("{:016x}", fnv1a(path_key(&dir).as_bytes()))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Mutex qui désigne la première instance du profil (session Windows courante).
pub fn mutex_name(key: &str) -> String {
    format!(r"Local\DragonTree-{key}")
}

/// Tube nommé où écoute la première instance.
pub fn pipe_name(key: &str) -> String {
    format!(r"\\.\pipe\DragonTree-{key}")
}

/// Issue du démarrage (`start`).
pub enum Startup {
    /// Première instance du profil : à garder en vie (mutex et écoute) jusqu'à la fin de l'application.
    Primary(PrimaryInstance),
    /// Les dépôts ont été transmis à l'instance déjà ouverte : cette instance doit quitter.
    Forwarded,
    /// Instance unique impossible (erreur de Windows, instance ouverte qui ne répond pas) : démarrer seul.
    Alone(io::Error),
}

/// Établit l'instance unique du profil `key`, ou transmet `message` à la première instance. Si la première ne répond
/// pas (elle se ferme, par exemple), la place est redemandée jusqu'à `timeout`.
pub fn start(key: &str, message: &Forwarded, timeout: Duration) -> Startup {
    let deadline = Instant::now() + timeout;
    loop {
        match sys::claim(&mutex_name(key)) {
            Ok(Some(mutex)) => {
                return PrimaryInstance::listen(key, mutex).map_or_else(Startup::Alone, Startup::Primary);
            }
            Ok(None) => match send(key, message, deadline.saturating_duration_since(Instant::now())) {
                Ok(()) => return Startup::Forwarded,
                Err(e) if Instant::now() >= deadline => return Startup::Alone(e),
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            },
            Err(e) => return Startup::Alone(e),
        }
    }
}

/// Envoie `message` à la première instance du profil `key` et lui permet de passer au premier plan. Attend au plus
/// `timeout` que son tube soit libre (un autre message en cours) ou créé (elle démarre).
pub fn send(key: &str, message: &Forwarded, timeout: Duration) -> io::Result<()> {
    sys::send(&pipe_name(key), &message.encode(), timeout)
}

/// Première instance du profil : tient le mutex et reçoit les messages des instances suivantes.
pub struct PrimaryInstance {
    // Détruits dans cet ordre : l'écoute s'arrête, puis le profil est libéré.
    _listener: Listener,
    _mutex: sys::Mutex,
    messages: Receiver<Forwarded>,
    ctx: Arc<OnceLock<egui::Context>>,
}

impl PrimaryInstance {
    fn listen(key: &str, mutex: sys::Mutex) -> io::Result<PrimaryInstance> {
        let (tx, messages) = mpsc::channel();
        let ctx: Arc<OnceLock<egui::Context>> = Arc::default();
        let wake = ctx.clone();
        let listener = Listener::start(key, move |mut message| {
            // Sur le thread d'écoute, pas sur celui de l'interface : seuls les dossiers existants sont gardés.
            message.open.retain(|p| p.is_dir());
            if tx.send(message).is_ok()
                && let Some(ctx) = wake.get()
            {
                ctx.request_repaint();
            }
        })?;
        Ok(PrimaryInstance { _listener: listener, _mutex: mutex, messages, ctx })
    }

    /// Interface à réveiller à l'arrivée d'un message ; ceux arrivés avant attendent `take_messages`.
    pub fn wake(&self, ctx: egui::Context) {
        let _ = self.ctx.set(ctx);
    }

    /// Messages arrivés depuis le dernier appel.
    pub fn take_messages(&self) -> Vec<Forwarded> {
        self.messages.try_iter().collect()
    }
}

/// Écoute du tube nommé d'un profil, sur un thread de fond : chaque message reçu est passé à `on_message`. S'arrête
/// quand la valeur est détruite.
pub struct Listener {
    stop: Arc<std::sync::atomic::AtomicBool>,
    pipe: String,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Listener {
    pub fn start(key: &str, on_message: impl FnMut(Forwarded) + Send + 'static) -> io::Result<Listener> {
        let pipe = pipe_name(key);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread = sys::listen(&pipe, stop.clone(), on_message)?;
        Ok(Listener { stop, pipe, thread: Some(thread) })
    }
}

impl Drop for Listener {
    /// Le thread attend un client : on le réveille par une connexion vide après avoir levé le drapeau d'arrêt, puis
    /// on attend qu'il ait fermé le tube (au plus 1 s) : le profil rouvert aussitôt pourra le recréer (le tube est créé
    /// avec `FILE_FLAG_FIRST_PIPE_INSTANCE`, refusé tant que l'ancien existe).
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = sys::send(&self.pipe, &[], Duration::from_millis(200));
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        if let Some(thread) = self.thread.take() {
            while !thread.is_finished() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    use windows_sys::Win32::Foundation::{
        ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE,
        INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::CreateMutexW;
    use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

    use super::{Forwarded, MAX_MESSAGE};

    /// Mutex nommé tenu par la première instance (fermé à la destruction).
    pub struct Mutex {
        _handle: OwnedHandle,
    }

    fn wide(s: &str) -> Vec<u16> {
        std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Crée le mutex `name` ; `None` s'il existait déjà (une autre instance le tient).
    pub fn claim(name: &str) -> io::Result<Option<Mutex>> {
        let name = wide(name);
        // SAFETY: nom terminé par un zéro ; le handle rendu est possédé par `OwnedHandle`, qui le ferme.
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let existed = GetLastError() == ERROR_ALREADY_EXISTS;
            let handle = OwnedHandle::from_raw_handle(handle);
            Ok((!existed).then_some(Mutex { _handle: handle }))
        }
    }

    /// Crée le tube `name` (première instance : un tube du même nom créé par un autre processus est refusé) et lance
    /// le thread qui l'écoute ; un seul client à la fois, les suivants attendent leur tour.
    pub fn listen(
        name: &str,
        stop: Arc<AtomicBool>,
        mut on_message: impl FnMut(Forwarded) + Send + 'static,
    ) -> io::Result<std::thread::JoinHandle<()>> {
        let name = wide(name);
        // SAFETY: nom terminé par un zéro ; sécurité par défaut (pointeur nul) ; le handle rendu est possédé par `File`.
        let pipe = unsafe {
            let handle = CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                0,
                64 * 1024,
                0,
                std::ptr::null(),
            );
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            File::from_raw_handle(handle)
        };
        let thread = std::thread::Builder::new().name("dragontree-instance".into()).spawn(move || {
            let handle = pipe.as_raw_handle() as HANDLE;
            loop {
                // SAFETY: handle du tube valide tant que `pipe` existe ; appel synchrone (pas d'OVERLAPPED).
                let connected = unsafe { ConnectNamedPipe(handle, std::ptr::null_mut()) != 0 }
                    // SAFETY: lit l'erreur de l'appel précédent, sur ce thread.
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                if connected {
                    let mut bytes = Vec::new();
                    let read = (&pipe).take(MAX_MESSAGE as u64 + 1).read_to_end(&mut bytes);
                    if let Some(message) = read.ok().and_then(|_| Forwarded::decode(&bytes)) {
                        on_message(message);
                    }
                } else {
                    std::thread::sleep(Duration::from_millis(50));
                }
                // SAFETY: handle du tube valide ; prépare le tube pour le client suivant.
                unsafe { DisconnectNamedPipe(handle) };
            }
        })?;
        Ok(thread)
    }

    /// Écrit `bytes` dans le tube `name` après avoir permis à son serveur de passer au premier plan ; réessaie tant que
    /// le tube est occupé ou pas encore créé, au plus `timeout`.
    pub fn send(name: &str, bytes: &[u8], timeout: Duration) -> io::Result<()> {
        let deadline = Instant::now() + timeout;
        let mut pipe = loop {
            match std::fs::OpenOptions::new().write(true).open(name) {
                Ok(pipe) => break pipe,
                Err(e)
                    if Instant::now() < deadline
                        && matches!(e.raw_os_error(), Some(c) if c == ERROR_PIPE_BUSY as i32 || c == ERROR_FILE_NOT_FOUND as i32) =>
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(e),
            }
        };
        let mut pid = 0u32;
        // SAFETY: handle du tube valide tant que `pipe` existe ; `pid` est rempli par l'API.
        if unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } != 0 {
            // SAFETY: simple autorisation donnée à un processus ; sans effet si celui-ci n'en a pas besoin.
            unsafe { AllowSetForegroundWindow(pid) };
        }
        pipe.write_all(bytes)?;
        // Attend que la première instance ait tout lu avant de fermer (puis de quitter).
        let _ = pipe.sync_all();
        Ok(())
    }
}

#[cfg(not(windows))]
mod sys {
    use std::io;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    use super::Forwarded;

    pub struct Mutex;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "instance unique : Windows seulement")
    }

    pub fn claim(_name: &str) -> io::Result<Option<Mutex>> {
        Err(unsupported())
    }

    pub fn listen(
        _: &str,
        _: Arc<AtomicBool>,
        _: impl FnMut(Forwarded) + Send + 'static,
    ) -> io::Result<std::thread::JoinHandle<()>> {
        Err(unsupported())
    }

    pub fn send(_name: &str, _bytes: &[u8], _timeout: Duration) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_round_trips_and_a_foreign_one_is_refused() {
        let message =
            Forwarded { open: vec![PathBuf::from(r"C:\Dépôts\contoso"), PathBuf::from(r"\\serveur\northwind")] };
        assert_eq!(Forwarded::decode(&message.encode()), Some(message));
        assert_eq!(Forwarded::decode(&Forwarded::default().encode()), Some(Forwarded::default()));
        assert_eq!(Forwarded::decode(b""), None, "connexion vide (arrêt de l'écoute)");
        assert_eq!(Forwarded::decode(b"pas du json"), None);
        assert_eq!(Forwarded::decode(br#"{"version":2,"open":[]}"#), None, "autre version");
        let huge = format!(r#"{{"version":1,"open":["{}"]}}"#, "a".repeat(MAX_MESSAGE));
        assert_eq!(Forwarded::decode(huge.as_bytes()), None, "trop gros");
    }

    #[test]
    fn the_profile_key_is_a_stable_hash_of_the_normalized_folder() {
        // Valeurs de référence de FNV-1a 64 bits : la clé ne change pas d'une version de DragonTree à l'autre.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        let key = profile_key(Path::new(r"C:\Users\Contoso\AppData\Roaming\DragonTree"));
        assert_eq!(key.len(), 16);
        assert_eq!(key, profile_key(Path::new("c:/users/contoso/appdata/roaming/dragontree/")), "casse, séparateurs");
        assert_ne!(key, profile_key(Path::new(r"C:\tests\profil")), "autre profil, autre instance");
        assert_eq!(mutex_name("0123"), r"Local\DragonTree-0123");
        assert_eq!(pipe_name("0123"), r"\\.\pipe\DragonTree-0123");
    }
}
