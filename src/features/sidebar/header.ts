import type { AgentState } from "@/shared/ipc";
import type { SidebarTree } from "./tree";

/**
 * Le compteur agrégé de l'en-tête — `1 waiting / 7 agents` (spec §4.1).
 *
 * La spec en fait une exigence en deux temps : le compteur dit combien d'agents attendent,
 * et **il reste visible quand la colonne est repliée**. Or repliée, la colonne fait 46 px :
 * `1 waiting / 7 agents` n'y tient pas. L'en-tête a donc deux formes, et c'est la seule
 * décision de ce module.
 *
 * Ce qui compte est le **nombre en attente** : c'est lui qui demande quelque chose à
 * l'utilisateur. Le total est un contexte, et c'est lui qui saute à 46 px. La forme longue
 * n'est jamais perdue pour autant : elle reste dans `summary`, que la vue pose en infobulle
 * et en `aria-label` — un lecteur d'écran lit la même phrase dans les deux formes.
 *
 * Rien n'est produit ici : les états viennent du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le compteur les compte.
 */

/** Un morceau du compteur long, et la teinte qui le distingue de ses voisins. */
export interface HeaderChip {
    readonly text: string;
    readonly tone: "accent" | "rule" | "plain";
}

/** Le compteur réduit à ce qui tient dans 46 px : un glyphe et un nombre. */
export interface HeaderBadge {
    /** L'état compté, ou `null` quand le badge ne compte que des agents. */
    readonly state: AgentState | null;
    readonly count: number;
    /** Vrai quand le badge parle d'un agent qui attend — la seule teinte de l'interface. */
    readonly urgent: boolean;
}

export type SidebarHeaderModel =
    | {
          readonly shape: "full";
          readonly title: string;
          readonly chips: readonly HeaderChip[];
          /** La phrase entière, aussi posée en infobulle. */
          readonly summary: string;
      }
    | {
          readonly shape: "compact";
          readonly badge: HeaderBadge;
          /** La même phrase entière : le repli abrège l'affichage, pas l'information. */
          readonly summary: string;
      };

export function composeSidebarHeader(
    tree: SidebarTree,
    columnCollapsed: boolean,
): SidebarHeaderModel {
    const summary = summarize(tree);

    return columnCollapsed
        ? { shape: "compact", badge: badgeOf(tree), summary }
        : { shape: "full", title: "workspaces", chips: chipsOf(tree), summary };
}

/**
 * Repliée, la colonne montre **ce qui attend**, et le total seulement à défaut.
 *
 * Un `3 / 7` tiendrait aussi, mais il oblige à se rappeler lequel des deux nombres est
 * lequel. Le glyphe de `waiting` est déjà le signe que l'œil cherche dans la colonne ; le
 * répéter en tête coûte moins qu'un second nombre à interpréter.
 */
function badgeOf(tree: SidebarTree): HeaderBadge {
    return tree.waitingCount > 0
        ? { state: "waiting", count: tree.waitingCount, urgent: true }
        : { state: null, count: tree.tabCount, urgent: false };
}

function chipsOf(tree: SidebarTree): readonly HeaderChip[] {
    if (tree.tabCount === 0) return [{ text: "0", tone: "plain" }];

    const agents: HeaderChip = { text: agentWord(tree.tabCount), tone: "plain" };
    if (tree.waitingCount === 0) return [agents];

    return [
        { text: `${tree.waitingCount} waiting`, tone: "accent" },
        { text: "/", tone: "rule" },
        agents,
    ];
}

function summarize(tree: SidebarTree): string {
    if (tree.tabCount === 0) return "no agents";
    if (tree.waitingCount === 0) return agentWord(tree.tabCount);
    return `${tree.waitingCount} waiting / ${agentWord(tree.tabCount)}`;
}

function agentWord(count: number): string {
    return `${count} agent${count > 1 ? "s" : ""}`;
}
