//! Les cinq états de la ligne `hooks`, et ce que chacun laisse faire.
//!
//! La ligne répond à une question simple — « les hooks de cet outil sont-ils posés ? » —
//! dont la réponse vient de trois endroits qui ne se connaissent pas : ce que les quatre
//! tests ont dit de l'entrée ([`Verification`]), ce que les **autres** entrées déclarent
//! (le doublon), et ce que le fichier de l'utilisateur porte
//! ([`Presence`](crate::features::hooks::Presence)). Les composer est une règle, pas une
//! mise en forme, et c'est pour ça que ça se passe ici et non dans la fenêtre.
//!
//! **La précédence est la règle**, et elle va du plus général au plus précis :
//!
//! 1. une entrée que la séquence n'autorise pas ne reçoit rien — c'est
//!    [`Verification::allows_hooks`], calculé une fois et jamais rejoué ;
//! 2. une entrée dont une autre a déjà pris le fichier ne l'écrit pas une seconde fois ;
//! 3. un adaptateur qui n'instrumente rien n'a rien à poser, et le dit ;
//! 4. alors seulement, l'état du fichier décide.
//!
//! Les trois premières marches produisent toutes `blocked`, et pourtant **aucune ne dit la
//! même chose** : le bouton reste à sa place, éteint, avec sa raison à gauche — « le
//! masquer ferait croire que les hooks n'existent pas pour cet outil ».

use std::path::PathBuf;

use super::values::Command;
use super::verification::{Verification, VerificationState};
use crate::features::hooks::Presence;

/// Les cinq états de la ligne, et rien de plus (maquette §4.3).
///
/// La fenêtre les distingue **par la forme** avant la couleur : coche, cercle vide, flèche
/// vers le haut, croix, cadenas. La discipline est celle de `shared/agent-state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HookState {
    /// Le bloc en place est celui qu'Ash écrirait.
    Installed,
    /// **Rien n'est posé**, et rien n'empêche de le poser. Le fichier ne porte aucun hook.
    Missing,
    /// Un bloc d'Ash est là, mais pas celui-ci.
    Outdated,
    /// Il y a dans ce fichier quelque chose qu'Ash n'a pas mis — les hooks de
    /// l'utilisateur, ou une entrée d'Ash qu'une main a modifiée.
    ///
    /// **Ash n'écrit pas de lui-même** : il montre le diff de ce qu'il écrirait et laisse
    /// choisir. Les deux cas se ressemblent du point de vue de celui qui regarde — « il y a
    /// là quelque chose que je n'ai pas mis, montre-le-moi » — et c'était une faute de n'en
    /// traiter qu'un ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md), amendement
    /// du 2026-08-12).
    Conflict,
    /// Quelque chose empêche d'écrire. La raison est dans [`HooksReport::summary`].
    Blocked,
}

/// Ce qu'un bouton de la ligne — ou du diff — déclenche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum HookAction {
    /// Pose les entrées d'Ash, **à côté** de celles de l'utilisateur : c'est la fusion.
    Install,
    Update,
    /// Destructif, donc secondaire dans la fenêtre.
    Remove,
    /// **N'écrit rien** : elle ouvre le diff de ce qu'Ash écrirait, sur le fichier tel
    /// qu'il est. C'est le geste qui précède tous les autres quand il y a un conflit.
    SeeTheDiff,
}

/// Une issue offerte depuis le diff — ce que l'utilisateur peut trancher.
///
/// **Elle porte son libellé**, et ce n'est pas un détail de vue : « merge » et « install »
/// sont le même geste pour le backend et deux promesses différentes pour celui qui lit
/// l'écran. Le mot qui dit ce que l'écriture va préserver appartient à celui qui sait ce
/// qu'elle fait ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HookChoice {
    pub action: HookAction,
    /// Le mot du bouton — `merge, keeping every hook`.
    pub label: String,
    /// Ce que ce geste fait au fichier, en une phrase.
    pub note: String,
}

/// Ce que la ligne `hooks` d'une entrée affiche, et ce qu'elle laisse faire.
///
/// Elle voyage **avec** la déclaration, comme la vérification, et pour la même raison : une
/// entrée dont le chemin change change d'état de hooks au même instant, et deux tables
/// séparées laisseraient un intervalle où l'écran montrerait l'un sous l'autre.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct HooksReport {
    pub state: HookState,
    /// La phrase de la ligne — `installed · v1`, `no ash hooks in this file`…
    pub summary: String,
    /// Les deux lignes de prose sous la ligne : **la conséquence**, pas la répétition.
    pub note: String,
    /// Le fichier concerné, quand il y en a un.
    pub file: Option<String>,
    pub action: HookAction,
    /// Le bouton est-il allumé ? Il reste **visible** dans tous les cas.
    pub enabled: bool,
    /// Ce que le diff propose de trancher, dans l'ordre. Vide quand rien ne s'écrit.
    pub choices: Vec<HookChoice>,
    /// Le diff de ce qu'Ash écrirait, sur le fichier **tel qu'il est**, avant toute
    /// écriture. `None` quand il n'y a rien à écrire, ou rien à lire.
    pub diff: Option<String>,
    /// La copie qui sera prise **avant** l'action, annoncée avant et non après (§4.2).
    pub backup: Option<String>,
}

impl HooksReport {
    /// Ce qu'une entrée que rien n'a encore jugée affiche.
    ///
    /// C'est la valeur d'une déclaration à l'instant où elle naît : elle n'a pas prouvé son
    /// dossier, donc rien ne sera écrit. Le registre la remplace dès qu'il regarde.
    pub fn until_verified() -> Self {
        blocked("path unverified", WAITING_ON_THE_TESTS)
    }
}

/// Les libellés des issues, écrits une fois.
///
/// « merge » plutôt qu'« install » quand le fichier porte déjà des hooks : le mot doit dire
/// ce que l'écriture **préserve**, sinon la seule façon de le savoir est de cliquer.
fn merge_choice() -> HookChoice {
    HookChoice {
        action: HookAction::Install,
        label: "merge, keeping every hook".to_owned(),
        note: "ash adds its own entries next to yours, in the same event arrays. \
               nothing already there is replaced, and a .bak is written first."
            .to_owned(),
    }
}

fn rewrite_choice() -> HookChoice {
    HookChoice {
        action: HookAction::Install,
        label: "restore ash's entries".to_owned(),
        note: "only the entries carrying ash's marker are rewritten. \
               everything else in the file is left as it is, and a .bak is written first."
            .to_owned(),
    }
}

fn update_choice(available: u32) -> HookChoice {
    HookChoice {
        action: HookAction::Update,
        label: format!("update to v{available}"),
        note: "ash rewrites its own entries in place, and touches nothing else.".to_owned(),
    }
}

fn remove_choice() -> HookChoice {
    HookChoice {
        action: HookAction::Remove,
        label: "remove ash's hooks".to_owned(),
        note: "the entries carrying ash's marker are taken out, and the file goes back \
               to what it was, byte for byte. yours stay."
            .to_owned(),
    }
}

const WAITING_ON_THE_TESTS: &str =
    "the button stays where it is, dimmed, with its reason on the left. \
     as soon as tests 1–3 pass it lights up — without waiting for test 4.";

/// Ce que le port a trouvé pour une entrée : où, et dans quel état.
///
/// Le fichier voyage avec le verdict parce que l'écran le nomme dans les cinq cas — un
/// refus qui ne dit pas *quel* fichier il refuse n'apprend rien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockAt {
    pub file: PathBuf,
    pub presence: Presence,
}

/// L'état de la ligne `hooks` d'une entrée, composé de ses trois sources.
///
/// `taken_by` est l'entrée qui a déjà ce dossier, s'il y en a une ; `found` est ce que le
/// port a vu, ou `None` quand l'adaptateur n'instrumente rien.
///
/// Les deux premières marches sont ici, et **les deux dernières sont dans [`foreseen`]** :
/// ce sont celles qui ne parlent que du fichier, donc les seules qu'un outil non déclaré
/// puisse emprunter.
pub fn report(
    verification: &Verification,
    adapter: &str,
    taken_by: Option<&Command>,
    found: Option<BlockAt>,
) -> HooksReport {
    // 1 — la séquence. C'est **elle** qui autorise Ash à écrire chez l'utilisateur, et elle
    // n'est jamais rejouée : ni ici, ni dans la fenêtre (ADR-0009).
    if !verification.allows_hooks {
        return blocked(
            match verification.state {
                VerificationState::Invalid => "unavailable until the path is verified",
                _ => "path unverified",
            },
            WAITING_ON_THE_TESTS,
        );
    }

    // 2 — le doublon. Il n'invalide rien : il bloque la **seconde** écriture, parce que les
    // deux entrées désignent le même fichier et que le second bloc n'y ferait rien.
    if let Some(other) = taken_by {
        return blocked(
            &format!("already written by {other} in this file"),
            "two entries on the same configuration folder write the same block twice — \
             the second one does nothing. reset or repoint one of them.",
        );
    }

    foreseen(adapter, found)
}

/// Les cinq états d'un outil **que personne n'a déclaré** — les marches 3 et 4, seules.
///
/// C'est la ligne d'une suggestion : un outil qu'Ash a vu tourner (ADR-0006), lu sur le
/// dossier par défaut de son adaptateur. Les deux premières marches de [`report`] n'ont rien
/// à y dire, et c'est le point :
///
/// - **il n'y a pas de vérification à consulter**, parce qu'il n'y a pas d'entrée. Ouvrir la
///   fenêtre ne doit rien vérifier — le test 3 parcourt le `PATH` et le test 4 lance la
///   commande —, et une suggestion n'autorise aucune écriture : le seul geste qu'elle offre
///   est de se déclarer, ce qui ne touche à aucun fichier de l'utilisateur (ADR-0007) ;
/// - **il n'y a pas de doublon possible**, parce qu'un outil déjà déclaré n'est plus une
///   suggestion (voir [`super::suggestions`]).
///
/// Ce qui reste est donc exactement ce que la ligne d'une carte déclarée dirait du même
/// fichier — les **cinq** états, et non les trois d'`Instrumented` : un conflit s'y distingue
/// d'une absence, ce qui est toute la raison d'être de cette lecture.
pub fn foreseen(adapter: &str, found: Option<BlockAt>) -> HooksReport {
    // 3 — l'adaptateur de repli n'a pas de hooks, et c'est sa définition
    // (ADR-0008) : il est l'adaptateur de l'outil dont on ne sait rien.
    let Some(BlockAt { file, presence }) = found else {
        return blocked(
            &format!("the {adapter} adapter has no hooks to install"),
            "without a dedicated adapter, ash watches the process, not its hooks: \
             this tool never shows as waiting.",
        );
    };

    let shown = file.display().to_string();
    let backup = format!("{shown}.bak");

    // 4 — ce que le fichier porte. Le sens de l'écart de version se demande **avant** le
    // classement, et à `Presence` : la sidebar pose la même question pour son marqueur
    // `outdated`, et deux comparaisons écrites à la main diraient un jour deux choses du
    // même fichier.
    let behind = presence.is_behind();
    match presence {
        Presence::Current { version } => HooksReport {
            state: HookState::Installed,
            summary: format!("installed · v{version}"),
            note: "remove takes out the entries carrying ash's marker, leaves the rest of \
                   the file intact, and writes a .bak first."
                .to_owned(),
            file: Some(shown),
            action: HookAction::Remove,
            enabled: true,
            choices: vec![remove_choice()],
            diff: None,
            backup: Some(backup),
        },
        // Rien d'Ash, et rien de personne : l'absence, écrite en toutes lettres. C'est la
        // demande la plus concrète de l'utilisateur — « on ne comprend pas » — et elle vient
        // de ce que l'absence ressemblait à un refus.
        Presence::Missing { others: 0, diff } => HooksReport {
            state: HookState::Missing,
            summary: "no ash hooks in this file".to_owned(),
            note: "nothing is installed, and nothing is in the way. until you install, \
                   the tool stays visible in the sidebar but never shows as waiting."
                .to_owned(),
            file: Some(shown),
            action: HookAction::Install,
            enabled: true,
            choices: vec![merge_choice()],
            diff: Some(diff),
            backup: Some(backup),
        },
        // Le cas des vrais utilisateurs : quelqu'un qui outille déjà son agent. Ce n'est
        // plus un refus — c'est un conflit qu'on montre, et qu'on tranche.
        Presence::Missing { others, diff } => HooksReport {
            state: HookState::Conflict,
            summary: format!(
                "{others} hook{} here {} not ash's",
                if others > 1 { "s" } else { "" },
                if others > 1 { "are" } else { "is" }
            ),
            note: "ash wrote nothing yet. see the diff of what it would add, then choose: \
                   merging keeps every hook already there — ash only ever writes, and later \
                   removes, what carries its own marker."
                .to_owned(),
            file: Some(shown),
            action: HookAction::SeeTheDiff,
            enabled: true,
            choices: vec![merge_choice()],
            diff: Some(diff),
            backup: Some(backup),
        },
        Presence::Superseded {
            installed,
            available,
            diff,
        } => HooksReport {
            state: HookState::Outdated,
            summary: if behind {
                format!("v{installed} · v{available} available")
            } else {
                // L'écart n'est pas dans ce sens-là : marqueur de version illisible (lu 0),
                // ou bloc posé par un Ash plus récent — deux builds dans le même
                // `~/.claude`. Nommer une version « disponible » plus basse que celle qui
                // est en place proposerait une mise à jour vers l'arrière ; la ligne dit
                // donc l'écart sans le chiffrer. **Ce que cette phrase devrait dire de
                // chacun des deux cas reste à trancher pour lui-même.**
                format!("v{installed} · out of date")
            },
            note: "until you update, ash keeps working — just coarser. nothing blinks.".to_owned(),
            file: Some(shown),
            action: HookAction::Update,
            enabled: true,
            choices: vec![update_choice(available), remove_choice()],
            diff: Some(diff),
            backup: Some(backup),
        },
        Presence::HandEdited { diff } => HooksReport {
            state: HookState::Conflict,
            summary: "ash's entries were edited by hand".to_owned(),
            note: "ash does not write of its own accord. it shows the diverging lines and \
                   lets you choose. a conflict does not degrade the display: the hooks \
                   already in place keep working."
                .to_owned(),
            file: Some(shown),
            action: HookAction::SeeTheDiff,
            enabled: true,
            choices: vec![rewrite_choice(), remove_choice()],
            diff: Some(diff),
            backup: Some(backup),
        },
        // Les deux refus qui restent : on ne devine pas où écrire, et on ne devine pas ce
        // qu'on n'a pas pu lire.
        Presence::NotAnObject => refusal(
            &shown,
            &format!("{shown} is not a JSON object ash can write into"),
            "ash wrote nothing: it wouldn't know where to put its entries. \
             fix the file, or point this entry at another folder.",
        ),
        Presence::Unreadable { why } => refusal(
            &shown,
            &format!("ash can't read {shown}"),
            &format!("ash wrote nothing — {why}"),
        ),
    }
}

/// Une ligne éteinte : le bouton reste, la raison est à gauche.
fn blocked(summary: &str, note: &str) -> HooksReport {
    HooksReport {
        state: HookState::Blocked,
        summary: summary.to_owned(),
        note: note.to_owned(),
        file: None,
        action: HookAction::Install,
        enabled: false,
        choices: Vec::new(),
        diff: None,
        backup: None,
    }
}

/// Un refus qui nomme son fichier — le blocage vient du fichier, pas de l'entrée.
fn refusal(file: &str, summary: &str, note: &str) -> HooksReport {
    HooksReport {
        file: Some(file.to_owned()),
        ..blocked(summary, note)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::settings::verification::Verification;

    /// Test Data Builder : une vérification qui autorise l'écriture, sans rien d'autre.
    fn allowing() -> Verification {
        let mut verification = Verification::unverified();
        verification.allows_hooks = true;
        verification.state = VerificationState::Valid;
        verification
    }

    /// Un fichier sans entrée d'Ash, qui porte `others` hooks de l'utilisateur.
    fn missing(others: usize) -> Presence {
        Presence::Missing {
            others,
            diff: "--- the file as it is\n+++ what ash would write\n+ ash-event".to_owned(),
        }
    }

    fn at(presence: Presence) -> Option<BlockAt> {
        Some(BlockAt {
            file: PathBuf::from("/home/someone/.claude/settings.json"),
            presence,
        })
    }

    #[test]
    fn given_an_entry_whose_path_is_not_verified_when_its_hooks_line_is_built_then_the_button_stays_but_stays_off(
    ) {
        // Given — « le bouton installer reste à sa place, éteint, avec sa raison à gauche.
        // le masquer ferait croire que les hooks n'existent pas pour cet outil. »
        let mut verification = Verification::unverified();
        verification.state = VerificationState::Invalid;

        // When
        let line = report(&verification, "claude-code", None, at(missing(0)));

        // Then — le fichier n'a même pas été regardé : c'est la séquence qui tranche
        assert_eq!(line.state, HookState::Blocked);
        assert_eq!(line.action, HookAction::Install);
        assert!(!line.enabled);
        assert_eq!(line.summary, "unavailable until the path is verified");
    }

    #[test]
    fn given_two_entries_on_the_same_folder_when_the_second_asks_for_its_hooks_then_it_names_the_one_that_holds_the_file(
    ) {
        // Given — le doublon n'invalide rien : il bloque la seconde écriture. Dire
        // « indisponible » sans nommer l'autre entrée laisserait chercher laquelle
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            Some(&Command::parse("claude").expect("un nom valide")),
            at(missing(0)),
        );

        // Then
        assert_eq!(line.state, HookState::Blocked);
        assert!(!line.enabled);
        assert_eq!(line.summary, "already written by claude in this file");
    }

    #[test]
    fn given_a_file_that_already_carries_hooks_of_its_own_when_the_line_is_built_then_it_offers_the_diff_and_a_merge_instead_of_a_dead_end(
    ) {
        // Given — c'est ce qu'un utilisateur ayant déjà ses propres hooks heurtait en
        // premier, et le produit devenait alors inutilisable pour lui : « déplace-les
        // toi-même » était la seule issue. Il doit voir un conflit, un diff, et un choix
        let verification = allowing();

        // When
        let line = report(&verification, "claude-code", None, at(missing(1)));

        // Then
        assert_eq!(line.state, HookState::Conflict);
        assert!(line.enabled, "le bouton du diff est allumé");
        assert_eq!(line.action, HookAction::SeeTheDiff);
        assert_eq!(line.summary, "1 hook here is not ash's");
        assert_eq!(
            line.choices
                .iter()
                .map(|choice| choice.action)
                .collect::<Vec<_>>(),
            [HookAction::Install],
            "et depuis le diff, il peut fusionner"
        );
        assert!(line.choices[0]
            .note
            .contains("nothing already there is replaced"));
        assert_eq!(
            line.backup.as_deref(),
            Some("/home/someone/.claude/settings.json.bak"),
            "la copie est annoncée avant le geste, parce qu'il y aura une écriture"
        );
    }

    #[test]
    fn given_a_file_with_no_hooks_at_all_when_the_line_is_built_then_the_absence_is_written_in_full(
    ) {
        // Given — « aujourd'hui l'absence ne se distingue pas assez du refus, et on ne
        // comprend pas ». Un mot — `missing` — ne dit ni ce qui manque, ni que rien ne
        // s'y oppose
        let verification = allowing();

        // When
        let line = report(&verification, "claude-code", None, at(missing(0)));

        // Then
        assert_eq!(line.state, HookState::Missing);
        assert_eq!(line.summary, "no ash hooks in this file");
        assert!(line.note.contains("nothing is in the way"));
        assert_eq!(line.action, HookAction::Install);
        assert!(line.diff.is_some(), "et le diff est là avant d'écrire");
    }

    #[test]
    fn given_a_file_ash_cannot_write_into_when_the_line_is_built_then_it_stays_a_refusal() {
        // Given — un `settings.json` qui n'est pas un objet JSON. La fusion a levé
        // l'impasse des hooks étrangers ; elle n'autorise pas à deviner où écrire
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            None,
            at(Presence::NotAnObject),
        );

        // Then
        assert_eq!(line.state, HookState::Blocked);
        assert!(!line.enabled);
        assert!(
            line.choices.is_empty(),
            "rien à trancher : rien ne s'écrira"
        );
        assert!(line.note.contains("ash wrote nothing"));
    }

    #[test]
    fn given_a_block_someone_edited_by_hand_when_the_line_is_built_then_it_offers_the_diff_and_promises_no_write(
    ) {
        // Given — « ash n'écrit pas. il montre les lignes qui divergent et laisse choisir. »
        // L'action ouvre un écran ; annoncer une sauvegarde promettrait une écriture
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            None,
            at(Presence::HandEdited {
                diff: "- moi\n+ ash".to_owned(),
            }),
        );

        // Then
        assert_eq!(line.state, HookState::Conflict);
        assert_eq!(line.action, HookAction::SeeTheDiff);
        assert_eq!(line.diff.as_deref(), Some("- moi\n+ ash"));
        assert_eq!(
            line.choices
                .iter()
                .map(|choice| choice.action)
                .collect::<Vec<_>>(),
            [HookAction::Install, HookAction::Remove],
            "les deux issues : remettre les entrées d'Ash, ou les retirer"
        );
    }

    #[test]
    fn given_a_block_from_an_older_ash_when_the_line_is_built_then_it_says_which_version_replaces_which(
    ) {
        // Given — `v1 · v2 available` : une direction, pas un statut. « Mettre à jour » sans
        // dire de quoi vers quoi ne dit pas ce qui changerait
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            None,
            at(Presence::Superseded {
                installed: 1,
                available: 2,
                diff: "- v1\n+ v2".to_owned(),
            }),
        );

        // Then
        assert_eq!(line.state, HookState::Outdated);
        assert_eq!(line.summary, "v1 · v2 available");
        assert_eq!(line.action, HookAction::Update);
        assert!(line.enabled);
    }

    #[test]
    fn given_an_adapter_that_instruments_nothing_when_the_line_is_built_then_it_says_so_instead_of_offering_an_install(
    ) {
        // Given — `generic` n'a pas de hooks, et ce n'est pas une panne : c'est ce qu'il
        // est. Proposer `install` ferait attendre un `waiting` qui n'arrivera jamais
        let verification = allowing();

        // When
        let line = report(&verification, "generic", None, None);

        // Then
        assert_eq!(line.state, HookState::Blocked);
        assert!(!line.enabled);
        assert_eq!(line.summary, "the generic adapter has no hooks to install");
        assert!(line.note.contains("never shows as waiting"));
    }

    #[test]
    fn given_a_tool_no_one_declared_when_its_line_is_built_then_the_file_alone_decides_and_no_verification_is_consulted(
    ) {
        // Given — la ligne d'une suggestion (ADR-0006) : ouvrir la fenêtre ne doit rien
        // vérifier, et le test 3 parcourt le `PATH` quand le test 4 lance la commande. Ce
        // qu'il reste est le fichier — et il donne les **cinq** états, pas les trois
        // d'`Instrumented` : `conflict` n'en fait pas partie
        let file = at(missing(1));

        // When
        let line = foreseen("claude-code", file);

        // Then — exactement ce qu'une carte déclarée dirait du même fichier
        assert_eq!(line.state, HookState::Conflict);
        assert_eq!(line.summary, "1 hook here is not ash's");
        assert_eq!(
            line.file.as_deref(),
            Some("/home/someone/.claude/settings.json")
        );
    }

    #[test]
    fn given_the_block_already_in_place_when_the_line_is_built_then_the_only_action_left_is_to_remove_it(
    ) {
        // Given — l'état nominal après une installation. Le geste restant est destructif,
        // et la copie se promet **avant** l'action, pas après (§4.2)
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            None,
            at(Presence::Current { version: 1 }),
        );

        // Then
        assert_eq!(line.state, HookState::Installed);
        assert_eq!(line.summary, "installed · v1");
        assert_eq!(line.action, HookAction::Remove);
        assert_eq!(
            line.choices
                .iter()
                .map(|choice| choice.action)
                .collect::<Vec<_>>(),
            [HookAction::Remove],
            "le geste inverse reste offert quand les entrées sont en place"
        );
        assert_eq!(
            line.backup.as_deref(),
            Some("/home/someone/.claude/settings.json.bak")
        );
    }
}
