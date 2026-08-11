//! Le socket d'événements : le chemin par lequel un hook rejoint Ash.
//!
//! C'est le transport d'[ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), et rien
//! d'autre. Il reçoit ce qu'un hook a déclaré, vérifie que l'onglet nommé existe, et le
//! passe à son port. Il n'interprète aucun verbe, ne tient aucun état, et ne connaît pas
//! les cinq mots d'`AgentState` : traduire est le travail de l'adaptateur d'ADR-0008,
//! décider est celui de la machine à états d'ADR-0007 §6.4.
//!
//! ```text
//! Ash ──spawn──▶ bash(ASH_TAB_ID=01J…, ASH_SOCK=~/.ash/ash.sock)
//!                  └─▶ claude
//!                        └─▶ hook: ash-event working --tab $ASH_TAB_ID
//!                                     │
//! Ash ◀──socket unix──────────────────┘
//! ```
//!
//! ## Frontière de sécurité
//!
//! Un socket unix est une **surface d'attaque** : tout processus capable d'ouvrir le
//! fichier peut écrire dans Ash, et un état d'agent falsifié fait mentir la sidebar. Trois
//! contraintes, écrites ici parce que c'est ici qu'elles protègent quelque chose :
//!
//! - **`0700` sur `~/.ash/`, `0600` sur le socket.** Les permissions d'un socket unix sont
//!   vérifiées à la connexion sur macOS ; le dossier privé ferme en plus la fenêtre entre
//!   le `bind` et la pose des permissions, pendant laquelle le socket existe encore avec
//!   les droits du `umask`. Après ça, seul l'utilisateur peut se connecter — c'est la
//!   frontière qu'on défend, et la seule qu'on puisse défendre : un processus du **même**
//!   utilisateur peut déjà tout faire à Ash, socket ou pas.
//! - **Une trame est bornée** ([`MAX_FRAME_BYTES`]) et lue derrière un `take` : une ligne
//!   sans fin est refusée sans avoir été accumulée.
//! - **Une connexion muette ne retient pas l'écoute indéfiniment** ([`READ_TIMEOUT`]).
//!
//! Rien de tout ça ne remonte en erreur : une trame qui cloche est ignorée, et l'écoute
//! continue. Un hook cassé ne doit pas priver les autres onglets de leurs états.

use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::error::AgentError;
use super::wire::{EventFrame, MAX_FRAME_BYTES};

/// Au-delà, on rend la main : le client s'est connecté sans rien dire.
///
/// Les connexions sont servies l'une après l'autre — un hook écrit une ligne et raccroche,
/// ce qui prend des microsecondes, et un fil par connexion coûterait plus cher que ce
/// qu'il éviterait. Ce délai est ce qui empêche une connexion oubliée de faire de ce choix
/// un blocage.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Le port de livraison : ce que le transport fait d'un événement une fois arrivé.
///
/// Le trait appartient à `agents`, et le composition root le branche sur le registre de
/// `pty` : c'est ce qui laisse les deux features s'ignorer, et ce qui rend l'écoute
/// vérifiable sans lancer un seul PTY.
///
/// `knows` est ici plutôt que dans une seconde interface parce que la même autorité
/// répond aux deux questions — le registre détient les onglets
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et parce que la
/// règle « un événement pour un onglet inconnu s'ignore » doit rester du côté d'`agents`,
/// où elle se teste.
pub trait EventSink: Send + Sync {
    /// Cet onglet existe-t-il encore ?
    fn knows(&self, tab_id: &str) -> bool;

    /// Livre l'événement. Appelé seulement quand [`EventSink::knows`] a dit oui.
    fn deliver(&self, event: &EventFrame);
}

/// Le socket en écoute, et de quoi l'éteindre.
///
/// L'arrêt est **explicite** : le composition root le demande sur `RunEvent::Exit`, comme
/// pour la surveillance git. Laisser la fin du processus s'en charger ne nettoierait pas
/// le fichier, et un socket résiduel est exactement ce qui empêche le démarrage suivant de
/// se lier.
pub struct EventSocket {
    path: PathBuf,
    stopped: Arc<AtomicBool>,
}

impl EventSocket {
    /// Ferme l'écoute et retire le fichier. Idempotent.
    ///
    /// `accept()` est bloquant et rien ne le réveille : fermer le descripteur ne suffit
    /// pas. On se connecte donc **à soi-même** pour lui rendre la main une dernière fois,
    /// après avoir posé l'ordre d'arrêt qu'il lira aussitôt. C'est ce qui rend l'arrêt
    /// immédiat et vérifiable, plutôt qu'une écoute non bloquante qui tournerait à vide.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = UnixStream::connect(&self.path);
        let _ = std::fs::remove_file(&self.path);
    }

    /// Là où les onglets doivent écrire — la valeur d'`ASH_SOCK`.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for EventSocket {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Ouvre le socket à son adresse habituelle et sert les événements sur un fil dédié.
pub fn listen(sink: Arc<dyn EventSink>) -> Result<Arc<EventSocket>, AgentError> {
    listen_on(super::wire::socket_path(), sink)
}

/// Idem, à une adresse donnée. C'est la forme que les tests utilisent.
pub fn listen_on(path: PathBuf, sink: Arc<dyn EventSink>) -> Result<Arc<EventSocket>, AgentError> {
    let listener = bind(&path)?;
    let stopped = Arc::new(AtomicBool::new(false));

    let serving = Arc::clone(&stopped);
    std::thread::spawn(move || serve(&listener, sink.as_ref(), &serving));

    Ok(Arc::new(EventSocket { path, stopped }))
}

/// Ouvre le socket, en traitant le fichier laissé par un Ash tué brutalement.
///
/// Un `bind` sur un chemin occupé échoue toujours, que le fichier soit vivant ou mort :
/// c'est en **s'y connectant** qu'on les distingue. Une connexion refusée dit qu'il ne
/// reste qu'un fichier, et on le retire ; une connexion acceptée dit qu'un autre Ash est
/// là, et on renonce — lui prendre son socket couperait les hooks de tous ses onglets.
fn bind(path: &Path) -> Result<UnixListener, AgentError> {
    if let Some(parent) = path.parent() {
        prepare_directory(parent)?;
    }

    if path.exists() {
        if UnixStream::connect(path).is_ok() {
            return Err(AgentError::AlreadyListening(path.to_owned()));
        }
        std::fs::remove_file(path)
            .map_err(|why| AgentError::Bind(path.to_owned(), why.to_string()))?;
    }

    let listener = UnixListener::bind(path)
        .map_err(|why| AgentError::Bind(path.to_owned(), why.to_string()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|why| AgentError::Bind(path.to_owned(), why.to_string()))?;

    Ok(listener)
}

/// Le dossier qui accueille le socket, resserré en `0700` — mais **seulement s'il est le
/// nôtre**.
///
/// Voir la frontière de sécurité en tête de module : le dossier privé est ce qui couvre le
/// socket entre son `bind` et sa mise en `0600`. Ça ne justifie pas de resserrer un
/// dossier qu'on n'a pas créé et qui n'est pas à nous — `/tmp` en est l'exemple limite. La
/// protection qui tient dans tous les cas reste le `0600` du socket lui-même, que macOS
/// vérifie à la connexion.
fn prepare_directory(parent: &Path) -> Result<(), AgentError> {
    let ours = !parent.exists() || parent == super::wire::ash_directory();

    let failed = |why: std::io::Error| AgentError::Directory(parent.to_owned(), why.to_string());
    std::fs::create_dir_all(parent).map_err(failed)?;
    if ours {
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
    }
    Ok(())
}

/// La boucle d'écoute. Bloquante : c'est son fil qui la porte.
fn serve(listener: &UnixListener, sink: &dyn EventSink, stopped: &AtomicBool) {
    for connection in listener.incoming() {
        // Lu **avant** de servir : la connexion qui vient d'arriver est peut-être celle que
        // [`EventSocket::stop`] a ouverte pour nous réveiller.
        if stopped.load(Ordering::Acquire) {
            return;
        }

        match connection {
            Ok(stream) => receive(stream, sink),
            // Un `accept` en erreur ne dit rien sur le suivant — sauf si le socket a
            // disparu sous nos pieds, et l'ordre d'arrêt le dira à la passe suivante.
            Err(_) => continue,
        }
    }
}

/// Une connexion, une trame, et rien de plus.
///
/// Tout ce qui cloche se termine par un abandon silencieux : c'est la conduite voulue.
/// Un hook mal configuré, un client hostile ou une mise à jour ratée ne doivent pas
/// pouvoir faire tomber l'écoute pour les autres onglets.
fn receive(stream: UnixStream, sink: &dyn EventSink) {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));

    // `take` borne la lecture : au-delà, la ligne est tronquée puis refusée par
    // `from_line`, sans que le tampon ait grossi au-delà de la borne.
    let mut reader = BufReader::new(stream.take(MAX_FRAME_BYTES as u64 + 1));
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }

    let Ok(frame) = EventFrame::from_line(&line) else {
        return;
    };

    // Un événement pour un onglet inconnu s'ignore. Il n'y a rien à rattraper : l'onglet
    // vient d'être fermé, ou l'agent a été lancé hors d'Ash avec un `ASH_TAB_ID` hérité
    // d'une session précédente. Deviner l'onglet par le `cwd` est précisément ce
    // qu'ADR-0007 interdit.
    if !sink.knows(&frame.tab_id) {
        return;
    }

    sink.deliver(&frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::mpsc;
    use std::sync::Mutex;

    /// Un port de livraison qui dit ce qu'il a reçu, et pour qui il répond.
    ///
    /// Il rend l'attente **déterministe** : les tests lisent sur son canal au lieu de
    /// dormir en espérant que la boucle soit passée.
    struct FakeSink {
        known: Vec<String>,
        delivered: Mutex<mpsc::Sender<EventFrame>>,
    }

    impl FakeSink {
        fn knowing(tabs: &[&str]) -> (Arc<Self>, mpsc::Receiver<EventFrame>) {
            let (sender, receiver) = mpsc::channel();
            let sink = Arc::new(Self {
                known: tabs.iter().map(|tab| (*tab).to_owned()).collect(),
                delivered: Mutex::new(sender),
            });
            (sink, receiver)
        }
    }

    impl EventSink for FakeSink {
        fn knows(&self, tab_id: &str) -> bool {
            self.known.iter().any(|known| known == tab_id)
        }

        fn deliver(&self, event: &EventFrame) {
            let _ = self.delivered.lock().unwrap().send(event.clone());
        }
    }

    /// Un chemin de socket à soi, court — la limite d'un chemin unix est de 104 octets, et
    /// le `TMPDIR` par utilisateur de macOS en mange déjà la moitié.
    fn a_socket_path() -> PathBuf {
        PathBuf::from(format!("/tmp/ash-test-{}.sock", ulid::Ulid::generate()))
    }

    /// Ce qu'un hook fait : ouvrir, écrire une ligne, raccrocher.
    ///
    /// L'écriture est tolérée en échec : une ligne trop longue est coupée par le serveur
    /// avant sa fin, et le client s'en prend un `EPIPE`. C'est le comportement attendu.
    fn post(path: &Path, line: &str) {
        let mut stream = UnixStream::connect(path).unwrap();
        let _ = stream.write_all(line.as_bytes());
    }

    /// L'attente d'un test : bornée, et sans jamais dormir dans le cas nominal.
    fn received(delivered: &mpsc::Receiver<EventFrame>) -> Option<EventFrame> {
        delivered.recv_timeout(Duration::from_secs(5)).ok()
    }

    #[test]
    fn given_a_hook_posting_working_for_a_live_tab_when_it_reaches_the_socket_then_ash_receives_it_correlated_by_its_tab_id(
    ) {
        // Given — le chemin complet d'ADR-0007 : le hook connaît `ASH_TAB_ID`, et c'est la
        // seule chose qui relie son événement à un onglet.
        let (sink, delivered) = FakeSink::knowing(&["01J0TAB"]);
        let socket = listen_on(a_socket_path(), sink).unwrap();

        // When
        post(
            socket.path(),
            &EventFrame::new("working", "01J0TAB").to_line().unwrap(),
        );

        // Then
        assert_eq!(
            received(&delivered),
            Some(EventFrame::new("working", "01J0TAB"))
        );
    }

    #[test]
    fn given_an_event_for_a_tab_ash_does_not_know_when_it_arrives_then_it_is_ignored_and_the_next_one_is_still_served(
    ) {
        // Given — un onglet fermé pendant qu'un hook partait, ou un agent lancé hors d'Ash
        // avec un `ASH_TAB_ID` d'une session précédente. Deviner l'onglet est interdit
        // (ADR-0007) ; tomber l'est encore plus.
        let (sink, delivered) = FakeSink::knowing(&["01J0TAB"]);
        let socket = listen_on(a_socket_path(), sink).unwrap();

        // When
        post(
            socket.path(),
            &EventFrame::new("done", "01J0GHOST").to_line().unwrap(),
        );
        post(
            socket.path(),
            &EventFrame::new("waiting", "01J0TAB").to_line().unwrap(),
        );

        // Then — c'est la seconde trame qui prouve que l'écoute a survécu à la première
        assert_eq!(
            received(&delivered),
            Some(EventFrame::new("waiting", "01J0TAB"))
        );
    }

    #[test]
    fn given_a_client_writing_garbage_or_an_endless_line_when_it_reaches_the_socket_then_ash_keeps_serving(
    ) {
        // Given — le socket est ouvrable par n'importe quel processus de l'utilisateur.
        // Une trame qui cloche s'ignore ; elle ne fait tomber personne, et elle ne fait pas
        // grossir Ash.
        let (sink, delivered) = FakeSink::knowing(&["01J0TAB"]);
        let socket = listen_on(a_socket_path(), sink).unwrap();

        // When
        post(socket.path(), "ceci n'est pas du json\n");
        post(
            socket.path(),
            &format!("{}\n", "x".repeat(MAX_FRAME_BYTES * 4)),
        );
        post(socket.path(), "");
        post(
            socket.path(),
            &EventFrame::new("working", "01J0TAB").to_line().unwrap(),
        );

        // Then
        assert_eq!(
            received(&delivered),
            Some(EventFrame::new("working", "01J0TAB"))
        );
    }

    #[test]
    fn given_a_socket_file_left_behind_by_an_ash_that_was_killed_when_ash_starts_again_then_it_listens_anyway(
    ) {
        // Given — `kill -9` ne délie rien : le fichier reste, et un `bind` dessus échoue.
        // Sans ce rattrapage, Ash n'aurait plus jamais d'événements jusqu'à un `rm` manuel.
        let path = a_socket_path();
        std::fs::write(&path, b"un socket mort").unwrap();
        let (sink, delivered) = FakeSink::knowing(&["01J0TAB"]);

        // When
        let socket = listen_on(path, sink).unwrap();
        post(
            socket.path(),
            &EventFrame::new("working", "01J0TAB").to_line().unwrap(),
        );

        // Then
        assert_eq!(
            received(&delivered),
            Some(EventFrame::new("working", "01J0TAB"))
        );
    }

    #[test]
    fn given_an_ash_already_listening_when_a_second_one_starts_then_it_refuses_instead_of_stealing_the_socket(
    ) {
        // Given — retirer le socket d'un Ash vivant couperait les hooks de tous ses
        // onglets, en silence. Le second doit renoncer, pas gagner.
        let path = a_socket_path();
        let (first, _delivered) = FakeSink::knowing(&[]);
        let _listening = listen_on(path.clone(), first).unwrap();

        // When
        let (second, _) = FakeSink::knowing(&[]);
        let refused = listen_on(path, second);

        // Then
        assert!(matches!(refused, Err(AgentError::AlreadyListening(_))));
    }

    #[test]
    fn given_a_listening_socket_when_ash_shuts_down_then_the_socket_file_is_removed() {
        // Given — c'est le critère de sortie : un fichier résiduel est ce qui empêche le
        // démarrage suivant de se lier.
        let (sink, _delivered) = FakeSink::knowing(&[]);
        let socket = listen_on(a_socket_path(), sink).unwrap();
        assert!(socket.path().exists());

        // When
        socket.stop();

        // Then
        assert!(!socket.path().exists());
        assert!(UnixStream::connect(socket.path()).is_err());
    }

    #[test]
    fn given_a_socket_opened_by_ash_when_its_permissions_are_read_then_no_other_user_can_reach_it()
    {
        // Given — un état d'agent falsifié fait mentir la sidebar, et le socket est la
        // seule porte d'entrée. Voir la frontière de sécurité en tête de module.
        let (sink, _delivered) = FakeSink::knowing(&[]);
        let directory = PathBuf::from(format!("/tmp/ash-test-{}", ulid::Ulid::generate()));

        // When
        let socket = listen_on(directory.join("ash.sock"), sink).unwrap();

        // Then
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(socket.path()), 0o600);
        assert_eq!(mode(&directory), 0o700);

        drop(socket);
        let _ = std::fs::remove_dir_all(&directory);
    }
}
