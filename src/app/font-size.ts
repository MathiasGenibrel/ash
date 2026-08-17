import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * La taille de police du terminal.
 *
 * **La taille vit en Rust** (`src-tauri/src/features/theme/font_size.rs`), et ce module ne
 * fait que la rendre : c'est la même règle que pour le thème et pour les onglets — le
 * frontend affiche un état, il ne le détient pas
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le choix se fait dans le
 * menu natif (`⌘+`, `⌘-`, `⌘0`), arrive ici par l'event `ash://terminal-font-size`, et se
 * relit au démarrage par la commande `terminal_font_size`.
 *
 * C'est aussi là que sont les **bornes** : rien n'est borné ici, parce que rien n'est
 * décidé ici. Ce module ne refuse qu'une valeur qui n'est pas une taille — un backend plus
 * récent que la webview, ou un event bricolé.
 *
 * Elle vaut pour **toute l'application**, et non par onglet : un `⌘+` agrandit les
 * terminaux de tous les onglets, y compris ceux qui sont masqués, et le prochain onglet
 * ouvert naît déjà à la bonne taille.
 *
 * Il vit dans `app/` et non dans `shared/ipc/` pour la même raison que `theme.ts` : une
 * préférence d'apparence est un objet de fenêtre, dont le seul lecteur est le composition
 * root — c'est lui qui la passe à la feature qui sait la peindre.
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands`. */
const TERMINAL_FONT_SIZE_EVENT = "ash://terminal-font-size";

/**
 * Ce sur quoi la fenêtre s'ouvre tant que le backend n'a pas répondu.
 *
 * Le même 13 que `FontSize::DEFAULT` côté Rust, et c'est la seule duplication assumée : la
 * première surface de terminal est créée avant le premier aller-retour, et la faire naître
 * sans taille demanderait de retarder l'ouverture du premier onglet. Une taille gardée
 * d'une session précédente arrive un instant plus tard et se pose comme un `⌘+`.
 */
export const DEFAULT_FONT_SIZE = 13;

/**
 * La taille telle que le backend l'a sérialisée.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown` : une valeur qui n'est pas une
 * taille — `null`, une chaîne, un flottant absurde — ne doit pas rendre les terminaux
 * illisibles ni les faire disparaître.
 */
export function parseFontSize(value: unknown): number | null {
    return typeof value === "number" && Number.isInteger(value) && value > 0 ? value : null;
}

/**
 * Ce qu'on rend à qui suit la taille de police.
 *
 * `current` autant que `subscribe` : une surface créée après un `⌘+` doit naître à la
 * taille en cours, sans attendre le changement suivant.
 */
export interface FontSizeChanges {
    readonly current: number;
    /** Prévient après chaque changement de taille. Rend de quoi se désabonner. */
    subscribe(listener: (points: number) => void): () => void;
}

/**
 * Le pas que la fenêtre de réglages demande — les mêmes trois que le menu Vue.
 *
 * Le contrat est un **pas**, jamais un nombre : les bornes et la valeur courante sont à
 * `FontSize`, en Rust, et une fenêtre qui enverrait une taille en deviendrait le second
 * détenteur ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export type FontStep = "bigger" | "smaller" | "default";

/** Ce que `followTerminalFontSize` rend : de quoi suivre, et de quoi attendre le backend. */
export interface FontSizeBinding {
    changes: FontSizeChanges;
    /**
     * Demande un pas au backend. Rejette si l'appel n'aboutit pas.
     *
     * Rien n'est posé ici : la nouvelle taille revient par `ash://terminal-font-size`, comme
     * après un `⌘+`, et c'est cette annonce qui fait relire sa grille à chaque terminal
     * ouvert. Une taille posée au passage se serait décalée d'une borne atteinte.
     */
    step(step: FontStep): Promise<void>;
    /** Le raccordement à la taille que le backend détient. Rejette s'il n'a pas lieu. */
    ready: Promise<void>;
}

/**
 * Relie la fenêtre à la taille de police que le backend détient.
 *
 * La fonction n'est pas `async`, pour la même raison que `followThemeMode` : ses abonnés
 * doivent pouvoir se brancher tout de suite, sur une valeur déjà posée. L'abonnement à
 * l'event est pris avant la lecture de la commande pour ne pas perdre un changement qui
 * arriverait entre les deux.
 */
export function followTerminalFontSize(): FontSizeBinding {
    const listeners = new Set<(points: number) => void>();
    let current = DEFAULT_FONT_SIZE;

    const apply = (points: number | null): void => {
        // Une taille identique n'est pas un changement : la relecture au démarrage rend
        // presque toujours la valeur déjà posée, et refaire la grille de chaque terminal
        // pour rien enverrait un `SIGWINCH` gratuit dans chaque PTY.
        if (points === null || points === current) return;
        current = points;
        for (const listener of listeners) listener(current);
    };

    const ready = (async (): Promise<void> => {
        await listen<unknown>(TERMINAL_FONT_SIZE_EVENT, (event) => {
            apply(parseFontSize(event.payload));
        });

        apply(parseFontSize(await invoke<unknown>("terminal_font_size")));
    })();

    return {
        changes: {
            get current(): number {
                return current;
            },
            subscribe: (listener) => {
                listeners.add(listener);
                return () => {
                    listeners.delete(listener);
                };
            },
        },
        step: (asked) => invoke<void>("step_terminal_font_size", { step: asked }),
        ready,
    };
}
