import type { AgentState } from "@/shared/ipc";

/**
 * La règle d'état qui appartient à la **sidebar** : celui qu'une ligne repliée montre à la
 * place de ses enfants.
 *
 * La **présentation** des cinq états — glyphe, mot, teinte — n'est pas ici : elle a un
 * second lecteur depuis la ligne de statut de la zone terminal, donc elle vit dans
 * [`@/shared/agent-state`]. Ce qui reste ici est ce que la sidebar est seule à décider.
 *
 * Rien n'est **produit** : les états viennent du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et trois d'entre eux des
 * hooks d'[ADR-0007](../../../docs/adr/0007-etats-par-hooks.md).
 */

/**
 * L'état qu'une ligne de dépôt ou de worktree montre pour ses enfants.
 *
 * L'ordre d'urgence n'est pas cosmétique : `waiting` est le seul état qui **demande**
 * quelque chose à l'utilisateur, donc il l'emporte sur tout, y compris sur une erreur —
 * une erreur attendra, une question bloque un agent. `idle` ne remonte jamais tant qu'il
 * reste autre chose à dire.
 */
export function bubbleState(states: readonly AgentState[]): AgentState {
    const urgency: readonly AgentState[] = ["waiting", "error", "working", "done", "idle"];
    return urgency.find((state) => states.includes(state)) ?? "idle";
}
