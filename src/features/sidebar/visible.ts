import type { AgentState } from "@/shared/ipc";
import {
    tabStates,
    type SidebarGroup,
    type SidebarTabNode,
    type SidebarTree,
    type WorktreeNode,
} from "./tree";
import type { SubagentNode } from "./subagents";

/**
 * Ce que la colonne **montre réellement**, replis compris.
 *
 * C'est la contrepartie de [`./states`] : `bubbleState` choisit l'état qu'une ligne repliée
 * porte, ce module dit quelles lignes portent quoi une fois les trois replis appliqués — la
 * colonne (`⌘B`), un groupe de dépôt, un worktree.
 *
 * Il existe pour une raison précise. La garantie de la spec §4.1 — *une ligne repliée ne
 * doit jamais cacher un agent qui attend* — porte sur la colonne entière, pas sur une ligne
 * isolée, et elle ne se vérifie donc ni dans `bubbleState` ni dans un test de rendu : `bun
 * test` n'a pas de DOM. En sortant le **plan** du rendu, la garantie devient une propriété
 * d'une fonction pure, et [`./view`] se contente de peindre ce plan — les deux ne peuvent
 * pas diverger, puisqu'il n'y en a qu'un.
 */

/** Ce qu'une ligne repliable montre : son état agrégé **ou** ses enfants, jamais les deux. */
export interface RowPlan<Child> {
    /**
     * Le glyphe que la ligne porte à la place de ses enfants.
     *
     * `null` quand elle est dépliée : ses enfants disent alors leur état eux-mêmes, et un
     * second glyphe sur la ligne du dessus ne ferait que répéter le plus urgent d'entre eux.
     */
    readonly badge: AgentState | null;
    readonly children: readonly Child[];
}

/**
 * Le plan d'un groupe : ses worktrees, ou son état agrégé quand il est replié.
 *
 * La forme **à plat** n'a pas de ligne de groupe (ADR-0012, amendement du 2026-08-26) : son
 * unique worktree *est* sa ligne — que le dépôt existe ou non —, donc elle ne peut rien
 * replier à ce niveau et ne porte jamais de glyphe. Ce qu'elle montre reste porté par le
 * repli du worktree, qui n'a pas bougé.
 */
export function planGroup(group: SidebarGroup): RowPlan<WorktreeNode> {
    if (group.kind === "flat") return { badge: null, children: [group.worktree] };

    return group.collapsed
        ? { badge: group.state, children: [] }
        : { badge: null, children: group.worktrees };
}

/** Le plan d'un worktree : ses onglets, ou son état agrégé quand il est replié. */
export function planWorktree(worktree: WorktreeNode): RowPlan<SidebarTabNode> {
    return worktree.collapsed
        ? { badge: worktree.state, children: [] }
        : { badge: null, children: worktree.tabs };
}

/**
 * Le plan d'une ligne d'onglet : ses sous-agents, et jamais de glyphe à leur place.
 *
 * `badge` est **toujours** `null`, et ce n'est pas une omission : une ligne d'onglet porte
 * déjà son propre état, à gauche de son nom. Elle ne remonte donc rien de ses enfants — ils
 * sont juste en dessous, et le plus urgent d'entre eux se lit d'un coup d'œil. C'est la ligne
 * de **worktree** qui les remonte quand elle est repliée, et c'est ce que [`tabStates`]
 * porte.
 *
 * Une ligne d'onglet n'a pas de repli à elle : ses enfants se montrent ou n'existent pas.
 */
export function planTab(tab: SidebarTabNode): RowPlan<SubagentNode> {
    return { badge: null, children: tab.subagents };
}

/**
 * Le plan d'une entrée du rail (`⌘B`).
 *
 * À 46 px il n'y a plus de hiérarchie à montrer : le rail aplatit le groupe et pose le
 * glyphe de **chaque** onglet sous ses initiales, quel que soit le repli des lignes en
 * dessous. Les replis de groupe et de worktree ne s'y appliquent donc pas — c'est
 * délibéré : une colonne repliée est déjà une vue réduite, la réduire deux fois masquerait
 * exactement ce qu'on est venu y chercher.
 */
export function planRailEntry(group: SidebarGroup): RailPlan {
    const tabs =
        group.kind === "repo"
            ? group.worktrees.flatMap((worktree) => worktree.tabs)
            : group.worktree.tabs;

    return { badge: group.state, children: tabs };
}

/** Une entrée de rail porte **toujours** un état : elle n'a pas de forme dépliée. */
export interface RailPlan {
    readonly badge: AgentState;
    readonly children: readonly SidebarTabNode[];
}

/**
 * Tous les états qu'un œil peut lire dans la colonne, dans l'ordre où elle les pose.
 *
 * C'est la surface d'état de la sidebar, et rien d'autre : le compteur de l'en-tête est
 * composé à part ([`./header`]), pour que cette propriété reste celle des **lignes** — un
 * compteur qui rattraperait une ligne muette serait un pansement, pas une garantie.
 */
export function visibleStates(tree: SidebarTree, columnCollapsed: boolean): readonly AgentState[] {
    return tree.groups.flatMap((group) =>
        columnCollapsed ? railStates(group) : groupStates(group),
    );
}

function railStates(group: SidebarGroup): AgentState[] {
    const plan = planRailEntry(group);
    // Les surfaces d'outil n'ont pas d'état : elles ne comptent pas dans ce que le rail
    // replié promet de montrer.
    return [plan.badge, ...plan.children.flatMap((tab) => (tab.state === null ? [] : [tab.state]))];
}

function groupStates(group: SidebarGroup): AgentState[] {
    const plan = planGroup(group);
    return [...badge(plan), ...plan.children.flatMap(worktreeStates)];
}

function worktreeStates(worktree: WorktreeNode): AgentState[] {
    const plan = planWorktree(worktree);
    // Chaque onglet déplié dit son état **et** ceux de ses enfants : ce sont autant de
    // lignes que l'œil lit dans la colonne, et la garantie porte sur toutes.
    return [...badge(plan), ...plan.children.flatMap(tabStates)];
}

/**
 * Y a-t-il une ligne fille à l'écran ?
 *
 * Les durées d'une ligne fille se calculent à l'affichage, donc il faut un battement pour les
 * faire avancer. Il n'a aucune raison de tourner quand la colonne n'a rien à animer, et
 * `mountSidebar` s'en sert pour ne pas redessiner une sidebar immobile une fois par seconde.
 */
export function showsSubagents(tree: SidebarTree, columnCollapsed: boolean): boolean {
    // Le rail de 46 px ne montre pas les enfants : il n'a donc jamais de durée à animer.
    if (columnCollapsed) return false;
    return tree.groups.some((group) =>
        planGroup(group).children.some((worktree) =>
            planWorktree(worktree).children.some((tab) => tab.subagents.length > 0),
        ),
    );
}

function badge(plan: RowPlan<unknown>): AgentState[] {
    return plan.badge === null ? [] : [plan.badge];
}
