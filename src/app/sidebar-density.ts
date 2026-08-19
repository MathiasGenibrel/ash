import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * La densité de la sidebar : `comfortable` ou `compact` (spec §9).
 *
 * Troisième module de la même famille que `theme.ts` et `font-size.ts`, et il en suit la
 * règle : le choix vit en Rust (`src-tauri/src/features/theme/density.rs`), et ce module ne
 * fait que le **poser sur le document**
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * Ce qui est décidé ici : rien. Les deux hauteurs de ligne sont dans `styles.css`, sous
 * `[data-density]`, à côté des deux palettes — un pixel de retrait est du dessin, et le CSS
 * est le seul endroit du dépôt où il se lit. Ce module pose l'attribut ; la feuille de style
 * fait le reste, exactement comme `applyTheme` pose `data-theme`.
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands`. */
const SIDEBAR_DENSITY_EVENT = "ash://sidebar-density";

/** Les deux paliers. Miroir de `SidebarDensity` en Rust — voir `mirror.ts`. */
export type SidebarDensity = "comfortable" | "compact";

/** Le défaut, celui sur lequel Ash s'ouvre depuis toujours. */
export const DEFAULT_SIDEBAR_DENSITY: SidebarDensity = "comfortable";

/**
 * La densité telle que le backend l'a sérialisée.
 *
 * Une densité inconnue — un backend plus récent que la webview — ne doit pas laisser la
 * colonne sans mesures : elle est ignorée, et la précédente reste.
 */
export function parseSidebarDensity(value: unknown): SidebarDensity | null {
    return value === "comfortable" || value === "compact" ? value : null;
}

/** Pose la densité sur la racine du document. `styles.css` fait le reste. */
export function applySidebarDensity(root: HTMLElement, density: SidebarDensity): void {
    root.dataset["density"] = density;
}

/** Ce que `followSidebarDensity` rend : de quoi montrer, de quoi choisir, de quoi attendre. */
export interface SidebarDensityBinding {
    readonly current: SidebarDensity;
    /** Prévient à chaque changement — la fenêtre de réglages le montre, elle ne le retient pas. */
    subscribe(listener: (density: SidebarDensity) => void): () => void;
    /** Demande une densité au backend. Rejette si l'appel n'aboutit pas. */
    choose(density: SidebarDensity): Promise<void>;
    ready: Promise<void>;
}

/**
 * Relie la racine du document à la densité que le backend détient.
 *
 * La densité par défaut est posée **tout de suite**, avant tout aller-retour, comme la
 * palette : sans ça, la colonne s'ouvrirait sans attribut et le CSS n'aurait rien à lire.
 */
export function followSidebarDensity(root: HTMLElement): SidebarDensityBinding {
    const listeners = new Set<(density: SidebarDensity) => void>();
    let current: SidebarDensity = DEFAULT_SIDEBAR_DENSITY;
    applySidebarDensity(root, current);

    const apply = (density: SidebarDensity | null): void => {
        if (density === null || density === current) return;
        current = density;
        applySidebarDensity(root, current);
        for (const listener of listeners) listener(current);
    };

    const ready = (async (): Promise<void> => {
        await listen<unknown>(SIDEBAR_DENSITY_EVENT, (event) => {
            apply(parseSidebarDensity(event.payload));
        });

        apply(parseSidebarDensity(await invoke<unknown>("sidebar_density")));
    })();

    return {
        get current(): SidebarDensity {
            return current;
        },
        subscribe: (listener) => {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        },
        choose: (density) => invoke<void>("choose_sidebar_density", { density }),
        ready,
    };
}
