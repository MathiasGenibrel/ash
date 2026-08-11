//! La suite contractuelle du trait [`Adapter`] : ce que **toute** implémentation doit tenir.
//!
//! [ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md) promet que « les
//! particularités d'un outil ne peuvent pas fuir dans la sidebar ni dans le moteur d'états ».
//! Une promesse n'est vérifiable que si elle est exécutable : ce module est l'endroit où
//! elle l'est. Chaque implémentation — `generic` aujourd'hui, `claude-code` et les
//! suivantes demain — passe par [`check_adapter_contract`], et n'a plus à réécrire ces
//! invariants dans ses propres tests, seulement ses comportements propres.
//!
//! Le contrat ne teste pas des appels : il teste des **invariants**, sur un corpus
//! d'événements dont l'adaptateur ne choisit pas le contenu.

use std::path::{Component, Path};

use super::adapter::{Adapter, RawEvent};
use super::state::AgentState;

/// Ce que la vérification a trouvé. Vide = le contrat est tenu.
///
/// On rend un rapport plutôt que de paniquer au premier écart : une implémentation qui
/// démarre en viole souvent plusieurs, et les découvrir une par une coûte un cycle de
/// compilation à chaque fois.
#[derive(Debug, Default)]
pub(crate) struct ContractReport {
    pub violations: Vec<String>,
}

impl ContractReport {
    fn require(&mut self, holds: bool, invariant: &str) {
        if !holds {
            self.violations.push(invariant.to_owned());
        }
    }
}

/// Les noms d'événements qu'un adaptateur pourrait être tenté de reconnaître « au cas où ».
///
/// Ils sont donnés à **toutes** les implémentations, y compris à celles dont ce n'est pas
/// le vocabulaire : c'est ce qui rend vérifiable qu'un adaptateur sans instrumentation ne
/// produit rien, et qu'aucun adaptateur ne rend `idle` sur un mot qui y ressemble.
fn tempting_events() -> Vec<RawEvent> {
    [
        "",
        "Stop",
        "Notification",
        "PreToolUse",
        "PostToolUse",
        "SubagentStop",
        "SessionEnd",
        "idle",
        "working",
        "waiting",
        "done",
        "error",
        "ash:unknown",
    ]
    .into_iter()
    .map(RawEvent::new)
    .collect()
}

/// Vérifie les invariants que toute implémentation d'[`Adapter`] doit tenir.
///
/// `own_events` est le vocabulaire propre de l'outil — les événements que son
/// instrumentation fera réellement remonter. Il est vide pour un adaptateur qui
/// n'instrumente rien.
pub(crate) fn check_adapter_contract(
    adapter: &dyn Adapter,
    own_events: &[RawEvent],
) -> ContractReport {
    let mut report = ContractReport::default();

    check_identity(adapter, &mut report);

    let corpus: Vec<RawEvent> = tempting_events()
        .into_iter()
        .chain(own_events.iter().cloned())
        .collect();
    check_interpretation(adapter, &corpus, &mut report);
    check_instrumentation(adapter, &mut report);

    report
}

/// L'identifiant est une clé : il indexe la configuration reconnue (ADR-0006) et
/// l'attribution d'un commit (ADR-0014). Un identifiant vide, majuscule, espacé ou calculé
/// à chaque appel casse silencieusement ces deux rattachements.
fn check_identity(adapter: &dyn Adapter, report: &mut ContractReport) {
    let id = adapter.id();

    report.require(!id.is_empty(), "id() ne doit pas être vide");
    report.require(
        id.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "id() doit être un slug ascii minuscule — il s'écrit dans des fichiers de configuration",
    );
    report.require(
        adapter.id() == id,
        "id() doit rendre la même valeur à chaque appel",
    );
}

/// Ce que `interpret` n'a pas le droit de faire, quel que soit l'outil.
fn check_interpretation(adapter: &dyn Adapter, corpus: &[RawEvent], report: &mut ContractReport) {
    let instruments = adapter
        .instrumentation(Path::new("/ash-contract/alpha"))
        .is_some();

    for event in corpus {
        let interpreted = adapter.interpret(event);

        report.require(
            interpreted == adapter.interpret(event),
            "interpret() doit être déterministe : un adaptateur ne retient pas d'état, \
             c'est la machine à états qui arbitre",
        );

        report.require(
            interpreted != Some(AgentState::Idle),
            "interpret() ne doit jamais rendre `idle` : c'est le mot de la sonde pour \
             « aucun agent ici », qu'aucun événement d'outil ne peut affirmer",
        );

        if !instruments {
            report.require(
                !matches!(
                    interpreted,
                    Some(AgentState::Working) | Some(AgentState::Waiting)
                ),
                "un adaptateur sans instrumentation ne doit rendre ni `working` ni \
                 `waiting` : ces deux états n'ont d'autre producteur que les hooks (ADR-0007)",
            );
        }
    }
}

/// Ce que `instrumentation` doit garantir avant que la feature `hooks` n'écrive chez
/// l'utilisateur.
fn check_instrumentation(adapter: &dyn Adapter, report: &mut ContractReport) {
    // Deux dossiers distincts : c'est le cas des deux comptes Claude d'ADR-0007, et le
    // seul moyen de voir un adaptateur qui aurait codé son chemin en dur.
    let alpha = Path::new("/ash-contract/alpha");
    let beta = Path::new("/ash-contract/beta");

    let for_alpha = adapter.instrumentation(alpha);
    let for_beta = adapter.instrumentation(beta);

    report.require(
        for_alpha.is_some() == for_beta.is_some(),
        "instrumentation() doit décrire une capacité de l'outil, pas dépendre du dossier \
         qu'on lui donne",
    );
    report.require(
        for_alpha == adapter.instrumentation(alpha),
        "instrumentation() doit être déterministe pour un même dossier",
    );

    let (Some(alpha_block), Some(beta_block)) = (for_alpha, for_beta) else {
        return;
    };

    for (config_dir, instrumentation) in [(alpha, &alpha_block), (beta, &beta_block)] {
        report.require(
            instrumentation.file.starts_with(config_dir)
                && !instrumentation
                    .file
                    .components()
                    .any(|component| component == Component::ParentDir),
            "instrumentation().file doit rester sous le dossier de configuration donné : \
             Ash écrit dans les fichiers de l'utilisateur, et la cible ne se négocie pas \
             (ADR-0007)",
        );
        report.require(
            !instrumentation.block.trim().is_empty(),
            "instrumentation().block ne doit pas être vide : `hooks` écrirait des \
             marqueurs autour de rien",
        );
        report.require(
            instrumentation.version >= 1,
            "instrumentation().version doit démarrer à 1 : la version 0 ne se distingue \
             pas d'un bloc sans version",
        );
    }

    report.require(
        alpha_block.file != beta_block.file,
        "instrumentation() doit instrumenter chaque dossier de configuration séparément — \
         deux comptes du même outil sont deux blocs (ADR-0007)",
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::features::agents::adapter::{Instrumentation, SubagentSupport};

    /// Un adaptateur de test, réglable défaut par défaut — c'est ce qui permet de vérifier
    /// que la suite contractuelle **attrape** ce qu'elle prétend attraper.
    #[derive(Default)]
    struct AdapterBuilder {
        id: Option<String>,
        instrumented_file: Option<PathBuf>,
        always: Option<AgentState>,
    }

    impl AdapterBuilder {
        fn new() -> Self {
            Self::default()
        }

        fn id(mut self, id: &str) -> Self {
            self.id = Some(id.to_owned());
            self
        }

        /// Le chemin est **absolu et fixe** : c'est la faute qu'on veut voir attraper.
        fn hardcoded_file(mut self, file: &str) -> Self {
            self.instrumented_file = Some(PathBuf::from(file));
            self
        }

        fn always_answering(mut self, state: AgentState) -> Self {
            self.always = Some(state);
            self
        }

        fn build(self) -> FakeAdapter {
            FakeAdapter {
                id: self.id.unwrap_or_else(|| "fake".to_owned()),
                instrumented_file: self.instrumented_file,
                always: self.always,
            }
        }
    }

    struct FakeAdapter {
        id: String,
        instrumented_file: Option<PathBuf>,
        always: Option<AgentState>,
    }

    impl Adapter for FakeAdapter {
        fn id(&self) -> &str {
            &self.id
        }

        fn instrumentation(&self, _config_dir: &Path) -> Option<Instrumentation> {
            self.instrumented_file.as_ref().map(|file| Instrumentation {
                file: file.clone(),
                block: "{}".to_owned(),
                version: 1,
            })
        }

        fn interpret(&self, _raw: &RawEvent) -> Option<AgentState> {
            self.always
        }

        fn subagents(&self) -> SubagentSupport {
            SubagentSupport::None
        }
    }

    #[test]
    fn given_an_adapter_without_instrumentation_that_claims_working_when_checked_then_the_contract_rejects_it(
    ) {
        // Given — l'heuristique qu'ADR-0007 écarte, écrite dans un adaptateur : sans hook
        // installé, il affirme quand même que l'agent travaille
        let guesser = AdapterBuilder::new()
            .always_answering(AgentState::Working)
            .build();

        // When
        let report = check_adapter_contract(&guesser, &[]);

        // Then
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.contains("sans instrumentation")),
            "violations : {:?}",
            report.violations
        );
    }

    #[test]
    fn given_an_adapter_that_answers_idle_when_checked_then_the_contract_rejects_it() {
        // Given — `idle` veut dire « aucun agent ici » ; un outil qui parle est la preuve
        // du contraire
        let confused = AdapterBuilder::new()
            .hardcoded_file("/ash-contract/alpha/settings.json")
            .always_answering(AgentState::Idle)
            .build();

        // When
        let report = check_adapter_contract(&confused, &[]);

        // Then
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.contains("`idle`")),
            "violations : {:?}",
            report.violations
        );
    }

    #[test]
    fn given_an_adapter_with_a_hardcoded_config_path_when_checked_then_the_contract_rejects_it() {
        // Given — le bug qui casse les deux comptes Claude d'ADR-0007 : le dossier reçu
        // est ignoré au profit d'un chemin en dur
        let hardcoded = AdapterBuilder::new()
            .hardcoded_file("/home/someone/.claude/settings.json")
            .build();

        // When
        let report = check_adapter_contract(&hardcoded, &[]);

        // Then — il sort du dossier donné, et il écrit au même endroit pour tous les comptes
        assert_eq!(
            report.violations.len(),
            3,
            "violations : {:?}",
            report.violations
        );
    }

    #[test]
    fn given_an_adapter_whose_id_is_not_a_slug_when_checked_then_the_contract_rejects_it() {
        // Given — un identifiant qui finira dans `~/.ash/config.toml` et dans le journal
        let shouted = AdapterBuilder::new().id("Claude Code").build();

        // When
        let report = check_adapter_contract(&shouted, &[]);

        // Then
        assert!(
            report
                .violations
                .iter()
                .any(|violation| violation.contains("slug")),
            "violations : {:?}",
            report.violations
        );
    }
}
