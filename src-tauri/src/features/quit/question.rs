use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::features::pty::TabInfo;

/// Les onglets, relus **au moment du geste**.
///
/// Un port et non le registre lui-même, pour la raison habituelle : la décision de quitter
/// se vérifie alors sans lancer un seul PTY. L'implémentation vit au composition root, qui
/// est le seul à savoir d'où viennent les onglets.
///
/// Elle rend une liste et jamais une erreur : le composition root décide quoi faire d'un
/// registre qui ne répond pas, et la règle qu'il applique — laisser partir — est écrite
/// là-bas, à côté de l'adaptateur.
pub trait ObservedTabs: Send + Sync {
    fn tabs(&self) -> Vec<TabInfo>;
}

/// Ce que la réponse « quitter quand même » laisse derrière elle.
///
/// La confirmation de l'utilisateur ne quitte pas elle-même : elle demande à Tauri de
/// quitter, ce qui repasse par la question. Sans ce laissez-passer, la modale reposerait sa
/// propre question, indéfiniment.
///
/// **Il ne sert qu'une fois.** Un laissez-passer qui resterait ouvert ferait qu'un `⌘Q`
/// ultérieur — si l'arrêt avait été empêché par autre chose entre-temps — partirait sans
/// rien demander, c'est-à-dire exactement la panne que cette tranche corrige.
#[derive(Default)]
pub struct QuitGate(AtomicBool);

impl QuitGate {
    /// L'utilisateur a répondu « quitter quand même ».
    pub fn open(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Consomme le laissez-passer : `true` s'il était ouvert, et il ne l'est plus.
    fn take(&self) -> bool {
        self.0.swap(false, Ordering::SeqCst)
    }
}

/// Poser la question à l'écran. Reçoit les onglets concernés, dans l'ordre des onglets.
pub type AskToQuit = Box<dyn Fn(&[TabInfo]) + Send + Sync>;

/// « Ash peut-il partir ? » — la seule réponse, pour les quatre demandes de sortie.
pub struct QuitQuestion {
    tabs: Arc<dyn ObservedTabs>,
    gate: Arc<QuitGate>,
    ask: AskToQuit,
}

impl QuitQuestion {
    pub fn new(tabs: Arc<dyn ObservedTabs>, gate: Arc<QuitGate>, ask: AskToQuit) -> Self {
        Self { tabs, gate, ask }
    }

    /// `true` : rien à perdre, Ash s'arrête. `false` : la question vient d'être posée.
    ///
    /// L'ordre des trois tests n'est pas indifférent. Le laissez-passer passe **avant** la
    /// lecture des onglets : quand l'utilisateur a répondu « quitter quand même », relire les
    /// agents ne servirait qu'à reposer la question à laquelle il vient de répondre.
    pub fn may_leave(&self) -> bool {
        if self.gate.take() {
            return true;
        }

        let running: Vec<TabInfo> = self
            .tabs
            .tabs()
            .into_iter()
            .filter(|tab| tab.agent.is_some())
            .collect();

        if running.is_empty() {
            return true;
        }

        (self.ask)(&running);
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::features::agents::{AgentState, Instrumented, RecognizedAgent};

    use super::*;

    /// Test Data Builder : un onglet, et le seul champ dont la question dépende.
    struct TabBuilder {
        cwd: String,
        agent: Option<RecognizedAgent>,
        process: String,
        state: AgentState,
    }

    impl TabBuilder {
        /// Un shell à son invite : aucun outil reconnu.
        fn a_shell() -> Self {
            Self {
                cwd: "/wt/ash-sidebar".to_owned(),
                agent: None,
                process: "zsh".to_owned(),
                state: AgentState::Idle,
            }
        }

        /// Ce qu'un `vim` ou un `tail -f` donne : un avant-plan occupé, sans agent.
        fn running(mut self, process: &str) -> Self {
            self.process = process.to_owned();
            self
        }

        fn recognized(mut self, command: &str) -> Self {
            self.process = command.to_owned();
            self.agent = Some(RecognizedAgent {
                command: command.to_owned(),
                adapter: "claude-code".to_owned(),
                instrumented: Instrumented::Installed,
            });
            self
        }

        fn at(mut self, cwd: &str) -> Self {
            self.cwd = cwd.to_owned();
            self
        }

        fn state(mut self, state: AgentState) -> Self {
            self.state = state;
            self
        }

        fn build(self) -> TabInfo {
            TabInfo {
                tab_id: format!("tab-{}", self.cwd),
                cwd: self.cwd,
                process: self.process,
                agent: self.agent,
                state: self.state,
                state_since: 0,
                subagents: Vec::new(),
                usage: None,
                location: None,
                paused: false,
            }
        }
    }

    /// Test Data Builder : la question, ses onglets, et ce qu'elle a demandé jusqu'ici.
    struct Asked {
        question: QuitQuestion,
        gate: Arc<QuitGate>,
        asked: Arc<Mutex<Vec<Vec<TabInfo>>>>,
    }

    struct Tabs(Vec<TabInfo>);

    impl ObservedTabs for Tabs {
        fn tabs(&self) -> Vec<TabInfo> {
            self.0.clone()
        }
    }

    impl Asked {
        fn over(tabs: Vec<TabInfo>) -> Self {
            let asked = Arc::new(Mutex::new(Vec::new()));
            let recording = Arc::clone(&asked);
            let gate = Arc::new(QuitGate::default());
            let question = QuitQuestion::new(
                Arc::new(Tabs(tabs)),
                Arc::clone(&gate),
                Box::new(move |running| {
                    recording.lock().unwrap().push(running.to_vec());
                }),
            );
            Self {
                question,
                gate,
                asked,
            }
        }

        /// Les chemins nommés dans chaque question posée.
        fn questions(&self) -> Vec<Vec<String>> {
            self.asked
                .lock()
                .unwrap()
                .iter()
                .map(|running| running.iter().map(|tab| tab.cwd.clone()).collect())
                .collect()
        }
    }

    #[test]
    fn given_a_tab_where_an_agent_is_recognized_when_ash_is_asked_to_quit_then_it_stays_and_names_that_tab(
    ) {
        // Given
        let asked = Asked::over(vec![TabBuilder::a_shell()
            .recognized("claude")
            .at("/wt/ash-sidebar")
            .state(AgentState::Working)
            .build()]);

        // When
        let may_leave = asked.question.may_leave();

        // Then
        assert!(!may_leave);
        assert_eq!(asked.questions(), vec![vec!["/wt/ash-sidebar".to_owned()]]);
    }

    #[test]
    fn given_only_tabs_without_a_recognized_agent_when_ash_is_asked_to_quit_then_it_leaves_without_asking(
    ) {
        // Given — le critère est l'agent, pas le processus en avant-plan : un `vim` et un
        // `tail -f` sont des choses qu'on ferme tous les jours en quittant son terminal
        let asked = Asked::over(vec![
            TabBuilder::a_shell().running("vim").build(),
            TabBuilder::a_shell().running("tail").at("/tmp").build(),
        ]);

        // When
        let may_leave = asked.question.may_leave();

        // Then
        assert!(may_leave);
        assert_eq!(asked.questions(), Vec::<Vec<String>>::new());
    }

    #[test]
    fn given_two_tabs_carrying_an_agent_and_one_without_when_ash_is_asked_to_quit_then_only_the_two_are_named(
    ) {
        // Given
        let asked = Asked::over(vec![
            TabBuilder::a_shell()
                .recognized("claude")
                .at("/wt/ash-177")
                .state(AgentState::Working)
                .build(),
            TabBuilder::a_shell().running("vim").at("/tmp").build(),
            TabBuilder::a_shell()
                .recognized("claude")
                .at("/dev/ash")
                .state(AgentState::Waiting)
                .build(),
        ]);

        // When
        let may_leave = asked.question.may_leave();

        // Then — l'ordre est celui des onglets, et le `vim` n'est pas de la liste
        assert!(!may_leave);
        assert_eq!(
            asked.questions(),
            vec![vec!["/wt/ash-177".to_owned(), "/dev/ash".to_owned()]]
        );
    }

    #[test]
    fn given_the_user_answered_quit_anyway_when_the_exit_comes_back_round_then_ash_leaves_without_asking_again(
    ) {
        // Given — la réponse « quitter quand même » ne quitte pas elle-même : elle demande à
        // Tauri de quitter, ce qui repasse par ici
        let asked = Asked::over(vec![TabBuilder::a_shell().recognized("claude").build()]);
        asked.gate.open();

        // When
        let may_leave = asked.question.may_leave();

        // Then
        assert!(may_leave);
        assert_eq!(asked.questions(), Vec::<Vec<String>>::new());
    }

    #[test]
    fn given_a_pass_that_was_already_used_when_ash_is_asked_to_quit_again_then_the_question_comes_back(
    ) {
        // Given — un laissez-passer qui resterait ouvert ferait qu'un second `⌘Q` partirait
        // sans rien demander : c'est la panne que cette tranche corrige
        let asked = Asked::over(vec![TabBuilder::a_shell().recognized("claude").build()]);
        asked.gate.open();
        assert!(asked.question.may_leave());

        // When
        let may_leave = asked.question.may_leave();

        // Then
        assert!(!may_leave);
        assert_eq!(asked.questions().len(), 1);
    }

    #[test]
    fn given_no_tab_at_all_when_ash_is_asked_to_quit_then_it_leaves_without_asking() {
        // Given
        let asked = Asked::over(Vec::new());

        // When
        let may_leave = asked.question.may_leave();

        // Then
        assert!(may_leave);
        assert_eq!(asked.questions(), Vec::<Vec<String>>::new());
    }
}
