import { elapsedSince } from "@/shared/agent-state";
import type { AgentState, Subagent } from "@/shared/ipc";
import { truncate } from "./labels";

/**
 * Les lignes filles d'un onglet : ce qu'elles portent, et comment on les nomme.
 *
 * Une ligne de sous-agent est **inerte** : elle n'a pas de terminal à elle — c'est le même
 * processus, dans le même onglet ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md))
 * — donc cliquer dessus sélectionne le **parent**, et non elle. Ce module ne le décide pas :
 * il compose ce qu'on lit, et [`./view`] pose l'action du parent sur toute la ligne.
 *
 * Rien n'est produit ici non plus que dans le reste de la colonne : le backend dit quels
 * enfants tournent, dans quel état, et depuis quand
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce module met en forme.
 */

/** Ce qu'un enfant qui ne dit pas son type devient à l'écran. */
export const ANONYMOUS_SUBAGENT = "subagent";

/**
 * Au-delà, le nom d'un sous-agent est coupé.
 *
 * Plus court que [`MAX_LABEL`](./labels) parce que la ligne est **indentée d'un niveau de
 * plus** et qu'elle partage sa largeur avec un glyphe, un état et une durée. Rien ne garantit
 * qu'un `agentType` soit court : `Explore` tient, `dev-integration-with-a-long-name` non.
 */
export const MAX_SUBAGENT_LABEL = 18;

/** Un enfant, rangé pour la colonne : nommé, daté, jamais résolu. */
export interface SubagentNode {
    readonly agentId: string;
    /** Le nom affiché, déjà tronqué. */
    readonly label: string;
    /** Le nom entier, pour l'infobulle. */
    readonly title: string;
    readonly state: AgentState;
    /** La date d'entrée dans cet état, telle que le backend l'a envoyée. */
    readonly since: number;
}

/** Ce qu'une ligne fille montre, à un instant donné. */
export interface SubagentRow {
    readonly label: string;
    readonly title: string;
    readonly state: AgentState;
    /** `1m20s`, ou `null` quand il n'y a rien d'honnête à écrire. */
    readonly elapsed: string | null;
}

/**
 * Les enfants d'un onglet, tels que la colonne les range.
 *
 * Un enfant que l'outil n'a pas nommé garde sa ligne : son `agentId` le distingue de ses
 * frères, et le masquer effacerait un travail qui a lieu. C'est seulement son **libellé**
 * qui devient générique.
 */
export function subagentNodes(subagents: readonly Subagent[]): readonly SubagentNode[] {
    return subagents.map((child) => {
        const name = child.agentType ?? ANONYMOUS_SUBAGENT;
        return {
            agentId: child.agentId,
            label: truncate(name, MAX_SUBAGENT_LABEL),
            title: name,
            state: child.state,
            since: child.since,
        };
    });
}

/**
 * Ce qu'une ligne fille affiche maintenant.
 *
 * La durée se calcule **ici**, à chaque rendu, à partir d'une date absolue : c'est la même
 * discipline que la ligne de statut, et pour la même raison — une durée transportée ferait
 * changer la fiche de l'onglet à chaque seconde, donc redessiner la colonne entière pour
 * animer un compteur.
 */
export function composeSubagentRow(node: SubagentNode, now: number): SubagentRow {
    return {
        label: node.label,
        title: node.title,
        state: node.state,
        elapsed: elapsedSince(node.since, now),
    };
}
