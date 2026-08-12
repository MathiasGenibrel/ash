//! La machine à états d'un agent : le diagramme de la spec §6.2, et ses règles §6.4.
//!
//! Une machine par onglet. Elle ne connaît ni socket, ni adaptateur, ni PTY : on lui
//! **raconte** ce qui arrive avec [`AgentEvent`], et elle dit dans quel état l'onglet se
//! trouve. C'est ce qui la rend prouvable en entier sans lancer un seul processus.
//!
//! Les trois règles qui la gouvernent, et qui ne se rediscutent pas ici :
//!
//! - **un hook fait autorité sur la sonde**
//!   ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)). La sonde ne sait que deux
//!   choses : qu'un agent a pris l'avant-plan, et qu'un processus a disparu. Elle
//!   n'infirme jamais ce qu'un hook a déclaré, et **rien** ne se déduit de la sortie du PTY ;
//! - **Ash ne devine pas.** Un agent silencieux depuis dix minutes en `working` reste
//!   `working`. Il n'y a donc, volontairement, aucun délai d'expiration d'un état actif
//!   dans ce fichier — voir [`AgentMachine::tick`] ;
//! - **le backend détient l'état**
//!   ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le frontend rend
//!   ce que cette machine décide ; il ne recalcule rien.

use std::sync::Arc;
use std::time::{Duration, Instant};

use super::state::AgentState;
use crate::shared::time::Clock;

/// Combien de temps une ligne `done`/`error` reste visible avant de redevenir une ligne
/// shell `idle` (spec §6.4).
///
/// Le décompte ne part **pas** du passage en `done` : il part du moment où la fenêtre a
/// eu le focus. Voir [`AgentMachine::tick`].
pub const LINGER: Duration = Duration::from_secs(30);

/// Ce qu'un hook déclare, une fois traduit dans le vocabulaire commun.
///
/// C'est délibérément un type à part d'[`AgentState`] : `idle` n'est pas déclarable. Un
/// agent ne dit jamais « je n'existe pas » — c'est la sonde qui le constate, et le temps
/// qui l'entérine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    Working,
    Waiting,
    Done,
    Error,
}

impl Declared {
    /// Ce qu'un état traduit par un adaptateur devient dans le vocabulaire de la machine.
    ///
    /// `None` pour `idle`, et c'est la porte que ce type existe pour tenir : un outil qui
    /// parle est la preuve qu'il est là, donc il ne peut pas déclarer son absence. La
    /// traduction inverse est [`state_of`] ; les deux vivent côte à côte pour qu'un état
    /// ajouté ne puisse pas être oublié d'un seul côté.
    pub fn of(state: AgentState) -> Option<Self> {
        match state {
            AgentState::Working => Some(Declared::Working),
            AgentState::Waiting => Some(Declared::Waiting),
            AgentState::Done => Some(Declared::Done),
            AgentState::Error => Some(Declared::Error),
            AgentState::Idle => None,
        }
    }
}

/// Comment un processus s'est terminé.
///
/// C'est la seule chose que la sonde ajoute à ce qu'elle sait déjà, et elle ne sert qu'à
/// un cas : trancher entre `done` et `error` quand aucun hook ne l'a fait (spec §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// Tué par un signal — `SIGSEGV`, `SIGKILL`, `Ctrl-C`. Jamais une fin normale.
    Signal(i32),
    /// Parti, et personne ne sait comment.
    ///
    /// C'est le cas **normal** de la sonde d'[ADR-0005](../../../../docs/adr/0005-sonde-cwd-libproc.md) :
    /// Ash n'a pas lancé l'agent — c'est l'utilisateur qui l'a tapé dans son shell
    /// ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)) — donc
    /// aucun `wait()` ne lui rendra jamais son code de sortie. Tout ce que `tcgetpgrp`
    /// apprend, c'est que l'avant-plan est revenu au shell.
    ///
    /// Ça vaut **échec**, et c'est une décision de produit : un agent instrumenté déclare sa
    /// fin lui-même (`SessionEnd` → `done`), donc disparaître sans l'avoir dite est
    /// anormal — plantage, `kill`, `Ctrl-C` en plein travail. Dire `done` à sa place
    /// annoncerait un travail terminé là où il a été interrompu, ce qui est la seule des
    /// deux erreurs qui trompe l'utilisateur sur ce qu'il lui reste à faire.
    Unseen,
}

impl Exit {
    fn is_success(self) -> bool {
        matches!(self, Exit::Code(0))
    }
}

/// Tout ce qui peut arriver à un agent, du point de vue de la machine.
///
/// C'est l'unique entrée : la jonction avec le socket d'événements, les adaptateurs et la
/// boucle de sonde se fait en traduisant vers ces quatre variantes, et nulle part ailleurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEvent {
    /// Un hook a déclaré l'état de l'agent. Fait autorité.
    Hook(Declared),
    /// La sonde a vu une commande reconnue prendre l'avant-plan de l'onglet (spec §6.1).
    ///
    /// **C'est un front, pas un niveau** : à émettre quand l'avant-plan *devient* un agent,
    /// pas à chaque passe de la boucle. Un agent qui a rendu la main reste souvent au
    /// premier plan à son invite ; répété trois fois par seconde, cet événement ferait
    /// repasser en `working` un agent que son hook vient de déclarer `done`.
    AgentStarted,
    /// La sonde a constaté la disparition du processus, avec son code de sortie.
    ProcessVanished(Exit),
    /// La fenêtre Ash a pris ou perdu le focus.
    ///
    /// C'est un **niveau**, et pas un front : il faut pouvoir savoir si la fenêtre était
    /// déjà au premier plan au moment où un agent a fini. Sans ça, une ligne `done`
    /// obtenue sous les yeux de l'utilisateur resterait affichée pour toujours.
    WindowFocus(bool),
}

/// L'état d'un agent, et ce qu'il faut de mémoire pour le tenir.
///
/// Le compte à rebours des 30 s vit ici plutôt que dans un minuteur : un `Instant` retenu
/// se relit quand on veut, et il ne réveille personne. C'est la boucle qui passe déjà —
/// celle d'ADR-0005 — qui appelle [`Self::tick`].
pub struct AgentMachine {
    clock: Arc<dyn Clock>,
    state: AgentState,
    /// Vrai si la fenêtre Ash est au premier plan, pour autant qu'on nous l'ait dit.
    ///
    /// Faux au départ, et c'est le défaut prudent : au pire la ligne d'un agent fini
    /// reste affichée jusqu'au prochain focus, alors qu'un défaut à vrai la ferait
    /// disparaître sans que personne ne l'ait vue.
    window_focused: bool,
    /// Depuis quand la ligne d'un agent fini est **vue**, donc depuis quand elle a le
    /// droit de s'effacer.
    ///
    /// `None` sur un état actif, et `None` aussi sur un `done` qui n'a pas encore été
    /// regardé : c'est ce second cas qui porte le « indéfiniment » de la spec §6.4.
    seen_since: Option<Instant>,
}

impl AgentMachine {
    /// Un onglet neuf : un shell sans agent.
    ///
    /// L'horloge est injectée parce que le temps est un effet système comme un autre, et
    /// parce que les règles de §6.4 seraient invérifiables autrement : les prouver
    /// coûterait trente secondes de sommeil par test.
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            state: AgentState::Idle,
            window_focused: false,
            seen_since: None,
        }
    }

    pub fn state(&self) -> AgentState {
        self.state
    }

    /// Il s'est passé quelque chose. Rend le nouvel état s'il a changé, `None` sinon.
    ///
    /// Le `None` n'est pas un détail de confort : c'est lui qui garde la frontière Tauri
    /// muette quand rien ne bouge, comme le fait déjà la boucle de sonde.
    pub fn on(&mut self, event: AgentEvent) -> Option<AgentState> {
        let now = self.clock.now();
        match event {
            AgentEvent::Hook(declared) => self.enter(state_of(declared), now),
            AgentEvent::AgentStarted => self.started(now),
            AgentEvent::ProcessVanished(exit) => self.vanished(exit, now),
            AgentEvent::WindowFocus(focused) => {
                self.window_focused = focused;
                // Le focus ne change aucun état — il ne fait qu'ouvrir le compte à rebours
                // de la ligne d'un agent fini que personne n'avait encore pu voir.
                if focused && has_finished(self.state) && self.seen_since.is_none() {
                    self.seen_since = Some(now);
                }
                None
            }
        }
    }

    /// Le temps a passé : la ligne d'un agent fini a-t-elle fait le sien ?
    ///
    /// Rend `Some(Idle)` la première fois que les 30 s sont écoulées — l'onglet redevient
    /// une ligne shell. `None` partout ailleurs, et notamment :
    ///
    /// - sur un `done` que la fenêtre n'a pas encore montré : la ligne reste, indéfiniment ;
    /// - sur un agent en `working` silencieux depuis une heure. **Aucun état actif n'expire
    ///   jamais** : c'est exactement ce que « Ash ne devine pas » veut dire, et c'est
    ///   pourquoi le seuil de 60 s de la spec §6.4 n'apparaît nulle part dans ce fichier.
    ///   Il ne déclenche rien.
    pub fn tick(&mut self) -> Option<AgentState> {
        let seen_since = self.seen_since?;
        if self.clock.now().duration_since(seen_since) < LINGER {
            return None;
        }
        self.state = AgentState::Idle;
        self.seen_since = None;
        Some(AgentState::Idle)
    }

    /// La sonde a vu un agent démarrer.
    ///
    /// Elle n'a le droit de parler que quand aucun hook ne tient l'état : un agent en
    /// `waiting` reste `waiting`, même si son processus est bien là. Sur la ligne d'un
    /// agent fini, en revanche, un agent qui démarre reprend l'onglet — c'est le cas le
    /// plus courant, on relance `claude` dans l'onglet qu'on vient de lire.
    fn started(&mut self, now: Instant) -> Option<AgentState> {
        match self.state {
            AgentState::Working | AgentState::Waiting => None,
            _ => self.enter(AgentState::Working, now),
        }
    }

    /// Le processus a disparu (spec §6.4).
    ///
    /// Sans événement `done`, c'est le code de sortie qui tranche : `done` s'il vaut zéro,
    /// `error` sinon. **Avec** un `done` déjà reçu, il n'y a rien à trancher : le hook a
    /// parlé, et un code de sortie non nul ne le contredit pas — un agent peut très bien
    /// finir son travail puis sortir sur un `Ctrl-C`.
    fn vanished(&mut self, exit: Exit, now: Instant) -> Option<AgentState> {
        match self.state {
            AgentState::Working | AgentState::Waiting => {
                let ended = if exit.is_success() {
                    AgentState::Done
                } else {
                    AgentState::Error
                };
                self.enter(ended, now)
            }
            // Un shell sans agent qui perd un processus n'a rien à annoncer, et la ligne
            // d'un agent déjà fini a son état.
            _ => None,
        }
    }

    fn enter(&mut self, state: AgentState, now: Instant) -> Option<AgentState> {
        if self.state == state {
            return None;
        }
        self.state = state;
        // La ligne d'un agent fini obtenue pendant que l'utilisateur regarde a été vue :
        // son compte à rebours part tout de suite. Sinon il attend le focus.
        self.seen_since = (has_finished(state) && self.window_focused).then_some(now);
        Some(state)
    }
}

/// L'état que porte une déclaration de hook.
fn state_of(declared: Declared) -> AgentState {
    match declared {
        Declared::Working => AgentState::Working,
        Declared::Waiting => AgentState::Waiting,
        Declared::Done => AgentState::Done,
        Declared::Error => AgentState::Error,
    }
}

/// Vrai pour les deux états qui décrivent un agent qui n'est plus là.
///
/// Ce sont les seuls qui s'effacent d'eux-mêmes : ils désignent une ligne à lire, pas un
/// travail en cours.
fn has_finished(state: AgentState) -> bool {
    matches!(state, AgentState::Done | AgentState::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::fakes::ManualClock;

    /// Test Data Builder : une machine posée dans l'état que le scénario veut décrire.
    ///
    /// Les états ne sont pas des champs — ils s'atteignent par les mêmes événements que
    /// dans l'application. Un `Given` reste donc lisible sans jamais construire un état
    /// qu'aucune séquence réelle ne produirait.
    struct AgentBuilder {
        clock: Arc<ManualClock>,
        events: Vec<AgentEvent>,
    }

    impl AgentBuilder {
        fn new() -> Self {
            Self {
                clock: ManualClock::new(),
                events: Vec::new(),
            }
        }

        /// La fenêtre Ash est au premier plan — l'utilisateur regarde.
        fn watched(mut self) -> Self {
            self.events.push(AgentEvent::WindowFocus(true));
            self
        }

        /// Un agent a démarré dans l'onglet : c'est la sonde qui le dit.
        fn started(mut self) -> Self {
            self.events.push(AgentEvent::AgentStarted);
            self
        }

        /// Un hook a déclaré cet état.
        fn declared(mut self, declared: Declared) -> Self {
            self.events.push(AgentEvent::Hook(declared));
            self
        }

        fn build(self) -> (AgentMachine, Arc<ManualClock>) {
            let mut machine = AgentMachine::new(Arc::clone(&self.clock) as Arc<dyn Clock>);
            for event in self.events {
                machine.on(event);
            }
            (machine, self.clock)
        }
    }

    #[test]
    fn given_a_shell_at_its_prompt_when_an_agent_starts_then_the_tab_becomes_working() {
        // Given — la flèche « lancement » du diagramme §6.2. C'est le seul moment où la
        // sonde a le droit d'ouvrir un état d'agent : elle a vu une commande reconnue
        // prendre l'avant-plan (spec §6.1).
        let (mut machine, _clock) = AgentBuilder::new().build();

        // When
        let announced = machine.on(AgentEvent::AgentStarted);

        // Then
        assert_eq!(announced, Some(AgentState::Working));
        assert_eq!(machine.state(), AgentState::Working);
    }

    #[test]
    fn given_a_working_agent_when_a_hook_says_it_asks_a_question_then_it_becomes_waiting() {
        // Given — la flèche « question ». `waiting` est le seul état qui justifie
        // d'interrompre l'utilisateur, et il est indevinable de l'extérieur : seul un hook
        // peut le produire (ADR-0007).
        let (mut machine, _clock) = AgentBuilder::new().started().build();

        // When
        let announced = machine.on(AgentEvent::Hook(Declared::Waiting));

        // Then
        assert_eq!(announced, Some(AgentState::Waiting));
    }

    #[test]
    fn given_a_waiting_agent_when_a_hook_says_it_resumed_then_it_goes_back_to_working() {
        // Given — la flèche « réponse » : l'utilisateur a répondu, l'agent repart.
        let (mut machine, _clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Waiting)
            .build();

        // When
        let announced = machine.on(AgentEvent::Hook(Declared::Working));

        // Then
        assert_eq!(announced, Some(AgentState::Working));
    }

    #[test]
    fn given_a_working_agent_when_a_hook_says_it_finished_then_it_becomes_done() {
        // Given — la flèche « fin ».
        let (mut machine, _clock) = AgentBuilder::new().started().build();

        // When
        let announced = machine.on(AgentEvent::Hook(Declared::Done));

        // Then
        assert_eq!(announced, Some(AgentState::Done));
    }

    #[test]
    fn given_a_working_agent_when_a_hook_reports_a_failure_then_it_becomes_error() {
        // Given — la flèche « échec ». Un hook peut annoncer l'échec lui-même, sans
        // attendre que la sonde constate la disparition du processus.
        let (mut machine, _clock) = AgentBuilder::new().started().build();

        // When
        let announced = machine.on(AgentEvent::Hook(Declared::Error));

        // Then
        assert_eq!(announced, Some(AgentState::Error));
    }

    #[test]
    fn given_a_waiting_agent_when_the_probe_sees_its_process_running_then_the_hook_still_holds() {
        // Given — le conflit qui compte : la sonde voit `claude` au premier plan et le
        // croirait au travail, alors qu'il attend une réponse. Le hook fait autorité sur la
        // sonde (ADR-0007), sinon l'unique état qui mérite d'interrompre l'utilisateur
        // serait écrasé trois fois par seconde.
        let (mut machine, _clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Waiting)
            .build();

        // When
        let announced = machine.on(AgentEvent::AgentStarted);

        // Then
        assert_eq!(announced, None);
        assert_eq!(machine.state(), AgentState::Waiting);
    }

    #[test]
    fn given_a_working_agent_with_no_event_for_ten_minutes_when_the_loop_ticks_then_it_is_still_working(
    ) {
        // Given — un agent qui compile, qui télécharge, ou qui réfléchit longuement. La
        // spec §6.4 est explicite : au-delà de 60 s sans événement, `working` reste
        // `working`. Ash ne devine pas, et c'est une règle de **non-action** — ce test
        // existe pour qu'aucune expiration ne soit ajoutée un jour « pour faire propre ».
        let (mut machine, clock) = AgentBuilder::new().watched().started().build();

        // When — dix minutes de boucle, et pas un seul événement
        let announced: Vec<_> = (0..600)
            .filter_map(|_| {
                clock.advance(1);
                machine.tick()
            })
            .collect();

        // Then
        assert_eq!(announced, vec![]);
        assert_eq!(machine.state(), AgentState::Working);
    }

    #[test]
    fn given_a_working_agent_when_its_process_vanishes_with_code_zero_then_it_becomes_done() {
        // Given — un agent sans hook `Stop`, ou un hook perdu : la disparition du processus
        // est le dernier recours de la spec §6.4.
        let (mut machine, _clock) = AgentBuilder::new().started().build();

        // When
        let announced = machine.on(AgentEvent::ProcessVanished(Exit::Code(0)));

        // Then
        assert_eq!(announced, Some(AgentState::Done));
    }

    #[test]
    fn given_a_waiting_agent_when_its_process_vanishes_with_a_non_zero_code_then_it_becomes_error()
    {
        // Given — un agent tué pendant qu'il posait une question. La règle vaut depuis
        // `waiting` comme depuis `working` : ce qui compte est qu'aucun `done` n'ait été
        // déclaré.
        let (mut machine, _clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Waiting)
            .build();

        // When
        let announced = machine.on(AgentEvent::ProcessVanished(Exit::Code(2)));

        // Then
        assert_eq!(announced, Some(AgentState::Error));
    }

    #[test]
    fn given_a_working_agent_when_its_process_is_killed_by_a_signal_then_it_becomes_error() {
        // Given — `SIGSEGV`, `SIGKILL`, ou un `Ctrl-C` : jamais une fin normale, et un
        // `Exit::Signal(0)` ne doit surtout pas passer pour un succès.
        let (mut machine, _clock) = AgentBuilder::new().started().build();

        // When
        let announced = machine.on(AgentEvent::ProcessVanished(Exit::Signal(9)));

        // Then
        assert_eq!(announced, Some(AgentState::Error));
    }

    #[test]
    fn given_an_agent_that_declared_done_when_its_process_vanishes_badly_then_it_stays_done() {
        // Given — la règle du code de sortie ne s'applique qu'« sans événement `done` ».
        // Un agent qui a rendu son travail puis dont le shell sort sur un code non nul a
        // bien fini : dire `error` ici afficherait un échec là où il n'y en a pas.
        let (mut machine, _clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Done)
            .build();

        // When
        let announced = machine.on(AgentEvent::ProcessVanished(Exit::Code(130)));

        // Then
        assert_eq!(announced, None);
        assert_eq!(machine.state(), AgentState::Done);
    }

    #[test]
    fn given_a_shell_without_an_agent_when_a_process_vanishes_then_nothing_is_announced() {
        // Given — l'utilisateur lance `ls`, `vim`, un `make` qui échoue. Aucun de ces
        // programmes n'est un agent, et faire clignoter la sidebar en rouge pour un `grep`
        // sans résultat rendrait l'état `error` inutilisable.
        let (mut machine, _clock) = AgentBuilder::new().build();

        // When
        let announced = machine.on(AgentEvent::ProcessVanished(Exit::Code(1)));

        // Then
        assert_eq!(announced, None);
        assert_eq!(machine.state(), AgentState::Idle);
    }

    #[test]
    fn given_a_done_line_the_user_has_seen_when_thirty_seconds_pass_then_the_tab_becomes_a_shell_row_again(
    ) {
        // Given — la flèche « retour shell » du diagramme §6.2 : la fenêtre était au premier
        // plan quand l'agent a fini, donc la ligne a été vue et son compte à rebours part
        // aussitôt.
        let (mut machine, clock) = AgentBuilder::new()
            .watched()
            .started()
            .declared(Declared::Done)
            .build();

        // When
        clock.advance(29);
        let still_shown = machine.tick();
        clock.advance(1);
        let expired = machine.tick();

        // Then — et pas une seconde plus tôt : trente secondes est le temps de lever les
        // yeux vers la sidebar.
        assert_eq!(still_shown, None);
        assert_eq!(expired, Some(AgentState::Idle));
        assert_eq!(machine.state(), AgentState::Idle);
    }

    #[test]
    fn given_a_done_line_produced_while_the_window_was_hidden_when_hours_pass_then_it_stays_visible(
    ) {
        // Given — Ash derrière l'éditeur, l'agent finit tout seul. C'est le cas que la spec
        // §6.4 protège : effacer la ligne au bout de 30 s ferait disparaître l'information
        // avant que personne ne l'ait lue, et un agent aurait travaillé pour rien.
        let (mut machine, clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Done)
            .build();

        // When
        clock.advance(3 * 3600);
        let announced = machine.tick();

        // Then
        assert_eq!(announced, None);
        assert_eq!(machine.state(), AgentState::Done);
    }

    #[test]
    fn given_a_done_line_never_seen_when_the_window_finally_gets_the_focus_then_the_countdown_starts_from_there(
    ) {
        // Given — l'autre moitié de la même règle : « depuis » se compte à partir du focus,
        // pas du passage en `done`. Une heure d'absence ne doit pas consommer le délai de
        // lecture.
        let (mut machine, clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Done)
            .build();
        clock.advance(3600);

        // When — l'utilisateur revient sur Ash
        machine.on(AgentEvent::WindowFocus(true));
        clock.advance(29);
        let just_after_the_focus = machine.tick();
        clock.advance(1);
        let expired = machine.tick();

        // Then
        assert_eq!(just_after_the_focus, None);
        assert_eq!(expired, Some(AgentState::Idle));
    }

    #[test]
    fn given_an_error_line_the_user_has_seen_when_thirty_seconds_pass_then_it_becomes_a_shell_row_again(
    ) {
        // Given — la spec §6.4 traite `done` et `error` ensemble. Une ligne d'échec qui
        // resterait pour toujours transformerait la sidebar en journal d'erreurs.
        let (mut machine, clock) = AgentBuilder::new()
            .watched()
            .started()
            .declared(Declared::Error)
            .build();

        // When
        clock.advance(30);
        let expired = machine.tick();

        // Then
        assert_eq!(expired, Some(AgentState::Idle));
    }

    #[test]
    fn given_a_done_line_still_visible_when_a_new_agent_starts_then_the_tab_works_again() {
        // Given — on relance `claude` dans l'onglet qu'on vient de lire, avant la fin des
        // 30 s. La ligne doit repartir au travail : ne pas le faire montrerait un agent
        // « terminé » pendant qu'il travaille.
        let (mut machine, clock) = AgentBuilder::new()
            .watched()
            .started()
            .declared(Declared::Done)
            .build();
        clock.advance(5);

        // When
        let announced = machine.on(AgentEvent::AgentStarted);

        // Then — et le compte à rebours est oublié : plus rien n'expire tant qu'il travaille
        clock.advance(3600);
        assert_eq!(announced, Some(AgentState::Working));
        assert_eq!(machine.tick(), None);
        assert_eq!(machine.state(), AgentState::Working);
    }

    #[test]
    fn given_a_tab_that_became_a_shell_row_again_when_the_loop_keeps_ticking_then_it_announces_nothing_more(
    ) {
        // Given — la boucle d'ADR-0005 passe trois fois par seconde. Réannoncer `idle` à
        // chaque passe réveillerait la webview pour un état identique, ce que tout le reste
        // du registre s'applique à éviter.
        let (mut machine, clock) = AgentBuilder::new()
            .watched()
            .started()
            .declared(Declared::Done)
            .build();
        clock.advance(30);
        machine.tick();

        // When
        let announced: Vec<_> = (0..100)
            .filter_map(|_| {
                clock.advance(1);
                machine.tick()
            })
            .collect();

        // Then
        assert_eq!(announced, vec![]);
    }

    #[test]
    fn given_an_agent_that_repeats_its_state_when_the_hook_fires_again_then_nothing_is_announced() {
        // Given — un hook peut se déclencher plusieurs fois d'affilée : `PreToolUse` part à
        // chaque outil, et un agent qui lit dix fichiers déclare dix fois `working`.
        let (mut machine, _clock) = AgentBuilder::new()
            .started()
            .declared(Declared::Working)
            .build();

        // When
        let announced = machine.on(AgentEvent::Hook(Declared::Working));

        // Then
        assert_eq!(announced, None);
    }
}
