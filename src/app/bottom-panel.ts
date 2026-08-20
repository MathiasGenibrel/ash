import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { CLOSED_PANEL, PANEL_VIEWS, type BottomPanelState, type PanelView } from "@/features/panel";

/**
 * La hauteur du panneau bas, son ouverture et la vue qu'il montre.
 *
 * **Les trois vivent en Rust** (`src-tauri/src/features/theme/bottom_panel.rs`), avec le thème
 * et la colonne de gauche : c'est une préférence d'apparence (spec §9), elle survit au
 * redémarrage, et le frontend la rend sans la détenir
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * Il vit dans `app/` et non dans la feature, comme `sidebar-column.ts` et pour la même
 * raison : le panneau n'appelle aucune commande Tauri, c'est le composition root qui relie.
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands`. */
const BOTTOM_PANEL_EVENT = "ash://bottom-panel";

/**
 * Le panneau tel que le backend l'a sérialisé.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown` : une valeur qui n'est pas un
 * panneau — `null`, une vue que cette webview ne connaît pas, un backend plus récent — ne doit
 * pas ouvrir une surface vide ni coincer une hauteur à zéro. Elle est alors ignorée, et la
 * dernière valeur connue reste.
 */
export function parseBottomPanel(value: unknown): BottomPanelState | null {
    if (typeof value !== "object" || value === null) return null;
    const { height, open, view } = value as { height?: unknown; open?: unknown; view?: unknown };
    if (typeof height !== "number" || !Number.isFinite(height) || height <= 0) return null;
    if (typeof open !== "boolean") return null;
    if (typeof view !== "string" || !PANEL_VIEWS.includes(view as PanelView)) return null;
    return { height, open, view: view as PanelView };
}

/** Ce qu'on rend à qui suit le panneau : de quoi suivre, de quoi agir, de quoi attendre. */
export interface BottomPanelBinding {
    /** L'état en cours — un panneau qui se dessine maintenant n'attend pas le prochain geste. */
    readonly current: BottomPanelState;
    /** Prévient après chaque changement. */
    subscribe(listener: (panel: BottomPanelState) => void): void;
    /** Demande une vue. Le backend décide que la même vue redemandée referme le panneau. */
    showView(view: PanelView): void;
    /** La hauteur réglée au relâchement du bord, ou par une flèche sur le séparateur. */
    setHeight(height: number): void;
    /** Refermer — un glissement relâché sous le plancher, ou `Échap`. */
    close(): void;
    /** Le raccordement à l'état que le backend détient. Rejette s'il n'a pas lieu. */
    ready: Promise<void>;
}

/**
 * Relie la fenêtre au panneau que le backend détient.
 *
 * La fonction n'est pas `async`, pour la même raison que `followSidebarColumn` : la fenêtre se
 * monte avant le premier aller-retour, et le panneau s'ouvrira quand le backend l'aura dit. Un
 * échec de raccordement laisse un panneau fermé, ce qui est exactement ce qu'un premier
 * démarrage donne : il n'y a rien à rattraper — et surtout, le terminal garde toute sa
 * hauteur.
 *
 * **Les trois gestes ne posent rien** : ils partent, le backend retient, écrit, et annonce.
 * Une hauteur posée au passage donnerait deux routes vers l'écran, et il faudrait qu'elles
 * restent d'accord — ici, chaque désaccord serait un `SIGWINCH` de travers vers la TUI qui
 * tourne dans le terminal ([ADR-0003](../../docs/adr/0003-zone-terminal-unique.md)).
 */
export function followBottomPanel(): BottomPanelBinding {
    let current = CLOSED_PANEL;
    const listeners = new Set<(panel: BottomPanelState) => void>();

    const apply = (panel: BottomPanelState | null): void => {
        if (panel === null) return;
        current = panel;
        for (const listener of listeners) listener(panel);
    };

    const ready = (async (): Promise<void> => {
        // L'abonnement **avant** la lecture : un `⌘⌃G` joué entre les deux se perdrait dans
        // l'autre ordre, et le panneau resterait fermé en se croyant ouvert.
        await listen<unknown>(BOTTOM_PANEL_EVENT, (event) => {
            apply(parseBottomPanel(event.payload));
        });
        apply(parseBottomPanel(await invoke<unknown>("bottom_panel")));
    })();

    // Un geste qui n'aboutit pas ne laisse **aucune moitié d'état** : le backend n'a rien
    // retenu, il n'annonce rien, et le panneau reste où il est. Il n'y a donc rien à
    // rattraper, et une bannière parlerait d'une hauteur au moment où l'on regarde son
    // terminal — c'est la conduite déjà retenue pour la colonne et les épingles.
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
        showView: (view) => {
            send("show_bottom_panel_view", { view });
        },
        setHeight: (height) => {
            send("set_bottom_panel_height", { height: Math.round(height) });
        },
        close: () => {
            send("close_bottom_panel", {});
        },
        ready,
    };
}
