//! Le port par lequel un onglet apprend son état d'agent.
//!
//! `pty` tient les onglets et la sonde ; il ne décide pas d'un état. La règle — un hook
//! fait autorité, `waiting` n'a pas d'autre source, une ligne finie s'efface au bout de
//! trente secondes — vit dans `features/agents`, avec les hooks et les adaptateurs qui la
//! nourrissent ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
//!
//! C'est la convention du dépôt — *les effets système passent par un trait que la feature
//! possède* — appliquée à une **décision** plutôt qu'à un effet, et pour le même bénéfice :
//! le registre se teste sans hooks, sans socket et sans horloge, et les deux features
//! continuent de s'ignorer. Le composition root les relie, comme il relie déjà `pty` à la
//! résolution de `git`.

use crate::features::agents::{AgentStatus, Presence};

use super::registry::TabId;

/// À qui le registre demande l'état d'un onglet.
///
/// Une seule question, et elle est posée à **chaque** passe de la boucle de sonde : c'est
/// elle qui fait avancer le temps des états qui expirent, et c'est sa réponse qui voyage
/// dans le `TabInfo` jusqu'à la webview
/// ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
pub trait AgentStates: Send + Sync {
    /// Quel état afficher pour cet onglet, compte tenu de ce que la sonde vient de voir ?
    ///
    /// La réponse porte sa **date d'entrée**, et non une durée : une durée changerait à
    /// chaque passe, donc la fiche de l'onglet aussi, donc l'event `ash://tab-changed`
    /// partirait chaque seconde pour chaque onglet actif. Le registre transporte une date
    /// stable ; le compteur qui s'incrémente est un problème d'affichage.
    fn state(&self, tab_id: &TabId, seen: Presence) -> AgentStatus;

    /// Cet onglet n'existe plus : rien de ce qui le concernait n'a à lui survivre.
    fn forget(&self, tab_id: &TabId);
}
