//! Le port par lequel un onglet apprend **quel outil** tient son avant-plan.
//!
//! Le pendant exact de [`super::agent_states`] : `pty` tient les onglets et la sonde, il ne
//! décide pas de ce qu'est un agent. La table des outils connus vit dans `features/agents`
//! ([ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)), et la
//! conciliation entre cette table et les entrées de `~/.ash/tools.json` vit dans
//! `features/settings`, qui possède la liste des commandes reconnues.
//!
//! Le registre **demande**, il ne déduit pas — c'est ce qui lui évite de connaître un seul
//! nom d'outil, et ce qui laisse les trois features s'ignorer.

use crate::features::agents::{ProgramIdentity, RecognizedAgent};

/// À qui le registre demande l'identité de l'outil qui tient l'avant-plan d'un onglet.
///
/// La question est posée à **chaque** passe de la boucle de sonde : la réponse doit donc
/// être stable pour un même programme, sans quoi la fiche d'onglet changerait trois fois par
/// seconde et l'event `ash://tab-changed` deviendrait un flux (voir
/// [`super::registry::TabInfo`]).
///
/// Elle ne produit **aucun état d'agent** : reconnaître un outil n'est pas savoir ce qu'il
/// fait ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)).
pub trait AgentRecognition: Send + Sync {
    fn recognize(&self, program: &ProgramIdentity) -> Option<RecognizedAgent>;
}

/// Ce qu'un assemblage qui ne reconnaît rien répond — et ce que valait Ash avant ADR-0006.
///
/// Il existe pour les tests du registre qui ne parlent pas d'agents : la plupart de ses
/// règles — ordre, fermeture, crédits, localisation — n'ont rien à voir avec la
/// reconnaissance.
pub struct NoRecognition;

impl AgentRecognition for NoRecognition {
    fn recognize(&self, _program: &ProgramIdentity) -> Option<RecognizedAgent> {
        None
    }
}
