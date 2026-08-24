//! Les quatre tests de la spec §9.1, et les cinq états qu'ils produisent.
//!
//! C'est le garde-fou du produit : rien n'autorise Ash à écrire des hooks dans la
//! configuration d'un outil tant que cette séquence n'a pas dit que le dossier est bien
//! celui qu'on croit ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! **La séquence s'arrête au premier échec**, et c'est ce qui donne son sens à la rangée
//! de pastilles : une pastille grise après un échec ne dit pas « ça n'a pas marché », elle
//! dit « on n'a pas essayé ». Les deux ne se confondent pas à l'écran non plus.
//!
//! **Les deux temps.** Les tests 1 à 3 sont locaux et instantanés : un `read_dir` et un
//! parcours du `PATH`. Le test 4 lance la commande, donc il coûte le démarrage d'un
//! programme entier. Les séparer n'est pas une optimisation d'affichage — c'est ce qui
//! permet aux hooks d'être installables **dès que 1–3 passent**, sans attendre que le
//! quatrième réponde. [`FirstPass`] est cette frontière, rendue explicite.
//!
//! **Ce qui décide de la sévérité.** Un échec des tests 1 ou 2 dit que *le dossier n'est
//! pas ce qu'on croit* : rien ne doit y être écrit, l'entrée est invalide. Un échec des
//! tests 3 ou 4 dit que *le dossier est bon mais la paire ne l'est pas* : Ash écrit quand
//! même si on insiste, et le dit — c'est la « réserve » de la maquette. Cette ligne de
//! partage est portée par [`ToolTest::decisive`], une fois, et tout le reste en découle.

use std::sync::Arc;
use std::time::Duration;

use super::permits::Permits;
use super::ports::{CommandRunner, ConfigFiles, Folder, Launch};
use super::values::{Command, ConfigTarget};

/// Au-delà, la commande du test 4 est tuée.
///
/// Cinq secondes, comme le `git status` de `features/git/git_cli.rs`, et pour la même
/// raison : c'est le point où l'on préfère une réserve honnête à une fenêtre figée. Une
/// commande qui met plus de cinq secondes à répondre `--version` n'est de toute façon pas
/// dans un état où l'on veut lui écrire des hooks.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// L'identifiant de l'adaptateur de repli d'[ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md).
///
/// Il est nommé ici parce que c'est **la correction qui a une chance** quand un dossier ne
/// porte pas la signature d'un adaptateur dédié : ce dossier n'est peut-être pas une
/// configuration `claude-code`, mais il peut très bien être une configuration `generic`.
const FALLBACK_ADAPTER: &str = "generic";

/// Les quatre tests, dans l'ordre où ils se lancent.
///
/// L'ordre n'est pas décoratif : chacun suppose le précédent. Chercher une signature dans
/// un dossier qu'on n'a pas pu ouvrir ne dirait rien, et lancer une commande pour savoir
/// quel dossier elle utilise n'a de sens qu'une fois qu'on sait que le dossier est le bon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolTest {
    /// 1 — le dossier existe et se lit.
    FolderReadable,
    /// 2 — il porte la signature de l'adaptateur.
    AdapterSignature,
    /// 3 — la commande existe dans le `PATH`.
    CommandInPath,
    /// 4 — la commande utilise réellement ce dossier.
    CommandUsesFolder,
}

impl ToolTest {
    /// Les quatre, dans l'ordre. Toute liste de tests dérive de celle-ci.
    pub const ALL: [ToolTest; 4] = [
        ToolTest::FolderReadable,
        ToolTest::AdapterSignature,
        ToolTest::CommandInPath,
        ToolTest::CommandUsesFolder,
    ];

    /// Le numéro affiché sur la pastille, et dans `stopped at test <n>`.
    pub fn number(self) -> u8 {
        match self {
            ToolTest::FolderReadable => 1,
            ToolTest::AdapterSignature => 2,
            ToolTest::CommandInPath => 3,
            ToolTest::CommandUsesFolder => 4,
        }
    }

    /// Le libellé long, celui du panneau d'état.
    pub fn label(self) -> &'static str {
        match self {
            ToolTest::FolderReadable => "the folder exists and is readable",
            ToolTest::AdapterSignature => "it carries the adapter's signature",
            ToolTest::CommandInPath => "the command exists in PATH",
            ToolTest::CommandUsesFolder => "the command really uses this folder",
        }
    }

    /// Le libellé court, celui de la note de barème sous l'en-tête de section.
    pub fn short_label(self) -> &'static str {
        match self {
            ToolTest::FolderReadable => "folder readable",
            ToolTest::AdapterSignature => "adapter signature",
            ToolTest::CommandInPath => "command in PATH",
            ToolTest::CommandUsesFolder => "command uses this folder",
        }
    }

    /// Son échec invalide-t-il l'entrée, ou la réserve-t-il seulement ?
    ///
    /// **C'est la seule ligne de partage entre `invalid` et `valid with a caveat`**, et
    /// elle est ici pour n'être écrite qu'une fois. Les deux premiers tests jugent le
    /// **dossier** — celui dans lequel Ash écrirait ; les deux derniers jugent la
    /// **paire** commande/dossier, qui décide seulement de ce que les hooks déclencheront.
    pub fn decisive(self) -> bool {
        matches!(self, ToolTest::FolderReadable | ToolTest::AdapterSignature)
    }
}

/// L'état d'une pastille de la rangée.
///
/// [`TestOutcome::Pending`] et [`TestOutcome::Skipped`] disent tous deux « pas lancé », et
/// ne se peignent pourtant pas pareil : le premier attend, le second ne viendra jamais
/// parce que la chaîne s'est arrêtée avant lui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum TestOutcome {
    /// Rien n'a encore été lancé.
    Pending,
    /// En cours — le seul état animé.
    Running,
    Passed,
    /// Échoué sans conséquence pour le dossier : la réserve.
    Warned,
    /// Échoué, et le dossier n'est pas celui qu'on croit.
    Failed,
    /// Pas lancé parce qu'un test précédent a échoué.
    Skipped,
}

/// Les cinq états de la maquette §3.4, et rien de plus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum VerificationState {
    Unverified,
    Verifying,
    Valid,
    Caveat,
    Invalid,
}

/// Ce qui était attendu, et ce qui a été trouvé.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Mismatch {
    pub expected: String,
    pub found: String,
}

/// Ce qu'on propose de faire, quand quelque chose a une chance de marcher.
///
/// `None` est un cas normal et non un manque : quand rien de ce qu'Ash sait faire ne peut
/// aider, proposer quand même une action serait un conseil générique — exactement ce que le
/// critère d'acceptation refuse. La question, elle, est toujours là.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedFix {
    /// La question posée à l'utilisateur, telle qu'elle s'affiche.
    pub question: String,
    /// Ce que le bouton `apply` ferait, ou `None` s'il n'y a rien à appliquer.
    pub apply: Option<FixAction>,
}

/// Ce qu'`apply` change dans l'entrée — et rien d'autre ne change jamais.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FixAction {
    /// Basculer l'entrée sur un autre adaptateur.
    UseAdapter { adapter: String },
    /// Pointer l'entrée sur un autre dossier.
    UseFolder { path: String },
}

/// Ce qu'une entrée a prouvé, à un instant donné.
///
/// Une seule structure pour les cinq états plutôt qu'une énumération de formes : la
/// rangée de pastilles et la phrase existent dans les cinq cas, et la fenêtre les dessine
/// sans se demander lequel elle a sous la main.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub state: VerificationState,
    /// Les quatre pastilles, dans l'ordre de [`ToolTest::ALL`].
    pub tests: Vec<TestOutcome>,
    /// La phrase de la ligne `test`.
    pub summary: String,
    /// `stopped at test <n>` — présent seulement quand la chaîne s'est arrêtée.
    pub stopped_at: Option<u8>,
    pub detail: Option<Mismatch>,
    pub fix: Option<SuggestedFix>,
    /// La commande réellement lancée, montrée pendant l'attente du test 4.
    pub launched: Option<String>,
    /// Les hooks peuvent-ils être écrits ?
    ///
    /// **Calculé ici et transporté tel quel**, jamais recalculé côté fenêtre : c'est la
    /// règle qui décide d'écrire dans un fichier de l'utilisateur, et elle n'a qu'un
    /// propriétaire. La fenêtre l'annonce, elle ne la rejoue pas
    /// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    pub allows_hooks: bool,
}

impl Verification {
    /// L'état de départ : rien n'a été lancé, et la fenêtre le dit.
    pub fn unverified() -> Self {
        Self::of(
            VerificationState::Unverified,
            [TestOutcome::Pending; 4],
            "path changed — unverified",
        )
    }

    fn of(state: VerificationState, tests: [TestOutcome; 4], summary: &str) -> Self {
        Self {
            state,
            tests: tests.to_vec(),
            summary: summary.to_owned(),
            stopped_at: None,
            detail: None,
            fix: None,
            launched: None,
            // Les trois états qui laissent écrire, et eux seuls : `valid`, la réserve, et
            // **l'attente du test 4** — les tests 1 à 3 ont déjà répondu, le dossier est
            // bon, et faire patienter l'utilisateur derrière un démarrage de programme ne
            // lui apprendrait rien de plus (maquette §3.2).
            allows_hooks: matches!(
                state,
                VerificationState::Verifying | VerificationState::Valid | VerificationState::Caveat
            ),
        }
    }

    fn stopped_at(mut self, test: ToolTest) -> Self {
        self.stopped_at = Some(test.number());
        self
    }

    fn detailing(mut self, expected: &str, found: &str) -> Self {
        self.detail = Some(Mismatch {
            expected: expected.to_owned(),
            found: found.to_owned(),
        });
        self
    }

    fn fixed_by(mut self, question: String, apply: Option<FixAction>) -> Self {
        self.fix = Some(SuggestedFix { question, apply });
        self
    }

    fn launching(mut self, launch: &Launch) -> Self {
        self.launched = Some(launch.shown());
        self
    }
}

/// Ce que la vérification a besoin de savoir d'un adaptateur.
///
/// De la **donnée**, et pas le trait `Adapter` lui-même : la feature `settings` ne connaît
/// aucun adaptateur concret ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)),
/// et c'est la composition root qui traduit ceux qu'elle assemble en profils. Un adaptateur
/// de plus reste une ligne de plus là-bas, et rien à changer ici.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterProfile {
    /// L'identifiant stable — `claude-code`, `generic`.
    pub id: String,
    /// Le dossier proposé quand l'entrée n'en nomme aucun, `None` si l'adaptateur n'en a
    /// pas. C'est aussi la correction proposée quand le dossier nommé n'existe pas.
    pub default_config: Option<String>,
    /// Ce dont la présence signe une configuration de cet outil (test 2).
    ///
    /// Vide veut dire « cet adaptateur ne signe rien » — et le test 2 passe alors, faute
    /// de quoi que ce soit à vérifier. C'est le cas de `generic`, par construction : il
    /// est l'adaptateur de l'outil dont on ne sait rien.
    pub signature: Vec<String>,
    /// La variable d'environnement par laquelle on impose le dossier au lancement (test 4).
    ///
    /// `None` veut dire que rien ne permet de relier la commande à un dossier : le test 4
    /// ne peut alors **rien lancer**, et le dit au lieu de faire semblant.
    pub config_env: Option<String>,
    /// L'invocation inoffensive qui fait répondre l'outil — `--version` et ses semblables.
    pub probe_args: Vec<String>,
}

/// La sortie du premier temps.
#[derive(Debug)]
pub enum FirstPass {
    /// Tout est joué : la chaîne s'est arrêtée avant le test 4, ou n'avait rien à lancer.
    Settled(Verification),
    /// Les tests 1 à 3 sont passés. Il reste à lancer ce que [`Launch`] décrit.
    Pending {
        /// Ce que la fenêtre montre pendant l'attente.
        shown: Verification,
        launch: Launch,
    },
}

impl FirstPass {
    /// Ce que la fenêtre affiche à la fin du premier temps, dans les deux cas.
    pub fn shown(&self) -> &Verification {
        match self {
            FirstPass::Settled(verification) => verification,
            FirstPass::Pending { shown, .. } => shown,
        }
    }
}

/// La séquence, et les effets qu'elle exerce.
///
/// Elle ne détient aucun état : deux vérifications de la même entrée sont indépendantes, et
/// c'est ce qui rend `re-verify all` possible sans coordination.
pub struct Verifier {
    files: Arc<dyn ConfigFiles>,
    commands: Arc<dyn CommandRunner>,
    profiles: Vec<AdapterProfile>,
    /// Combien de commandes peuvent tourner à la fois — voir [`Permits`].
    permits: Permits,
}

impl Verifier {
    pub fn new(
        files: Arc<dyn ConfigFiles>,
        commands: Arc<dyn CommandRunner>,
        profiles: Vec<AdapterProfile>,
    ) -> Self {
        Self {
            files,
            commands,
            profiles,
            permits: Permits::new(super::permits::MAX_CONCURRENT_PROBES),
        }
    }

    /// Les identifiants des adaptateurs, dans l'ordre où on les propose.
    pub fn adapters(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.id.clone()).collect()
    }

    /// **Le dossier qu'une entrée vise — et le seul endroit qui le décide.**
    ///
    /// « L'entrée, sinon le défaut de l'adaptateur » a un unique propriétaire, et c'est
    /// celui-ci : le `Verifier` est le seul à porter les profils, donc le seul à savoir ce
    /// que « rien » veut dire pour un adaptateur donné. La résolution du `~` se fait dans la
    /// foulée, parce que les deux formes du chemin sont **une seule valeur** : la vérification
    /// lit la forme résolue, l'écran montre la forme déclarée, le doublon compare, et la
    /// mémoire retient — les quatre sur la même chose.
    ///
    /// `None` veut dire « cette entrée ne vise aucun dossier » : elle n'en nomme pas, et son
    /// adaptateur n'en propose pas. Ce n'est pas un dossier vide, et c'est le test 1 qui le
    /// dit à l'utilisateur.
    pub fn target(&self, adapter: &str, config: Option<&str>) -> Option<ConfigTarget> {
        let declared = match config {
            Some(named) => named,
            None => self
                .profiles
                .iter()
                .find(|p| p.id == adapter)?
                .default_config
                .as_deref()?,
        };
        Some(ConfigTarget::resolving(
            declared,
            self.files.home().as_deref(),
        ))
    }

    /// Le dossier conventionnel d'un adaptateur, **et seulement s'il est là** (ADR-0006).
    ///
    /// C'est ce qu'on propose dans le champ d'un formulaire d'ajout ouvert par le marqueur
    /// de la sidebar : le dossier de configuration se propose, il ne se demande pas. Rien
    /// n'est écrit — c'est une valeur de champ, que l'utilisateur peut effacer, et la
    /// séquence des quatre tests la juge ensuite comme elle jugerait un chemin tapé.
    ///
    /// **Le disque a le dernier mot, et c'est le seul critère.** Proposer `~/.codex` à qui
    /// n'a jamais lancé codex fabriquerait une entrée dont le test 1 dirait aussitôt
    /// « nothing at ~/.codex » : une proposition qui échoue est pire qu'un champ vide,
    /// parce qu'elle a l'air d'une réponse. On ne propose donc que ce que le test 1
    /// accepterait — [`Folder::Readable`], et rien d'autre : un dossier illisible ou un
    /// fichier s'arrêteraient au même test, avec la même impression de piège.
    ///
    /// **Une seule lecture, du seul dossier que l'adaptateur nomme.** Aucun parcours de
    /// `$HOME`, aucun scan, donc aucune permission macOS à demander (ADR-0006 : reconnaître
    /// est de la lecture, et de la lecture bornée). Elle est posée au moment où l'écran
    /// s'ouvre, sur un geste, et jamais dans la boucle de sonde.
    ///
    /// `None` veut dire « rien à proposer », et couvre les trois cas d'un seul mot :
    /// adaptateur inconnu de cette compilation, adaptateur sans dossier conventionnel
    /// (`generic`, par construction), ou dossier absent.
    pub fn proposed_config(&self, adapter: &str) -> Option<String> {
        let target = self.target(adapter, None)?;
        match self.files.read_folder(target.resolved()) {
            Folder::Readable(_) => Some(target.declared().to_owned()),
            Folder::Missing | Folder::NotADirectory | Folder::Unreadable => None,
        }
    }

    /// Le premier temps : les tests 1 à 3. **Ne lance rien.**
    pub fn first_pass(&self, command: &Command, adapter: &str, config: Option<&str>) -> FirstPass {
        let Some(profile) = self.profiles.iter().find(|p| p.id == adapter) else {
            // La composition root n'assemble que des adaptateurs connus, et `declare` les
            // refuse autrement : ce cas n'arrive qu'à un `~/.ash/tools.json` édité à la main
            // — ou écrit par une version d'Ash qui embarquait cet adaptateur —, et
            // `NewTool::restore` garde l'entrée exprès pour qu'on la voie. Le dire vaut mieux
            // que de le faire passer.
            return FirstPass::Settled(
                invalid(
                    ToolTest::AdapterSignature,
                    [TestOutcome::Passed, TestOutcome::Failed],
                    &format!("no adapter named {adapter} in this build of ash"),
                )
                .detailing(
                    "an adapter this build embeds",
                    &format!("{adapter}, which it does not"),
                )
                .fixed_by(
                    format!("use the {FALLBACK_ADAPTER} adapter instead?"),
                    Some(FixAction::UseAdapter {
                        adapter: FALLBACK_ADAPTER.to_owned(),
                    }),
                ),
            );
        };

        // Test 1 — le dossier existe et se lit. Ce que l'entrée vise vient d'un seul
        // endroit ([`Verifier::target`]), sous ses deux formes : celle qu'on montre, et
        // celle qu'on lit.
        let Some(target) = self.target(adapter, config) else {
            return FirstPass::Settled(
                invalid(
                    ToolTest::FolderReadable,
                    [TestOutcome::Failed],
                    &format!("no configuration folder — the {adapter} adapter has no default"),
                )
                .detailing(
                    "a configuration folder",
                    "none: the entry names no folder and the adapter proposes none",
                )
                .fixed_by("name the folder this tool reads?".to_owned(), None),
            );
        };
        let raw = target.declared();

        let entries = match self.files.read_folder(target.resolved()) {
            Folder::Readable(entries) => entries,
            Folder::Missing => {
                return FirstPass::Settled(
                    invalid(
                        ToolTest::FolderReadable,
                        [TestOutcome::Failed],
                        &format!("nothing at {raw}"),
                    )
                    .detailing("a readable folder", "nothing at this path")
                    .fixed_by(
                        // Le défaut de l'adaptateur a une chance : c'est le dossier que
                        // l'outil lit quand personne ne lui en impose un.
                        match profile.default_config.as_deref().filter(|d| *d != raw) {
                            Some(default) => format!("use the adapter default {default} instead?"),
                            None => "choose another folder?".to_owned(),
                        },
                        profile
                            .default_config
                            .as_deref()
                            .filter(|d| *d != raw)
                            .map(|default| FixAction::UseFolder {
                                path: default.to_owned(),
                            }),
                    ),
                );
            }
            Folder::NotADirectory => {
                return FirstPass::Settled(
                    invalid(
                        ToolTest::FolderReadable,
                        [TestOutcome::Failed],
                        &format!("{raw} is a file, not a folder"),
                    )
                    .detailing("a readable folder", "a file")
                    // Pointer le dossier qui contient ce fichier a une chance : c'est
                    // l'erreur qu'on fait en désignant `settings.json` au lieu de `~/.claude`.
                    .fixed_by(
                        "use the folder that contains it?".to_owned(),
                        parent_of(raw).map(|path| FixAction::UseFolder { path }),
                    ),
                );
            }
            Folder::Unreadable => {
                return FirstPass::Settled(
                    invalid(
                        ToolTest::FolderReadable,
                        [TestOutcome::Failed],
                        &format!("ash can't read {raw}"),
                    )
                    .detailing("a readable folder", "permission denied")
                    // Rien à appliquer : les permissions d'un dossier ne se changent pas
                    // depuis une fenêtre de réglages, et prétendre le contraire serait le
                    // conseil générique que le critère refuse.
                    .fixed_by(
                        "grant ash access to this folder, or choose another one?".to_owned(),
                        None,
                    ),
                );
            }
        };

        // Test 2 — la signature de l'adaptateur.
        let missing: Vec<&str> = profile
            .signature
            .iter()
            .map(String::as_str)
            .filter(|wanted| !entries.iter().any(|found| found == wanted))
            .collect();
        if !missing.is_empty() {
            let absent = missing
                .iter()
                .map(|name| format!("no {name}"))
                .collect::<Vec<_>>()
                .join(", ");
            let fallback = (adapter != FALLBACK_ADAPTER
                && self.profiles.iter().any(|p| p.id == FALLBACK_ADAPTER))
            .then_some(FALLBACK_ADAPTER);
            return FirstPass::Settled(
                invalid(
                    ToolTest::AdapterSignature,
                    [TestOutcome::Passed, TestOutcome::Failed],
                    &format!("doesn't look like a {adapter} config — {absent}"),
                )
                .detailing(&profile.signature.join(", "), &summarise(&entries))
                .fixed_by(
                    match fallback {
                        Some(other) => format!("use the {other} adapter instead?"),
                        None => "choose another folder?".to_owned(),
                    },
                    fallback.map(|other| FixAction::UseAdapter {
                        adapter: other.to_owned(),
                    }),
                ),
            );
        }

        // Test 3 — la commande existe dans le `PATH`.
        let Some(program) = self.commands.locate(command) else {
            return FirstPass::Settled(caveat(
                ToolTest::CommandInPath,
                [
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Warned,
                ],
                &format!("folder recognised · command {command} not found in PATH"),
            ));
        };

        // Test 4 — la commande utilise réellement ce dossier.
        let Some(variable) = profile.config_env.as_deref() else {
            // Rien ne relie cette commande à un dossier : lancer quand même prouverait
            // seulement qu'elle démarre, ce qui n'est pas la question posée.
            return FirstPass::Settled(caveat(
                ToolTest::CommandUsesFolder,
                [
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Warned,
                ],
                &format!(
                    "folder recognised · the {adapter} adapter can't point {command} at a folder"
                ),
            ));
        };

        let launch = Launch {
            program,
            args: profile.probe_args.clone(),
            env: vec![(variable.to_owned(), target.resolved().display().to_string())],
            timeout: PROBE_TIMEOUT,
        };
        let shown = Verification::of(
            VerificationState::Verifying,
            [
                TestOutcome::Passed,
                TestOutcome::Passed,
                TestOutcome::Passed,
                TestOutcome::Running,
            ],
            "folder recognised · test 4 of 4",
        )
        .launching(&launch);

        FirstPass::Pending { shown, launch }
    }

    /// Le second temps : lancer la commande, et rapporter ce qu'elle a répondu.
    ///
    /// **Bloque tant qu'aucun jeton n'est libre** : c'est ce qui borne `re-verify all`.
    pub fn second_pass(&self, command: &Command, launch: &Launch) -> Verification {
        let _permit = self.permits.acquire();
        let answered = self.commands.run(launch);
        drop(_permit);

        let all = [TestOutcome::Passed; 4];
        match answered {
            Ok(answer) if answer.succeeded => Verification::of(
                VerificationState::Valid,
                all,
                &format!("folder recognised · {command} answers with this folder"),
            ),
            Ok(answer) => caveat(
                ToolTest::CommandUsesFolder,
                [
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Warned,
                ],
                &format!(
                    "folder recognised · {command} refused this folder{}",
                    first_line(&answer.output)
                ),
            ),
            Err(why) => caveat(
                ToolTest::CommandUsesFolder,
                [
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Passed,
                    TestOutcome::Warned,
                ],
                &format!("folder recognised · {command} didn't answer — {why}"),
            ),
        }
    }
}

/// Un état invalide : la chaîne s'arrête, et ce qui suit n'a **pas été lancé**.
fn invalid(stopped: ToolTest, done: impl AsRef<[TestOutcome]>, summary: &str) -> Verification {
    Verification::of(
        VerificationState::Invalid,
        fill(done.as_ref(), TestOutcome::Skipped),
        summary,
    )
    .stopped_at(stopped)
}

/// Une réserve : le dossier est bon, la paire ne l'est pas.
fn caveat(stopped: ToolTest, done: impl AsRef<[TestOutcome]>, summary: &str) -> Verification {
    Verification::of(
        VerificationState::Caveat,
        fill(done.as_ref(), TestOutcome::Skipped),
        summary,
    )
    .stopped_at(stopped)
}

/// Complète une rangée partielle : ce qui n'a pas été lancé le reste.
fn fill(done: &[TestOutcome], rest: TestOutcome) -> [TestOutcome; 4] {
    let mut tests = [rest; 4];
    for (slot, outcome) in tests.iter_mut().zip(done) {
        *slot = *outcome;
    }
    tests
}

/// Le dossier qui contient ce chemin, tel qu'on le réécrirait dans l'entrée.
fn parent_of(raw: &str) -> Option<String> {
    let (parent, _) = raw.rsplit_once('/')?;
    (!parent.is_empty()).then(|| parent.to_owned())
}

/// La première ligne utile d'une sortie, préfixée pour se coller à une phrase.
fn first_line(output: &str) -> String {
    match output.lines().map(str::trim).find(|line| !line.is_empty()) {
        Some(line) => format!(" — {line}"),
        None => String::new(),
    }
}

/// Ce qu'il y a dans un dossier, dit en une ligne — le « found » du rappel d'erreur.
///
/// Les fichiers sont groupés par extension et les dossiers nommés : c'est ce qui permet de
/// reconnaître d'un coup d'œil qu'on a désigné un dossier de notes plutôt qu'une
/// configuration. Une liste brute de trente entrées ne dirait rien.
fn summarise(entries: &[String]) -> String {
    if entries.is_empty() {
        return "an empty folder".to_owned();
    }

    let mut groups: Vec<(String, usize)> = Vec::new();
    for entry in entries {
        let kind = match entry.rsplit_once('.') {
            // Un nom qui commence par un point est un nom, pas une extension : `.git` est
            // « `.git` », pas « un fichier sans nom d'extension `git` ».
            Some((stem, extension)) if !stem.is_empty() => format!(".{extension} files"),
            _ => entry.clone(),
        };
        match groups.iter_mut().find(|(name, _)| *name == kind) {
            Some((_, count)) => *count += 1,
            None => groups.push((kind, 1)),
        }
    }

    groups.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let shown = groups
        .iter()
        .take(3)
        .map(|(name, count)| {
            if name.ends_with(" files") {
                format!("{count} {name}")
            } else if *count > 1 {
                format!("{count} × {name}")
            } else {
                name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    match groups.len().checked_sub(3) {
        Some(rest) if rest > 0 => format!("{shown}, and {rest} more"),
        _ => shown,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::features::settings::fakes::{FakeCommands, FakeFolders};

    /// Un nom de commande valide, tel que `NewTool::declare` en produit.
    fn named(command: &str) -> Command {
        Command::parse(command).unwrap_or_else(|why| panic!("{command} est un nom valide : {why}"))
    }

    /// Test Data Builder : un profil d'adaptateur dédié, dont on ne surcharge que ce que
    /// le scénario regarde.
    fn claude_code() -> AdapterProfile {
        AdapterProfile {
            id: "claude-code".to_owned(),
            default_config: Some("~/.claude".to_owned()),
            signature: vec!["settings.json".to_owned(), "projects".to_owned()],
            config_env: Some("CLAUDE_CONFIG_DIR".to_owned()),
            probe_args: vec!["--version".to_owned()],
        }
    }

    fn generic() -> AdapterProfile {
        AdapterProfile {
            id: "generic".to_owned(),
            default_config: None,
            signature: Vec::new(),
            config_env: None,
            probe_args: vec!["--version".to_owned()],
        }
    }

    struct VerifierBuilder {
        files: FakeFolders,
        commands: FakeCommands,
        profiles: Vec<AdapterProfile>,
    }

    impl VerifierBuilder {
        fn new() -> Self {
            Self {
                files: FakeFolders::new("/Users/ash"),
                commands: FakeCommands::new(),
                profiles: vec![claude_code(), generic()],
            }
        }

        fn folder(mut self, path: &str, entries: &[&str]) -> Self {
            self.files = self.files.folder(path, entries);
            self
        }

        fn at(mut self, path: &str, found: Folder) -> Self {
            self.files = self.files.at(path, found);
            self
        }

        fn in_path(mut self, command: &str, program: &str) -> Self {
            self.commands = self.commands.in_path(command, program);
            self
        }

        fn answering(mut self, succeeded: bool) -> Self {
            self.commands = self.commands.answering(succeeded);
            self
        }

        fn build(self) -> (Verifier, Arc<FakeCommands>) {
            let commands = Arc::new(self.commands);
            let verifier = Verifier::new(
                Arc::new(self.files),
                Arc::clone(&commands) as Arc<dyn CommandRunner>,
                self.profiles,
            );
            (verifier, commands)
        }
    }

    #[test]
    fn given_a_configuration_folder_that_does_not_exist_when_the_sequence_runs_then_it_stops_at_the_first_test(
    ) {
        // Given — le test 1 est le seul qui puisse se prononcer : chercher une signature
        // dans un dossier qu'on n'a pas ouvert ne dirait rien
        let (verifier, _) = VerifierBuilder::new().build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude"));

        // Then
        let shown = first.shown();
        assert_eq!(shown.state, VerificationState::Invalid);
        assert_eq!(shown.stopped_at, Some(1));
        assert_eq!(
            shown.tests,
            vec![
                TestOutcome::Failed,
                TestOutcome::Skipped,
                TestOutcome::Skipped,
                TestOutcome::Skipped
            ]
        );
    }

    #[test]
    fn given_a_folder_without_the_adapter_signature_when_the_sequence_runs_then_it_names_what_was_expected_and_what_was_found(
    ) {
        // Given — le dossier de notes qu'on a désigné par erreur : il existe, il se lit, et
        // ce n'est pas une configuration `claude-code`
        let (verifier, _) = VerifierBuilder::new()
            .folder(
                "/Users/ash/dev/notes",
                &["a.md", "b.md", "c.md", ".git", "README.md"],
            )
            .build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/dev/notes"));

        // Then — « l'erreur nomme le test échoué, ce qui était attendu et ce qui a été
        // trouvé »
        let shown = first.shown();
        assert_eq!(shown.stopped_at, Some(2));
        assert_eq!(
            shown.detail,
            Some(Mismatch {
                expected: "settings.json, projects".to_owned(),
                found: "4 .md files, .git".to_owned(),
            })
        );
    }

    #[test]
    fn given_a_folder_that_is_not_a_claude_config_when_a_fix_is_proposed_then_it_offers_the_adapter_that_still_has_a_chance(
    ) {
        // Given — « ce dossier n'est pas une config claude-code, mais il peut très bien
        // être une config generic »
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/dev/notes", &["a.md"])
            .build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/dev/notes"));

        // Then
        assert_eq!(
            first.shown().fix,
            Some(SuggestedFix {
                question: "use the generic adapter instead?".to_owned(),
                apply: Some(FixAction::UseAdapter {
                    adapter: "generic".to_owned()
                }),
            })
        );
    }

    #[test]
    fn given_a_configuration_path_that_points_at_a_file_when_a_fix_is_proposed_then_it_offers_the_folder_that_contains_it(
    ) {
        // Given — désigner `settings.json` au lieu de `~/.claude` est l'erreur courante, et
        // la correction qui a une chance n'est pas la même que pour un dossier absent
        let (verifier, _) = VerifierBuilder::new()
            .at("/Users/ash/.claude/settings.json", Folder::NotADirectory)
            .build();

        // When
        let first = verifier.first_pass(
            &named("claude"),
            "claude-code",
            Some("~/.claude/settings.json"),
        );

        // Then
        assert_eq!(
            first.shown().fix.clone().and_then(|fix| fix.apply),
            Some(FixAction::UseFolder {
                path: "~/.claude".to_owned()
            })
        );
    }

    #[test]
    fn given_a_folder_ash_may_not_read_when_a_fix_is_proposed_then_it_does_not_pretend_to_be_able_to_apply_one(
    ) {
        // Given — les permissions d'un dossier ne se changent pas depuis cette fenêtre :
        // offrir un bouton `apply` mentirait sur ce qu'il ferait
        let (verifier, _) = VerifierBuilder::new()
            .at("/Users/ash/.claude", Folder::Unreadable)
            .build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude"));

        // Then
        let fix = first.shown().fix.clone().expect("une question est posée");
        assert_eq!(fix.apply, None);
        assert!(fix.question.contains("grant ash access"));
    }

    #[test]
    fn given_a_valid_folder_whose_command_is_not_in_path_when_the_sequence_runs_then_it_is_a_caveat_and_nothing_is_launched(
    ) {
        // Given — « the folder is right, the pair isn't. ash still writes if you insist,
        // and says so »
        let (verifier, commands) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .build();

        // When
        let first = verifier.first_pass(&named("claude-perso"), "claude-code", Some("~/.claude"));

        // Then
        let shown = first.shown();
        assert_eq!(shown.state, VerificationState::Caveat);
        assert_eq!(
            shown.summary,
            "folder recognised · command claude-perso not found in PATH"
        );
        assert_eq!(commands.launches(), Vec::<Launch>::new());
    }

    #[test]
    fn given_a_caveat_or_a_valid_entry_when_the_hooks_ask_whether_they_may_be_written_then_both_say_yes_and_an_invalid_one_says_no(
    ) {
        // Given — « only valid and valid with a caveat allow hooks to be written »
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .build();

        // When
        let caveat = verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude"));
        let invalid = verifier.first_pass(&named("claude"), "claude-code", Some("~/nowhere"));

        // Then
        assert!(caveat.shown().allows_hooks);
        assert!(!invalid.shown().allows_hooks);
        assert!(!Verification::unverified().allows_hooks);
    }

    #[test]
    fn given_the_first_three_tests_passing_when_the_first_pass_ends_then_the_hooks_may_already_be_written(
    ) {
        // Given — c'est la conséquence fonctionnelle du résultat en deux temps : le bouton
        // s'allume dès que 1–3 passent, sans attendre le quatrième
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .in_path("claude", "/usr/local/bin/claude")
            .build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude"));

        // Then
        let shown = first.shown();
        assert_eq!(shown.state, VerificationState::Verifying);
        assert!(shown.allows_hooks);
        assert_eq!(shown.summary, "folder recognised · test 4 of 4");
        assert_eq!(shown.tests[3], TestOutcome::Running);
    }

    #[test]
    fn given_the_first_three_tests_passing_when_the_command_answers_with_this_folder_then_the_entry_is_valid(
    ) {
        // Given
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .in_path("claude", "/usr/local/bin/claude")
            .answering(true)
            .build();

        // When
        let second = match verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude")) {
            FirstPass::Pending { launch, .. } => verifier.second_pass(&named("claude"), &launch),
            FirstPass::Settled(_) => panic!("le test 4 devait rester à lancer"),
        };

        // Then
        assert_eq!(second.state, VerificationState::Valid);
        assert_eq!(second.tests, vec![TestOutcome::Passed; 4]);
    }

    #[test]
    fn given_a_command_that_refuses_the_folder_when_the_second_pass_ends_then_the_entry_is_valid_with_a_caveat(
    ) {
        // Given — le dossier reste bon : ce qui vacille est la paire, et Ash écrit quand
        // même si on insiste
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .in_path("claude", "/usr/local/bin/claude")
            .answering(false)
            .build();

        // When
        let second = match verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude")) {
            FirstPass::Pending { launch, .. } => verifier.second_pass(&named("claude"), &launch),
            FirstPass::Settled(_) => panic!("le test 4 devait rester à lancer"),
        };

        // Then
        assert_eq!(second.state, VerificationState::Caveat);
        assert!(second.allows_hooks);
        assert_eq!(second.stopped_at, Some(4));
    }

    #[test]
    fn given_an_adapter_that_cannot_bind_a_folder_to_a_command_when_the_sequence_runs_then_it_says_so_instead_of_launching_anything(
    ) {
        // Given — `generic` ne sait imposer aucun dossier : lancer la commande prouverait
        // seulement qu'elle démarre, ce qui n'est pas la question du test 4
        let (verifier, commands) = VerifierBuilder::new()
            .folder("/Users/ash/notes", &["anything"])
            .in_path("kimi", "/usr/local/bin/kimi")
            .build();

        // When
        let first = verifier.first_pass(&named("kimi"), "generic", Some("~/notes"));

        // Then
        let shown = first.shown();
        assert_eq!(shown.state, VerificationState::Caveat);
        assert_eq!(shown.tests[3], TestOutcome::Warned);
        assert_eq!(commands.launches(), Vec::<Launch>::new());
    }

    #[test]
    fn given_a_command_the_path_does_not_resolve_when_the_sequence_runs_then_ash_never_launches_a_path_of_its_own_making(
    ) {
        // Given — **frontière de sécurité** : le seul programme qu'Ash lance est celui que
        // le `PATH` a résolu au test 3. Sans lui, il n'y a rien à lancer — et surtout pas
        // un chemin recomposé à partir de la saisie
        let (verifier, commands) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .build();

        // When
        let first = verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude"));

        // Then
        assert!(matches!(first, FirstPass::Settled(_)));
        assert!(commands.launches().is_empty());
    }

    #[test]
    fn given_a_configuration_folder_when_the_command_is_launched_then_the_folder_travels_by_the_environment_and_never_as_an_argument(
    ) {
        // Given — **frontière de sécurité** : un chemin passé en argument serait relu par
        // la commande comme une option ; passé par l'environnement, rien ne l'interprète.
        // Les arguments viennent de l'adaptateur, jamais de l'écran
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .in_path("claude", "/usr/local/bin/claude")
            .build();

        // When
        let launch = match verifier.first_pass(&named("claude"), "claude-code", Some("~/.claude")) {
            FirstPass::Pending { launch, .. } => launch,
            FirstPass::Settled(_) => panic!("le test 4 devait rester à lancer"),
        };

        // Then
        assert_eq!(launch.program, PathBuf::from("/usr/local/bin/claude"));
        assert_eq!(launch.args, vec!["--version".to_owned()]);
        assert_eq!(
            launch.env,
            vec![(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "/Users/ash/.claude".to_owned()
            )]
        );
    }

    #[test]
    fn given_a_configuration_folder_named_like_an_option_when_the_command_is_launched_then_it_still_never_reaches_the_arguments(
    ) {
        // Given — **frontière de sécurité**, prise par le seul bout que l'utilisateur tient :
        // le champ de chemin. Un dossier qui se lit comme une option de ligne de commande est
        // ce qui transformerait une vérification en exécution d'autre chose. Il n'y a aucun
        // shell, et le chemin ne voyage que par l'environnement — donc rien ne le relit
        let hostile = "~/--dangerously-skip-permissions";
        let (verifier, _) = VerifierBuilder::new()
            .folder(
                "/Users/ash/--dangerously-skip-permissions",
                &["settings.json", "projects"],
            )
            .in_path("claude", "/usr/local/bin/claude")
            .build();

        // When
        let launch = match verifier.first_pass(&named("claude"), "claude-code", Some(hostile)) {
            FirstPass::Pending { launch, .. } => launch,
            FirstPass::Settled(_) => panic!("le test 4 devait rester à lancer"),
        };

        // Then — les arguments restent ceux de l'adaptateur, et rien d'autre
        assert_eq!(launch.args, vec!["--version".to_owned()]);
        assert_eq!(launch.program, PathBuf::from("/usr/local/bin/claude"));
        assert_eq!(
            launch.env,
            vec![(
                "CLAUDE_CONFIG_DIR".to_owned(),
                "/Users/ash/--dangerously-skip-permissions".to_owned()
            )]
        );
    }

    #[test]
    fn given_a_folder_full_of_notes_when_it_is_summarised_then_it_reads_as_a_glance_and_not_as_a_listing(
    ) {
        // Given — c'est le « found » du rappel d'erreur : trente noms bruts ne diraient
        // rien, alors que « 12 .md files, .git » se reconnaît d'un coup d'œil
        let entries: Vec<String> = (0..12)
            .map(|n| format!("note{n}.md"))
            .chain([".git".to_owned()])
            .collect();

        // When
        let summarised = summarise(&entries);

        // Then
        assert_eq!(summarised, "12 .md files, .git");
    }

    #[test]
    fn given_an_adapter_whose_conventional_folder_is_on_disk_when_a_form_opens_on_it_then_that_folder_is_proposed(
    ) {
        // Given — le dossier que `claude-code` lit quand personne ne lui en impose un,
        // et il est là : l'utilisateur a déjà lancé `claude`
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .build();

        // When
        let proposed = verifier.proposed_config("claude-code");

        // Then — la forme **déclarée**, celle que le champ montre et que l'utilisateur
        // reconnaît, pas le chemin résolu
        assert_eq!(proposed.as_deref(), Some("~/.claude"));
    }

    #[test]
    fn given_a_conventional_folder_that_is_not_there_when_a_form_opens_on_it_then_nothing_is_proposed(
    ) {
        // Given — rien à `~/.claude` : proposer le dossier ferait une entrée dont le
        // test 1 dirait aussitôt « nothing at ~/.claude »
        let (verifier, _) = VerifierBuilder::new().build();

        // When
        let proposed = verifier.proposed_config("claude-code");

        // Then
        assert_eq!(proposed, None);
    }

    #[test]
    fn given_a_conventional_folder_that_ash_cannot_read_when_a_form_opens_on_it_then_nothing_is_proposed(
    ) {
        // Given — il existe, mais le test 1 s'y arrêterait quand même : une proposition
        // qui échoue a l'air d'une réponse
        let (verifier, _) = VerifierBuilder::new()
            .at("/Users/ash/.claude", Folder::Unreadable)
            .build();

        // When
        let proposed = verifier.proposed_config("claude-code");

        // Then
        assert_eq!(proposed, None);
    }

    #[test]
    fn given_an_adapter_with_no_conventional_folder_when_a_form_opens_on_it_then_nothing_is_proposed(
    ) {
        // Given — `generic` est l'adaptateur de l'outil dont on ne sait rien : il ne
        // nomme aucun dossier, par construction (ADR-0008)
        let (verifier, _) = VerifierBuilder::new()
            .folder("/Users/ash/.claude", &["settings.json", "projects"])
            .build();

        // When
        let proposed = verifier.proposed_config("generic");

        // Then — et le champ vide n'est pas muet : la séquence part sur le brouillon, et
        // son test 1 dit « no configuration folder — the generic adapter has no default »
        assert_eq!(proposed, None);
    }
}
