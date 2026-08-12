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
#[serde(rename_all = "camelCase")]
pub enum HookState {
    /// Le bloc en place est celui qu'Ash écrirait.
    Installed,
    /// Rien n'est posé, et rien n'empêche de le poser.
    Missing,
    /// Un bloc d'Ash est là, mais pas celui-ci.
    Outdated,
    /// Une main est passée dans le bloc : **Ash n'écrit pas**, et montre le diff.
    Conflict,
    /// Quelque chose empêche d'écrire. La raison est dans [`HooksReport::summary`].
    Blocked,
}

/// Ce que le bouton de la ligne propose. Un seul, jamais deux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HookAction {
    Install,
    Update,
    /// Destructif, donc secondaire dans la fenêtre.
    Remove,
    /// **N'écrit rien** : elle ouvre un écran. C'est ce qui distingue le conflit des autres.
    SeeTheDiff,
}

/// Ce que la ligne `hooks` d'une entrée affiche, et ce qu'elle laisse faire.
///
/// Elle voyage **avec** la déclaration, comme la vérification, et pour la même raison : une
/// entrée dont le chemin change change d'état de hooks au même instant, et deux tables
/// séparées laisseraient un intervalle où l'écran montrerait l'un sous l'autre.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksReport {
    pub state: HookState,
    /// La phrase de la ligne — `installed · v1`, `missing`, `v1 · v2 available`…
    pub summary: String,
    /// Les deux lignes de prose sous la ligne : **la conséquence**, pas la répétition.
    pub note: String,
    /// Le fichier concerné, quand il y en a un.
    pub file: Option<String>,
    pub action: HookAction,
    /// Le bouton est-il allumé ? Il reste **visible** dans tous les cas.
    pub enabled: bool,
    /// Les lignes qui divergent — seulement en conflit, et c'est le refus lui-même.
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

    // 4 — ce que le fichier porte.
    match presence {
        Presence::Current { version } => HooksReport {
            state: HookState::Installed,
            summary: format!("installed · v{version}"),
            note: "remove deletes the block and its markers, leaves the rest of the file \
                   intact, and writes a .bak first."
                .to_owned(),
            file: Some(shown),
            action: HookAction::Remove,
            enabled: true,
            diff: None,
            backup: Some(backup),
        },
        Presence::Missing => HooksReport {
            state: HookState::Missing,
            summary: "missing".to_owned(),
            note: "the tool stays visible in the sidebar, but without waiting: \
                   ash can't tell that it is waiting."
                .to_owned(),
            file: Some(shown),
            action: HookAction::Install,
            enabled: true,
            diff: None,
            backup: Some(backup),
        },
        Presence::Superseded {
            installed,
            available,
        } => HooksReport {
            state: HookState::Outdated,
            summary: if installed < available {
                format!("v{installed} · v{available} available")
            } else {
                // Même numéro, autre contenu : le bloc a changé de forme sans changer de
                // version. Écrire « v1 · v1 available » ferait lire une erreur d'affichage.
                format!("v{installed} · out of date")
            },
            note: "until you update, ash keeps working — just coarser. nothing blinks.".to_owned(),
            file: Some(shown),
            action: HookAction::Update,
            enabled: true,
            diff: None,
            backup: Some(backup),
        },
        Presence::HandEdited { diff } => HooksReport {
            state: HookState::Conflict,
            summary: "block edited by hand".to_owned(),
            note: "ash does not write. it shows the diverging lines and lets you choose. \
                   a conflict does not degrade the display: the hooks already in place keep \
                   working."
                .to_owned(),
            file: Some(shown),
            action: HookAction::SeeTheDiff,
            enabled: true,
            diff: Some(diff),
            // Rien ne sera écrit : annoncer une copie promettrait une action qui n'aura
            // pas lieu.
            backup: None,
        },
        // Le refus que les vrais utilisateurs heurtent en premier. Il nomme le fichier, dit
        // qu'Ash n'a rien écrit, et pourquoi il ne le fera pas de lui-même.
        Presence::ForeignHooks => refusal(
            &shown,
            &format!("{shown} already carries hooks that aren't ash's"),
            "ash wrote nothing. merging them would mean editing outside its markers, and \
             writing a second \"hooks\" key would silently disable yours. move them into \
             the ash block yourself, or point this entry at another folder.",
        ),
        Presence::NotAnObject => refusal(
            &shown,
            &format!("{shown} is not a JSON object"),
            "ash wrote nothing: it wouldn't know where to put its block.",
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
        let line = report(&verification, "claude-code", None, at(Presence::Missing));

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
            at(Presence::Missing),
        );

        // Then
        assert_eq!(line.state, HookState::Blocked);
        assert!(!line.enabled);
        assert_eq!(line.summary, "already written by claude in this file");
    }

    #[test]
    fn given_a_file_that_already_carries_hooks_of_its_own_when_the_line_is_built_then_it_names_the_file_and_says_nothing_was_written(
    ) {
        // Given — c'est le refus qu'un utilisateur ayant déjà ses propres hooks heurte en
        // premier. « Ça a échoué » ne suffit pas : il lui faut le fichier, et la raison
        let verification = allowing();

        // When
        let line = report(
            &verification,
            "claude-code",
            None,
            at(Presence::ForeignHooks),
        );

        // Then
        assert_eq!(line.state, HookState::Blocked);
        assert!(!line.enabled);
        assert!(line.summary.contains("/home/someone/.claude/settings.json"));
        assert!(line.note.contains("ash wrote nothing"));
        assert_eq!(
            line.file.as_deref(),
            Some("/home/someone/.claude/settings.json")
        );
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
                diff: "- ash\n+ moi".to_owned(),
            }),
        );

        // Then
        assert_eq!(line.state, HookState::Conflict);
        assert_eq!(line.action, HookAction::SeeTheDiff);
        assert_eq!(line.diff.as_deref(), Some("- ash\n+ moi"));
        assert_eq!(line.backup, None);
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
            line.backup.as_deref(),
            Some("/home/someone/.claude/settings.json.bak")
        );
    }
}
