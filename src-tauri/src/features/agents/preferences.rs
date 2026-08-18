//! Quels états ont le droit d'interrompre l'utilisateur, et le fichier qui s'en souvient
//! (spec §8, et le bloc `[notifications]` de la spec §9).
//!
//! **Ce module ne décide pas ce qu'une bannière dit** — c'est [`super::notify`], et lui seul,
//! qui sait quels états ont quelque chose à dire ([`super::notify::SWITCHABLE_STATES`]). Il
//! décide de ce que l'utilisateur, lui, laisse passer : trois interrupteurs, et rien d'autre.
//! Les deux règles se composent dans [`super::notify::notice`], et la seconde ne peut
//! qu'**enlever** — allumer `idle` ne fabrique aucune bannière, parce qu'il n'y a pas de
//! phrase pour `idle`.
//!
//! **Le choix vit ici, en Rust, et non dans un état de la webview**
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le producteur de
//! bannières est le superviseur, et il ne demande rien à personne au moment d'interrompre.
//! Un interrupteur qui vivrait dans la fenêtre de réglages ne pourrait filtrer que ce que la
//! fenêtre affiche — c'est-à-dire rien, puisqu'une bannière sort justement quand elle n'est
//! pas là.
//!
//! ## Le fichier
//!
//! `~/.ash/notifications.json`, et c'est **le mécanisme du thème** — un petit fichier dans le
//! dossier privé d'Ash, relu au démarrage, tolérant à tout, derrière un trait que la feature
//! possède (`features/theme/store.rs`). La spec §9 dessine ces trois booléens dans
//! `~/.ash/config.toml` ; rien du dépôt ne lit ni n'écrit encore de TOML, et fabriquer un
//! second mécanisme de persistance pour trois booléens coûterait plus que de suivre celui qui
//! existe. Le jour où `config.toml` sera lu, c'est ce fichier-là qui y sera versé — un seul
//! point de lecture à déplacer, celui-ci.
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | [`NotificationStore`] | [`FileNotificationStore`] — `~/.ash/notifications.json` | `FakeStore` (ci-dessous) |

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::state::AgentState;

/// Les trois interrupteurs de la spec §9, avec les défauts de la spec §8.
///
/// `done` est à **`false`**, et c'est une règle de produit, pas un goût : « `done` ne notifie
/// pas en v1 ». L'allumer est le seul moyen de le changer, et c'est un geste de
/// l'utilisateur — rien du code ne le fait à sa place.
/// **Rien de ce type ne traverse la frontière Tauri**, et il n'a donc pas de jumeau
/// TypeScript : ce que la fenêtre reçoit est la section composée par `settings`, un
/// interrupteur par ligne avec sa phrase. `serde` n'est là que pour le fichier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChoices {
    /// Un agent attend une réponse — le seul état qui vaille vraiment une interruption.
    #[serde(default = "interrupting")]
    pub waiting: bool,
    /// Un agent a échoué.
    #[serde(default = "interrupting")]
    pub error: bool,
    /// Un agent a terminé. Éteint par défaut (spec §8).
    #[serde(default)]
    pub done: bool,
}

/// Les deux interrupteurs que la spec §8 laisse allumés.
fn interrupting() -> bool {
    true
}

impl Default for NotificationChoices {
    fn default() -> Self {
        Self {
            waiting: true,
            error: true,
            done: false,
        }
    }
}

impl NotificationChoices {
    /// Cet état a-t-il le droit d'interrompre ?
    ///
    /// `idle` et `working` n'ont pas d'interrupteur et n'en auront pas : un agent qui
    /// travaille n'a rien à dire, et le `match` est exhaustif pour que l'ajout d'un sixième
    /// état oblige à répondre ici.
    #[must_use]
    pub fn allows(self, state: AgentState) -> bool {
        match state {
            AgentState::Waiting => self.waiting,
            AgentState::Error => self.error,
            AgentState::Done => self.done,
            AgentState::Idle | AgentState::Working => false,
        }
    }

    /// Les mêmes choix, cet interrupteur-là mis dans cette position.
    ///
    /// Un état sans interrupteur ne change rien : la fenêtre envoie un des cinq mots du
    /// contrat, et rien ne garantit ici que ce soit l'un des trois — c'est le seul endroit
    /// où le vérifier une fois pour toutes.
    #[must_use]
    pub fn with(self, state: AgentState, enabled: bool) -> Self {
        match state {
            AgentState::Waiting => Self {
                waiting: enabled,
                ..self
            },
            AgentState::Error => Self {
                error: enabled,
                ..self
            },
            AgentState::Done => Self {
                done: enabled,
                ..self
            },
            AgentState::Idle | AgentState::Working => self,
        }
    }
}

/// Où les trois interrupteurs se gardent d'une session à l'autre.
///
/// Un trait, comme tous les effets système du dépôt : sans lui, vérifier qu'un choix survit
/// au redémarrage demanderait d'écrire dans le `$HOME` de qui lance les tests.
pub trait NotificationStore: Send + Sync {
    /// Ce qui est gardé, ou `None` — première ouverture, fichier absent, fichier abîmé.
    fn load(&self) -> Option<NotificationChoices>;
    fn save(&self, choices: NotificationChoices) -> Result<(), std::io::Error>;
}

/// Les choix dans `~/.ash/notifications.json`.
pub struct FileNotificationStore {
    path: PathBuf,
}

impl FileNotificationStore {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// `~/.ash/notifications.json`, à côté du socket d'events et de `theme.json`.
    pub fn in_home() -> Self {
        Self::at(super::wire::ash_directory().join("notifications.json"))
    }
}

impl NotificationStore for FileNotificationStore {
    /// **Tolérante à tout**, comme celle du thème : un fichier absent, tronqué, vide ou
    /// rempli d'autre chose rend `None`, et Ash repart sur les défauts de la spec §8. Une
    /// préférence de notification n'est jamais une raison d'empêcher une fenêtre d'ouvrir.
    fn load(&self) -> Option<NotificationChoices> {
        decode(&std::fs::read_to_string(&self.path).ok()?)
    }

    fn save(&self, choices: NotificationChoices) -> Result<(), std::io::Error> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, encode(choices))
    }
}

/// Le contenu du fichier, ou `None` s'il ne dit rien qu'on comprenne.
///
/// Un champ manquant n'est **pas** un fichier incompréhensible : il vaut son défaut, et c'est
/// ce qui fait qu'un fichier écrit par un Ash antérieur au troisième interrupteur se relit
/// sans rien perdre. Un champ inconnu se laisse tomber, pour la raison écrite dans
/// `features/theme/appearance.rs` : revenir à la version précédente ne doit pas coûter le
/// réglage.
fn decode(content: &str) -> Option<NotificationChoices> {
    serde_json::from_str::<NotificationChoices>(content).ok()
}

fn encode(choices: NotificationChoices) -> String {
    format!(
        "{}\n",
        serde_json::to_string(&choices).unwrap_or_else(|_| String::from("{}"))
    )
}

/// Ce que l'utilisateur laisse interrompre — **la** source de vérité.
///
/// Le pendant de `ThemeState` pour la spec §8 : un choix détenu en Rust, relu au démarrage,
/// écrit à chaque geste, et lu par le seul endroit qui poste des bannières.
pub struct NotificationPreferences {
    current: Mutex<NotificationChoices>,
    store: Arc<dyn NotificationStore>,
}

impl NotificationPreferences {
    /// Repart des choix de la session précédente, ou des défauts de la spec §8.
    pub fn restore(store: Arc<dyn NotificationStore>) -> Self {
        let current = store.load().unwrap_or_default();
        Self {
            current: Mutex::new(current),
            store,
        }
    }

    pub fn choices(&self) -> NotificationChoices {
        *self.locked()
    }

    /// Met un interrupteur dans cette position, et rend les choix qui en résultent.
    ///
    /// L'écriture peut échouer — disque plein, `~/.ash` non inscriptible — et ça ne remet pas
    /// le choix en cause : il s'applique tout de suite, il ne survivra simplement pas au
    /// redémarrage. C'est la conduite de `ThemeState`, pour la même raison.
    pub fn choose(&self, state: AgentState, enabled: bool) -> NotificationChoices {
        let mut current = self.locked();
        let after = current.with(state, enabled);
        if after == *current {
            return after;
        }
        *current = after;
        drop(current);
        let _ = self.store.save(after);
        after
    }

    /// Un verrou empoisonné veut dire qu'un fil a paniqué **ailleurs** en le tenant. La
    /// valeur qu'il protège est un booléen : elle est intacte, et propager la panique ferait
    /// tomber la boucle de sonde pour un réglage de notification.
    fn locked(&self) -> std::sync::MutexGuard<'_, NotificationChoices> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le fichier de préférence, en mémoire.
    #[derive(Default)]
    struct FakeStore {
        content: Mutex<Option<NotificationChoices>>,
        /// Un disque qui refuse d'écrire — plein, ou en lecture seule.
        read_only: bool,
    }

    impl NotificationStore for FakeStore {
        fn load(&self) -> Option<NotificationChoices> {
            *self.content.lock().unwrap()
        }

        fn save(&self, choices: NotificationChoices) -> Result<(), std::io::Error> {
            if self.read_only {
                return Err(std::io::Error::other("lecture seule"));
            }
            *self.content.lock().unwrap() = Some(choices);
            Ok(())
        }
    }

    #[test]
    fn given_a_first_launch_when_ash_asks_what_may_interrupt_then_waiting_and_error_do_and_done_does_not(
    ) {
        // Given — le tableau de la spec §8, et le seul endroit du produit qui le porte. Un
        // `done` allumé par défaut ferait sonner la machine à chaque fin de tâche, et la
        // première conduite d'un utilisateur ainsi dérangé est de couper les notifications
        // d'Ash — donc de perdre `waiting`, la seule interruption qui compte
        let preferences = NotificationPreferences::restore(Arc::new(FakeStore::default()));

        // When
        let choices = preferences.choices();

        // Then
        assert!(choices.allows(AgentState::Waiting));
        assert!(choices.allows(AgentState::Error));
        assert!(!choices.allows(AgentState::Done));
    }

    #[test]
    fn given_a_switch_turned_off_in_a_previous_session_when_ash_starts_again_then_it_is_still_off()
    {
        // Given — c'est tout l'intérêt d'un réglage : le refaire à chaque lancement
        // reviendrait à ne pas l'avoir
        let store = Arc::new(FakeStore::default());
        let first =
            NotificationPreferences::restore(Arc::clone(&store) as Arc<dyn NotificationStore>);
        first.choose(AgentState::Waiting, false);
        first.choose(AgentState::Done, true);

        // When — la session suivante
        let next = NotificationPreferences::restore(store as Arc<dyn NotificationStore>);

        // Then
        assert!(!next.choices().allows(AgentState::Waiting));
        assert!(next.choices().allows(AgentState::Done));
    }

    #[test]
    fn given_a_preferences_file_that_says_nothing_understandable_when_it_is_read_then_ash_falls_back_to_the_spec_defaults(
    ) {
        // Given — un fichier tronqué par une coupure, vidé, ou édité à la main. Et le
        // deuxième sens, celui qu'on ne peut pas jouer en revenant en arrière : un fichier
        // écrit avant qu'un interrupteur n'existe, ou par un Ash qui en porte un de plus
        let unreadable = ["", "{", "null", "\"waiting\""];
        let partial = "{\"waiting\":false,\"cursor\":\"bar\"}";

        // When
        let read: Vec<Option<NotificationChoices>> = unreadable.iter().map(|c| decode(c)).collect();
        let survivor = decode(partial);

        // Then — rien d'illisible ne fait taire une bannière, et un champ manquant vaut son
        // défaut plutôt que de rendre tout le fichier caduc
        assert_eq!(read, vec![None; unreadable.len()]);
        assert_eq!(
            survivor,
            Some(NotificationChoices {
                waiting: false,
                error: true,
                done: false,
            })
        );
    }

    #[test]
    fn given_a_choice_saved_to_disk_when_a_new_session_loads_it_then_it_survived_the_restart() {
        // Given — le fichier est le seul lien entre deux sessions ; ce test est le seul qui
        // le touche vraiment
        let path = std::env::temp_dir()
            .join(format!("ash-notifications-{}", std::process::id()))
            .join("notifications.json");
        let store = FileNotificationStore::at(path.clone());

        // When
        store
            .save(NotificationChoices {
                waiting: true,
                error: false,
                done: true,
            })
            .expect("le dossier temporaire est inscriptible");
        let next_session = FileNotificationStore::at(path.clone()).load();

        // Then
        assert_eq!(
            next_session,
            Some(NotificationChoices {
                waiting: true,
                error: false,
                done: true,
            })
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn given_a_preference_that_cannot_be_written_when_the_user_flips_a_switch_then_it_still_applies(
    ) {
        // Given — `~/.ash` non inscriptible : refuser la bascule pour cette raison serait
        // incompréhensible pour qui vient de cliquer, et le laisserait interrompu par un
        // état qu'il a éteint sous ses yeux
        let preferences = NotificationPreferences::restore(Arc::new(FakeStore {
            read_only: true,
            ..FakeStore::default()
        }));

        // When
        let after = preferences.choose(AgentState::Waiting, false);

        // Then
        assert!(!after.allows(AgentState::Waiting));
        assert!(!preferences.choices().allows(AgentState::Waiting));
    }

    #[test]
    fn given_a_state_that_has_no_switch_when_something_asks_to_let_it_interrupt_then_it_still_does_not(
    ) {
        // Given — la fenêtre envoie l'un des cinq mots du contrat, et rien sur le fil ne
        // garantit que ce soit l'un des trois. Un `working` allumé poserait une bannière à
        // chaque fois qu'un agent se remet au travail, c'est-à-dire en permanence
        let choices = NotificationChoices::default();

        // When
        let forced = choices
            .with(AgentState::Working, true)
            .with(AgentState::Idle, true);

        // Then
        assert!(!forced.allows(AgentState::Working));
        assert!(!forced.allows(AgentState::Idle));
        assert_eq!(forced, choices);
    }
}
