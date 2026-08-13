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

use super::adapter::{hook_mark, Adapter, RawEvent, SubagentSupport};
use super::state::AgentState;

/// Les invariants du contrat, un par un.
///
/// C'est une énumération et non une phrase parce que c'est l'identité d'un invariant qui
/// se cite — dans un test qui vérifie que la suite attrape bien ce qu'elle prétend
/// attraper, et demain dans le rapport que lira l'auteur d'un adaptateur. Une prose
/// reformulée cassait le test ; pire, elle pouvait le faire passer sur un autre invariant
/// dont le texte contenait par hasard le même mot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Invariant {
    IdIsNotEmpty,
    IdIsASlug,
    InterpretIsDeterministic,
    InterpretNeverAnswersIdle,
    ChildEventsNeverBecomeTabState,
    NoChildEventsWithoutSubagentSupport,
    SubagentSupportNamesAChildVerb,
    NoWorkingNorWaitingWithoutInstrumentation,
    InstrumentationIsACapability,
    InstrumentationIsDeterministic,
    InstrumentationStaysUnderTheConfigDir,
    InstrumentationDescribesAtLeastOneEntry,
    InstrumentationEntriesCarryTheirMark,
    InstrumentationEntriesNameWhereTheyGo,
    InstrumentationVersionStartsAtOne,
    InstrumentationIsPerConfigDir,
}

impl Invariant {
    /// La raison d'être de l'invariant — c'est ce que lit l'auteur d'un adaptateur le jour
    /// où il tombe dessus, et c'est là que se trouve la valeur pédagogique du contrat.
    fn reason(self) -> &'static str {
        match self {
            Self::IdIsNotEmpty => {
                "id() ne doit pas être vide : il indexe la configuration \
                 reconnue (ADR-0006) et l'attribution d'un commit (ADR-0014)"
            }
            Self::IdIsASlug => {
                "id() doit être un slug ascii minuscule — il s'écrit dans \
                 des fichiers de configuration"
            }
            Self::InterpretIsDeterministic => {
                "interpret() doit être déterministe : un adaptateur ne retient pas d'état, \
                 c'est la machine à états qui arbitre"
            }
            Self::InterpretNeverAnswersIdle => {
                "interpret() ne doit jamais rendre `idle` : c'est le mot de la sonde pour \
                 « aucun agent ici », qu'aucun événement d'outil ne peut affirmer"
            }
            Self::ChildEventsNeverBecomeTabState => {
                "un événement de sous-agent ne doit produire aucun état d'onglet : un enfant \
                 qui finit ne rend pas l'outil disponible, et le traduire serait la déduction \
                 qu'ADR-0007 refuse (amendement du 2026-08-13)"
            }
            Self::NoChildEventsWithoutSubagentSupport => {
                "un adaptateur qui répond `SubagentSupport::None` ne doit reconnaître aucun \
                 événement d'enfant : le cœur n'afficherait alors des lignes filles pour un \
                 outil qui a déclaré n'en avoir pas"
            }
            Self::SubagentSupportNamesAChildVerb => {
                "un adaptateur qui répond `SubagentSupport::Reported` doit reconnaître au \
                 moins un verbe d'enfant dans son propre vocabulaire : sans lui, il annonce \
                 au cœur des lignes filles qui n'arriveront jamais, et l'utilisateur \
                 attendrait une colonne qui ne se remplira pas (spec §6.5)"
            }
            Self::NoWorkingNorWaitingWithoutInstrumentation => {
                "un adaptateur sans instrumentation ne doit rendre ni `working` ni \
                 `waiting` : ces deux états n'ont d'autre producteur que les hooks (ADR-0007)"
            }
            Self::InstrumentationIsACapability => {
                "instrumentation() doit décrire une capacité de l'outil, pas dépendre du \
                 dossier qu'on lui donne"
            }
            Self::InstrumentationIsDeterministic => {
                "instrumentation() doit être déterministe pour un même dossier : un bloc \
                 qui porte un horodatage ou un nonce ferait réécrire le fichier de \
                 l'utilisateur à chaque démarrage"
            }
            Self::InstrumentationStaysUnderTheConfigDir => {
                "instrumentation().file doit rester sous le dossier de configuration \
                 donné : Ash écrit dans les fichiers de l'utilisateur, et la cible ne se \
                 négocie pas (ADR-0007)"
            }
            Self::InstrumentationDescribesAtLeastOneEntry => {
                "instrumentation() doit décrire au moins une entrée : sans elle, `hooks` \
                 écrirait chez l'utilisateur sans rien y poser"
            }
            Self::InstrumentationEntriesCarryTheirMark => {
                "chaque entrée doit porter le marqueur de sa version : c'est à lui seul \
                 qu'Ash reconnaît ce qui est à lui dans le fichier de l'utilisateur, donc \
                 ce qu'il a le droit de retirer (ADR-0007)"
            }
            Self::InstrumentationEntriesNameWhereTheyGo => {
                "chaque entrée doit nommer le chemin de clés qui mène à son tableau : \
                 `hooks` fusionne sans connaître un seul outil, et ne devine aucun chemin"
            }
            Self::InstrumentationVersionStartsAtOne => {
                "instrumentation().version doit démarrer à 1 : la version 0 ne se distingue \
                 pas d'un bloc sans version"
            }
            Self::InstrumentationIsPerConfigDir => {
                "instrumentation() doit instrumenter chaque dossier de configuration \
                 séparément — deux comptes du même outil sont deux blocs (ADR-0007)"
            }
        }
    }
}

impl std::fmt::Display for Invariant {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?} — {}", self.reason())
    }
}

/// Ce que la vérification a trouvé. Vide = le contrat est tenu.
///
/// On rend un rapport plutôt que de paniquer au premier écart : une implémentation qui
/// démarre en viole souvent plusieurs, et les découvrir une par une coûte un cycle de
/// compilation à chaque fois.
#[derive(Debug, Default)]
pub(crate) struct ContractReport {
    violations: Vec<Invariant>,
}

impl ContractReport {
    /// Un invariant violé ne se compte qu'une fois : le corpus le met à l'épreuve sur une
    /// dizaine d'événements, et treize copies de la même ligne ne disent rien de plus.
    fn require(&mut self, holds: bool, invariant: Invariant) {
        if !holds && !self.violations.contains(&invariant) {
            self.violations.push(invariant);
        }
    }

    pub(crate) fn is_satisfied(&self) -> bool {
        self.violations.is_empty()
    }

    pub(crate) fn violations(&self) -> &[Invariant] {
        &self.violations
    }
}

impl std::fmt::Display for ContractReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for violation in &self.violations {
            writeln!(formatter, "- {violation}")?;
        }
        Ok(())
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
        // Le verbe canonique du sixième hook, à côté du nom que Claude Code lui donne. Il
        // est ici pour la même raison que les autres : c'est **le** mot qu'un adaptateur
        // serait tenté de traduire en `done`, et l'amendement du 2026-08-13 à ADR-0007 dit
        // qu'il n'en est pas un.
        "subagent-stop",
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
/// l'attribution d'un commit (ADR-0014). Un identifiant vide, majuscule ou espacé casse
/// silencieusement ces deux rattachements.
fn check_identity(adapter: &dyn Adapter, report: &mut ContractReport) {
    let id = adapter.id();

    report.require(!id.is_empty(), Invariant::IdIsNotEmpty);
    report.require(
        id.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        Invariant::IdIsASlug,
    );
}

/// Ce que `interpret` n'a pas le droit de faire, quel que soit l'outil.
fn check_interpretation(adapter: &dyn Adapter, corpus: &[RawEvent], report: &mut ContractReport) {
    let instruments = adapter
        .instrumentation(Path::new("/ash-contract/alpha"))
        .is_some();

    // L'autre sens de [`Invariant::NoChildEventsWithoutSubagentSupport`], et il attrape la
    // faute inverse : un adaptateur qui **promet** des sous-tâches sans jamais nommer le verbe
    // qui en annonce une. La promesse est ce sur quoi le cœur se fonde pour dire qu'il ne
    // manque aucune ligne (spec §6.5) ; une promesse que rien ne peut tenir est pire qu'un
    // `None` honnête, parce qu'elle est indiscernable d'un outil qui n'a rien à montrer.
    report.require(
        adapter.subagents() != SubagentSupport::Reported
            || corpus
                .iter()
                .any(|event| adapter.child_event(event).is_some()),
        Invariant::SubagentSupportNamesAChildVerb,
    );

    for event in corpus {
        let interpreted = adapter.interpret(event);

        // Rejouer le même événement est ce qui attrape l'adaptateur qui se souvient — par
        // exemple celui qui dédoublonne un `Stop` et ne le traduit qu'une fois.
        report.require(
            interpreted == adapter.interpret(event),
            Invariant::InterpretIsDeterministic,
        );

        report.require(
            interpreted != Some(AgentState::Idle),
            Invariant::InterpretNeverAnswersIdle,
        );

        // Le garde-fou de l'amendement du 2026-08-13, rendu exécutable : les deux méthodes
        // lisent le même mot brut, et le même mot ne peut pas parler des deux à la fois.
        // C'est ce qui empêche un `SubagentStop` de repasser par la porte de l'état d'onglet
        // le jour où quelqu'un l'ajoutera « pour que la ligne se mette à jour ».
        let child = adapter.child_event(event);
        report.require(
            child.is_none() || interpreted.is_none(),
            Invariant::ChildEventsNeverBecomeTabState,
        );
        report.require(
            child.is_none() || adapter.subagents() == SubagentSupport::Reported,
            Invariant::NoChildEventsWithoutSubagentSupport,
        );

        if !instruments {
            report.require(
                !matches!(
                    interpreted,
                    Some(AgentState::Working) | Some(AgentState::Waiting)
                ),
                Invariant::NoWorkingNorWaitingWithoutInstrumentation,
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
        Invariant::InstrumentationIsACapability,
    );
    report.require(
        for_alpha == adapter.instrumentation(alpha),
        Invariant::InstrumentationIsDeterministic,
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
            Invariant::InstrumentationStaysUnderTheConfigDir,
        );
        report.require(
            !instrumentation.entries.is_empty(),
            Invariant::InstrumentationDescribesAtLeastOneEntry,
        );
        report.require(
            instrumentation
                .entries
                .iter()
                .all(|entry| entry.item.contains(&hook_mark(instrumentation.version))),
            Invariant::InstrumentationEntriesCarryTheirMark,
        );
        report.require(
            instrumentation.entries.iter().all(|entry| {
                !entry.path.is_empty() && entry.path.iter().all(|key| !key.is_empty())
            }),
            Invariant::InstrumentationEntriesNameWhereTheyGo,
        );
        report.require(
            instrumentation.version >= 1,
            Invariant::InstrumentationVersionStartsAtOne,
        );
    }

    report.require(
        alpha_block.file != beta_block.file,
        Invariant::InstrumentationIsPerConfigDir,
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::features::agents::adapter::{ChildEvent, HookEntry, Instrumentation};

    /// Un adaptateur de test, réglable défaut par défaut — c'est ce qui permet de vérifier
    /// que la suite contractuelle **attrape** ce qu'elle prétend attraper.
    #[derive(Default)]
    struct AdapterBuilder {
        id: Option<String>,
        instrumented_file: Option<PathBuf>,
        always: Option<AgentState>,
        child_verb: Option<String>,
        reports_subagents: bool,
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

        /// Ce mot-là annonce la fin d'un enfant.
        fn ending_children_on(mut self, verb: &str) -> Self {
            self.child_verb = Some(verb.to_owned());
            self
        }

        /// L'outil déclare avoir des sous-tâches.
        fn reporting_subagents(mut self) -> Self {
            self.reports_subagents = true;
            self
        }

        fn build(self) -> FakeAdapter {
            FakeAdapter {
                id: self.id.unwrap_or_else(|| "fake".to_owned()),
                instrumented_file: self.instrumented_file,
                always: self.always,
                child_verb: self.child_verb,
                reports_subagents: self.reports_subagents,
            }
        }
    }

    struct FakeAdapter {
        id: String,
        instrumented_file: Option<PathBuf>,
        always: Option<AgentState>,
        child_verb: Option<String>,
        reports_subagents: bool,
    }

    impl Adapter for FakeAdapter {
        fn id(&self) -> &str {
            &self.id
        }

        fn instrumentation(&self, _config_dir: &Path) -> Option<Instrumentation> {
            self.instrumented_file.as_ref().map(|file| Instrumentation {
                file: file.clone(),
                entries: vec![HookEntry {
                    path: vec!["hooks".to_owned(), "Stop".to_owned()],
                    item: format!("{{\"command\": \"fake {}\"}}", hook_mark(1)),
                }],
                version: 1,
            })
        }

        fn interpret(&self, _raw: &RawEvent) -> Option<AgentState> {
            self.always
        }

        fn child_event(&self, raw: &RawEvent) -> Option<ChildEvent> {
            (self.child_verb.as_deref() == Some(raw.kind())).then_some(ChildEvent::Ended)
        }

        fn subagents(&self) -> SubagentSupport {
            if self.reports_subagents {
                SubagentSupport::Reported
            } else {
                SubagentSupport::None
            }
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
                .violations()
                .contains(&Invariant::NoWorkingNorWaitingWithoutInstrumentation),
            "violations : {report}"
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
                .violations()
                .contains(&Invariant::InterpretNeverAnswersIdle),
            "violations : {report}"
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
            report.violations(),
            [
                Invariant::InstrumentationStaysUnderTheConfigDir,
                Invariant::InstrumentationIsPerConfigDir,
            ]
        );
    }

    #[test]
    fn given_an_adapter_that_turns_a_finished_subagent_into_a_tab_state_when_checked_then_the_contract_rejects_it(
    ) {
        // Given — la faute que l'amendement du 2026-08-13 nomme et interdit : traduire un
        // `SubagentStop` en état d'onglet. Un enfant qui finit ne rend pas `claude`
        // disponible ; l'onglet afficherait `done` pendant que l'agent principal travaille,
        // et la sidebar annoncerait un travail terminé qui ne l'est pas.
        let confused = AdapterBuilder::new()
            .hardcoded_file("/ash-contract/alpha/settings.json")
            .reporting_subagents()
            .ending_children_on("subagent-stop")
            .always_answering(AgentState::Done)
            .build();

        // When
        let report = check_adapter_contract(&confused, &[]);

        // Then
        assert!(
            report
                .violations()
                .contains(&Invariant::ChildEventsNeverBecomeTabState),
            "violations : {report}"
        );
    }

    #[test]
    fn given_an_adapter_that_reports_children_while_declaring_it_has_none_when_checked_then_the_contract_rejects_it(
    ) {
        // Given — `SubagentSupport` est ce sur quoi le cœur se fonde pour décider s'il peut
        // afficher des lignes filles (spec §6.5). Un adaptateur qui répond `None` et
        // reconnaît quand même un verbe d'enfant ferait apparaître des lignes sous un outil
        // qui a déclaré ne pas en avoir — donc suggérer qu'il en manque ailleurs.
        let inconsistent = AdapterBuilder::new()
            .hardcoded_file("/ash-contract/alpha/settings.json")
            .ending_children_on("subagent-stop")
            .build();

        // When
        let report = check_adapter_contract(&inconsistent, &[]);

        // Then
        assert!(
            report
                .violations()
                .contains(&Invariant::NoChildEventsWithoutSubagentSupport),
            "violations : {report}"
        );
    }

    #[test]
    fn given_an_adapter_that_promises_subagents_without_naming_the_verb_that_ends_one_when_checked_then_the_contract_rejects_it(
    ) {
        // Given — la faute exactement inverse de la précédente, et celle qui se commet le
        // plus facilement : on passe `subagents()` à `Reported` en préparant une tranche, et
        // l'on oublie le verbe. L'onglet promet alors des lignes filles que rien n'écrira
        // jamais, et un utilisateur qui n'en voit aucune ne peut pas distinguer « aucun
        // sous-agent ne tourne » de « Ash ne les entend pas » (spec §6.5).
        let boastful = AdapterBuilder::new()
            .hardcoded_file("/ash-contract/alpha/settings.json")
            .reporting_subagents()
            .build();

        // When
        let report = check_adapter_contract(&boastful, &[]);

        // Then
        assert!(
            report
                .violations()
                .contains(&Invariant::SubagentSupportNamesAChildVerb),
            "violations : {report}"
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
            report.violations().contains(&Invariant::IdIsASlug),
            "violations : {report}"
        );
    }
}
