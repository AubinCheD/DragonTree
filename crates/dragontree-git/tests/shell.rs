//! Lignes de commande de l'utilisateur exécutées par `cmd.exe` (commandes personnalisées) : dossier, sortie, code de
//! sortie, environnement, ligne transmise telle quelle, annulation et programme lancé en arrière-plan par la ligne.
#![cfg(windows)]

use std::time::{Duration, Instant};

use dragontree_git::shell::{ansi_view_in, run_shell};
use dragontree_git::{CancelToken, GitError};

#[test]
fn the_line_runs_in_the_folder_and_returns_its_output_and_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let out =
        run_shell(dir.path(), "echo bonjour& echo erreur 1>&2& echo x> cree.txt& exit /b 3", &[], &CancelToken::new())
            .unwrap();
    assert_eq!(out.code, 3);
    assert!(out.output.contains("bonjour"), "{}", out.output);
    assert!(out.output.contains("erreur"), "la sortie d'erreur est gardée : {}", out.output);
    assert!(dir.path().join("cree.txt").exists(), "exécutée dans le dossier demandé");
    let ok = run_shell(dir.path(), "echo fini", &[], &CancelToken::new()).unwrap();
    assert_eq!(ok.code, 0);
}

#[test]
fn environment_values_are_passed_to_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let env = [("DT_TEST_VALEUR".to_string(), Some("a&b".to_string()))];
    let out = run_shell(dir.path(), "echo [%DT_TEST_VALEUR:&=et%]", &env, &CancelToken::new()).unwrap();
    assert!(out.output.contains("[aetb]"), "{}", out.output);
}

#[test]
fn a_variable_without_value_is_removed_from_the_inherited_environment() {
    // SAFETY : nom propre à ce test, lu seulement ici.
    unsafe { std::env::set_var("DT_TEST_HERITEE", "parent") };
    let dir = tempfile::tempdir().unwrap();
    let env = [("DT_TEST_HERITEE".to_string(), None)];
    let out = run_shell(dir.path(), "echo [%DT_TEST_HERITEE%]", &env, &CancelToken::new()).unwrap();
    assert!(out.output.contains("[%DT_TEST_HERITEE%]"), "variable encore définie : {}", out.output);
}

#[test]
fn the_ansi_view_shows_the_best_fit_characters_a_program_reads() {
    // Page 1252 : les formes « pleine chasse » et les primes deviennent des caractères ASCII qui comptent.
    assert_eq!(ansi_view_in("a\u{FF02}b\u{FF3C}c\u{2033}d\u{02BA}e", 1252).as_deref(), Some("a\"b\\c\"d\"e"));
    assert_eq!(ansi_view_in("é ü&", 1252).as_deref(), Some("é ü&"));
    // UTF-8 : rien ne change.
    assert_eq!(ansi_view_in("a\u{FF02}b", 65001).as_deref(), Some("a\u{FF02}b"));
}

#[test]
fn the_line_is_passed_verbatim_even_when_it_starts_with_a_quote() {
    let dir = tempfile::tempdir().unwrap();
    // Sans `/S`, cmd retirerait ici le premier et le dernier guillemet de la ligne.
    let out = run_shell(dir.path(), r#""%ComSpec%" /C "echo dedans""#, &[], &CancelToken::new()).unwrap();
    assert!(out.output.contains("dedans"), "{}", out.output);
    let out = run_shell(dir.path(), r#"if "a b"=="a b" (echo egal) else (echo different)"#, &[], &CancelToken::new())
        .unwrap();
    assert!(out.output.contains("egal") && !out.output.contains("different"), "{}", out.output);
}

#[test]
fn delayed_expansion_is_off_so_exclamation_marks_stay() {
    let dir = tempfile::tempdir().unwrap();
    let env = [("DT_TEST_VALEUR".to_string(), Some("non".to_string()))];
    let out = run_shell(dir.path(), "echo !DT_TEST_VALEUR!", &env, &CancelToken::new()).unwrap();
    assert!(out.output.contains("!DT_TEST_VALEUR!"), "{}", out.output);
}

#[test]
fn cancelling_stops_the_line_and_its_subprocesses() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        trigger.cancel();
    });
    let start = Instant::now();
    // `ping` sur la boucle locale sert d'attente (aucun accès au réseau).
    let res = run_shell(dir.path(), "ping -n 60 127.0.0.1 >nul", &[], &cancel);
    assert_eq!(res.unwrap_err(), GitError::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(20), "annulation trop lente : {:?}", start.elapsed());
}

#[test]
fn a_program_started_in_the_background_does_not_hold_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let start = Instant::now();
    // `start /b` laisse tourner `ping` après la fin de cmd, avec les tubes de sortie hérités.
    let out =
        run_shell(dir.path(), "start /b ping -n 30 127.0.0.1 >nul& echo lance", &[], &CancelToken::new()).unwrap();
    assert!(out.output.contains("lance"), "{}", out.output);
    assert!(start.elapsed() < Duration::from_secs(15), "attendu la fin du programme lancé : {:?}", start.elapsed());
}
