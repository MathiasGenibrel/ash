/**
 * API publique de la feature terminal.
 *
 * Le reste du frontend n'importe que ce fichier : ni `xterm-view`, ni `pty-bridge`, ni
 * `workbench` ne sont des points d'entrée.
 */

import "./terminal.css";

import type { FontSizeSignal, TabId, TabInfo, ThemeSignal } from "./ports";
import { askToClose } from "./confirm-dialog";
import { tauriGit } from "./git-bridge";
import { WorktreeMetadataStore } from "./metadata-store";
import { tauriPty } from "./pty-bridge";
import { StatusLine, composeStatusLine } from "./status-line";
import { TabBar } from "./tab-bar";
import { noTabs, type Step, type TabsState } from "./tabs";
import { XtermView } from "./xterm-view";
import { TerminalWorkbench, type Origin } from "./workbench";

export type { FontSizeSignal, PtyFrame, TabId, TabInfo, TerminalSize, ThemeSignal } from "./ports";
export type { Origin } from "./workbench";
export type { Step } from "./tabs";
/**
 * Les tokens que le terminal lit dans la table de `app/styles.css`.
 *
 * Publiés parce qu'ils sont le contrat entre l'application, qui détient les palettes, et
 * la feature, qui les consomme — xterm.js peint ses cellules lui-même et ne peut pas
 * résoudre un `var(--ash-…)`. Voir `theme.ts`.
 */
export { TERMINAL_THEME_TOKENS } from "./theme";

/** Ce que la feature annonce de ses onglets à qui les affiche autrement — la sidebar. */
export type TabsListener = (tabs: readonly TabInfo[], activeTabId: TabId | null) => void;

/** Les actions d'onglet, telles que le menu applicatif et la barre les déclenchent. */
export interface Terminals {
    openTab(origin: Origin): Promise<void>;
    closeActiveTab(): Promise<void>;
    selectTab(tabId: TabId): Promise<void>;
    selectTabAt(position: number): Promise<void>;
    /**
     * `Ctrl+Tab` / `Ctrl+Shift+Tab` : l'onglet voisin dans l'ordre du backend, en bouclant.
     *
     * Un seul point d'entrée pour les deux sens : ce sont la même règle lue dans deux
     * directions, et deux méthodes en auraient fait deux règles à garder d'accord.
     */
    cycleTab(step: Step): Promise<void>;
    clearActiveScrollback(): Promise<void>;
    /**
     * S'abonne à l'état des onglets.
     *
     * La feature ne connaît pas la sidebar : c'est le composition root qui relie les deux.
     * Et il n'y a **qu'un** abonnement à la boucle de sonde — deux features qui écouteraient
     * le même event afficheraient deux vérités qui se croisent
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    onTabs(listener: TabsListener): void;
    /**
     * `⌘B` a replié ou déplié la sidebar.
     *
     * Repliée, elle ne porte plus le contexte, et la zone terminal le reprend à deux
     * endroits : le titre d'un onglet devient `omelette-web/claude`, et la ligne de statut
     * gagne le rappel de l'agent qui attend. Les deux sont la même information déplacée,
     * d'où un seul appel — deux réglages séparés finiraient par se contredire.
     */
    setSidebarCollapsed(collapsed: boolean): void;
}

/**
 * Monte la barre d'onglets et la pile de terminaux dans `host`.
 *
 * Rien n'est ouvert ici : c'est au composition root de décider que l'application démarre
 * sur un onglet. C'est lui, aussi, qui passe `theme` et `fontSize` : la feature ne détecte
 * ni les bascules de palette ni les changements de taille, elle en est prévenue.
 *
 * Un onglet porte au plus un PTY, et un seul terminal est visible à la fois
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)). L'apparence, elle, ne se
 * règle pas par onglet : la taille de police vaut pour toute l'application, et c'est une
 * décision de `features::theme` côté Rust — pas un effet de bord du câblage.
 */
export function mountTerminals(
    host: HTMLElement,
    theme: ThemeSignal,
    fontSize: FontSizeSignal,
): Terminals {
    host.classList.add("terminal-workbench");

    // La pile est un conteneur positionné : chaque onglet s'y superpose en occupant
    // toute la surface, et seul l'actif est visible. Voir `xterm-view.ts` — les onglets
    // masqués gardent leur taille, sans quoi leur grille serait détruite au retour.
    const stack = document.createElement("div");
    stack.className = "terminal-stack";

    const listeners: TabsListener[] = [];

    // La ligne de statut parle de l'onglet **actif** et du worktree qui le porte
    // (ADR-0012). Elle ne détient rien : le `cwd` vient de la sonde, l'état git de la
    // surveillance, l'état d'agent du backend.
    const status = new StatusLine();
    let shown: TabsState = noTabs;
    let sidebarCollapsed = false;

    // Déclaration de fonction, et non `const` : le cache l'appelle depuis un rappel posé
    // dans son constructeur, donc avant la fin de ce bloc.
    function drawStatus(): void {
        const active = shown.tabs.find((tab) => tab.tabId === shown.activeTabId) ?? null;
        const worktreeRoot = active?.location?.worktreeRoot ?? null;
        status.render(composeStatusLine(shown, metadata.of(worktreeRoot), sidebarCollapsed));
    }

    const metadata = new WorktreeMetadataStore(tauriGit, drawStatus);

    const workbench = new TerminalWorkbench({
        bridge: tauriPty,
        // Chaque terminal suit le thème et la taille de police pour son compte, et s'en
        // désabonne en se libérant : l'atelier n'a à connaître ni la palette ni la taille
        // pour savoir qu'un onglet est ouvert.
        createView: () => new XtermView(stack, theme, fontSize),
        confirmClose: (tab) => askToClose(host, tab.cwd),
        onRender: (state) => {
            bar.render(state);
            shown = state;
            drawStatus();
            for (const listener of listeners) listener(state.tabs, state.activeTabId);
        },
    });

    const bar = new TabBar({
        select: (tabId) => void workbench.select(tabId),
        close: (tabId) => void workbench.closeTab(tabId),
        openInCurrentWorktree: () => void workbench.openTab("current-worktree"),
        openAtHome: () => void workbench.openTab("home"),
        clearActive: () => void workbench.clearActive(),
    });

    bar.render(workbench.tabs);
    drawStatus();
    host.append(bar.element, stack, status.element);

    return {
        openTab: (origin) => workbench.openTab(origin),
        closeActiveTab: () => workbench.closeActive(),
        selectTab: (tabId) => workbench.select(tabId),
        selectTabAt: (position) => workbench.selectAt(position),
        cycleTab: (step) => workbench.cycle(step),
        clearActiveScrollback: () => workbench.clearActive(),
        onTabs: (listener) => {
            listeners.push(listener);
            // L'abonné arrive après le premier rendu : lui donner l'état courant tout de
            // suite lui évite d'attendre le prochain `cd` pour afficher quoi que ce soit.
            listener(workbench.tabs.tabs, workbench.tabs.activeTabId);
        },
        setSidebarCollapsed: (collapsed) => {
            sidebarCollapsed = collapsed;
            if (bar.showLocationInTitles(collapsed)) bar.render(workbench.tabs);
            drawStatus();
        },
    };
}
