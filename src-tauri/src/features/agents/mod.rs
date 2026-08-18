//! Les agents : leur vocabulaire d'états, et ce qui le produit.
//!
//! Les cinq états sont la seule chose que le reste du produit a le droit de connaître
//! ([ADR-0008](../../../../docs/adr/0008-abstraction-adapter.md)). Trois pièces se
//! partagent le travail, et les frontières entre elles sont nettes :
//!
//! - le **transport** ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) — le
//!   socket unix par lequel un hook lancé dans un agent rejoint Ash, et le format qui y
//!   circule. C'est délibérément le côté qui **écoute** qui possède l'adresse du socket :
//!   `pty` la lui demande pour la poser dans `ASH_SOCK`, il n'en garde pas de copie ;
//! - le trait [`Adapter`], qui **traduit** le vocabulaire d'un outil vers le nôtre, et n'a
//!   aucun moyen d'en faire passer un sixième mot ;
//! - [`AgentMachine`], qui **décide** de l'état d'un onglet à partir de ce qui lui arrive
//!   (spec §6.4). Un adaptateur traduit ; il n'arbitre pas, et il ne connaît ni l'onglet
//!   ni l'horloge.
//!
//! Une quatrième pièce vit **à côté** de la machine, et non dedans : [`Subagents`], les
//! lignes filles d'un onglet (spec §6.5). Elle est séparée parce que l'amendement du
//! 2026-08-13 à ADR-0007 l'exige — le cycle de vie des enfants passe par une méthode
//! distincte du trait ([`Adapter::child_event`]), et aucun événement d'enfant n'a de chemin
//! vers l'état de l'onglet. La suite contractuelle le vérifie sur chaque implémentation.
//!
//! La couture entre les trois est [`Supervisor`] : il tient une machine par onglet, traduit
//! les [`EventFrame`] du socket en [`RawEvent`] puis en [`AgentEvent`], et répond à la
//! question que `pty` lui pose à chaque passe de sonde. La machine, elle, continue de
//! recevoir des événements déjà traduits sans savoir comment ils sont arrivés — c'est ce qui
//! permet de prouver toutes les règles de la spec §6.4 sans lancer ni processus, ni socket,
//! ni minuteur.
//!
//! **La feature n'expose rien au frontend, et c'est le résultat attendu** : un état d'agent
//! atteint l'écran par le `TabInfo` de `pty` et par l'event `ash://tab-changed` qui le
//! porte, pas par un second canal. Un event propre à `agents` n'aurait aucun abonné
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — il en a existé un,
//! `ash://agent-event`, qui poussait un verbe brut dans la webview et que personne
//! n'écoutait ; il a été retiré avec cette tranche.
//!
//! **Les effets système de la feature**, chacun avec ses deux adaptateurs — celui du
//! système, et celui des tests :
//!
//! | Port | Système | Tests |
//! |---|---|---|
//! | `EventSink` (`socket.rs`) | `HookEvents` (`lib.rs`) | `FakeSink` (`socket.rs`) |
//! | `Notifier` (`notify.rs`) | `AppNotifier` (`lib.rs`) | `FakeNotifier` (`fakes.rs`) |
//! | `NotificationStore` (`preferences.rs`) | `FileNotificationStore` (idem) | `FakeNotificationStore` (`fakes.rs`) |
//!
//! Le premier n'est **pas** le socket : celui-ci est l'effet que la feature exerce
//! elle-même, et ses tests l'exercent pour de vrai. Ce que `EventSink` abstrait, c'est la
//! **livraison** — savoir qu'un onglet existe, et prévenir la webview. C'est ce qui laisse
//! `agents` et `pty` s'ignorer, et ce qui rend l'écoute vérifiable sans lancer un seul PTY.
//!
//! Le second est la seule chose du produit qui sorte de la fenêtre (spec §8). Il est un
//! trait pour la raison la plus simple qui soit : aucun `cargo test` ne doit faire
//! apparaître une bannière sur l'écran de qui le lance. Ce qu'il porte — les deux états qui
//! interrompent, jamais quand Ash est devant, et jamais deux fois pour le même changement —
//! est décidé dans [`notify`], et posé par [`Supervisor`], seul endroit du système qui sache
//! qu'un état vient de **changer** plutôt que d'être.
//!
//! Le troisième garde ce que l'utilisateur, lui, laisse passer — les trois interrupteurs de
//! la spec §9 ([`NotificationChoices`]). Il est un effet système pour la même raison que celui
//! du thème : un choix qui survit au redémarrage vit dans un fichier, et aucun test ne doit
//! écrire dans le `$HOME` de qui le lance. **Le filtre est consulté sur le chemin qui poste**,
//! par [`Supervisor`] : une bannière ne sort que quand Ash est en arrière-plan, donc rien de
//! ce que la fenêtre pourrait filtrer n'arriverait à temps.

mod adapter;
mod adapters;
/// Privé et `#[cfg(test)]` : la suite contractuelle sert les implémentations de cette
/// feature, et personne d'autre. L'ouvrir au reste du crate inviterait une autre feature à
/// vérifier un adaptateur qu'elle n'a pas écrit — donc à connaître le trait par l'intérieur.
#[cfg(test)]
mod contract;
mod error;
#[cfg(test)]
mod fakes;
mod machine;
mod notify;
mod preferences;
mod providers;
mod socket;
mod state;
mod subagents;
mod supervisor;
mod wire;

pub use adapter::{
    hook_mark, Adapter, ChildEvent, HookEntry, Instrumentation, RawEvent, SubagentSupport,
    HOOK_MARK,
};
pub use adapters::{ClaudeCodeAdapter, GenericAdapter};
pub use error::AgentError;
pub use machine::{AgentEvent, AgentMachine, Declared, Exit, LINGER};
pub use notify::{Notice, Notifier, SwitchableState, SWITCHABLE_STATES};
pub use preferences::{
    FileNotificationStore, NotificationChoices, NotificationPreferences, NotificationStore,
};
pub use providers::{
    recognize, Declared as DeclaredProvider, Instrumented, ProgramIdentity, Provider,
    RecognizedAgent, RecognizedProvider, KNOWN_PROVIDERS,
};
pub use socket::{listen, EventSink, EventSocket};
pub use state::{AgentState, AgentStatus};
// `Subagents` reste privé : c'est la mémoire du superviseur, pas un type que `pty` ou le
// composition root aient à nommer. Ce qui sort est la ligne qui traverse la frontière, et le
// réglage que l'assemblage pose.
pub use subagents::{Subagent, SUBAGENT_LINGER};
pub use supervisor::{Presence, Supervisor, TabAgents};
pub use wire::{socket_path, EventFrame};
