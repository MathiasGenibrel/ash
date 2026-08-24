//! **Les outils qu'Ash a vus tourner et que personne n'a déclarés** — la section `tools`
//! sous les cartes.
//!
//! La fenêtre ouvrait sur « no tools declared » pendant qu'Ash savait très bien que `claude`
//! tenait l'avant-plan de trois onglets : la reconnaissance d'ADR-0006 ne servait qu'au
//! marqueur discret de la sidebar, et il fallait deviner qu'on passait par là pour déclarer
//! un outil. Ce module est le second lecteur de cette même connaissance.
//!
//! ## Ce qu'il ne fait pas
//!
//! **Il ne découvre rien.** La source est le port [`RunningTools`], c'est-à-dire ce que la
//! sonde a reconnu dans l'avant-plan des onglets ouverts — pas un parcours du `PATH`, pas un
//! scan de disque, pas une autorisation macOS (ADR-0006). Un outil installé mais jamais lancé
//! n'apparaît pas, et c'est assumé : l'ajout à la main reste là pour lui.
//!
//! **Il n'écrit rien, et ne vérifie rien.** Une suggestion n'est pas une entrée : elle n'a
//! pas de vérification — les tests 3 et 4 de la spec §9.1 parcourent le `PATH` et lancent la
//! commande, et ouvrir une fenêtre ne doit faire ni l'un ni l'autre. Son unique geste est de
//! se déclarer, ce qui la fait rejoindre les cartes et repartir dans le flux qui existe
//! déjà — vérification en deux temps, puis bouton d'installation
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! ## Ce qu'il lit, et combien de fois
//!
//! Un fichier de configuration par outil suggéré, et **au plus une fois par
//! [`FRESHNESS`](super::FRESHNESS)** : la lecture passe par la mémoire courte de
//! [`ToolRecognition`], celle-là même que la boucle de sonde alimente trois fois par
//! seconde. Ouvrir la fenêtre ne rouvre donc en général aucun fichier — il vient d'être lu.

use std::sync::Arc;

use super::hooks::{foreseen, HookState};
use super::ports::RunningTools;
use super::recognition::ToolRecognition;
use super::registry::ToolRegistry;
use crate::features::agents::RecognizedProvider;

/// Un outil qu'Ash a vu tourner, et que la fenêtre propose de déclarer d'un clic.
///
/// Elle porte **ce que la ligne montre**, et rien qu'elle ne montre : le nom, l'adaptateur
/// que la table lui donne, l'état de sa configuration et la phrase qui l'explique. Pas
/// d'action, pas de diff, pas de bouton allumé — un outil non déclaré n'ouvre aucun droit
/// d'écriture, et un `HooksReport` entier ferait voyager un `install` qu'aucun geste de
/// cette ligne ne déclenche ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ToolSuggestion {
    /// Le nom de l'outil — `claude`, et non `2.1.234` (ADR-0006).
    pub command: String,
    /// L'identifiant de l'adaptateur que la table lui donne.
    pub adapter: String,
    /// Ce que sa configuration porte, dans les **cinq** états de la ligne `hooks`.
    ///
    /// Cinq et non les trois d'`Instrumented` : un conflit ne se corrige pas comme une
    /// absence, et les confondre ferait lire une panne là où l'utilisateur a simplement ses
    /// propres hooks (ADR-0007).
    pub hooks: HookState,
    /// La phrase de la ligne — `no ash hooks in this file`, `1 hook here is not ash's`…
    pub summary: String,
    /// Le fichier lu, quand il y en a un.
    pub file: Option<String>,
}

/// La conciliation « ce qui tourne » / « ce qui est déclaré », et la lecture qui l'accompagne.
///
/// Elle n'est pas dans [`ToolRecognition`] alors qu'elle en consomme la mémoire, et c'est le
/// composition root qui l'explique : la reconnaissance est **construite avant** le registre
/// de PTY, qui la reçoit par son port ; le port qui rend les outils en cours, lui, ne peut
/// être branché qu'**après**. Les deux dans un même objet demanderaient une liaison tardive,
/// donc un état mutable dans la composition root pour éviter un cycle qui n'existe pas.
pub struct ToolSuggestions {
    tools: Arc<ToolRegistry>,
    recognition: Arc<ToolRecognition>,
    running: Arc<dyn RunningTools>,
}

impl ToolSuggestions {
    pub fn new(
        tools: Arc<ToolRegistry>,
        recognition: Arc<ToolRecognition>,
        running: Arc<dyn RunningTools>,
    ) -> Self {
        Self {
            tools,
            recognition,
            running,
        }
    }

    /// Ce que la section `tools` propose sous les cartes.
    ///
    /// Un registre empoisonné rend **une liste vide**, et non la liste de ce qui tourne :
    /// sans les déclarations, on proposerait de déclarer un outil qui l'est déjà, et l'ajout
    /// serait refusé devant l'utilisateur. Ne rien proposer vaut mieux que proposer un
    /// doublon.
    pub fn suggest(&self) -> Vec<ToolSuggestion> {
        let Ok(declared) = self.tools.declarations() else {
            return Vec::new();
        };
        self.running
            .running()
            .into_iter()
            .filter(|found| {
                !declared
                    .iter()
                    .any(|tool| tool.command.as_str() == found.command)
            })
            .map(|found| self.describe(&found))
            .collect()
    }

    /// La ligne d'une suggestion, lue sur le **dossier par défaut de son adaptateur**.
    ///
    /// Par défaut, parce qu'une suggestion n'a pas d'entrée, donc aucun dossier désigné : le
    /// seul que quelqu'un connaisse est celui que l'adaptateur nomme, et c'est aussi celui
    /// que la déclaration visera. Un adaptateur qui n'en nomme aucun, ou qui n'instrumente
    /// rien, sort par la même porte — `blocked`, avec sa raison.
    fn describe(&self, found: &RecognizedProvider) -> ToolSuggestion {
        let line = foreseen(
            &found.adapter,
            self.recognition.presence(&found.adapter, None),
        );
        ToolSuggestion {
            command: found.command.clone(),
            adapter: found.adapter.clone(),
            hooks: line.state,
            summary: line.summary,
            file: line.file,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::features::hooks::Presence;
    use crate::features::settings::fakes::{
        FakeBlocks, FakeCommands, FakeFolders, FakeRunning, FakeToolStore, TestClock,
    };
    use crate::features::settings::ports::HookBlocks;
    use crate::features::settings::store::ToolStore;
    use crate::features::settings::tool::NewTool;
    use crate::features::settings::values::ConfigTarget;
    use crate::features::settings::verification::{AdapterProfile, Verifier};
    use crate::features::settings::{BlockAt, FRESHNESS};
    use crate::shared::time::Clock;

    /// Le port des blocs, **et le compte des fichiers ouverts** : c'est le budget de lecture
    /// qui est la règle, et un commentaire ne le garde pas.
    struct CountingBlocks {
        inner: FakeBlocks,
        reads: AtomicUsize,
        /// Les dossiers réellement ouverts, dans l'ordre — de quoi affirmer qu'aucun autre
        /// ne l'a été.
        opened: Mutex<Vec<String>>,
    }

    impl CountingBlocks {
        fn around(inner: FakeBlocks) -> Self {
            Self {
                inner,
                reads: AtomicUsize::new(0),
                opened: Mutex::new(Vec::new()),
            }
        }

        fn reads(&self) -> usize {
            self.reads.load(Ordering::SeqCst)
        }

        fn opened(&self) -> Vec<String> {
            self.opened.lock().expect("le verrou du double").clone()
        }
    }

    impl HookBlocks for CountingBlocks {
        fn inspect(&self, adapter: &str, config_dir: &ConfigTarget) -> Option<BlockAt> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut opened) = self.opened.lock() {
                opened.push(config_dir.resolved().display().to_string());
            }
            self.inner.inspect(adapter, config_dir)
        }

        fn install(&self, adapter: &str, config_dir: &ConfigTarget) -> Result<(), String> {
            self.inner.install(adapter, config_dir)
        }

        fn remove(
            &self,
            adapter: &str,
            config_dir: &ConfigTarget,
        ) -> Result<crate::features::hooks::Removal, String> {
            self.inner.remove(adapter, config_dir)
        }

        fn foresee_removal(
            &self,
            adapter: &str,
            config_dir: &ConfigTarget,
        ) -> Option<crate::features::hooks::Withdrawal> {
            self.inner.foresee_removal(adapter, config_dir)
        }
    }

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

    /// Test Data Builder : ce qui tourne, ce qui est déclaré, et ce que le disque porte.
    struct SuggestionsBuilder {
        blocks: FakeBlocks,
        running: Vec<RecognizedProvider>,
        declared: Vec<(&'static str, &'static str)>,
    }

    impl SuggestionsBuilder {
        fn new() -> Self {
            Self {
                blocks: FakeBlocks::new().without_hooks("generic"),
                running: Vec::new(),
                declared: Vec::new(),
            }
        }

        /// Un onglet où cet outil tient l'avant-plan.
        fn running(mut self, command: &str, adapter: &str) -> Self {
            self.running.push(RecognizedProvider {
                command: command.to_owned(),
                adapter: adapter.to_owned(),
            });
            self
        }

        /// Une entrée déjà déclarée dans la fenêtre de réglages.
        fn declaring(mut self, command: &'static str, adapter: &'static str) -> Self {
            self.declared.push((command, adapter));
            self
        }

        /// Ce que ce dossier de configuration porte.
        fn carrying(mut self, folder: &str, presence: Presence) -> Self {
            self.blocks = self.blocks.at(folder, presence);
            self
        }

        fn build(self) -> (ToolSuggestions, Arc<CountingBlocks>, Arc<TestClock>) {
            let folders =
                FakeFolders::new("/Users/ash").folder("/Users/ash/.claude", &["projects"]);
            let verifier = Arc::new(Verifier::new(
                Arc::new(folders),
                Arc::new(FakeCommands::new()),
                profiles(),
            ));
            let blocks = Arc::new(CountingBlocks::around(self.blocks));
            let tools = Arc::new(ToolRegistry::restore(
                verifier,
                Arc::clone(&blocks) as Arc<dyn HookBlocks>,
                Arc::new(FakeToolStore::empty()) as Arc<dyn ToolStore>,
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
            let recognition = Arc::new(ToolRecognition::new(
                Arc::clone(&tools),
                Arc::clone(&clock) as Arc<dyn Clock>,
            ));
            let suggestions = ToolSuggestions::new(
                tools,
                recognition,
                Arc::new(FakeRunning::seeing(self.running)) as Arc<dyn RunningTools>,
            );
            (suggestions, blocks, clock)
        }
    }

    /// Un fichier qui ne porte rien d'Ash, et `others` hooks de l'utilisateur.
    fn nothing_of_ash(others: usize) -> Presence {
        Presence::Missing {
            others,
            diff: "+ ash-event".to_owned(),
        }
    }

    #[test]
    fn given_a_tab_running_a_tool_of_the_embedded_table_when_the_window_asks_then_it_is_proposed_with_its_adapter(
    ) {
        // Given — Ash sait que `claude` tourne, et la fenêtre ouvrait quand même sur
        // « no tools declared ». La reconnaissance ne servait qu'au marqueur de la sidebar,
        // et il fallait deviner qu'on passait par là (ADR-0006)
        let (suggestions, _, _) = SuggestionsBuilder::new()
            .running("claude", "claude-code")
            .carrying("/Users/ash/.claude", nothing_of_ash(0))
            .build();

        // When
        let proposed = suggestions.suggest();

        // Then — et la ligne dit que les hooks sont absents, pas qu'ils sont refusés
        assert_eq!(
            proposed,
            vec![ToolSuggestion {
                command: "claude".to_owned(),
                adapter: "claude-code".to_owned(),
                hooks: HookState::Missing,
                summary: "no ash hooks in this file".to_owned(),
                file: Some("/Users/ash/.claude/settings.json".to_owned()),
            }]
        );
    }

    #[test]
    fn given_a_tool_already_declared_when_it_keeps_running_then_it_is_not_proposed_a_second_time() {
        // Given — la carte est déjà là, avec sa ligne `hooks` et son bouton. Le proposer
        // encore ferait apparaître deux fois le même outil dans l'écran, dont une fois sous
        // un geste que le backend refuserait
        let (suggestions, _, _) = SuggestionsBuilder::new()
            .running("claude", "claude-code")
            .declaring("claude", "claude-code")
            .carrying("/Users/ash/.claude", nothing_of_ash(0))
            .build();

        // When
        let proposed = suggestions.suggest();

        // Then
        assert!(proposed.is_empty());
    }

    #[test]
    fn given_a_suggested_tool_whose_file_carries_hooks_of_its_own_when_it_is_described_then_the_conflict_is_not_an_absence(
    ) {
        // Given — c'est le troisième critère de l'issue : les cinq états de `HookState`, pas
        // les trois d'`Instrumented`. Ce dernier n'a pas de `conflict`, et un utilisateur qui
        // outille déjà son agent lirait « rien n'est posé » là où quelque chose l'est
        let (suggestions, _, _) = SuggestionsBuilder::new()
            .running("claude", "claude-code")
            .carrying("/Users/ash/.claude", nothing_of_ash(2))
            .build();

        // When
        let proposed = suggestions.suggest();

        // Then
        assert_eq!(
            proposed.first().map(|one| (one.hooks, one.summary.clone())),
            Some((HookState::Conflict, "2 hooks here are not ash's".to_owned()))
        );
    }

    #[test]
    fn given_a_suggested_tool_whose_ash_block_was_edited_by_hand_when_it_is_described_then_it_says_conflict_too(
    ) {
        // Given — l'autre conflit : le marqueur `# ash:hook v` est là, et une main est passée
        // dessus. Ash ne réécrit jamais de lui-même ; il faut d'abord que la ligne le dise
        // (ADR-0007, amendement du 2026-08-12)
        let (suggestions, _, _) = SuggestionsBuilder::new()
            .running("claude", "claude-code")
            .carrying(
                "/Users/ash/.claude",
                Presence::HandEdited {
                    diff: "- moi\n+ ash".to_owned(),
                },
            )
            .build();

        // When
        let proposed = suggestions.suggest();

        // Then
        assert_eq!(
            proposed.first().map(|one| one.hooks),
            Some(HookState::Conflict)
        );
    }

    #[test]
    fn given_a_suggested_tool_no_adapter_can_instrument_when_it_is_described_then_it_says_so_instead_of_letting_a_failure_be_read(
    ) {
        // Given — `codex` est reconnu, et `generic` ne pose aucun hook (ADR-0008). Une ligne
        // muette se lirait comme une panne, et attendre un `waiting` qui n'arrivera jamais
        let (suggestions, _, _) = SuggestionsBuilder::new()
            .running("codex", "generic")
            .build();

        // When
        let proposed = suggestions.suggest();

        // Then
        assert_eq!(
            proposed,
            vec![ToolSuggestion {
                command: "codex".to_owned(),
                adapter: "generic".to_owned(),
                hooks: HookState::Blocked,
                summary: "the generic adapter has no hooks to install".to_owned(),
                file: None,
            }]
        );
    }

    #[test]
    fn given_a_window_reopened_within_the_freshness_window_when_it_asks_again_then_no_file_is_reopened(
    ) {
        // Given — le budget de lecture est une règle de l'issue, pas une intention : un
        // fichier de configuration par outil suggéré, au plus une fois par `FRESHNESS`.
        // C'est la mémoire courte de la reconnaissance qui le tient, celle-là même que la
        // boucle de sonde alimente trois fois par seconde
        let (suggestions, blocks, clock) = SuggestionsBuilder::new()
            .running("claude", "claude-code")
            .carrying("/Users/ash/.claude", nothing_of_ash(0))
            .build();

        // When — trois ouvertures rapprochées, puis une bien après la fraîcheur
        let first = suggestions.suggest();
        suggestions.suggest();
        suggestions.suggest();
        let within = blocks.reads();
        clock.tick(FRESHNESS.as_secs() + 1);
        let later = suggestions.suggest();

        // Then — une seule lecture pour les trois, et le seul dossier que l'adaptateur nomme
        assert_eq!(within, 1);
        assert_eq!(
            blocks.reads(),
            2,
            "la fraîcheur passée, le fichier est relu"
        );
        assert_eq!(
            blocks.opened(),
            ["/Users/ash/.claude", "/Users/ash/.claude"]
        );
        assert_eq!(later, first, "et la réponse ne bouge pas pour autant");
    }
}
