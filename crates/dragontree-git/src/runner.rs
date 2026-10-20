use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

use crate::error::GitError;

/// Jeton d'annulation partagé entre l'interface et une commande en cours.
#[derive(Clone, Default, Debug)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Résultat d'une commande git terminée avec un code accepté.
#[derive(Debug, Clone)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub code: i32,
}

impl Output {
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }
}

/// L'exécutable git utilisé par DragonTree.
#[derive(Debug, Clone)]
pub struct Git {
    exe: PathBuf,
}

/// Dossiers d'installation de Git pour Windows déduits de l'exécutable (`<racine>\cmd\git.exe`,
/// `<racine>\bin\git.exe` ou `<racine>\mingw64\bin\git.exe` : on remonte de quelques dossiers, du plus proche
/// au plus lointain). Un `git` seul (trouvé par le PATH) est d'abord résolu en `git.exe` dans `path_var`.
/// `exists` isole le disque pour les tests.
pub fn git_install_roots(git_exe: &Path, path_var: Option<&OsStr>, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let exe = if git_exe.parent().is_some_and(|p| !p.as_os_str().is_empty()) {
        Some(git_exe.to_path_buf())
    } else {
        path_var.and_then(|paths| std::env::split_paths(paths).map(|dir| dir.join("git.exe")).find(|p| exists(p)))
    };
    exe.map(|exe| exe.ancestors().skip(1).take(4).map(Path::to_path_buf).collect()).unwrap_or_default()
}

impl Git {
    /// Cherche `git` dans le PATH, puis dans les emplacements d'installation usuels.
    pub fn discover() -> Result<Git, GitError> {
        let mut candidates = vec![PathBuf::from("git")];
        if cfg!(windows) {
            for var in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(base) = std::env::var_os(var) {
                    candidates.push(PathBuf::from(base).join("Git").join("cmd").join("git.exe"));
                }
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                candidates.push(PathBuf::from(local).join("Programs").join("Git").join("cmd").join("git.exe"));
            }
        }
        candidates.into_iter().map(|exe| Git { exe }).find(|git| git.version().is_ok()).ok_or(GitError::NotFound)
    }

    pub fn with_exe(exe: PathBuf) -> Git {
        Git { exe }
    }

    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// Sortie de `git --version`, par exemple `git version 2.44.0.windows.1`.
    pub fn version(&self) -> Result<String, GitError> {
        let out = self.cmd(&std::env::temp_dir()).arg("--version").run()?;
        Ok(out.stdout_str().trim().to_string())
    }

    /// Prépare une commande exécutée dans `cwd`.
    pub fn cmd(&self, cwd: &Path) -> GitCommand {
        GitCommand {
            exe: self.exe.clone(),
            cwd: cwd.to_path_buf(),
            args: Vec::new(),
            env: Vec::new(),
            stdin: None,
            cancel: None,
            ok_codes: vec![0],
        }
    }
}

/// Commande git en préparation (constructeur).
#[derive(Debug, Clone)]
pub struct GitCommand {
    exe: PathBuf,
    cwd: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    stdin: Option<Vec<u8>>,
    cancel: Option<CancelToken>,
    ok_codes: Vec<i32>,
}

impl GitCommand {
    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args.extend(it.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    pub fn env(mut self, k: impl AsRef<OsStr>, v: impl AsRef<OsStr>) -> Self {
        self.env.push((k.as_ref().to_os_string(), v.as_ref().to_os_string()));
        self
    }

    pub fn envs(mut self, kv: &[(String, String)]) -> Self {
        for (k, v) in kv {
            self.env.push((k.into(), v.into()));
        }
        self
    }

    pub fn stdin(mut self, data: Vec<u8>) -> Self {
        self.stdin = Some(data);
        self
    }

    pub fn cancel(mut self, token: CancelToken) -> Self {
        self.cancel = Some(token);
        self
    }

    /// Commande de lecture : ne prend pas de verrou optionnel (index rafraîchi en mémoire seulement).
    pub fn read_only(self) -> Self {
        self.env("GIT_OPTIONAL_LOCKS", "0")
    }

    /// Interdit toute fenêtre d'authentification (opérations en arrière-plan).
    pub fn non_interactive(self) -> Self {
        self.env("GCM_INTERACTIVE", "never")
    }

    /// Codes de sortie acceptés en plus de 0 (ex. `diff --no-index` renvoie 1).
    pub fn allow_codes(mut self, codes: &[i32]) -> Self {
        for c in codes {
            if !self.ok_codes.contains(c) {
                self.ok_codes.push(*c);
            }
        }
        self
    }

    /// Ligne de commande lisible pour le journal (`git log --all …`).
    pub fn display(&self) -> String {
        let mut s = String::from("git");
        for a in &self.args {
            let a = a.to_string_lossy();
            s.push(' ');
            if a.is_empty() || a.contains(char::is_whitespace) || a.contains('"') {
                s.push('"');
                s.push_str(&a.replace('"', "\\\""));
                s.push('"');
            } else {
                s.push_str(&a);
            }
        }
        s
    }

    pub fn run(self) -> Result<Output, GitError> {
        self.execute(None, None)
    }

    /// Exécute en transmettant la sortie au fil de l'eau ; les rappels sont appelés sur le
    /// thread appelant. Le stderr est découpé en lignes sur `\r` et `\n` (progression).
    pub fn run_streaming(
        self,
        on_stdout: &mut dyn FnMut(&[u8]),
        on_stderr_line: &mut dyn FnMut(&str),
    ) -> Result<Output, GitError> {
        self.execute(Some(on_stdout), Some(on_stderr_line))
    }

    fn execute(
        self,
        on_stdout: Option<StdoutSink<'_>>,
        on_stderr_line: Option<StderrSink<'_>>,
    ) -> Result<Output, GitError> {
        let Some(observer) = OBSERVER.get() else {
            return self.execute_inner(on_stdout, on_stderr_line);
        };
        let read_only = self.env.iter().any(|(k, v)| k == "GIT_OPTIONAL_LOCKS" && v == "0");
        let (cwd, display, start) = (self.cwd.clone(), self.display(), Instant::now());
        let result = self.execute_inner(on_stdout, on_stderr_line);
        let (code, stderr, cancelled) = match &result {
            Ok(o) => (Some(o.code), o.stderr.clone(), false),
            Err(GitError::Failed { code, stderr, .. }) => (Some(*code), stderr.clone(), false),
            Err(GitError::Cancelled) => (None, String::new(), true),
            Err(e) => (None, e.to_string(), false),
        };
        observer(&CommandRecord { cwd, display, read_only, code, duration: start.elapsed(), stderr, cancelled });
        result
    }

    fn execute_inner(
        self,
        on_stdout: Option<StdoutSink<'_>>,
        on_stderr_line: Option<StderrSink<'_>>,
    ) -> Result<Output, GitError> {
        let display = self.display();
        let mut command = Command::new(&self.exe);
        command
            .args(&self.args)
            .current_dir(&self.cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE");
        for (k, v) in &self.env {
            command.env(k, v);
        }
        let spawn_error = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::NotFound => GitError::NotFound,
            _ => GitError::Spawn(e.to_string()),
        };
        let run = Supervised { stdin: self.stdin, cancel: self.cancel.as_ref(), detach_after_exit: false };
        let done = run.run(command, spawn_error, on_stdout, on_stderr_line)?;
        let stderr = String::from_utf8_lossy(&done.stderr).into_owned();
        if self.ok_codes.contains(&done.code) {
            Ok(Output { stdout: done.stdout, stderr, code: done.code })
        } else {
            Err(GitError::from_failure(display, done.code, stderr))
        }
    }
}

/// Processus terminé : code de sortie (quel qu'il soit) et sorties qui n'ont pas été transmises au fil de l'eau.
pub(crate) struct Finished {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Exécution surveillée d'un processus : sans fenêtre, rattaché à un Job Object (l'annulation tue aussi ses
/// sous-processus), entrée standard fournie ou vide, sorties lues jusqu'à la fin.
pub(crate) struct Supervised<'a> {
    pub stdin: Option<Vec<u8>>,
    pub cancel: Option<&'a CancelToken>,
    /// Ne pas attendre la fermeture des tubes de sortie une fois le processus terminé et silencieux : un programme
    /// qu'il a lancé en arrière-plan (`start` de cmd) les garde ouverts tant qu'il tourne.
    pub detach_after_exit: bool,
}

impl Supervised<'_> {
    /// Lance `command` et attend sa fin (ou l'annulation) ; `spawn_error` traduit l'échec du lancement.
    pub(crate) fn run(
        self,
        mut command: Command,
        spawn_error: impl FnOnce(std::io::Error) -> GitError,
        mut on_stdout: Option<StdoutSink<'_>>,
        mut on_stderr_line: Option<StderrSink<'_>>,
    ) -> Result<Finished, GitError> {
        command
            .stdin(if self.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = command.spawn().map_err(spawn_error)?;
        // Rattacher le processus à un « job » permet de tuer aussi ses sous-processus à l'annulation
        // (git-remote-https, sh…), qui sinon garderaient les tubes ouverts.
        let job = job::JobGuard::attach(&child);

        if let Some(data) = self.stdin
            && let Some(mut sin) = child.stdin.take()
        {
            std::thread::spawn(move || {
                let _ = sin.write_all(&data);
            });
        }

        let (tx, rx) = mpsc::channel::<Chunk>();
        spawn_reader(child.stdout.take(), tx.clone(), Chunk::Out, Chunk::OutEof);
        spawn_reader(child.stderr.take(), tx, Chunk::Err, Chunk::ErrEof);

        let is_cancelled = || self.cancel.is_some_and(CancelToken::is_cancelled);
        let kill = |child: &mut std::process::Child| {
            match &job {
                Some(job) => {
                    job.terminate();
                    // Attendre la fin réelle des sous-processus : sinon ils peuvent encore tenir
                    // des fichiers ouverts (nettoyage d'un clone annulé).
                    job.wait_empty(Duration::from_secs(3));
                }
                None => {
                    let _ = child.kill();
                }
            }
            let _ = child.wait();
        };

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut line_buf = Vec::new();
        let (mut out_eof, mut err_eof) = (false, false);
        let mut last_chunk = Instant::now();
        while !(out_eof && err_eof) {
            if is_cancelled() {
                kill(&mut child);
                return Err(GitError::Cancelled);
            }
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(Chunk::Out(bytes)) => {
                    last_chunk = Instant::now();
                    match on_stdout.as_mut() {
                        Some(cb) => cb(&bytes),
                        None => stdout_buf.extend_from_slice(&bytes),
                    }
                }
                Ok(Chunk::Err(bytes)) => {
                    last_chunk = Instant::now();
                    stderr_buf.extend_from_slice(&bytes);
                    if stderr_buf.len() > MAX_STDERR {
                        let excess = stderr_buf.len() - MAX_STDERR;
                        stderr_buf.drain(..excess);
                    }
                    if let Some(cb) = on_stderr_line.as_mut() {
                        for &b in &bytes {
                            if b == b'\r' || b == b'\n' {
                                emit_line(&mut line_buf, cb);
                            } else {
                                line_buf.push(b);
                            }
                        }
                    }
                }
                Ok(Chunk::OutEof) => out_eof = true,
                Ok(Chunk::ErrEof) => err_eof = true,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if self.detach_after_exit
                        && last_chunk.elapsed() >= DETACH_QUIET
                        && matches!(child.try_wait(), Ok(Some(_)))
                    {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        if let Some(cb) = on_stderr_line.as_mut() {
            emit_line(&mut line_buf, cb);
        }

        let status = loop {
            if is_cancelled() {
                kill(&mut child);
                return Err(GitError::Cancelled);
            }
            match child.try_wait()? {
                Some(status) => break status,
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        Ok(Finished { code: status.code().unwrap_or(-1), stdout: stdout_buf, stderr: stderr_buf })
    }
}

/// Trace d'une commande git terminée (pour le journal de l'interface).
#[derive(Debug, Clone)]
pub struct CommandRecord {
    pub cwd: PathBuf,
    pub display: String,
    /// Commande de lecture (`GIT_OPTIONAL_LOCKS=0`).
    pub read_only: bool,
    /// Code de sortie ; `None` si git n'a pas pu être lancé ou a été annulé.
    pub code: Option<i32>,
    pub duration: Duration,
    pub stderr: String,
    pub cancelled: bool,
}

type Observer = Box<dyn Fn(&CommandRecord) + Send + Sync>;
static OBSERVER: OnceLock<Observer> = OnceLock::new();

/// Installe l'observateur appelé après chaque commande git (une seule fois par processus).
/// Renvoie `false` si un observateur était déjà installé.
pub fn set_command_observer(f: impl Fn(&CommandRecord) + Send + Sync + 'static) -> bool {
    OBSERVER.set(Box::new(f)).is_ok()
}

pub(crate) type StdoutSink<'a> = &'a mut dyn FnMut(&[u8]);
pub(crate) type StderrSink<'a> = &'a mut dyn FnMut(&str);

/// Taille maximale conservée du stderr (on garde la fin, où se trouvent les erreurs).
const MAX_STDERR: usize = 256 * 1024;

/// Silence, après la fin du processus, au-delà duquel `Supervised::detach_after_exit` cesse de lire ses tubes.
const DETACH_QUIET: Duration = Duration::from_millis(300);

enum Chunk {
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutEof,
    ErrEof,
}

fn spawn_reader<R: Read + Send + 'static>(
    pipe: Option<R>,
    tx: mpsc::Sender<Chunk>,
    wrap: fn(Vec<u8>) -> Chunk,
    eof: Chunk,
) {
    std::thread::spawn(move || {
        if let Some(mut pipe) = pipe {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(wrap(buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                }
            }
        }
        let _ = tx.send(eof);
    });
}

fn emit_line(line_buf: &mut Vec<u8>, cb: &mut &mut dyn FnMut(&str)) {
    if !line_buf.is_empty() {
        let line = String::from_utf8_lossy(line_buf);
        cb(&line);
        line_buf.clear();
    }
}

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JobObjectBasicAccountingInformation, QueryInformationJobObject, TerminateJobObject,
    };

    /// Job Windows contenant git et tous ses descendants.
    pub struct JobGuard(HANDLE);

    impl JobGuard {
        pub fn attach(child: &std::process::Child) -> Option<JobGuard> {
            // SAFETY: appels Win32 avec des handles valides ; le handle du job est fermé dans Drop.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                if AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                    CloseHandle(job);
                    return None;
                }
                Some(JobGuard(job))
            }
        }

        pub fn terminate(&self) {
            // SAFETY: self.0 est un handle de job valide tant que self existe.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }

        /// Attend que plus aucun processus du job ne soit actif (au plus `timeout`).
        pub fn wait_empty(&self, timeout: std::time::Duration) {
            let deadline = std::time::Instant::now() + timeout;
            loop {
                // SAFETY: structure POD remplie par l'API ; handle valide tant que self existe.
                let active = unsafe {
                    let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
                    let ok = QueryInformationJobObject(
                        self.0,
                        JobObjectBasicAccountingInformation,
                        (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                        std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                        std::ptr::null_mut(),
                    );
                    if ok == 0 { 0 } else { info.ActiveProcesses }
                };
                if active == 0 || std::time::Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }

    impl Drop for JobGuard {
        fn drop(&mut self) {
            // SAFETY: handle obtenu par CreateJobObjectW, fermé une seule fois.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    pub struct JobGuard;

    impl JobGuard {
        pub fn attach(_child: &std::process::Child) -> Option<JobGuard> {
            None
        }

        pub fn terminate(&self) {}

        pub fn wait_empty(&self, _timeout: std::time::Duration) {}
    }
}
