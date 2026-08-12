/**
 * Les deux frontières de la feature terminal.
 *
 * xterm.js et l'IPC Tauri sont derrière des interfaces pour la même raison que les
 * effets système le sont côté Rust : sans ça, la règle qui compte ici — n'acquitter
 * qu'une fois, et jamais après la fermeture — ne serait vérifiable qu'en lançant
 * l'application.
 */

import type { TabId, TabInfo, WorktreeMetadata, WorktreeMetadataChanged } from "@/shared/ipc";

/**
 * `TabId` et `TabInfo` sont le contrat partagé avec le backend, pas la propriété de cette
 * feature : la sidebar les lit aussi. Ils sont réexportés ici pour que les consommateurs
 * de la feature n'aient qu'un point d'entrée.
 */
export type { TabId, TabInfo } from "@/shared/ipc";

export interface TerminalSize {
    cols: number;
    rows: number;
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

/**
 * Ce que la feature attend de qui détient le thème — c'est-à-dire de `app/`.
 *
 * Elle n'a pas besoin de savoir **quelle** palette est en place : la table de tokens est
 * déjà posée sur le document quand l'avis arrive, et il n'y a plus qu'à la relire. Ce qui
 * lui manque, et qu'aucune règle CSS ne peut lui donner, c'est de savoir *quand* — xterm.js
 * peint ses cellules lui-même.
 *
 * Un port, et pas une écoute directe : la feature n'a **aucune** raison de savoir que le
 * thème vient d'un menu natif et de `matchMedia`. Un second détecteur ici — un
 * `MutationObserver` sur `data-theme`, un second `matchMedia` — ferait deux vérités là où
 * `app/theme.ts` en tient une.
 */
export interface ThemeSignal {
    /** S'abonne aux changements de palette. Rend de quoi se désabonner. */
    subscribe(listener: () => void): Unsubscribe;
}

/**
 * Ce que la feature attend de qui détient la taille de police — c'est-à-dire de `app/`.
 *
 * Un port pour la même raison que `ThemeSignal` : la taille est une préférence de
 * l'**application**, retenue par le backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la feature n'a
 * aucune raison de savoir qu'elle vient d'un menu natif et d'un event Tauri. Elle vaut
 * pour tous les onglets à la fois — voir `FontSize` côté Rust, qui porte la décision.
 *
 * `current` autant que `subscribe` : un onglet ouvert après un `⌘+` doit naître à la
 * taille en cours, comme il naît déjà à la bonne palette.
 */
export interface FontSizeSignal {
    /** La taille en cours, en points. */
    readonly current: number;
    /** S'abonne aux changements de taille. Rend de quoi se désabonner. */
    subscribe(listener: (points: number) => void): Unsubscribe;
}

/** Ce que la feature attend du backend. */
export interface PtyBridge {
    /** `cwd` à `null` vaut `~` — le `Cmd+Shift+T` de la spec §4.4. */
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
     * C'est par là, et par nulle part ailleurs, qu'un onglet apprend un `cd` — donc aussi
     * qu'il apprend avoir changé de dépôt : rien ici ne scrute, ne minute, ni ne relit la
     * liste à intervalle régulier. Seuls les onglets qui ont **changé** traversent la
     * frontière ; un onglet posé à son invite ne réveille pas la webview.
     */
    onTabsChanged(handler: (changed: TabInfo[]) => void): Promise<Unsubscribe>;
}

/**
 * Ce que la ligne de statut attend du backend git.
 *
 * Le pont vit dans la feature qui **consomme** l'état git, pas dans une feature git côté
 * frontend : la surveillance d'ADR-0011 est entièrement en Rust, et le TypeScript n'en
 * connaît que la commande et l'event que `features/git/commands.rs` déclare — jamais sa
 * structure interne.
 */
export interface GitBridge {
    /**
     * L'état git d'un worktree, tel que la surveillance le connaît.
     *
     * `null` pour un répertoire hors de tout dépôt, ou dont les fichiers de contrôle ne se
     * lisent pas : les deux se rendent pareil, sans branche.
     */
    metadata(worktreeRoot: string): Promise<WorktreeMetadata | null>;
    /**
     * S'abonne à la surveillance des fichiers de contrôle (spec §5.3).
     *
     * C'est par là qu'un `git commit` fait bouger la ligne de statut : rien ici ne sonde
     * ni ne relit à intervalle régulier.
     */
    onMetadataChanged(handler: (changed: WorktreeMetadataChanged) => void): Promise<Unsubscribe>;
}
