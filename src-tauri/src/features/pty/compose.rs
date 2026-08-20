//! La règle qui encadre le seul texte qu'Ash a le droit d'écrire dans un PTY.
//!
//! [ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) autorise Ash
//! à **rédiger** — jamais à envoyer — à trois conditions cumulatives : le texte est
//! visible dans le terminal, il est éditable comme s'il avait été tapé, et Ash ne presse
//! jamais `⏎`. Écrire des octets dans un PTY était déjà possible ; ce qui manquait, c'est
//! la règle. Elle tient ici, et elle se teste sans le moindre PTY.
//!
//! Trois refus et un report, dans cet ordre. Tous sont tranchés par
//! [`ComposeDesk::arbitrate`] — **sauf le troisième**, que le registre traite avant même
//! d'arriver ici, parce que lui seul sait quels onglets existent :
//!
//! 1. **aucun outil reconnu dans l'avant-plan** — l'ADR parle de « passer le travail à
//!    l'agent qui tourne déjà là ». Un shell à son invite n'est pas ça, et le texte y
//!    serait une ligne de commande : le composer reviendrait à préparer une commande dans
//!    le terminal de quelqu'un ;
//! 2. **le prompt n'est pas vide** — l'ADR le demande mot pour mot : « Ash doit refuser de
//!    composer dans un prompt non vide plutôt que d'insérer au milieu de la frappe » ;
//! 3. **l'onglet n'existe plus** — le registre répond seul là-dessus ;
//! 4. **un tour d'agent est en cours** — le texte est retenu et part à la fin du tour
//!    (« queued behind the current turn »), pour qu'il atterrisse dans le prompt et non au
//!    milieu d'une sortie. C'est un problème de placement, pas d'autorisation.
//!
//! # Comment Ash sait qu'un prompt est vide — et ce qu'il ne sait pas
//!
//! **Uniquement par ce qui a été écrit dans le PTY.** Chaque frappe de l'utilisateur passe
//! par `pty_write` ; le texte composé passe par le même chemin. Le pupitre compte donc les
//! octets **entrés** depuis la dernière validation. Il ne lit **jamais** la sortie du PTY :
//! déduire un état d'un flux de terminal est interdit
//! ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)), et ce serait la même faute
//! ici que pour un état d'agent.
//!
//! Le compte est **approximatif**, et toutes ses approximations penchent du même côté :
//! celui du refus. Une flèche du clavier, un `⌥⌫` qui efface un mot entier, un `⌃C` avalé
//! par un programme — le pupitre en sort avec un compte supérieur ou égal à la réalité,
//! donc il refuse là où il aurait pu accepter. C'est le sens que l'ADR demande.
//!
//! **L'angle mort** — nommé ici comme `agents/subagents.rs` nomme le sien : un programme
//! qui **pré-remplit** lui-même sa ligne de saisie (une session d'agent reprise avec un
//! brouillon, une recherche d'historique qui propose une commande) le fait par sa
//! **sortie**, que le pupitre ne regarde pas. Dans ce cas-là, et dans ce cas-là seulement,
//! Ash croit le prompt vide alors qu'il ne l'est pas, et compose par-dessus. Il n'existe
//! aucune source honnête pour ce cas : le savoir demanderait de lire le terminal.
//! L'utilisateur, lui, voit le résultat — c'est la première condition d'ADR-0015 — et il
//! peut tout effacer, ce qui est la seconde.

/// Ce qu'il est advenu d'une demande de composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "kebab-case")]
pub enum ComposeOutcome {
    /// Le texte est dans le terminal, visible et éditable. **Il n'a pas été envoyé.**
    Written,
    /// Un tour d'agent est en cours : le texte partira à sa fin.
    Queued,
    /// Il y a déjà quelque chose dans le prompt — Ash n'insère pas au milieu d'une frappe.
    PromptNotEmpty,
    /// Aucun outil reconnu ne tient l'avant-plan de cet onglet.
    NoAgent,
}

/// Ce que le registre voit de l'onglet au moment où on lui demande de composer.
///
/// Deux booléens, et **pas** un `TabInfo` : le pupitre n'a que faire du répertoire, du
/// titre ou de la place de l'onglet, et les lui donner rendrait la règle d'ADR-0015
/// intestable sans construire une fiche d'onglet entière. C'est le registre qui traduit ce
/// qu'il a annoncé en ces deux faits ; c'est le pupitre qui en tire l'issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Foreground {
    /// Un outil reconnu tient-il l'avant-plan ? (`features/agents` par le port
    /// [`super::recognition`] — `pty` ne connaît aucun nom d'outil.)
    pub agent_is_running: bool,
    /// Un tour d'agent est-il en cours ? C'est le seul cas qui **retient** au lieu de
    /// refuser.
    pub turn_in_progress: bool,
}

/// Ce que le registre retient d'un onglet pour arbitrer une composition.
///
/// Un compteur et un texte en attente. Rien de ce qui est ici ne vient de la **sortie** du
/// PTY — voir l'angle mort en tête de module.
///
/// # Invariant
///
/// `pending` est un **majorant** de ce que porte la ligne de saisie : jamais moins que la
/// réalité, parfois plus. Toute évolution de [`Self::wrote`] doit le préserver, parce que
/// c'est lui qui garantit que l'approximation penche du côté du refus.
#[derive(Debug, Default)]
pub struct ComposeDesk {
    /// Les octets entrés depuis la dernière validation. Majorant, jamais minorant.
    pending: usize,
    /// Le texte retenu le temps d'un tour d'agent.
    queued: Option<String>,
}

impl ComposeDesk {
    /// Prend acte de ce qui vient d'être écrit dans le PTY — frappe de l'utilisateur comme
    /// texte composé par Ash.
    pub fn wrote(&mut self, bytes: &[u8]) {
        for byte in bytes {
            match byte {
                // `⏎` : la ligne est partie. C'est le seul geste qui vide vraiment le
                // prompt, et il n'appartient qu'à l'utilisateur.
                b'\r' | b'\n' => self.pending = 0,
                // `⌃C` abandonne la ligne, `⌃U` la tue : les deux la vident pour de bon
                // dans un shell comme dans une saisie de style readline.
                0x03 | 0x15 => self.pending = 0,
                // Effacement arrière, et effacement de mot. Le second retire plus d'un
                // caractère ; on n'en décompte qu'un, parce que sous-estimer l'effacement
                // fait pencher le pupitre vers le refus, qui est le côté sûr.
                0x7f | 0x08 | 0x17 => self.pending = self.pending.saturating_sub(1),
                _ => self.pending += 1,
            }
        }
    }

    /// **La règle d'ADR-0015**, et le seul endroit où elle se lit : les trois refus et le
    /// report, dans l'ordre documenté en tête de module.
    ///
    /// Le pupitre décide **et** agit sur ce qui lui appartient : quand il rend
    /// [`ComposeOutcome::Queued`], le texte est déjà retenu ici. Ce qu'il ne fait jamais,
    /// c'est écrire dans le PTY — il n'en a pas le moyen, et c'est voulu : cette règle se
    /// teste sans le moindre PTY.
    ///
    /// [`ComposeOutcome::Written`] est donc une **instruction** pour l'appelant, pas un
    /// constat : le registre, et lui seul, pose alors les octets dans le terminal.
    pub fn arbitrate(&mut self, foreground: Foreground, text: &str) -> ComposeOutcome {
        // « Passer le travail à l'agent qui tourne déjà là » (ADR-0015) : un shell à son
        // invite n'est pas ça, et le texte y serait une ligne de commande.
        if !foreground.agent_is_running {
            return ComposeOutcome::NoAgent;
        }
        // Un texte déjà retenu compte comme un prompt occupé : il ira dans cette ligne de
        // saisie à la fin du tour, et en composer un second l'y rejoindrait.
        if !self.prompt_is_empty() || self.is_holding() {
            return ComposeOutcome::PromptNotEmpty;
        }
        if foreground.turn_in_progress {
            // La frappe atterrirait au milieu d'une sortie : on attend la fin du tour.
            // C'est un problème de placement, pas d'autorisation — l'envoi reste celui de
            // l'utilisateur, avant comme après.
            self.queued = Some(text.to_owned());
            return ComposeOutcome::Queued;
        }
        ComposeOutcome::Written
    }

    /// Le texte retenu, **quand le tour est fini** — et il n'est rendu qu'une fois.
    ///
    /// L'autre moitié du corollaire de file d'attente d'ADR-0015 : `arbitrate` retient,
    /// ceci rend. Rien ne sort tant que le tour dure, pour que la frappe atterrisse dans
    /// le prompt et non au milieu d'une sortie. **Ce n'est pas un envoi différé** : le
    /// texte sera écrit, jamais validé.
    pub fn release_after_turn(&mut self, turn_in_progress: bool) -> Option<String> {
        if turn_in_progress {
            return None;
        }
        self.queued.take()
    }

    /// Le prompt est-il vide, pour autant qu'Ash puisse le savoir ?
    fn prompt_is_empty(&self) -> bool {
        self.pending == 0
    }

    /// Y a-t-il un texte en attente ?
    fn is_holding(&self) -> bool {
        self.queued.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test Data Builder : un pupitre, et ce qui a été écrit dans son PTY.
    ///
    /// Défaut valide et déterministe : rien n'a été écrit, donc le prompt est vide.
    #[derive(Default)]
    struct DeskBuilder {
        desk: ComposeDesk,
    }

    impl DeskBuilder {
        fn typed(mut self, keys: &str) -> Self {
            self.desk.wrote(keys.as_bytes());
            self
        }

        fn pressed(mut self, byte: u8) -> Self {
            self.desk.wrote(&[byte]);
            self
        }
    }

    #[test]
    fn given_a_terminal_where_nothing_has_been_typed_when_asking_whether_the_prompt_is_empty_then_it_is(
    ) {
        // Given / When
        let desk = DeskBuilder::default().desk;

        // Then
        assert!(desk.prompt_is_empty());
    }

    #[test]
    fn given_a_user_who_is_typing_when_ash_wants_to_compose_then_the_prompt_is_not_empty() {
        // Given — le cas que l'ADR-0015 demande de traiter : insérer au milieu d'une
        // frappe mélangerait deux textes, et l'utilisateur enverrait ce mélange
        let desk = DeskBuilder::default().typed("expliq").desk;

        // Then
        assert!(!desk.prompt_is_empty());
    }

    #[test]
    fn given_a_line_that_the_user_has_sent_when_looking_again_then_the_prompt_is_empty_once_more() {
        // Given — `⏎` est le seul geste qui vide vraiment le prompt, et il n'appartient
        // qu'à l'utilisateur (ADR-0015)
        let desk = DeskBuilder::default().typed("ls -la\r").desk;

        // Then
        assert!(desk.prompt_is_empty());
    }

    #[test]
    fn given_a_line_abandoned_with_ctrl_c_when_looking_again_then_the_prompt_is_empty() {
        // Given — abandonner une ligne est aussi une façon de la vider, et c'est le geste
        // qu'on fait juste avant de demander autre chose
        let desk = DeskBuilder::default()
            .typed("mauvaise commande")
            .pressed(0x03)
            .desk;

        // Then
        assert!(desk.prompt_is_empty());
    }

    #[test]
    fn given_a_word_erased_with_option_backspace_when_the_line_is_not_yet_empty_then_ash_still_refuses(
    ) {
        // Given — `⌥⌫` efface un mot entier, et le pupitre n'en décompte qu'un caractère.
        // L'approximation penche vers le refus, jamais vers l'écriture par-dessus : c'est
        // le sens qu'ADR-0015 demande.
        let desk = DeskBuilder::default()
            .typed("resume le diff")
            .pressed(0x17)
            .desk;

        // Then
        assert!(!desk.prompt_is_empty());
    }

    #[test]
    fn given_a_prompt_emptied_one_backspace_at_a_time_when_the_last_one_is_pressed_then_it_is_empty(
    ) {
        // Given — l'utilisateur efface ce qu'il avait commencé, caractère par caractère
        let desk = DeskBuilder::default()
            .typed("ab")
            .pressed(0x7f)
            .pressed(0x7f)
            // Un effacement de trop ne rend pas le compte négatif : il n'y a rien
            // au-dessous de « vide ».
            .pressed(0x7f)
            .desk;

        // Then
        assert!(desk.prompt_is_empty());
    }

    /// Un onglet où un agent tourne, entre deux tours : le cas nominal d'ADR-0015.
    const AGENT_AT_REST: Foreground = Foreground {
        agent_is_running: true,
        turn_in_progress: false,
    };

    #[test]
    fn given_an_agent_at_rest_and_an_empty_prompt_when_ash_composes_then_the_registry_is_told_to_write(
    ) {
        // Given
        let mut desk = DeskBuilder::default().desk;

        // When
        let outcome = desk.arbitrate(AGENT_AT_REST, "resous les conflits");

        // Then
        assert_eq!(outcome, ComposeOutcome::Written);
        // Rien n'est retenu : c'est l'appelant qui écrit, tout de suite.
        assert_eq!(desk.release_after_turn(false), None);
    }

    #[test]
    fn given_a_tab_where_no_recognized_tool_holds_the_foreground_when_ash_composes_then_it_refuses()
    {
        // Given — un shell à son invite : le texte y serait une ligne de commande, et
        // ADR-0015 parle de passer le travail à l'agent qui tourne déjà là
        let mut desk = DeskBuilder::default().desk;
        let shell = Foreground {
            agent_is_running: false,
            turn_in_progress: false,
        };

        // When
        let outcome = desk.arbitrate(shell, "resous les conflits");

        // Then
        assert_eq!(outcome, ComposeOutcome::NoAgent);
    }

    #[test]
    fn given_a_prompt_the_user_has_started_typing_when_ash_composes_then_it_refuses_rather_than_inserting(
    ) {
        // Given — l'ADR-0015 le demande mot pour mot : pas d'insertion au milieu d'une
        // frappe, parce que l'utilisateur enverrait le mélange des deux textes
        let mut desk = DeskBuilder::default().typed("expliq").desk;

        // When
        let outcome = desk.arbitrate(AGENT_AT_REST, "resous les conflits");

        // Then
        assert_eq!(outcome, ComposeOutcome::PromptNotEmpty);
    }

    #[test]
    fn given_an_agent_in_the_middle_of_a_turn_when_ash_composes_then_the_text_waits_for_the_end_of_the_turn(
    ) {
        // Given — le corollaire de file d'attente : Ash écrit quand même, plus tard
        let mut desk = DeskBuilder::default().desk;
        let working = Foreground {
            agent_is_running: true,
            turn_in_progress: true,
        };

        // When
        let outcome = desk.arbitrate(working, "resous les conflits");

        // Then
        assert_eq!(outcome, ComposeOutcome::Queued);
        assert_eq!(desk.release_after_turn(true), None);
        assert_eq!(
            desk.release_after_turn(false).as_deref(),
            Some("resous les conflits")
        );
    }

    #[test]
    fn given_a_text_already_waiting_for_the_end_of_a_turn_when_ash_composes_again_then_it_refuses()
    {
        // Given — le texte retenu ira dans cette ligne de saisie : en composer un second
        // l'y rejoindrait, et l'utilisateur enverrait les deux d'un seul `⏎`
        let mut desk = DeskBuilder::default().desk;
        let working = Foreground {
            agent_is_running: true,
            turn_in_progress: true,
        };
        desk.arbitrate(working, "resous les conflits");

        // When
        let outcome = desk.arbitrate(working, "et relance les tests");

        // Then
        assert_eq!(outcome, ComposeOutcome::PromptNotEmpty);
        assert_eq!(
            desk.release_after_turn(false).as_deref(),
            Some("resous les conflits")
        );
    }

    #[test]
    fn given_a_text_held_for_the_end_of_a_turn_when_it_is_released_then_it_is_released_only_once() {
        // Given — le corollaire de file d'attente d'ADR-0015 : le texte part **une** fois,
        // à la fin du tour. Le rendre deux fois écrirait le prompt en double.
        let mut desk = ComposeDesk::default();
        desk.arbitrate(
            Foreground {
                agent_is_running: true,
                turn_in_progress: true,
            },
            "resous les conflits",
        );

        // When
        let first = desk.release_after_turn(false);
        let second = desk.release_after_turn(false);

        // Then
        assert_eq!(first.as_deref(), Some("resous les conflits"));
        assert_eq!(second, None);
    }
}
