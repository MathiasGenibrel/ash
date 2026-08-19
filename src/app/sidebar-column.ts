import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { DEFAULT_SIDEBAR_WIDTH, type SidebarColumnState } from "@/features/sidebar";

/**
 * La largeur de la colonne de gauche et son repli.
 *
 * **Les deux vivent en Rust** (`src-tauri/src/features/theme/sidebar_column.rs`), avec le
 * thème et la taille de police : c'est une préférence d'**apparence** (spec §9), elle survit
 * au redémarrage, et le frontend la rend sans la détenir
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Elle n'est **pas** dans
 * `~/.ash/state.json`, qui ne garde que les worktrees épinglés et les lignes repliées.
 *
 * `⌘B` et la poignée du bord agissent sur le même état, et c'est ce qui les rend cohérents :
 * refermer ne perd pas la largeur, rouvrir la restitue. Le raccourci demande une **bascule**
 * plutôt qu'un état, pour la même raison que la taille de police demande un pas — la webview
 * ne serait sinon plus la seule à savoir ce que la colonne devient.
 *
 * Il vit dans `app/` et non dans la sidebar, comme `theme.ts` et `sidebar-rows.ts` : la
 * colonne n'appelle aucune commande Tauri, c'est le composition root qui relie.
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands`. */
const SIDEBAR_COLUMN_EVENT = "ash://sidebar-column";

/** Ce sur quoi la fenêtre s'ouvre tant que le backend n'a pas répondu : les 240 px du design. */
export const DEFAULT_SIDEBAR_COLUMN: SidebarColumnState = {
    width: DEFAULT_SIDEBAR_WIDTH,
    collapsed: false,
};

/**
 * La colonne telle que le backend l'a sérialisée.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown` : une valeur qui n'est pas une
 * colonne — `null`, une largeur en chaîne, un backend plus récent que la webview — ne doit
 * pas faire disparaître la colonne ni la coincer à zéro. Elle est alors ignorée, et la
 * dernière valeur connue reste.
 */
export function parseSidebarColumn(value: unknown): SidebarColumnState | null {
    if (typeof value !== "object" || value === null) return null;
    const { width, collapsed } = value as { width?: unknown; collapsed?: unknown };
    if (typeof width !== "number" || !Number.isFinite(width) || width <= 0) return null;
    if (typeof collapsed !== "boolean") return null;
    return { width, collapsed };
}

/** Ce qu'on rend à qui suit la colonne : de quoi suivre, de quoi agir, de quoi attendre. */
export interface SidebarColumnBinding {
    /** L'état en cours — une colonne qui se dessine maintenant n'attend pas le prochain geste. */
    readonly current: SidebarColumnState;
    /** Prévient après chaque changement. */
    subscribe(listener: (column: SidebarColumnState) => void): void;
    /** La largeur réglée au relâchement du bord, ou par une flèche sur le séparateur. */
    setWidth(width: number): void;
    /** Refermer la colonne — un glissement relâché sous le plancher. */
    setCollapsed(collapsed: boolean): void;
    /** `⌘B`, et la touche du séparateur : la colonne devient l'inverse de ce qu'elle est. */
    toggle(): void;
    /** Le raccordement à l'état que le backend détient. Rejette s'il n'a pas lieu. */
    ready: Promise<void>;
}

/**
 * Relie la fenêtre à la colonne que le backend détient.
 *
 * La fonction n'est pas `async`, pour la même raison que `followThemeMode` : la colonne se
 * monte avant le premier aller-retour, et s'ajustera à la largeur gardée comme elle
 * s'ajusterait à un `⌘B`. Un échec de raccordement laisse une colonne de 240 px ouverte, ce
 * qui est exactement ce qu'un premier démarrage donne : il n'y a rien à rattraper.
 *
 * **Les trois gestes ne posent rien** : ils partent, le backend retient, écrit, et annonce.
 * Une largeur posée au passage donnerait deux routes vers l'écran, et il faudrait qu'elles
 * restent d'accord.
 */
export function followSidebarColumn(): SidebarColumnBinding {
    let current = DEFAULT_SIDEBAR_COLUMN;
    const listeners = new Set<(column: SidebarColumnState) => void>();

    const apply = (column: SidebarColumnState | null): void => {
        if (column === null) return;
        current = column;
        for (const listener of listeners) listener(column);
    };

    const ready = (async (): Promise<void> => {
        // L'abonnement **avant** la lecture : un `⌘B` joué entre les deux se perdrait dans
        // l'autre ordre, et la colonne resterait ouverte en se croyant fermée.
        await listen<unknown>(SIDEBAR_COLUMN_EVENT, (event) => {
            apply(parseSidebarColumn(event.payload));
        });
        apply(parseSidebarColumn(await invoke<unknown>("sidebar_column")));
    })();

    // Un geste qui n'aboutit pas ne laisse **aucune moitié d'état** : le backend n'a rien
    // retenu, il n'annonce rien, et la colonne reste où elle est. Il n'y a donc rien à
    // rattraper, et une bannière parlerait d'une largeur au moment où l'on regarde son
    // terminal — c'est la conduite déjà retenue pour les épingles.
    const send = (command: string, args: Record<string, unknown>): void => {
        invoke(command, args).catch(() => undefined);
    };

    return {
        get current() {
            return current;
        },
        subscribe(listener) {
            listeners.add(listener);
        },
        setWidth: (width) => {
            send("set_sidebar_column_width", { width: Math.round(width) });
        },
        setCollapsed: (collapsed) => {
            send("set_sidebar_column_collapsed", { collapsed });
        },
        toggle: () => {
            send("toggle_sidebar_column", {});
        },
        ready,
    };
}
