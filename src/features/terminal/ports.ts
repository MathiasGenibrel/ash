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
 * `startDir` est le répertoire **de lancement** du shell, pas son `cwd` vivant — la
 * sonde d'ADR-0005 n'existe pas encore. C'est lui que « nouvel onglet dans le worktree
 * courant » (spec §4.4) reprend, faute de mieux.
 */
export interface TabInfo {
    tabId: TabId;
    startDir: string;
}

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
}
