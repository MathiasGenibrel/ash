/**
 * API publique de la feature sidebar.
 *
 * Le reste du frontend n'importe que ce fichier : ni `tree`, ni `view`, ni `states`, ni
 * `labels` ne sont des points d'entrée.
 *
 * La sidebar **rend** la hiérarchie d'
 * [ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) ; elle ne la détient pas.
 * Elle n'appelle aucune commande Tauri et ne lit aucun fichier : le composition root lui
 * passe les onglets que le backend décrit, déjà situés
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */

import "./sidebar.css";

import type { TabId, TabInfo } from "@/shared/ipc";
import { buildSidebar } from "./tree";
import { SidebarView } from "./view";
import { showsSubagents } from "./visible";

export type { SidebarGroup, SidebarTree, WorktreeNode } from "./tree";

/** Ce que la sidebar sait demander, et qu'elle ne sait pas faire elle-même. */
export interface SidebarPorts {
    /** Cliquer un agent, c'est aller à son onglet. */
    selectTab(tabId: TabId): void;
    /** Le `+` du pied. */
    newTab(): void;
    /**
     * L'heure qu'il est, pour les durées des lignes de sous-agents.
     *
     * Injectée plutôt que lue, comme partout ailleurs où le temps entre dans le produit :
     * `Date.now` par défaut, et le composition root n'a rien à en dire.
     */
    now?: () => number;
}

export interface Sidebar {
    readonly element: HTMLElement;
    /** `⌘B` : replié, il ne reste que le rail. */
    readonly isCollapsed: boolean;
    render(tabs: readonly TabInfo[], activeTabId: TabId | null): void;
    /** Rend l'état après bascule, pour que l'appelant en tire la mise en page. */
    toggleCollapsed(): boolean;
}

export function mountSidebar(ports: SidebarPorts): Sidebar {
    // Trois replis, et ils ne se confondent pas : la **colonne** (`⌘B`), chaque **dépôt**,
    // et chaque **worktree** pris séparément (ADR-0012, spec §4.1). Ce sont les seuls états
    // que la sidebar détient — ils ne décrivent aucun agent, seulement ce qu'on regarde,
    // donc ils ont le droit de vivre ici sans contredire ADR-0009.
    let columnCollapsed = false;
    const collapsedWorktrees = new Set<string>();
    const collapsedGroups = new Set<string>();

    let tabs: readonly TabInfo[] = [];
    let activeTabId: TabId | null = null;
    const now = ports.now ?? ((): number => Date.now());

    // Le battement qui fait avancer les durées des lignes filles, **et seulement quand il y
    // en a une à l'écran**. La colonne entière se redessine à chaque rendu : la faire battre
    // en permanence coûterait un rendu par seconde pour animer un compteur qui n'existe pas
    // la plupart du temps. Sans sous-agent, la sidebar redevient exactement ce qu'elle était
    // — dessinée sur événement, et jamais autrement.
    let ticker: ReturnType<typeof setInterval> | null = null;

    const view = new SidebarView({
        selectTab: (tabId) => {
            ports.selectTab(tabId);
        },
        toggleWorktree: (key) => {
            if (!collapsedWorktrees.delete(key)) collapsedWorktrees.add(key);
            draw();
        },
        toggleGroup: (key) => {
            if (!collapsedGroups.delete(key)) collapsedGroups.add(key);
            draw();
        },
        newTab: () => {
            ports.newTab();
        },
    });

    function draw(): void {
        const tree = buildSidebar(tabs, { activeTabId, collapsedWorktrees, collapsedGroups });
        view.render(tree, columnCollapsed, now());
        beat(showsSubagents(tree, columnCollapsed));
    }

    /**
     * Ouvre ou ferme le battement des durées, sans jamais en laisser deux.
     *
     * Le battement rappelle [`draw`] lui-même, et non un rendu à lui : c'est ce qui lui permet
     * de **s'arrêter tout seul** quand la dernière ligne fille a fini d'expirer. Un second
     * chemin de rendu, qui ne repasserait pas par [`showsSubagents`], laisserait battre la
     * colonne pour toujours le jour où le backend n'a plus rien à annoncer — et deux chemins
     * de rendu finiraient de toute façon par ne plus dessiner la même chose.
     */
    function beat(wanted: boolean): void {
        if (wanted === (ticker !== null)) return;
        if (ticker !== null) {
            clearInterval(ticker);
            ticker = null;
            return;
        }
        ticker = setInterval(draw, 1000);
    }

    draw();

    return {
        element: view.element,
        get isCollapsed() {
            return columnCollapsed;
        },
        render(nextTabs, nextActive) {
            tabs = nextTabs;
            activeTabId = nextActive;
            draw();
        },
        toggleCollapsed() {
            columnCollapsed = !columnCollapsed;
            draw();
            return columnCollapsed;
        },
    };
}
