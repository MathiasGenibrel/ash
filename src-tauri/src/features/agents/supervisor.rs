//! Le superviseur : une [`AgentMachine`] par onglet, et les deux sources qui la nourrissent.
//!
//! C'est la couture qui manquait entre les trois pièces de la feature. Elle vit **ici**, et
//! ni dans `pty` ni dans le composition root, pour trois raisons qui tiennent ensemble :
//!
//! - le registre de `pty` ne connaît aujourd'hui que le **vocabulaire** [`AgentState`], ce
//!   qui est sain. Lui donner le mécanisme de décision — les adaptateurs, les hooks,
//!   l'horloge des trente secondes — le ferait déborder de son sujet, qui est de tenir des
//!   PTY ;
//! - le composition root n'a pas de test unitaire et n'en aura jamais. Une règle de produit
//!   qui s'y glisse n'en a pas non plus ;
//! - c'est ce qui tient la promesse d'[ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md) :
//!   le jour où les PTY passeront dans un démon `ashd`, les machines partiront avec eux,
//!   sans qu'aucune règle ne soit restée dans l'assemblage.
//!
//! `pty` pose donc une **question** — « quel état pour cet onglet, compte tenu de ce que la
//! sonde voit ? » — par un trait qu'il possède ([`crate::features::pty::AgentStates`]), et
//! le superviseur y répond. C'est la convention du dépôt (les effets système passent par un
//! trait que la feature possède) appliquée à une décision plutôt qu'à un effet.
//!
//! ## Qui a le droit de dire quoi
//!
//! | Source | Ce qu'elle produit |
//! |---|---|
//! | Un hook, traduit par [`Adapter::interpret`] | `working`, `waiting`, `done`, `error` |
//! | La sonde ([`Presence`]) | la **présence** d'un programme, et sa disparition |
//!
//! **`waiting` n'a aucune autre source qu'un hook**, et c'est structurel plutôt que
//! surveillé : la sonde n'entre dans ce fichier que par [`Presence`], qui ne porte que trois
//! valeurs et aucun état. Rien n'y lit la sortie du PTY
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ## Un onglet devient un agent, puis redevient un shell
//!
//! Une machine ne naît qu'au premier hook, et meurt dès que son verdict retombe sur `idle`.
//! Entre les deux, c'est elle qui répond ; en dehors, c'est la sonde — un onglet où personne
//! n'a jamais parlé montre `working` tant qu'un programme tient l'avant-plan, et `idle`
//! sinon, exactement comme avant cette tranche. C'est ce qui évite d'annoncer la fin d'un
//! agent quand l'utilisateur quitte `vim`
//! ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)).
//!
//! **Limite connue** : pendant les trente secondes où la ligne d'un agent fini reste
//! affichée, un programme lancé dans le même onglet est pris pour cet agent — il le remet au
//! travail, et sa sortie donne `error`. La distinguer demande de reconnaître les commandes
//! d'agent par leur nom (ADR-0006), ce qui est une tranche à part.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::adapter::{Adapter, RawEvent};
use super::machine::{AgentEvent, AgentMachine, Declared, Exit};
use super::state::AgentState;
use super::wire::EventFrame;
use crate::shared::time::Clock;

/// Ce que la sonde voit d'un onglet — et tout ce qu'elle a le droit d'en dire.
///
/// Trois valeurs, aucune conclusion : c'est le point exact où ADR-0007 se tient. La sonde
/// répond à une question de **présence** (`tcgetpgrp`), pas à « que fait cet agent ».
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Presence {
    /// Le shell tient l'avant-plan : rien ne tourne dans cet onglet.
    Prompt,
    /// Un programme tient l'avant-plan. Lequel ne regarde pas cette feature.
    Program,
    /// La sonde n'a rien pu observer.
    ///
    /// Ce n'est pas « rien ne tourne » : un onglet dont le système ne répond plus doit
    /// garder l'état qu'un hook lui a donné, pas retomber à `idle` parce qu'un appel
    /// système a échoué.
    #[default]
    Unknown,
}

/// Les machines à états, une par onglet, et ce qu'il faut pour les nourrir.
pub struct Supervisor {
    clock: Arc<dyn Clock>,
    /// Les adaptateurs embarqués, dans l'ordre où le composition root les a déclarés.
    ///
    /// La trame ne dit pas de quel outil elle vient — elle porte le vocabulaire canonique
    /// de la spec §6.3, que tous les adaptateurs partagent — donc c'est le premier qui
    /// reconnaît le mot qui répond. Un adaptateur sans instrumentation, `generic` en tête,
    /// ne reconnaît rien et ne peut donc rien avaler au passage.
    adapters: Vec<Arc<dyn Adapter>>,
    tabs: Mutex<Tabs>,
}

#[derive(Default)]
struct Tabs {
    /// La fenêtre Ash est-elle au premier plan ? Un **niveau**, tenu ici pour que les
    /// machines nées plus tard le connaissent aussi.
    focused: bool,
    live: HashMap<String, Tab>,
}

#[derive(Default)]
struct Tab {
    /// La machine de cet onglet, tant qu'un agent y vit.
    machine: Option<AgentMachine>,
    /// Ce que la sonde a vu la dernière fois — de quoi reconnaître un **front**.
    ///
    /// Retenu même sans machine : sans ça, la machine créée par le premier hook prendrait le
    /// programme déjà en cours pour un agent qui démarre, et remettrait au travail celui qui
    /// vient de déclarer sa fin.
    seen: Presence,
}

impl Supervisor {
    pub fn new(clock: Arc<dyn Clock>, adapters: Vec<Arc<dyn Adapter>>) -> Self {
        Self {
            clock,
            adapters,
            tabs: Mutex::new(Tabs::default()),
        }
    }

    /// Un hook a parlé. C'est la seule source de `waiting`, et elle fait autorité.
    ///
    /// Le mot brut traverse l'adaptateur avant d'atteindre la machine : un verbe qu'aucun
    /// adaptateur ne reconnaît ne produit rien du tout — ni état, ni erreur. Deviner serait
    /// exactement ce qu'ADR-0007 écarte.
    pub fn on_hook(&self, event: &EventFrame) {
        let Some(declared) = self.translate(&event.kind) else {
            return;
        };
        let Ok(mut tabs) = self.tabs.lock() else {
            return;
        };

        let focused = tabs.focused;
        let clock = Arc::clone(&self.clock);
        let tab = tabs.live.entry(event.tab_id.clone()).or_default();
        tab.machine
            .get_or_insert_with(|| watching(clock, focused))
            .on(AgentEvent::Hook(declared));
    }

    /// Quel état pour cet onglet, compte tenu de ce que la sonde voit ?
    ///
    /// Appelée à chaque passe de la boucle d'ADR-0005 : c'est elle qui fait avancer le temps
    /// des machines, et c'est par son résultat — porté par le `TabInfo` du registre — que
    /// l'état atteint l'écran. Le frontend n'apprend jamais un état autrement.
    pub fn state(&self, tab_id: &str, seen: Presence) -> AgentState {
        let Ok(mut tabs) = self.tabs.lock() else {
            // Un superviseur empoisonné n'a plus de mémoire ; la sonde, elle, répond
            // toujours. Mieux vaut un onglet honnête qu'un onglet figé.
            return probed(seen);
        };

        let tab = tabs.live.entry(tab_id.to_owned()).or_default();
        let before = std::mem::replace(&mut tab.seen, seen);
        // Une passe aveugle ne raconte rien : elle ne doit pas non plus effacer le souvenir
        // de la précédente, sinon le retour du système passerait pour un lancement.
        if seen == Presence::Unknown {
            tab.seen = before;
        }

        let Some(machine) = tab.machine.as_mut() else {
            return probed(seen);
        };

        match (before, seen) {
            // Le lancement : un programme prend l'avant-plan d'un onglet qui n'en avait pas.
            // Un **front**, jamais un niveau — répété trois fois par seconde, il remettrait
            // au travail l'agent qu'un hook vient de dire fini.
            (Presence::Prompt | Presence::Unknown, Presence::Program) => {
                machine.on(AgentEvent::AgentStarted);
            }
            // La disparition : le shell a repris son terminal. On ne saura jamais avec quel
            // code — voir [`Exit::Unseen`].
            (Presence::Program, Presence::Prompt) => {
                machine.on(AgentEvent::ProcessVanished(Exit::Unseen));
            }
            _ => {}
        }

        machine.tick();
        let state = machine.state();
        if state == AgentState::Idle {
            // La ligne est redevenue une ligne shell : l'onglet n'est plus un agent, et
            // c'est de nouveau la sonde qui répond pour lui.
            tab.machine = None;
        }
        state
    }

    /// La fenêtre Ash a pris ou perdu le premier plan.
    ///
    /// Un niveau, poussé à toutes les machines : c'est lui qui décide si la ligne d'un agent
    /// fini a été **vue**, donc si ses trente secondes peuvent commencer (spec §6.4).
    pub fn on_window_focus(&self, focused: bool) {
        let Ok(mut tabs) = self.tabs.lock() else {
            return;
        };
        tabs.focused = focused;
        for tab in tabs.live.values_mut() {
            if let Some(machine) = tab.machine.as_mut() {
                machine.on(AgentEvent::WindowFocus(focused));
            }
        }
    }

    /// Cet onglet n'existe plus.
    ///
    /// Un état d'agent ne survit pas à son onglet : rien n'est restauré, et la mémoire d'un
    /// onglet fermé ne doit pas répondre à un identifiant réattribué (ADR-0009).
    pub fn forget(&self, tab_id: &str) {
        if let Ok(mut tabs) = self.tabs.lock() {
            tabs.live.remove(tab_id);
        }
    }

    /// Le mot reçu sur le socket, traduit par le premier adaptateur qui le reconnaît.
    fn translate(&self, kind: &str) -> Option<Declared> {
        let raw = RawEvent::new(kind);
        self.adapters
            .iter()
            .find_map(|adapter| adapter.interpret(&raw))
            .and_then(Declared::of)
    }
}

/// Une machine neuve, à qui l'on dit tout de suite si l'utilisateur regarde.
fn watching(clock: Arc<dyn Clock>, focused: bool) -> AgentMachine {
    let mut machine = AgentMachine::new(clock);
    machine.on(AgentEvent::WindowFocus(focused));
    machine
}

/// Ce que la sonde seule permet de dire d'un onglet où aucun agent n'a jamais parlé.
///
/// C'est le comportement du jalon J1, et il est inchangé : `vim`, `htop` ou un `make` qui
/// tourne donnent `working`, et un shell à son invite donne `idle`. Jamais `waiting`.
fn probed(seen: Presence) -> AgentState {
    match seen {
        Presence::Program => AgentState::Working,
        Presence::Prompt | Presence::Unknown => AgentState::Idle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::agents::adapters::{ClaudeCodeAdapter, GenericAdapter};
    use crate::features::agents::fakes::ManualClock;
    use std::path::PathBuf;

    const TAB: &str = "01J0TAB";

    /// Test Data Builder : le superviseur tel que le composition root l'assemble.
    ///
    /// Les adaptateurs sont les **vrais** — c'est ce qui fait que ces tests parlent du
    /// vocabulaire que Claude Code écrira réellement dans le `settings.json`, et non d'un
    /// mot inventé pour le test.
    struct SupervisorBuilder {
        clock: Arc<ManualClock>,
        focused: bool,
    }

    impl SupervisorBuilder {
        fn new() -> Self {
            Self {
                clock: ManualClock::new(),
                focused: false,
            }
        }

        /// La fenêtre Ash est au premier plan — l'utilisateur regarde.
        fn watched(mut self) -> Self {
            self.focused = true;
            self
        }

        fn build(self) -> (Supervisor, Arc<ManualClock>) {
            let adapters: Vec<Arc<dyn Adapter>> = vec![
                Arc::new(GenericAdapter),
                Arc::new(ClaudeCodeAdapter::new(PathBuf::from(
                    "/Applications/Ash.app/Contents/MacOS/ash-event",
                ))),
            ];
            let supervisor = Supervisor::new(Arc::clone(&self.clock) as Arc<dyn Clock>, adapters);
            supervisor.on_window_focus(self.focused);
            (supervisor, self.clock)
        }
    }

    /// Ce qu'un hook envoie : le mot canonique de la spec §6.3, et l'onglet qui l'a hérité.
    fn hook(word: &str, tab: &str) -> EventFrame {
        EventFrame::new(word, tab)
    }

    /// Une passe de la boucle de sonde, telle que le registre la fait.
    fn sweep(supervisor: &Supervisor, seen: Presence) -> AgentState {
        supervisor.state(TAB, seen)
    }

    #[test]
    fn given_a_tab_where_no_agent_ever_spoke_when_a_program_takes_the_foreground_then_it_works_and_never_waits(
    ) {
        // Given — `vim`, `htop`, un `make` : la sonde ne sait pas les nommer autrement que
        // « quelque chose tourne », et c'est tout ce qu'elle a le droit d'en dire
        // (ADR-0007, précision du 2026-08-11).
        let (supervisor, _clock) = SupervisorBuilder::new().build();

        // When
        let at_the_prompt = sweep(&supervisor, Presence::Prompt);
        let running = sweep(&supervisor, Presence::Program);

        // Then
        assert_eq!(at_the_prompt, AgentState::Idle);
        assert_eq!(running, AgentState::Working);
    }

    #[test]
    fn given_a_program_that_ran_without_a_single_hook_when_it_leaves_the_foreground_then_the_tab_is_a_shell_row_again(
    ) {
        // Given — quitter `vim` n'est pas la fin d'un agent. Annoncer `done` ici ferait
        // clignoter la sidebar à chaque commande, et rendrait l'état inutilisable pour ce
        // qu'il sert : reconnaître un agent qui a fini.
        let (supervisor, _clock) = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);

        // When
        let back_at_the_prompt = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(back_at_the_prompt, AgentState::Idle);
    }

    #[test]
    fn given_a_hook_saying_the_agent_asks_a_question_when_the_probe_keeps_seeing_it_running_then_the_tab_stays_waiting(
    ) {
        // Given — le conflit qui décide de toute cette tranche : la sonde voit `claude` au
        // premier plan et le croirait au travail, alors qu'il attend une réponse. Le hook
        // fait autorité (ADR-0007) ; sans ça, le seul état qui mérite d'interrompre
        // l'utilisateur serait écrasé trois fois par seconde.
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        supervisor.on_hook(&hook("waiting", TAB));
        let announced: Vec<AgentState> = (0..10)
            .map(|_| sweep(&supervisor, Presence::Program))
            .collect();

        // Then
        assert_eq!(announced, vec![AgentState::Waiting; 10]);
    }

    #[test]
    fn given_an_agent_that_declared_its_end_when_its_process_disappears_then_the_tab_still_says_done(
    ) {
        // Given — la séquence réelle d'une fin propre : `SessionEnd` part, *puis* le
        // processus quitte l'avant-plan. La disparition ne doit rien retrancher à ce que le
        // hook a dit, sinon toute fin normale s'afficherait en échec.
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));

        // When
        let after_the_process_left = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(after_the_process_left, AgentState::Done);
    }

    #[test]
    fn given_an_agent_at_work_when_its_process_disappears_without_declaring_its_end_then_the_tab_shows_an_error(
    ) {
        // Given — plantage, `kill`, `Ctrl-C` en plein travail : un agent instrumenté dit sa
        // fin lui-même, donc partir sans l'avoir dite est anormal. Ash n'aura jamais son
        // code de sortie — il n'a pas lancé le processus (ADR-0006) — et c'est le seul
        // endroit du produit où cette absence se tranche. Voir [`Exit::Unseen`].
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("working", TAB));

        // When
        let after_the_crash = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(after_the_crash, AgentState::Error);
    }

    #[test]
    fn given_a_word_no_adapter_understands_when_it_arrives_from_the_socket_then_the_tab_keeps_its_state(
    ) {
        // Given — `Stop` est un vrai nom de hook de Claude Code, `idle` un vrai état du
        // produit : ni l'un ni l'autre n'est un mot que le bloc d'Ash écrit (spec §6.3).
        // Les accepter reviendrait à deviner, et un `waiting` deviné est exactement ce
        // qu'ADR-0007 refuse.
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);

        // When
        for word in ["Stop", "Notification", "idle", "", "waiting "] {
            supervisor.on_hook(&hook(word, TAB));
        }

        // Then — la sonde continue de répondre, comme si rien n'était arrivé
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Working);
        assert_eq!(sweep(&supervisor, Presence::Prompt), AgentState::Idle);
    }

    #[test]
    fn given_two_claude_accounts_in_two_tabs_when_one_of_them_asks_a_question_then_only_its_own_tab_waits(
    ) {
        // Given — `claude` et `claude-perso`, deux dossiers de configuration, deux blocs de
        // hooks, et un seul socket. Ce qui les sépare est `ASH_TAB_ID`, et rien d'autre :
        // ni le `cwd`, ni un horodatage, ni le pid (ADR-0007).
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        supervisor.state("01J0PRO", Presence::Program);
        supervisor.state("01J0PERSO", Presence::Program);

        // When
        supervisor.on_hook(&hook("waiting", "01J0PERSO"));

        // Then
        assert_eq!(
            supervisor.state("01J0PRO", Presence::Program),
            AgentState::Working
        );
        assert_eq!(
            supervisor.state("01J0PERSO", Presence::Program),
            AgentState::Waiting
        );
    }

    #[test]
    fn given_a_done_line_the_user_has_seen_when_thirty_seconds_of_sweeps_pass_then_the_tab_becomes_a_shell_row_again(
    ) {
        // Given — la règle des 30 s de la spec §6.4 est prouvée par la machine ; ce qui se
        // prouve ici est qu'elle est **branchée** : personne d'autre que la boucle de sonde
        // ne fait avancer le temps, et une ligne `done` que rien ne rafraîchirait resterait
        // pour toujours.
        let (supervisor, clock) = SupervisorBuilder::new().watched().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));
        sweep(&supervisor, Presence::Prompt);

        // When
        clock.advance(29);
        let still_shown = sweep(&supervisor, Presence::Prompt);
        clock.advance(1);
        let expired = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(still_shown, AgentState::Done);
        assert_eq!(expired, AgentState::Idle);
    }

    #[test]
    fn given_a_done_line_produced_while_ash_was_hidden_when_the_window_comes_back_then_the_thirty_seconds_start_there(
    ) {
        // Given — Ash derrière l'éditeur, l'agent finit tout seul. Effacer la ligne au bout
        // de 30 s d'absence ferait disparaître l'information avant que personne ne l'ait
        // lue : un agent aurait travaillé pour rien.
        let (supervisor, clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("done", TAB));
        clock.advance(3600);

        // When
        let while_hidden = sweep(&supervisor, Presence::Prompt);
        supervisor.on_window_focus(true);
        clock.advance(30);
        let after_being_seen = sweep(&supervisor, Presence::Prompt);

        // Then
        assert_eq!(while_hidden, AgentState::Done);
        assert_eq!(after_being_seen, AgentState::Idle);
    }

    #[test]
    fn given_an_agent_whose_tab_is_closed_when_a_new_tab_takes_its_place_then_nothing_of_it_remains(
    ) {
        // Given — rien n'est restauré (ADR-0009), et surtout pas dans un onglet qui n'est
        // plus celui-là. Un état qui survivrait à son onglet serait un agent fantôme dans la
        // sidebar, sans processus derrière lui.
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));

        // When
        supervisor.forget(TAB);

        // Then
        assert_eq!(sweep(&supervisor, Presence::Program), AgentState::Working);
    }

    #[test]
    fn given_a_tab_the_probe_can_no_longer_observe_when_the_loop_sweeps_then_its_declared_state_survives(
    ) {
        // Given — un appel système qui échoue, un processus qui se dérobe entre deux
        // passes : c'est courant, et ça ne dit rien de l'agent. Retomber à `idle` là-dessus
        // ferait clignoter la sidebar au gré de la charge de la machine.
        let (supervisor, _clock) = SupervisorBuilder::new().build();
        sweep(&supervisor, Presence::Program);
        supervisor.on_hook(&hook("waiting", TAB));

        // When
        let blind = sweep(&supervisor, Presence::Unknown);
        let seeing_again = sweep(&supervisor, Presence::Program);

        // Then — et la reprise de la sonde n'est pas prise pour un nouveau lancement, qui
        // aurait écrasé le `waiting` par un `working`
        assert_eq!(blind, AgentState::Waiting);
        assert_eq!(seeing_again, AgentState::Waiting);
    }
}
