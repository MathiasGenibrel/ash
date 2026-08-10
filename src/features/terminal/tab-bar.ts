import type { TabId, TabInfo } from "./ports";
import type { TabsState } from "./tabs";

/**
 * Ce que la barre sait déclencher. Les mêmes actions que le menu applicatif — la spec
 * §4.4 demande que tout soit atteignable à la souris.
 */
export interface TabBarActions {
    select(tabId: TabId): void;
    close(tabId: TabId): void;
    openInCurrentWorktree(): void;
    openAtHome(): void;
    clearActive(): void;
}

/**
 * La barre d'onglets : ordre du backend, onglet actif nettement marqué, bouton `+`
 * (spec §4.2).
 *
 * Elle ne décide rien — elle affiche un `TabsState` et rend des intentions. Le DOM est
 * reconstruit à chaque rendu : quelques nœuds par onglet, contre le risque bien réel
 * d'une barre qui diverge de l'ordre que le backend détient.
 */
export class TabBar {
    readonly element: HTMLElement;

    /**
     * Le titre d'un onglet porte-t-il son workspace ?
     *
     * Oui quand la sidebar est repliée : elle ne porte plus le contexte, donc l'onglet
     * doit le porter — `omelette-web/claude` au lieu de `claude`. C'est la seconde
     * conséquence de `⌘B`, et sans elle une fenêtre repliée ne dit plus dans quel dépôt
     * chaque onglet travaille.
     */
    private withWorkspace = false;

    constructor(private readonly actions: TabBarActions) {
        this.element = document.createElement("div");
        this.element.className = "terminal-bar";
        this.element.setAttribute("role", "tablist");
    }

    render(state: TabsState): void {
        this.element.replaceChildren(
            ...state.tabs.map((tab, index) =>
                this.tabButton(tab, index + 1, tab.tabId === state.activeTabId),
            ),
            this.controls(state),
        );
    }

    /** Rend `true` si l'affichage a changé, donc s'il faut un nouveau rendu. */
    showWorkspaceInTitles(show: boolean): boolean {
        if (this.withWorkspace === show) return false;
        this.withWorkspace = show;
        return true;
    }

    private tabButton(tab: TabInfo, position: number, active: boolean): HTMLElement {
        const row = document.createElement("div");
        row.className = active ? "terminal-tab is-active" : "terminal-tab";

        const name = this.titleOf(tab);
        const label = document.createElement("button");
        label.type = "button";
        label.className = "terminal-tab-label";
        label.setAttribute("role", "tab");
        label.setAttribute("aria-selected", String(active));
        // Le survol montre le répertoire *courant* de l'onglet, sondé par le backend
        // (ADR-0005) : il suit les `cd` de l'utilisateur.
        label.title = tab.cwd;
        label.textContent = position <= 9 ? `⌘${position} ${name}` : name;
        label.addEventListener("click", () => {
            this.actions.select(tab.tabId);
        });

        const close = document.createElement("button");
        close.type = "button";
        close.className = "terminal-tab-close";
        close.title = "Fermer l'onglet (⌘W)";
        close.setAttribute("aria-label", `Fermer ${name}`);
        close.textContent = "×";
        close.addEventListener("click", (event) => {
            // Sans ça, le clic sélectionnerait l'onglet avant de le fermer, et la
            // confirmation s'afficherait sur un onglet qui vient de prendre le focus.
            event.stopPropagation();
            this.actions.close(tab.tabId);
        });

        row.append(label, close);
        return row;
    }

    private titleOf(tab: TabInfo): string {
        return tabTitle(tab, this.withWorkspace);
    }

    private controls(state: TabsState): HTMLElement {
        const group = document.createElement("div");
        group.className = "terminal-bar-controls";

        group.append(
            button("+", "Nouvel onglet dans le worktree courant (⌘N)", () => {
                this.actions.openInCurrentWorktree();
            }),
            button("+~", "Nouvel onglet à ~ (⇧⌘N)", () => {
                this.actions.openAtHome();
            }),
        );

        // Effacer le scrollback n'a pas de sens sans onglet ; le bouton disparaît plutôt
        // que de rester là, désactivé, à ne rien vouloir dire.
        if (state.activeTabId !== null) {
            group.append(
                button("⌫", "Effacer le scrollback (⌘K)", () => {
                    this.actions.clearActive();
                }),
            );
        }

        return group;
    }
}

/**
 * Le titre d'un onglet : le programme qui tient son avant-plan, tel que le backend le
 * nomme — `claude`, `bun`, `zsh`. Préfixé de son workspace quand la sidebar est repliée.
 */
export function tabTitle(tab: TabInfo, withWorkspace: boolean): string {
    if (!withWorkspace) return tab.process;
    const workspace = tab.location?.repo?.name ?? tab.location?.worktreeName ?? basename(tab.cwd);
    return `${workspace}/${tab.process}`;
}

function button(text: string, title: string, onClick: () => void): HTMLButtonElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "terminal-bar-button";
    element.title = title;
    element.setAttribute("aria-label", title);
    element.textContent = text;
    element.addEventListener("click", onClick);
    return element;
}

/** Dernier segment d'un chemin — `~` reste `~`, `/` reste `/`. */
function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
