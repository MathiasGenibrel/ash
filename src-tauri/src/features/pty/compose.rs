//! La règle qui encadre le seul texte qu'Ash a le droit d'écrire dans un PTY.
//!
//! [ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md) autorise Ash
//! à **rédiger** — jamais à envoyer — à trois conditions cumulatives : le texte est
//! visible dans le terminal, il est éditable comme s'il avait été tapé, et Ash ne presse
//! jamais `⏎`. Écrire des octets dans un PTY était déjà possible ; ce qui manquait, c'est
//! la règle. Elle tient ici, et elle se teste sans le moindre PTY.
//!
//! Trois refus et un report, dans cet ordre :
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

/// Ce que le registre retient d'un onglet pour arbitrer une composition.
///
/// Un compteur et un texte en attente. Rien de ce qui est ici ne vient de la **sortie** du
/// PTY — voir l'angle mort en tête de module.
#[derive(Debug, Default)]
pub struct ComposeDesk {
    /// Les octets entrés depuis la dernière validation.
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

    /// Le prompt est-il vide, pour autant qu'Ash puisse le savoir ?
    pub fn prompt_is_empty(&self) -> bool {
        self.pending == 0
    }

    /// Retient un texte le temps du tour en cours.
    pub fn hold(&mut self, text: String) {
        self.queued = Some(text);
    }

    /// Le texte retenu, s'il y en a un — et il n'est rendu qu'une fois.
    pub fn release(&mut self) -> Option<String> {
        self.queued.take()
    }

    /// Y a-t-il un texte en attente ?
    pub fn is_holding(&self) -> bool {
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

    #[test]
    fn given_a_text_held_for_the_end_of_a_turn_when_it_is_released_then_it_is_released_only_once() {
        // Given — le corollaire de file d'attente d'ADR-0015 : le texte part **une** fois,
        // à la fin du tour. Le rendre deux fois écrirait le prompt en double.
        let mut desk = ComposeDesk::default();
        desk.hold("resous les conflits".to_owned());

        // When
        let first = desk.release();
        let second = desk.release();

        // Then
        assert_eq!(first.as_deref(), Some("resous les conflits"));
        assert_eq!(second, None);
        assert!(!desk.is_holding());
    }
}
