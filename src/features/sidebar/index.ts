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

export type { AgentPresentation } from "./states";
export type { SidebarGroup, SidebarTree, WorktreeNode } from "./tree";

/** Ce que la sidebar sait demander, et qu'elle ne sait pas faire elle-même. */
export interface SidebarPorts {
    /** Cliquer un agent, c'est aller à son onglet. */
    selectTab(tabId: TabId): void;
    /** Le `+` du pied. */
    newTab(): void;
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
    // Deux replis, et ils ne se confondent pas : la **colonne** (`⌘B`), et chaque
    // **worktree** pris séparément (ADR-0012). Ce sont les seuls états que la sidebar
    // détient — ils ne décrivent aucun agent, seulement ce qu'on regarde.
    let columnCollapsed = false;
    const collapsedWorktrees = new Set<string>();

    let tabs: readonly TabInfo[] = [];
    let activeTabId: TabId | null = null;

    const view = new SidebarView({
        selectTab: (tabId) => {
            ports.selectTab(tabId);
        },
        toggleWorktree: (key) => {
            if (!collapsedWorktrees.delete(key)) collapsedWorktrees.add(key);
            draw();
        },
        newTab: () => {
            ports.newTab();
        },
    });

    function draw(): void {
        view.render(
            buildSidebar(tabs, { activeTabId, collapsed: collapsedWorktrees }),
            columnCollapsed,
        );
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
