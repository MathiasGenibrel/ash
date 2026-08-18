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

import type { TabId, TabInfo, Workspaces } from "@/shared/ipc";
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
     * Le clic sur une ligne de worktree **sans onglet** : en ouvrir un dans ce worktree
     * (spec §5.2).
     *
     * La sidebar ne sait pas ouvrir un PTY — elle nomme le worktree, et passe la main, comme
     * pour le marqueur d'instrumentation (ADR-0010). C'est le composition root qui relie.
     */
    openTabIn(worktreeRoot: string): void;
    /**
     * Épingler ou désépingler un worktree.
     *
     * Le geste **part**, il ne se pose pas ici : ce qui survit à la fermeture vit en Rust
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la colonne
     * redessinera quand le backend l'aura annoncé.
     */
    setPinned(worktreeRoot: string, pinned: boolean): void;
    /** Replier ou déplier une ligne — un worktree, ou un groupe de dépôt. Même chemin. */
    setCollapsed(key: string, collapsed: boolean): void;
    /**
     * Le marqueur « non instrumenté » d'une ligne d'agent : ouvrir les réglages sur cet outil.
     *
     * La sidebar **informe** ; c'est l'écran qui agit
     * ([ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md)). Elle ne sait ni
     * écrire un fichier, ni ce qu'instrumenter veut dire — elle nomme l'outil, et passe la
     * main.
     */
    instrument(command: string, adapter: string): void;
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
    /**
     * Dessine la colonne à partir de ce que le backend détient : les onglets, l'onglet actif,
     * et l'état gardé d'une session à l'autre — les épingles et les lignes repliées.
     */
    render(
        tabs: readonly TabInfo[],
        activeTabId: TabId | null,
        workspaces: Workspaces,
    ): void;
    /**
     * `⌘B` : replie ou déplie **la colonne entière**, et rend son état pour que l'appelant en
     * tire la mise en page.
     *
     * À ne pas confondre avec le repli d'une **ligne** : celui-là part au backend et survit à
     * la fermeture, celui-ci ne survit à rien. Les deux gestes ne portent donc pas le même
     * nom, et ce n'est pas une commodité de lecture — c'est ce qui empêche d'appeler l'un en
     * croyant appeler l'autre.
     */
    toggleColumnCollapsed(): boolean;
}

export function mountSidebar(ports: SidebarPorts): Sidebar {
    // Trois replis, et ils ne se confondent pas : la **colonne** (`⌘B`), chaque **dépôt**,
    // et chaque **worktree** pris séparément (ADR-0012, spec §4.1).
    //
    // Seul le premier vit ici. Les deux autres **survivent au redémarrage** (spec §5.2), donc
    // ils vivent en Rust avec les épingles, et la colonne les reçoit comme elle reçoit ses
    // onglets ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). `⌘B`, lui, ne
    // se replie pas par ligne et ne survit à rien : il n'a pas de raison de traverser la
    // frontière.
    let columnCollapsed = false;

    let tabs: readonly TabInfo[] = [];
    let activeTabId: TabId | null = null;
    let workspaces: Workspaces = { pinned: [], collapsed: [] };
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
        // Les deux replis de ligne partent au backend et reviennent par son annonce : rien
        // n'est posé ici, sans quoi la colonne et le fichier pourraient se contredire.
        toggleRowCollapsed: (key) => {
            ports.setCollapsed(key, !collapsed().has(key));
        },
        newTab: () => {
            ports.newTab();
        },
        openTabIn: (worktreeRoot) => {
            ports.openTabIn(worktreeRoot);
        },
        setPinned: (worktreeRoot, pinned) => {
            ports.setPinned(worktreeRoot, pinned);
        },
        instrument: (command, adapter) => {
            ports.instrument(command, adapter);
        },
    });

    /**
     * Les lignes repliées, telles que le backend les a annoncées.
     *
     * Un seul ensemble pour les deux niveaux : les clés d'un groupe sont préfixées (`repo:`,
     * `flat:`) et celles d'un worktree sont des chemins absolus, donc elles ne peuvent pas se
     * confondre — et `state.json` n'a qu'une liste à garder.
     */
    function collapsed(): ReadonlySet<string> {
        return new Set(workspaces.collapsed);
    }

    function draw(): void {
        const tree = buildSidebar(tabs, {
            activeTabId,
            collapsed: collapsed(),
            pinned: workspaces.pinned,
        });
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
        render(nextTabs, nextActive, nextWorkspaces) {
            tabs = nextTabs;
            activeTabId = nextActive;
            workspaces = nextWorkspaces;
            draw();
        },
        toggleColumnCollapsed() {
            columnCollapsed = !columnCollapsed;
            draw();
            return columnCollapsed;
        },
    };
}
