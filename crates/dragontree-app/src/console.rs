//! Messages de la ligne de commande dans la console qui a lancé DragonTree (P4).
//!
//! L'exécutable release est une application graphique (`windows_subsystem = "windows"`, `main.rs`) : Windows ne lui
//! donne ni console ni flux standard. Lancé depuis cmd ou PowerShell sans redirection, ce qu'il écrit (aide de
//! `--help`, option inconnue) n'irait nulle part. Avant d'écrire ces messages, [`attach_parent_console`] le rattache à
//! la console de son parent (`AttachConsole(ATTACH_PARENT_PROCESS)`), seulement si la sortie d'erreur n'est pas déjà
//! utilisable : un tube (Git Bash dans mintty), un fichier ou `NUL` restent tels quels.
//!
//! Constaté sous Windows 11 (voir la feuille de route, P4) :
//! - `AttachConsole` peut remplacer par la console un flux déjà redirigé (`dragontree --help > aide.txt` depuis cmd :
//!   la sortie standard perdait son fichier). Les flux utilisables avant l'attache sont donc remis ensuite
//!   (`SetStdHandle`).
//! - Il peut aussi laisser un flux inutilisable : Git Bash dans une console transmet à l'application graphique un
//!   handle de sortie d'erreur qui n'est pas valide chez elle, et `AttachConsole` ne le remplace pas (flux transmis avec
//!   `STARTF_USESTDHANDLES`). Un flux de sortie encore inutilisable après l'attache est ouvert sur `CONOUT$` (un seul
//!   handle, partagé si les deux sorties en ont besoin).
//! - La bibliothèque standard relit `GetStdHandle` à chaque écriture et écrit dans une console en UTF-16 : `eprintln!`
//!   affiche ensuite le texte, accents compris.
//!
//! Limites :
//! - cmd et PowerShell n'attendent pas la fin d'une application graphique lancée directement : leur invite revient
//!   aussitôt et le texte s'affiche après elle (Entrée redonne une invite propre). `start /wait`, un script `.cmd` ou
//!   `Start-Process -NoNewWindow -Wait` attendent la fin et affichent le texte avant l'invite.
//! - Réservé aux messages écrits juste avant de quitter : une application rattachée à une console en reçoit aussi les
//!   Ctrl+C.

/// État d'un flux standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    /// Pas de handle utilisable (nul, invalide) : application graphique lancée sans redirection.
    Missing,
    /// Handle utilisable : console, tube, fichier ou périphérique (`NUL`).
    Usable,
}

/// Ce qu'il faut faire avant d'écrire sur la sortie d'erreur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    /// Se rattacher à la console du processus parent.
    pub attach: bool,
    /// Flux (entrée, sortie, erreur) à remettre après l'attache, tels qu'ils étaient avant.
    pub restore: [bool; 3],
}

/// Décision selon l'état des flux (entrée, sortie, erreur) au moment d'écrire : se rattacher seulement si la sortie
/// d'erreur, celle des messages, manque ; remettre ensuite les flux qui étaient utilisables (redirections), que
/// `AttachConsole` peut remplacer par la console.
pub fn plan(streams: [Stream; 3]) -> Plan {
    let attach = streams[2] == Stream::Missing;
    Plan { attach, restore: streams.map(|s| attach && s == Stream::Usable) }
}

/// Résultat de [`attach_parent_console`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach {
    /// La sortie d'erreur était déjà utilisable : rien n'a changé.
    NotNeeded,
    /// Rattaché à la console du processus parent.
    Attached,
    /// Aucune console à rejoindre (lancement depuis l'Explorateur, parent sans console) : le message est perdu.
    NoConsole,
}

/// À appeler juste avant d'écrire un message de la ligne de commande sur la sortie d'erreur, puis de quitter : voir la
/// doc du module.
pub fn attach_parent_console() -> Attach {
    sys::attach_parent_console()
}

#[cfg(windows)]
mod sys {
    use windows_sys::Win32::Foundation::{
        GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR, SetLastError,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_UNKNOWN, GetFileType, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };

    use super::{Attach, Stream};

    /// Flux standard, dans l'ordre de [`super::plan`].
    const STREAMS: [STD_HANDLE; 3] = [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE];

    /// État d'un handle : manquant s'il est nul ou si Windows ne le reconnaît pas (`GetFileType` échoue).
    pub fn classify(handle: HANDLE) -> Stream {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Stream::Missing;
        }
        // SAFETY: `GetFileType` ne fait que lire le type du handle ; sur un handle invalide, elle échoue
        // (`FILE_TYPE_UNKNOWN` et une erreur), sans autre effet.
        let failed = unsafe {
            SetLastError(NO_ERROR);
            GetFileType(handle) == FILE_TYPE_UNKNOWN && GetLastError() != NO_ERROR
        };
        if failed { Stream::Missing } else { Stream::Usable }
    }

    fn std_handle(id: STD_HANDLE) -> HANDLE {
        // SAFETY: lecture d'un handle standard du processus.
        unsafe { GetStdHandle(id) }
    }

    pub fn attach_parent_console() -> Attach {
        let before = STREAMS.map(std_handle);
        let plan = super::plan(before.map(classify));
        if !plan.attach {
            return Attach::NotNeeded;
        }
        // SAFETY: appel sans pointeur ; échoue si le parent n'a pas de console (ou si ce processus en a déjà une).
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } == 0 {
            return Attach::NoConsole;
        }
        for ((id, handle), restore) in STREAMS.into_iter().zip(before).zip(plan.restore) {
            if restore {
                // SAFETY: `handle` est le handle standard d'avant l'attache, toujours ouvert.
                unsafe { SetStdHandle(id, handle) };
            }
        }
        // Un seul `CONOUT$`, ouvert à la première sortie qui en a besoin et partagé par les deux.
        let mut console = None;
        for id in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            if classify(std_handle(id)) == Stream::Missing
                && let Some(handle) = *console.get_or_insert_with(console_output)
            {
                // SAFETY: handle de console ouvert ci-dessus, jamais fermé (le processus quitte après le message).
                unsafe { SetStdHandle(id, handle) };
            }
        }
        Attach::Attached
    }

    /// Tampon d'écran de la console rattachée (`CONOUT$`).
    fn console_output() -> Option<HANDLE> {
        let name: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
        // SAFETY: nom terminé par un zéro ; pas d'attributs de sécurité ni de modèle.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (classify(handle) == Stream::Usable).then_some(handle)
    }
}

#[cfg(not(windows))]
mod sys {
    use super::Attach;

    pub fn attach_parent_console() -> Attach {
        Attach::NotNeeded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Stream::{Missing, Usable};
    #[cfg(windows)]
    use std::time::{Duration, Instant};

    #[test]
    fn only_a_missing_error_output_attaches_and_redirections_are_kept() {
        // cmd ou PowerShell, sans redirection : aucun flux.
        assert_eq!(plan([Missing; 3]), Plan { attach: true, restore: [false; 3] });
        // Git Bash (tubes) ou console déjà là : rien à faire.
        assert_eq!(plan([Usable; 3]), Plan { attach: false, restore: [false; 3] });
        // `dragontree --help > aide.txt` depuis cmd : la sortie standard garde son fichier.
        assert_eq!(plan([Missing, Usable, Missing]), Plan { attach: true, restore: [false, true, false] });
        assert_eq!(plan([Usable, Usable, Missing]), Plan { attach: true, restore: [true, true, false] });
        // `dragontree --help 2> erreurs.txt` : le message va dans le fichier.
        assert_eq!(plan([Missing, Missing, Usable]), Plan { attach: false, restore: [false; 3] });
    }

    #[cfg(windows)]
    #[test]
    fn handles_are_classified_by_what_windows_knows_of_them() {
        use std::os::windows::io::AsRawHandle;

        use sys::classify;
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

        assert_eq!(classify(std::ptr::null_mut()), Missing);
        assert_eq!(classify(INVALID_HANDLE_VALUE), Missing);
        let file = std::fs::File::open(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(classify(file.as_raw_handle()), Usable);
        let (reader, writer) = std::io::pipe().unwrap();
        assert_eq!(classify(reader.as_raw_handle()), Usable);
        assert_eq!(classify(writer.as_raw_handle()), Usable);
        // Valeur de handle jamais ouverte (multiple de 4, très haut) : Windows ne la reconnaît pas.
        assert_eq!(classify(0x7fff_fff0 as _), Missing);
    }

    /// Rôle du processus qui exécute le test suivant : absent (test lancé par cargo), `relais` ou `enfant`.
    #[cfg(windows)]
    const ROLE: &str = "DRAGONTREE_TEST_CONSOLE_ROLE";
    /// Façon de lancer l'enfant : `cmd`, `conout` ou `sans-console` (voir `console_relay`).
    #[cfg(windows)]
    const SCENARIO: &str = "DRAGONTREE_TEST_CONSOLE_SCENARIO";
    #[cfg(windows)]
    const SELF_TEST: &str = "console::tests::without_a_console_the_call_does_not_panic_and_keeps_the_redirected_output";
    /// Arguments qui relancent l'exécutable de test sur ce seul test.
    #[cfg(windows)]
    const SELF_ARGS: [&str; 4] = ["--exact", SELF_TEST, "--nocapture", "--test-threads=1"];
    /// Attente maximale de l'enfant par le relais ; le test attend le relais deux fois plus longtemps, pour recevoir son
    /// diagnostic. Au-delà, le processus attendu est tué et le test échoue au lieu de bloquer la suite.
    #[cfg(windows)]
    const LIMIT: Duration = Duration::from_secs(30);
    /// Valeur de handle jamais ouverte, que Windows ne reconnaît pas (voir le test précédent).
    #[cfg(windows)]
    const BOGUS: usize = 0x7fff_fff0;

    /// L'appel dans un processus sans console ni sortie d'erreur utilisable, comme l'exécutable release lancé depuis
    /// cmd ou depuis Git Bash dans une console. Le test se relance lui-même sur trois niveaux :
    /// - le relais, lancé avec une console cachée (`CREATE_NO_WINDOW`) ou sans console (`DETACHED_PROCESS`), sorties
    ///   standard et d'erreur dans des tubes vers ce test ;
    /// - l'enfant, lancé par le relais selon le scénario (voir `console_relay`), sans console ;
    /// - l'enfant appelle `attach_parent_console`, vérifie le résultat sans paniquer (voir `console_child`), puis écrit
    ///   son compte rendu sur sa sortie standard, le tube du relais.
    ///
    /// Vu rouge sans la remise des flux (`Plan::restore` toujours faux) : dans le scénario `cmd`, sortie standard
    /// remplacée par la console, compte rendu perdu ; avec un `CONOUT$` ouvert par sortie : scénario `conout` en échec.
    /// Chaque attente est bornée (`LIMIT`) ; en cas d'échec, le message reprend les sorties du relais et les paniques de
    /// l'enfant (écrites sur la sortie d'erreur qu'il a reçue du relais).
    #[cfg(windows)]
    #[test]
    fn without_a_console_the_call_does_not_panic_and_keeps_the_redirected_output() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};

        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};

        match std::env::var(ROLE).as_deref() {
            Ok("enfant") => return console_child(),
            Ok("relais") => return console_relay(),
            _ => {}
        }
        for (flags, scenario) in
            [(CREATE_NO_WINDOW, "cmd"), (CREATE_NO_WINDOW, "conout"), (DETACHED_PROCESS, "sans-console")]
        {
            let mut relay = Command::new(std::env::current_exe().unwrap())
                .args(SELF_ARGS)
                .env(ROLE, "relais")
                .env(SCENARIO, scenario)
                .creation_flags(flags)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let output = Drain::start(relay.stdout.take().unwrap());
            let errors = Drain::start(relay.stderr.take().unwrap());
            let status = wait_at_most(&mut relay, 2 * LIMIT);
            // Les tubes se ferment quand le relais et l'enfant ont fini ; un enfant qui survit au relais tué peut les
            // garder ouverts : on n'attend pas plus.
            let deadline = Instant::now() + Duration::from_secs(5);
            while !(output.done() && errors.done()) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let report = output.text();
            let context = format!(
                "{scenario} : relais {}\n--- sortie du relais ---\n{report}\n--- erreurs du relais et de l'enfant ---\n{}",
                status.map_or_else(|| format!("tué après {:?}", 2 * LIMIT), |s| s.to_string()),
                errors.text()
            );
            assert!(status.is_some_and(|s| s.success()), "{context}");
            assert!(report.contains(&format!("enfant ({scenario}) : réussi")), "{context}");
        }
    }

    /// Relais : lance l'enfant selon le scénario et attend sa fin au plus `LIMIT` (au-delà, il le tue et échoue).
    /// - `cmd` : comme cmd lance un programme (voir `spawn_like_cmd`), sur la console du relais ;
    /// - `conout` : par la bibliothèque standard, qui transmet les flux avec `STARTF_USESTDHANDLES` comme Git Bash, sur
    ///   la console du relais ;
    /// - `sans-console` : par la bibliothèque standard, sans console (`DETACHED_PROCESS`).
    ///
    /// Avec une console, l'enfant la quitte (`FreeConsole`) pour être sans console comme une application graphique.
    #[cfg(windows)]
    fn console_relay() {
        use std::os::windows::process::CommandExt;

        use windows_sys::Win32::System::Threading::DETACHED_PROCESS;

        let exe = std::env::current_exe().unwrap();
        let scenario = std::env::var(SCENARIO).unwrap();
        if scenario == "cmd" {
            return spawn_like_cmd(&exe);
        }
        let mut child = std::process::Command::new(&exe);
        child.args(SELF_ARGS).env(ROLE, "enfant");
        if scenario == "sans-console" {
            child.creation_flags(DETACHED_PROCESS);
        }
        match wait_at_most(&mut child.spawn().unwrap(), LIMIT) {
            Some(status) => assert!(status.success(), "l'enfant a échoué : {status}"),
            None => panic!("l'enfant ne s'est pas terminé en {LIMIT:?} : tué"),
        }
    }

    /// Lance l'enfant comme cmd lance un programme : `CreateProcessW` sans `STARTF_USESTDHANDLES`, handles hérités.
    /// Attente bornée par `LIMIT` : au-delà, l'enfant est tué (`TerminateProcess`) et le relais échoue.
    #[cfg(windows)]
    fn spawn_like_cmd(exe: &std::path::Path) {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess,
            WaitForSingleObject,
        };

        // SAFETY: le relais n'exécute que ce test (`--test-threads=1`), aucun autre thread ne lit l'environnement.
        unsafe { std::env::set_var(ROLE, "enfant") };
        let line = format!("\"{}\" {}", exe.display(), SELF_ARGS.join(" "));
        let mut line: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        // SAFETY: structures initialisées à zéro puis remplies par l'API ; ligne de commande modifiable, terminée par
        // un zéro ; handles du processus fermés après l'attente.
        let (waited, code) = unsafe {
            let mut startup: STARTUPINFOW = std::mem::zeroed();
            startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            let mut info: PROCESS_INFORMATION = std::mem::zeroed();
            let created = CreateProcessW(
                std::ptr::null(),
                line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut info,
            );
            assert_ne!(created, 0, "{}", std::io::Error::last_os_error());
            let waited = WaitForSingleObject(info.hProcess, LIMIT.as_millis() as u32);
            if waited != WAIT_OBJECT_0 {
                TerminateProcess(info.hProcess, 1);
            }
            let mut code = 1u32;
            GetExitCodeProcess(info.hProcess, &mut code);
            CloseHandle(info.hThread);
            CloseHandle(info.hProcess);
            (waited, code)
        };
        assert_eq!(waited, WAIT_OBJECT_0, "l'enfant ne s'est pas terminé en {LIMIT:?} : tué");
        assert_eq!(code, 0, "l'enfant a échoué (code {code})");
    }

    /// Enfant : sans console (`FreeConsole`), sortie d'erreur inutilisable comme dans une application graphique, puis
    /// `attach_parent_console` et ses vérifications selon le scénario :
    /// - `cmd`, `sans-console` : sortie standard héritée (le tube du relais), sortie d'erreur nulle. Attendu : `Attached`
    ///   avec la sortie d'erreur sur la console et la sortie standard gardée, ou `NoConsole`.
    /// - `conout` : sorties standard et d'erreur sur un handle jamais ouvert (`BOGUS`), qu'`AttachConsole` ne remplace pas
    ///   (flux transmis avec `STARTF_USESTDHANDLES`). Attendu : `Attached`, les deux sorties sur un même handle
    ///   `CONOUT$` utilisable. Le tube est remis en sortie standard avant les vérifications, pour le compte rendu.
    ///
    /// Une panique est écrite sur la sortie d'erreur reçue du relais (un tube vers le test) : celle du processus n'est
    /// plus utilisable, ou va dans la console cachée.
    #[cfg(windows)]
    fn console_child() {
        use std::io::Write;
        use std::os::windows::io::FromRawHandle;

        use sys::classify;
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::System::Console::{
            FreeConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
        };

        let scenario = std::env::var(SCENARIO).unwrap();
        // SAFETY: lecture des handles standard de ce processus, qui n'exécute que ce test.
        let (output, received_errors) = unsafe {
            FreeConsole();
            (GetStdHandle(STD_OUTPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE) as usize)
        };
        let label = scenario.clone();
        std::panic::set_hook(Box::new(move |info| {
            // SAFETY: handle reçu du relais, jamais fermé par ce processus (`ManuallyDrop`).
            let mut errors =
                std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_handle(received_errors as HANDLE) });
            let _ = writeln!(errors, "enfant ({label}) : {info}");
        }));
        assert_eq!(classify(output), Usable, "tube hérité du relais");
        let conout = scenario == "conout";
        let unusable = if conout { BOGUS as HANDLE } else { std::ptr::null_mut() };
        // SAFETY: remplacement des handles standard de ce processus ; `output` reste ouvert.
        let (attach, after_output, after_errors) = unsafe {
            SetStdHandle(STD_ERROR_HANDLE, unusable);
            if conout {
                SetStdHandle(STD_OUTPUT_HANDLE, unusable);
            }
            let attach = attach_parent_console();
            let after = (GetStdHandle(STD_OUTPUT_HANDLE), GetStdHandle(STD_ERROR_HANDLE));
            if conout {
                SetStdHandle(STD_OUTPUT_HANDLE, output);
            }
            (attach, after.0, after.1)
        };
        let expected = if scenario == "sans-console" { Attach::NoConsole } else { Attach::Attached };
        assert_eq!(attach, expected);
        if conout {
            assert_eq!(classify(after_errors), Usable, "sortie d'erreur : CONOUT$");
            assert_ne!(after_errors, unusable, "sortie d'erreur laissée sur le handle inutilisable");
            assert_eq!(after_output, after_errors, "un seul handle CONOUT$ pour les deux sorties");
        } else {
            if attach == Attach::Attached {
                assert_eq!(classify(after_errors), Usable, "sortie d'erreur : la console");
            }
            assert_eq!(after_output, output, "la sortie standard a perdu son tube");
        }
        println!("enfant ({scenario}) : réussi");
    }

    /// Attend la fin de `child` au plus `limit` ; au-delà, le tue et renvoie `None`.
    #[cfg(windows)]
    fn wait_at_most(child: &mut std::process::Child, limit: Duration) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return Some(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Lecture d'un tube dans un fil à part : le texte reçu reste lisible même si le tube ne se ferme jamais.
    #[cfg(windows)]
    struct Drain {
        received: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
        reader: std::thread::JoinHandle<()>,
    }

    #[cfg(windows)]
    impl Drain {
        fn start(mut pipe: impl std::io::Read + Send + 'static) -> Self {
            let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
            let buffer = received.clone();
            let reader = std::thread::spawn(move || {
                let mut chunk = [0u8; 4096];
                while let Ok(n @ 1..) = pipe.read(&mut chunk) {
                    buffer.lock().unwrap().extend_from_slice(&chunk[..n]);
                }
            });
            Self { received, reader }
        }

        fn done(&self) -> bool {
            self.reader.is_finished()
        }

        fn text(&self) -> String {
            String::from_utf8_lossy(&self.received.lock().unwrap()).into_owned()
        }
    }
}
