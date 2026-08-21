//! Ce que la ligne de statut montre — les sept interrupteurs du menu contextuel de la
//! vue 5c (spec §4.2).
//!
//! **Septième préférence du même fichier**, et pour les raisons qui y ont déjà mis le
//! panneau bas : c'est une préférence d'apparence de la fenêtre, elle décide de ce qui
//! s'affiche, elle survit à la fermeture, et elle se relit au même moment que les six
//! autres. Le partage avec l'autre magasin de préférences du dépôt est net :
//! `features/agents/preferences.rs` détient ce que le **superviseur** consulte au moment
//! d'interrompre — il n'a pas de fenêtre à qui demander —, tandis que tout ce qu'une
//! **fenêtre rend** est ici.
//!
//! Ce qui est détenu, ce sont les **choix**, jamais le dessin : le retrait automatique de la
//! ligne trop étroite reste dans `src/features/terminal/terminal.css`, sous ses `@container`.
//! Les deux règles cohabitent sans se connaître — l'une dit ce que l'utilisateur veut lire,
//! l'autre ce qui tient dans la place restante.
//!
//! **La spec disait « par fenêtre » ; c'est amendé** (2026-08-21) : la phrase visait à
//! exclure un réglage par **onglet**, et réorganiser sa barre à chaque lancement n'est pas un
//! réglage. Le choix vit donc en Rust et dans `~/.ash/theme.json`
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).

/// Les sept segments de la ligne, dans l'ordre du menu de la vue 5c.
///
/// L'ordre de cette énumération **est** celui du menu, et le frontend le suit sans le
/// redéclarer : `session`, `weekly`, `context`, `model`, puis — après le trait — `agent`,
/// `branch`, `cwd`. Les quatre premiers parlent de ce que la conversation consomme, les
/// trois derniers d'où l'on est et de ce que l'agent fait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum StatusBarSegment {
    /// La pastille du quota de session — `s 63% · 2h14`.
    Session,
    /// La pastille du quota hebdomadaire — **masquée par défaut** (spec §4.2).
    Weekly,
    /// La jauge de contexte et son libellé, qui ne se séparent pas : le libellé double la
    /// barre, et une barre sans son chiffre ne se lit plus.
    Context,
    /// Le nom court du modèle qui tourne — `Opus 5 1M`.
    Model,
    /// Le glyphe d'état de l'agent, son processus et sa durée.
    Agent,
    /// La branche, l'opération en cours et l'état de l'arbre — un seul segment, comme dans
    /// la ligne : l'opération et les compteurs qualifient la branche qui les porte.
    Branch,
    /// Le répertoire de l'onglet actif.
    Cwd,
}

impl StatusBarSegment {
    /// Les sept, dans l'ordre du menu.
    pub const ALL: [StatusBarSegment; 7] = [
        StatusBarSegment::Session,
        StatusBarSegment::Weekly,
        StatusBarSegment::Context,
        StatusBarSegment::Model,
        StatusBarSegment::Agent,
        StatusBarSegment::Branch,
        StatusBarSegment::Cwd,
    ];
}

/// Ce que la ligne de statut montre, segment par segment (spec §4.2, vue 5c).
///
/// **Un booléen par segment, nommé**, et non une liste d'identifiants : le fichier se lit à
/// l'œil nu, et une liste aurait porté un ordre que rien n'utilise encore — l'ordre de la
/// barre est celui du code, et le réorganiser est une autre tâche (#165).
///
/// Chaque champ porte son propre `#[serde(default)]`, comme les trois interrupteurs de
/// notification : un fichier écrit avant qu'un segment soit coupable se relit sans le
/// masquer, et un champ manquant n'est pas un fichier incompréhensible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct StatusBarSegments {
    #[serde(default = "shown")]
    pub session: bool,
    /// **Éteint par défaut** : le weekly ne tient pas dans la ligne à côté du reste, et le
    /// popover est précisément là pour le montrer quand même (spec §4.2).
    #[serde(default)]
    pub weekly: bool,
    #[serde(default = "shown")]
    pub context: bool,
    #[serde(default = "shown")]
    pub model: bool,
    #[serde(default = "shown")]
    pub agent: bool,
    #[serde(default = "shown")]
    pub branch: bool,
    #[serde(default = "shown")]
    pub cwd: bool,
}

/// Les six segments que la ligne montre à la première ouverture.
fn shown() -> bool {
    true
}

impl Default for StatusBarSegments {
    fn default() -> Self {
        Self {
            session: true,
            weekly: false,
            context: true,
            model: true,
            agent: true,
            branch: true,
            cwd: true,
        }
    }
}

impl StatusBarSegments {
    /// Ce segment est-il montré ?
    #[must_use]
    pub fn shows(self, segment: StatusBarSegment) -> bool {
        match segment {
            StatusBarSegment::Session => self.session,
            StatusBarSegment::Weekly => self.weekly,
            StatusBarSegment::Context => self.context,
            StatusBarSegment::Model => self.model,
            StatusBarSegment::Agent => self.agent,
            StatusBarSegment::Branch => self.branch,
            StatusBarSegment::Cwd => self.cwd,
        }
    }

    /// Les mêmes choix, ce segment-là retourné.
    ///
    /// **Une bascule et non une valeur posée** : le menu montre ce que le backend détient,
    /// donc il demande un changement plutôt que d'annoncer un état qu'il aurait lu juste
    /// avant. C'est ce qui empêche deux panneaux ouverts coup sur coup de se répondre le
    /// même booléen ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) —
    /// la conduite de `toggle_sidebar_column`, pour la même raison.
    #[must_use]
    pub fn toggled(self, segment: StatusBarSegment) -> Self {
        let flipped = !self.shows(segment);
        match segment {
            StatusBarSegment::Session => Self {
                session: flipped,
                ..self
            },
            StatusBarSegment::Weekly => Self {
                weekly: flipped,
                ..self
            },
            StatusBarSegment::Context => Self {
                context: flipped,
                ..self
            },
            StatusBarSegment::Model => Self {
                model: flipped,
                ..self
            },
            StatusBarSegment::Agent => Self {
                agent: flipped,
                ..self
            },
            StatusBarSegment::Branch => Self {
                branch: flipped,
                ..self
            },
            StatusBarSegment::Cwd => Self {
                cwd: flipped,
                ..self
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_first_launch_when_the_line_asks_what_it_shows_then_only_the_weekly_quota_is_hidden()
    {
        // Given — les défauts de la spec §4.2, et le seul endroit du produit qui les porte :
        // la ligne n'a la place que d'un quota, et le popover est là pour montrer l'autre
        let defaults = StatusBarSegments::default();

        // When
        let hidden: Vec<StatusBarSegment> = StatusBarSegment::ALL
            .into_iter()
            .filter(|segment| !defaults.shows(*segment))
            .collect();

        // Then
        assert_eq!(hidden, vec![StatusBarSegment::Weekly]);
    }

    #[test]
    fn given_a_shown_segment_when_it_is_toggled_then_it_is_the_only_one_that_changes() {
        // Given — le `cwd`, montré comme les cinq autres
        let before = StatusBarSegments::default();

        // When — l'utilisateur le décoche dans le menu
        let after = before.toggled(StatusBarSegment::Cwd);

        // Then — la branche et l'état de l'agent restent en place (scénario de la tâche)
        assert!(!after.cwd);
        assert_eq!(
            after,
            StatusBarSegments {
                cwd: false,
                ..before
            }
        );
    }

    #[test]
    fn given_a_hidden_segment_when_it_is_toggled_then_it_comes_back() {
        // Given — le weekly, masqué par défaut : un segment masqué doit pouvoir revenir,
        // sans quoi le décocher serait définitif
        let before = StatusBarSegments::default();

        // When
        let after = before.toggled(StatusBarSegment::Weekly);

        // Then
        assert!(after.weekly);
        assert_eq!(after.toggled(StatusBarSegment::Weekly), before);
    }

    #[test]
    fn given_a_preference_file_written_before_a_segment_was_switchable_when_it_is_read_then_nothing_disappears(
    ) {
        // Given — un `theme.json` écrit par un Ash qui ne coupait que les deux quotas : les
        // cinq autres champs manquent
        let older = r#"{ "session": true, "weekly": true }"#;

        // When
        let read: StatusBarSegments =
            serde_json::from_str(older).expect("un fichier plus ancien reste lisible");

        // Then — un champ absent vaut « montré », et non « masqué » : une mise à jour d'Ash
        // ne doit jamais vider la ligne de statut de ce que personne n'a décoché
        assert_eq!(
            read,
            StatusBarSegments {
                weekly: true,
                ..StatusBarSegments::default()
            }
        );
    }

    #[test]
    fn given_a_choice_written_by_ash_when_it_is_read_back_then_it_is_the_same_choice() {
        // Given — le fichier est le seul lien entre deux sessions (critère : les choix
        // survivent à un redémarrage)
        let chosen = StatusBarSegments::default()
            .toggled(StatusBarSegment::Context)
            .toggled(StatusBarSegment::Weekly);

        // When
        let written = serde_json::to_string(&chosen).expect("les sept booléens se sérialisent");
        let read: Option<StatusBarSegments> = serde_json::from_str(&written).ok();

        // Then
        assert_eq!(read, Some(chosen));
    }
}
