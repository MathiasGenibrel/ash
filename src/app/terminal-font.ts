import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * La police du terminal — la **famille**, pas sa taille.
 *
 * Même forme que `font-size.ts`, et pour la même raison : la préférence vit en Rust
 * (`src-tauri/src/features/theme/font.rs`), ce module ne fait que la rendre
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Elle se relit au démarrage
 * par la commande `terminal_font`, et arrive ensuite par `ash://terminal-font`.
 *
 * Deux différences avec la taille, et elles se tiennent :
 *
 * - ce qui part au backend est une **valeur** et non un pas. Il n'existe pas de « police
 *   suivante » : le choix se fait dans une liste, et c'est le backend qui rend la liste
 *   ([`installedMonospaceFonts`]) comme il rend le choix. La borne n'est donc pas ici non
 *   plus — `TerminalFont` refuse en Rust ce qui n'est pas un nom de famille ;
 * - la famille n'est **jamais** posée seule dans une déclaration CSS : elle voyage avec la
 *   pile de repli, pour qu'une police désinstallée entre deux démarrages laisse un terminal
 *   lisible plutôt qu'un rendu proportionnel ([`fontStack`]).
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands`. */
const TERMINAL_FONT_EVENT = "ash://terminal-font";

/**
 * Celle qu'Ash embarque, et sur laquelle la fenêtre s'ouvre.
 *
 * Le même nom que `TerminalFont::DEFAULT_FAMILY` côté Rust, et la même duplication assumée
 * que `DEFAULT_FONT_SIZE` : la première surface de terminal est créée avant le premier
 * aller-retour, et la faire naître sans police mesurerait sa cellule sur une face de repli.
 */
export const DEFAULT_TERMINAL_FONT = "JetBrains Mono";

/**
 * La pile que xterm.js reçoit : la famille choisie, puis les replis.
 *
 * Une famille seule serait un pari sur une liste que le backend a lue **au démarrage** : une
 * police désinstallée depuis, ou un `~/.ash/theme.json` recopié d'une autre machine, et
 * WebKit tomberait sur sa police par défaut — proportionnelle, donc un terminal qui
 * n'aligne plus rien. Les guillemets sont posés ici parce que la plupart des familles
 * portent une espace (`SF Mono`, `PT Mono`).
 */
export function fontStack(family: string): string {
    return `"${family}", ui-monospace, monospace`;
}

/**
 * La famille telle que le backend l'a sérialisée.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown` : une valeur qui n'est pas un nom
 * de famille ne doit pas laisser les terminaux sans police. Le vide est refusé ici pour la
 * même raison qu'en Rust — une déclaration `font-family: ""` n'a pas de sens.
 */
export function parseFontFamily(value: unknown): string | null {
    return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}

/** Ce qu'on rend à qui suit la police : la valeur en cours, et les changements. */
export interface FontFamilyChanges {
    readonly current: string;
    /** Prévient après chaque changement. Rend de quoi se désabonner. */
    subscribe(listener: (family: string) => void): () => void;
}

/** Ce que `followTerminalFont` rend : de quoi suivre, de quoi choisir, de quoi attendre. */
export interface TerminalFontBinding {
    family: FontFamilyChanges;
    /**
     * Demande une famille au backend. Rejette si l'appel n'aboutit pas.
     *
     * Rien n'est posé ici : la nouvelle police revient par `ash://terminal-font`, comme la
     * taille revient d'un `⌘+`. C'est ce qui fait que la fenêtre de réglages ne peut pas
     * afficher un choix que le backend n'a pas retenu.
     */
    choose(family: string): Promise<void>;
    ready: Promise<void>;
}

/** Les familles monospace que le système porte, telles que le backend les a lues. */
export function installedMonospaceFonts(): Promise<readonly string[]> {
    return invoke<readonly string[]>("monospace_fonts");
}

/**
 * Relie la fenêtre à la police que le backend détient.
 *
 * Pas `async`, comme `followThemeMode` et `followTerminalFontSize` : ses abonnés doivent
 * pouvoir se brancher tout de suite, sur une valeur déjà posée. L'abonnement à l'event est
 * pris avant la lecture de la commande pour ne pas perdre un changement qui arriverait entre
 * les deux.
 */
export function followTerminalFont(): TerminalFontBinding {
    const listeners = new Set<(family: string) => void>();
    let current = DEFAULT_TERMINAL_FONT;

    const apply = (family: string | null): void => {
        // Une famille identique n'est pas un changement : la relecture au démarrage rend
        // presque toujours la valeur déjà posée, et reposer `fontFamily` sur xterm.js le
        // fait remesurer sa cellule et refaire la grille de chaque terminal ouvert.
        if (family === null || family === current) return;
        current = family;
        for (const listener of listeners) listener(current);
    };

    const ready = (async (): Promise<void> => {
        await listen<unknown>(TERMINAL_FONT_EVENT, (event) => {
            apply(parseFontFamily(event.payload));
        });

        apply(parseFontFamily(await invoke<unknown>("terminal_font")));
    })();

    return {
        family: {
            get current(): string {
                return current;
            },
            subscribe: (listener) => {
                listeners.add(listener);
                return () => {
                    listeners.delete(listener);
                };
            },
        },
        choose: (family) => invoke<void>("choose_terminal_font", { font: family }),
        ready,
    };
}
