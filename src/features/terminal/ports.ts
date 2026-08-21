/**
 * Les deux frontières de la feature terminal.
 *
 * xterm.js et l'IPC Tauri sont derrière des interfaces pour la même raison que les
 * effets système le sont côté Rust : sans ça, la règle qui compte ici — n'acquitter
 * qu'une fois, et jamais après la fermeture — ne serait vérifiable qu'en lançant
 * l'application.
 */

import type {
    AccountUsage,
    ComposeOutcome,
    ShellTab,
    StoppedOperation,
    Tab,
    TabId,
    WorktreeMetadata,
    WorktreeMetadataChanged,
} from "@/shared/ipc";
import type { StatusBarLayout, StatusBarSegmentId } from "./status-bar";

/**
 * `TabId` et `TabInfo` sont le contrat partagé avec le backend, pas la propriété de cette
 * feature : la sidebar les lit aussi. Ils sont réexportés ici pour que les consommateurs
 * de la feature n'aient qu'un point d'entrée.
 */
export type { MergeTab, ShellTab, Tab, TabId, TabInfo } from "@/shared/ipc";
export { isShell } from "@/shared/ipc";

/**
 * La surface d'un onglet qui **n'est pas un terminal**
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md), reformulation du 2026-08-10 :
 * « un onglet est soit un terminal, soit une surface d'outil »).
 *
 * L'atelier ne sait pas ce qu'elle montre, et c'est le point : il sait la poser dans la
 * pile, la montrer, la cacher et la fermer — exactement ce qu'il fait d'un terminal. Ce
 * qu'il y a dedans est fabriqué par le composition root, qui seul connaît `features/merge`.
 */
export interface ToolSurface {
    readonly element: HTMLElement;
    setVisible(visible: boolean): void;
    /** L'onglet se ferme : le backend l'oublie, et la surface se retire. */
    close(): Promise<void>;
}

/**
 * Fabrique la surface d'un onglet qui n'est pas un shell.
 *
 * Rend `null` pour un genre d'onglet que l'application ne sait pas montrer — le jour où le
 * backend en ajouterait un troisième avant que la webview ne suive. La ligne existerait
 * alors dans la sidebar sans surface, plutôt que de faire tomber la fenêtre.
 */
export type ToolSurfaceFactory = (tab: Tab) => ToolSurface | null;

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

/**
 * Ce que la feature attend de qui détient la **police** — c'est-à-dire de `app/`, encore.
 *
 * Le jumeau de [`FontSizeSignal`], et pour les mêmes raisons : la famille est une préférence
 * de l'application, retenue par le backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et la feature n'a pas à
 * savoir qu'elle vient d'une fenêtre de réglages et d'un event Tauri.
 *
 * Ce qui traverse est une **pile** complète (`"SF Mono", ui-monospace, monospace`) et non la
 * seule famille choisie : la préférence peut nommer une police désinstallée depuis, et un
 * terminal sans repli monospace n'aligne plus rien. La pile est composée par `app/`, qui est
 * déjà le seul endroit à savoir ce qu'Ash embarque.
 */
export interface FontFamilySignal {
    /** La pile en cours, prête pour `fontFamily`. */
    readonly current: string;
    /** S'abonne aux changements de police. Rend de quoi se désabonner. */
    subscribe(listener: (stack: string) => void): Unsubscribe;
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
    /**
     * Rédige un texte dans l'onglet — **sans l'envoyer**
     * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
     *
     * Distincte de [`write`] et pas un raccourci pour elle : `write` transporte une frappe
     * de l'utilisateur, `compose` demande au backend d'appliquer une **règle** — refuser un
     * prompt non vide, refuser un onglet sans agent reconnu, attendre la fin d'un tour. La
     * réponse dit laquelle s'est appliquée, et c'est ce que l'écran doit rendre.
     */
    compose(tabId: TabId, text: string): Promise<ComposeOutcome>;
    resize(tabId: TabId, size: TerminalSize): Promise<void>;
    ack(tabId: TabId): Promise<void>;
    close(tabId: TabId): Promise<void>;
    /**
     * Les onglets vivants, dans l'ordre que le backend détient — et lui seul.
     *
     * **Les deux genres** depuis #30 : les shells, puis les surfaces de merge. L'ordre est
     * celui que `⌘1..9` numérote, et il est composé au composition root Rust
     * (`src-tauri/src/tabs.rs`), jamais ici.
     */
    tabs(): Promise<Tab[]>;
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
     *
     * Seuls des **shells** traversent ce canal : c'est la boucle de sonde du registre de
     * PTY, et un onglet de merge n'y entre jamais. Ce que l'onglet de merge a de changeant
     * — son compte de conflits — se relit à la demande, pas au rythme de 300 ms.
     */
    onTabsChanged(handler: (changed: ShellTab[]) => void): Promise<Unsubscribe>;
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
    /**
     * L'opération arrêtée d'un worktree, quand il y en a une (spec §7.4).
     *
     * `null` est le cas courant — rien n'est en cours. Lire cet état n'écrit rien et
     * n'exécute rien : `escapes` porte `abort` et `skip` comme du **texte à montrer**.
     */
    stoppedOperation(worktreeRoot: string): Promise<StoppedOperation | null>;
    /**
     * Le prompt à rédiger dans l'onglet de l'agent pour ce rebase arrêté.
     *
     * Composé par le backend — c'est lui qui détient l'état
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — et rendu ici sans
     * que rien n'ait encore été écrit nulle part.
     */
    conflictPrompt(worktreeRoot: string): Promise<string | null>;
}

/**
 * Ce qu'Ash sait de l'usage du **compte** — la troisième frontière, et la plus petite.
 *
 * Une lecture et un event, comme le thème : la webview lit une fois en s'affichant, puis
 * l'event la tient à jour. Elle ne redemande jamais, et n'a **aucun moyen de déclencher un
 * appel réseau** ([ADR-0016](../../../docs/adr/0016-ash-sort-sur-le-reseau.md), condition
 * 2) : c'est un fil de fond du backend qui décide quand appeler, et s'il appelle.
 */
export interface UsageBridge {
    /**
     * Les deux quotas, tels que le fil de fond les connaît **déjà**. N'attend rien, et
     * n'appelle rien : les deux peuvent être `null`, et c'est un cas nominal.
     */
    snapshot(): Promise<AccountUsage>;
    onAccountUsage(handler: (usage: AccountUsage) => void): Promise<Unsubscribe>;
}

/**
 * Ce que la ligne de statut montre — la quatrième frontière, et la seule qui écrive.
 *
 * Une lecture, une **bascule**, un event : le couple du thème, plus le geste. Ce qui part
 * vers le backend est l'identifiant du segment, jamais son nouvel état — le menu montre ce
 * que `features::theme` détient, et un menu qui renverrait le booléen qu'il a lu en
 * s'ouvrant en deviendrait le second détenteur
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface StatusBarBridge {
    /** La barre, telle que la session précédente l'a laissée. */
    layout(): Promise<StatusBarLayout>;
    /** Coche ou décoche ce segment. Le résultat revient par [`onLayout`]. */
    toggle(segment: StatusBarSegmentId): Promise<void>;
    /** La barre que le mode édition vient de composer — au relâchement seulement. */
    arrange(items: StatusBarLayout): Promise<void>;
    /** La disposition d'origine — le `reset all` de la spec §4.4, appliqué à la barre. */
    reset(): Promise<void>;
    onLayout(handler: (layout: StatusBarLayout) => void): Promise<Unsubscribe>;
}
