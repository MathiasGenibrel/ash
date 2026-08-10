/**
 * Les deux frontières de la feature terminal.
 *
 * xterm.js et l'IPC Tauri sont derrière des interfaces pour la même raison que les
 * effets système le sont côté Rust : sans ça, la règle qui compte ici — n'acquitter
 * qu'une fois, et jamais après la fermeture — ne serait vérifiable qu'en lançant
 * l'application.
 */

/** Identifiant d'onglet : l'ulid que le backend a posé dans `ASH_TAB_ID`. */
export type TabId = string;

export interface TerminalSize {
    cols: number;
    rows: number;
}

/**
 * Un onglet, tel que le backend le décrit. Miroir de `TabInfo` côté Rust.
 *
 * `cwd` est le répertoire **courant** de l'onglet : la sonde d'ADR-0005 le suit à
 * travers les `cd`, et même pendant qu'un programme tourne. C'est lui que « nouvel
 * onglet dans le worktree courant » (spec §4.4) reprend.
 *
 * Le frontend ne le calcule pas et ne le mémorise pas : il le relit du backend, qui seul
 * le détient ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface TabInfo {
    tabId: TabId;
    cwd: string;
}

/**
 * Ce que la boucle de sonde du backend annonce d'un onglet qui a bougé. Miroir de
 * `TabChange` côté Rust.
 *
 * Le backend pousse, le frontend ne demande rien : c'est lui qui détient les PTY, donc
 * lui qui sait quand un `cd` a eu lieu. Seuls les onglets qui ont **changé** traversent
 * la frontière — un onglet posé à son invite ne réveille pas la webview.
 */
export interface TabChange {
    tabId: TabId;
    cwd: string;
}

/** De quoi arrêter d'écouter un flux d'events. */
export type Unsubscribe = () => void;

/** Ce que le PTY envoie. Miroir de `PtyFrame` côté Rust. */
export type PtyFrame = { kind: "chunk"; data: string } | { kind: "exit"; code: number | null };

/** Ce que la feature attend d'un moteur de terminal. */
export interface TerminalView {
    readonly size: TerminalSize;
    /**
     * Écrit dans le terminal. `done` est appelé quand xterm.js a **consommé** le
     * morceau — c'est le seul signal sur lequel l'acquittement peut s'appuyer.
     */
    write(data: string, done: () => void): void;
    onInput(handler: (data: string) => void): void;
    onResize(handler: (size: TerminalSize) => void): void;
    /** Efface le scrollback — le `Cmd+K` de la spec §4.4. */
    clear(): void;
    /**
     * Montre ou masque la surface, **sans la démonter**.
     *
     * Un seul terminal est visible à la fois ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)),
     * mais les autres continuent de tourner et de recevoir leur sortie.
     */
    setVisible(visible: boolean): void;
    focus(): void;
    dispose(): void;
}

/** De quoi fabriquer la surface d'un nouvel onglet, sans que la feature connaisse le DOM. */
export type TerminalViewFactory = () => TerminalView;

/** Ce que la feature attend du backend. */
export interface PtyBridge {
    /** `cwd` à `null` vaut `~` — le `Cmd+Shift+N` de la spec §4.4. */
    open(
        size: TerminalSize,
        cwd: string | null,
        onFrame: (frame: PtyFrame) => void,
    ): Promise<TabId>;
    write(tabId: TabId, data: string): Promise<void>;
    resize(tabId: TabId, size: TerminalSize): Promise<void>;
    ack(tabId: TabId): Promise<void>;
    close(tabId: TabId): Promise<void>;
    /** Les onglets vivants, dans l'ordre que le backend détient — et lui seul. */
    tabs(): Promise<TabInfo[]>;
    /** Vrai si quelque chose tourne dans l'onglet : `Cmd+W` demandera confirmation. */
    hasForegroundProcess(tabId: TabId): Promise<boolean>;
    /**
     * S'abonne à la boucle de sonde du backend
     * ([ADR-0005](../../../docs/adr/0005-sonde-cwd-libproc.md)).
     *
     * C'est par là, et par nulle part ailleurs, qu'un titre d'onglet apprend un `cd` :
     * rien ici ne scrute, ne minute, ni ne relit la liste à intervalle régulier.
     */
    onTabsChanged(handler: (changes: TabChange[]) => void): Promise<Unsubscribe>;
}
