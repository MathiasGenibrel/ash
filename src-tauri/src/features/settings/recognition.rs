//! **La conciliation** : la table embarquée d'ADR-0006, les entrées déclarées à la main, et
//! ce que la configuration d'un outil reconnu porte.
//!
//! Elle vit ici parce que la feature possède « les commandes reconnues » de la spec §9 : la
//! table des outils connus appartient à `agents`, la déclaration appartient à `settings`, et
//! la règle de précédence entre les deux ne peut vivre que chez celui qui tient la seconde.
//! `pty` pose la question par le port qu'il possède
//! ([`AgentRecognition`](crate::features::pty::AgentRecognition)) ; il ne connaît ni la table
//! ni le registre.
//!
//! ## Ce que ce module ne fait pas
//!
//! **Il n'écrit rien, et ne demande aucune autorisation.** Reconnaître est de la lecture
//! (ADR-0006) ; instrumenter est une écriture chez l'utilisateur, et reste un geste explicite
//! qui passe par le flux qui existe déjà — vérification, sauvegarde `.bak`, entrées marquées,
//! diff à relire ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ## Pourquoi une mémoire courte
//!
//! La question est posée **à chaque passe de la boucle de sonde, pour chaque onglet** — trois
//! fois par seconde. Savoir si un `settings.json` porte le marqueur d'Ash demande de le lire :
//! le faire à chaque passe rouvrirait le même fichier des milliers de fois par heure pour une
//! réponse qui ne change que lorsque l'utilisateur clique dans la fenêtre de réglages.
//!
//! La mémoire est donc **courte** et non permanente : le fichier se modifie aussi depuis un
//! éditeur, et un état de hooks retenu pour toujours serait exactement la vérité périmée sur
//! laquelle on finirait par écrire. [`FRESHNESS`] est le compromis, et l'horloge est injectée
//! — une règle qui parle de secondes se prouve sans en dormir une seule.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::hooks::BlockAt;
use super::registry::{instrumented, ToolRegistry};
use crate::features::agents::{recognize, DeclaredProvider, ProgramIdentity, RecognizedAgent};
use crate::features::pty::AgentRecognition;
use crate::shared::time::Clock;

/// Combien de temps l'état d'instrumentation d'un dossier est réputé encore vrai.
///
/// Assez court pour qu'un bloc posé à la main dans un éditeur se voie sans relancer Ash,
/// assez long pour que la boucle de sonde n'ouvre pas un fichier par passe et par onglet.
pub const FRESHNESS: Duration = Duration::from_secs(5);

/// Ce que `pty` interroge : la table, les déclarations, et le disque — dans cet ordre.
pub struct ToolRecognition {
    tools: Arc<ToolRegistry>,
    clock: Arc<dyn Clock>,
    /// Ce qu'on a lu du disque, par outil, avec l'instant de la lecture.
    ///
    /// La lecture **entière** et non son résumé : la sidebar n'en veut que trois valeurs,
    /// la fenêtre de réglages en tire les cinq états d'une suggestion (voir
    /// [`super::suggestions`]). Deux mémoires pour un même fichier finiraient par ne pas
    /// dire la même chose de lui à la même seconde.
    seen: Mutex<HashMap<String, (Option<BlockAt>, Instant)>>,
}

impl ToolRecognition {
    pub fn new(tools: Arc<ToolRegistry>, clock: Arc<dyn Clock>) -> Self {
        Self {
            tools,
            clock,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Ce que la configuration d'un outil porte, **relu au plus une fois par [`FRESHNESS`]**.
    ///
    /// C'est le portillon unique devant le disque, et il sert les deux lecteurs : la boucle
    /// de sonde, qui pose la question trois fois par seconde et par onglet, et la fenêtre de
    /// réglages, qui la pose une fois par suggestion en s'affichant. Le second profite donc
    /// de la lecture du premier — ouvrir la fenêtre ne rouvre en général aucun fichier.
    ///
    /// Un registre empoisonné se lit comme « on n'en sait rien » : la reconnaissance d'un
    /// outil ne doit pas dépendre de la santé d'un verrou de la fenêtre de réglages.
    pub fn presence(&self, adapter: &str, config: Option<&str>) -> Option<BlockAt> {
        let key = format!("{adapter}\u{0}{}", config.unwrap_or_default());
        let now = self.clock.now();

        if let Ok(seen) = self.seen.lock() {
            if let Some((known, at)) = seen.get(&key) {
                if now.duration_since(*at) < FRESHNESS {
                    return known.clone();
                }
            }
        }

        let found = self.tools.presence(adapter, config);
        if let Ok(mut seen) = self.seen.lock() {
            seen.insert(key, (found.clone(), now));
        }
        found
    }
}

impl AgentRecognition for ToolRecognition {
    fn recognize(&self, program: &ProgramIdentity) -> Option<RecognizedAgent> {
        // Les entrées de l'utilisateur d'abord : elles l'emportent sur la table embarquée,
        // et c'est ainsi qu'on corrige un outil qu'Ash connaît mal ou qu'on en ajoute un
        // qu'il ne connaît pas (ADR-0006). Rien n'est lu sur le disque à ce stade.
        let declarations = self.tools.declarations().unwrap_or_default();
        let declared: Vec<DeclaredProvider> = declarations
            .iter()
            .map(|tool| DeclaredProvider {
                command: tool.command.as_str().to_owned(),
                adapter: tool.adapter.clone(),
            })
            .collect();

        let found = recognize(program, &declared)?;
        // Le dossier de configuration est celui de l'entrée quand elle en nomme un, et celui
        // de l'adaptateur sinon — la même règle que partout ailleurs dans la feature.
        let config = declarations
            .iter()
            .find(|tool| tool.command.as_str() == found.command)
            .and_then(|tool| tool.config.clone());

        Some(RecognizedAgent {
            instrumented: instrumented(self.presence(&found.adapter, config.as_deref()).as_ref()),
            command: found.command,
            adapter: found.adapter,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::features::agents::Instrumented;
    use crate::features::hooks::Presence;
    use crate::features::settings::fakes::{
        FakeBlocks, FakeCommands, FakeFolders, FakeToolStore, TestClock,
    };
    use crate::features::settings::persisted::PersistedTool;
    use crate::features::settings::store::ToolStore;
    use crate::features::settings::tool::NewTool;
    use crate::features::settings::verification::{AdapterProfile, Verifier};

    fn profiles() -> Vec<AdapterProfile> {
        vec![
            AdapterProfile {
                id: "generic".to_owned(),
                default_config: None,
                signature: Vec::new(),
                config_env: None,
                probe_args: vec!["--version".to_owned()],
            },
            AdapterProfile {
                id: "claude-code".to_owned(),
                default_config: Some("~/.claude".to_owned()),
                signature: vec!["projects".to_owned()],
                config_env: Some("CLAUDE_CONFIG_DIR".to_owned()),
                probe_args: vec!["--version".to_owned()],
            },
        ]
    }

    /// Test Data Builder : la conciliation, avec un disque décrit par le scénario.
    struct RecognitionBuilder {
        blocks: FakeBlocks,
        declared: Vec<(&'static str, &'static str)>,
        stored: Vec<PersistedTool>,
    }

    impl RecognitionBuilder {
        fn new() -> Self {
            Self {
                blocks: FakeBlocks::new().without_hooks("generic"),
                declared: Vec::new(),
                stored: Vec::new(),
            }
        }

        /// Ce dossier porte le marqueur `# ash:hook v` d'Ash.
        fn instrumented(mut self, folder: &str) -> Self {
            self.blocks = self.blocks.at(folder, Presence::Current { version: 1 });
            self
        }

        /// Ce dossier existe, et ne porte rien d'Ash.
        fn untouched(mut self, folder: &str) -> Self {
            self.blocks = self.blocks.at(
                folder,
                Presence::Missing {
                    others: 0,
                    diff: String::new(),
                },
            );
            self
        }

        /// Une entrée déclarée dans la fenêtre de réglages, pendant cette session.
        fn declaring(mut self, command: &'static str, adapter: &'static str) -> Self {
            self.declared.push((command, adapter));
            self
        }

        /// Une entrée que `~/.ash/tools.json` porte **avant** qu'Ash ne démarre — celle de
        /// la session précédente, ou celle qu'une main a écrite (spec §9).
        fn stored(mut self, command: &str, adapter: &str) -> Self {
            self.stored
                .push(FakeToolStore::entry(command, adapter, None));
            self
        }

        fn build(self) -> (ToolRecognition, Arc<TestClock>) {
            let folders = FakeFolders::new("/Users/ash")
                .folder("/Users/ash/.claude", &["projects"])
                .folder("/Users/ash/.claude-perso", &["projects"]);
            let verifier = Arc::new(Verifier::new(
                Arc::new(folders),
                Arc::new(FakeCommands::new()),
                profiles(),
            ));
            let tools = Arc::new(ToolRegistry::restore(
                verifier,
                Arc::new(self.blocks),
                Arc::new(FakeToolStore::carrying(self.stored)) as Arc<dyn ToolStore>,
            ));
            for (command, adapter) in self.declared {
                tools
                    .declare(NewTool {
                        command: command.to_owned(),
                        label: None,
                        adapter: adapter.to_owned(),
                        config: None,
                    })
                    .expect("la saisie du scénario est valide");
            }
            let clock = Arc::new(TestClock::new());
            (
                ToolRecognition::new(tools, Arc::clone(&clock) as Arc<dyn Clock>),
                clock,
            )
        }
    }

    fn claude_binary() -> ProgramIdentity {
        ProgramIdentity {
            executable: PathBuf::from("/Users/ash/.local/share/claude/versions/2.1.234"),
            name: "2.1.234".to_owned(),
            argv0: Some("claude".to_owned()),
        }
    }

    #[test]
    fn given_a_tool_declared_in_an_earlier_session_when_a_tab_runs_it_then_it_is_recognized_without_opening_the_window(
    ) {
        // Given — la reconnaissance est posée à chaque passe de la boucle de sonde, dès le
        // premier onglet, et la fenêtre de réglages n'est qu'un des lecteurs du registre
        // (ADR-0006/0009). Une déclaration qui n'arriverait au registre qu'à l'ouverture de
        // la fenêtre laisserait un outil déclaré la veille inconnu tant que personne n'a
        // cliqué. `kimi-mien` n'est dans aucune table embarquée : seul le fichier le nomme
        let (recognition, _) = RecognitionBuilder::new()
            .stored("kimi-mien", "generic")
            .build();
        let mine = ProgramIdentity {
            executable: PathBuf::from("/opt/bin/kimi-mien"),
            name: "kimi-mien".to_owned(),
            argv0: Some("kimi-mien".to_owned()),
        };

        // When
        let found = recognition.recognize(&mine);

        // Then
        assert_eq!(
            found.map(|agent| (agent.command, agent.adapter)),
            Some(("kimi-mien".to_owned(), "generic".to_owned()))
        );
    }

    #[test]
    fn given_a_recognized_tool_whose_config_carries_no_ash_marker_when_a_tab_runs_it_then_it_is_reported_as_not_instrumented(
    ) {
        // Given — c'est ce qui explique qu'un agent reconnu ne montre jamais `waiting`
        // (ADR-0007). Sans le dire, son absence se lirait comme une panne
        let (recognition, _) = RecognitionBuilder::new()
            .untouched("/Users/ash/.claude")
            .build();

        // When
        let found = recognition.recognize(&claude_binary());

        // Then
        assert_eq!(
            found,
            Some(RecognizedAgent {
                command: "claude".to_owned(),
                adapter: "claude-code".to_owned(),
                instrumented: Instrumented::Missing,
            })
        );
    }

    #[test]
    fn given_a_recognized_tool_whose_config_carries_the_marker_when_a_tab_runs_it_then_nothing_is_signalled(
    ) {
        // Given — le cas nominal : les hooks sont posés, l'onglet montrera les cinq états
        let (recognition, _) = RecognitionBuilder::new()
            .instrumented("/Users/ash/.claude")
            .build();

        // When
        let found = recognition.recognize(&claude_binary());

        // Then
        assert_eq!(
            found.map(|agent| agent.instrumented),
            Some(Instrumented::Installed)
        );
    }

    #[test]
    fn given_a_tool_no_adapter_can_instrument_when_a_tab_runs_it_then_it_says_so_instead_of_offering_a_gesture(
    ) {
        // Given — `kimi` est reconnu par son nom, et `generic` ne pose aucun hook (ADR-0008).
        // « rien n'est posé » et « rien ne peut l'être » ne se corrigent pas de la même façon
        let (recognition, _) = RecognitionBuilder::new().build();
        let kimi = ProgramIdentity {
            executable: PathBuf::from("/Users/ash/.kimi-code/bin/kimi"),
            name: "kimi".to_owned(),
            argv0: None,
        };

        // When
        let found = recognition.recognize(&kimi);

        // Then
        assert_eq!(
            found,
            Some(RecognizedAgent {
                command: "kimi".to_owned(),
                adapter: "generic".to_owned(),
                instrumented: Instrumented::Unsupported,
            })
        );
    }

    #[test]
    fn given_a_tool_both_embedded_and_declared_by_hand_when_a_tab_runs_it_then_the_declaration_answers_alone(
    ) {
        // Given — `claude` est dans la table **et** déclaré à la main sur un autre
        // adaptateur. Deux réponses feraient apparaître le même outil deux fois
        let (recognition, _) = RecognitionBuilder::new()
            .declaring("claude", "generic")
            .instrumented("/Users/ash/.claude")
            .build();

        // When
        let found = recognition.recognize(&claude_binary());

        // Then — une seule réponse, et c'est celle de l'utilisateur
        assert_eq!(
            found,
            Some(RecognizedAgent {
                command: "claude".to_owned(),
                adapter: "generic".to_owned(),
                instrumented: Instrumented::Unsupported,
            })
        );
    }

    #[test]
    fn given_a_tab_probed_three_times_a_second_when_the_same_tool_keeps_the_foreground_then_the_configuration_is_read_once(
    ) {
        // Given — la boucle d'ADR-0005 pose cette question à chaque passe et pour chaque
        // onglet. Sans mémoire, le `settings.json` de l'utilisateur serait ouvert des
        // milliers de fois par heure pour une réponse qui ne bouge pas
        let (recognition, clock) = RecognitionBuilder::new()
            .untouched("/Users/ash/.claude")
            .build();

        // When — une seconde de sonde, puis bien après la fraîcheur
        let passes: Vec<Option<RecognizedAgent>> = (0..3)
            .map(|_| recognition.recognize(&claude_binary()))
            .collect();
        clock.tick(FRESHNESS.as_secs() + 1);
        let later = recognition.recognize(&claude_binary());

        // Then — la réponse est stable, donc la fiche d'onglet aussi : rien ne réveille la
        // sidebar, et le fichier n'a été relu qu'après expiration
        assert!(passes.iter().all(|pass| *pass == passes[0]));
        assert_eq!(later, passes[0]);
    }

    #[test]
    fn given_an_ordinary_program_in_the_foreground_when_it_is_examined_then_no_configuration_is_read(
    ) {
        // Given — un `vim` ne doit rien déclencher : ni une entrée dans la sidebar, ni la
        // moindre lecture du dossier de configuration d'un outil (ADR-0006)
        let (recognition, _) = RecognitionBuilder::new()
            .untouched("/Users/ash/.claude")
            .build();
        let editor = ProgramIdentity {
            executable: PathBuf::from("/usr/bin/vim"),
            name: "vim".to_owned(),
            argv0: None,
        };

        // When
        let found = recognition.recognize(&editor);

        // Then
        assert_eq!(found, None);
    }
}
