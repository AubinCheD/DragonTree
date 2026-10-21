//! Instance unique par profil (RV4b) : une seconde instance transmet ses dépôts à la première par un tube nommé. Tout
//! se passe dans ce processus : la première et la seconde instance y sont deux appels à `start`, sur la clé d'un
//! profil temporaire (une instance ouverte de DragonTree n'est jamais visée).
#![cfg(windows)]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use dragontree_app::single_instance::{self, Forwarded, Listener, Startup};

const TIMEOUT: Duration = Duration::from_secs(10);

#[test]
fn a_message_sent_to_the_listener_arrives_with_its_paths() {
    let profile = tempfile::tempdir().unwrap();
    let key = single_instance::profile_key(profile.path());
    let (tx, rx) = mpsc::channel();
    let _listener = Listener::start(&key, move |msg| {
        let _ = tx.send(msg);
    })
    .expect("écoute du tube");

    let sent = Forwarded { open: vec![PathBuf::from(r"C:\depots\contoso"), PathBuf::from(r"D:\Projets\northwind é")] };
    single_instance::send(&key, &sent, TIMEOUT).expect("envoi");
    assert_eq!(rx.recv_timeout(TIMEOUT).expect("message reçu"), sent);
    // Le tube sert de nouveau après chaque message ; un message vide demande seulement le premier plan.
    single_instance::send(&key, &Forwarded::default(), TIMEOUT).expect("second envoi");
    assert_eq!(rx.recv_timeout(TIMEOUT).expect("second message reçu"), Forwarded::default());
}

#[test]
fn nothing_listens_on_another_profile() {
    let (profile, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let _listener = Listener::start(&single_instance::profile_key(profile.path()), |_| {}).expect("écoute du tube");
    let key = single_instance::profile_key(other.path());
    assert!(single_instance::send(&key, &Forwarded::default(), Duration::from_millis(200)).is_err());
}

#[test]
fn a_second_start_on_the_same_profile_forwards_its_repositories_to_the_first() {
    let profile = tempfile::tempdir().unwrap();
    let key = single_instance::profile_key(profile.path());
    let Startup::Primary(first) = single_instance::start(&key, &Forwarded::default(), TIMEOUT) else {
        panic!("première instance du profil attendue");
    };

    // La seconde instance transmet ses dépôts (un dossier qui n'existe pas est écarté) et doit quitter.
    let repo = tempfile::tempdir().unwrap();
    let missing = profile.path().join("absent");
    let wanted = Forwarded { open: vec![repo.path().to_path_buf(), missing] };
    assert!(matches!(single_instance::start(&key, &wanted, TIMEOUT), Startup::Forwarded), "transmis à la première");
    let deadline = Instant::now() + TIMEOUT;
    let received = loop {
        let messages = first.take_messages();
        if !messages.is_empty() {
            break messages;
        }
        assert!(Instant::now() < deadline, "message pas arrivé à la première instance");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(received, [Forwarded { open: vec![repo.path().to_path_buf()] }]);

    // `--profile` : un autre profil donne une instance distincte.
    let other = tempfile::tempdir().unwrap();
    let other_key = single_instance::profile_key(other.path());
    assert!(matches!(single_instance::start(&other_key, &Forwarded::default(), TIMEOUT), Startup::Primary(_)));

    // La première instance fermée, le profil est libre.
    drop(first);
    assert!(matches!(single_instance::start(&key, &Forwarded::default(), TIMEOUT), Startup::Primary(_)));
}
