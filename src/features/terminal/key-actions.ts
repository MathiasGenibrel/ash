/**
 * Les raccourcis qui pilotent **l'affichage** du terminal, et n'écrivent pas dans le PTY.
 *
 * C'est le pendant de `key-bindings.ts`, et non son extension. Les deux tables lisent le
 * même `KeyChord`, mais elles ne rendent pas la même chose et ne se trompent pas de la
 * même façon :
 *
 * - `resolveKeyBinding` rend des **octets** pour le PTY. Sa règle est celle d'ADR-0015 —
 *   jamais de `\r` — et son défaut est de rendre `null` pour **laisser passer** la frappe
 *   vers xterm.js, sans quoi macOS ne compose plus `|` ni `~`.
 * - `resolveKeyAction` rend une **action**, qu'on exécute puis qu'on **consomme** : la
 *   frappe ne doit ni atteindre le shell, ni laisser WKWebView appliquer son défaut.
 *
 * Un seul résolveur qui rendrait `{send} | {action}` ferait cohabiter derrière une même
 * interface deux modes d'erreur opposés — laisser passer d'un côté, couper de l'autre —
 * et ferait porter à la saisie des tests qui ne la concernent pas.
 *
 * Ce que ce module ne fait **pas** : deviner le défilement. `scrollOnUserInput` de
 * xterm.js vaut `true` par défaut, donc une frappe ordinaire ramène déjà l'affichage en
 * bas, et `scrollLines` borne `ydisp` entre `0` et `ybase`, donc le haut et le bas du
 * tampon s'atteignent sans bruit. Ces deux comportements sont **acquis**, pas réécrits
 * ici : les tester serait tester xterm.js.
 *
 * Comme `key-bindings.ts`, la table ne nomme aucun accélérateur du menu natif : macOS les
 * consomme dans `performKeyEquivalent:` avant que la webview ne voie un `keydown`, et une
 * entrée qui les recouvrirait serait morte. Aucun `Cmd+flèche` n'est déclaré dans
 * `src-tauri/src/menu.rs` ; `MENU_ACCELERATORS`, dans le test voisin, tient le garde-fou
 * du jour où ça changerait.
 */

import type { KeyChord } from "./key-bindings";

/**
 * Ce qu'un accord déclenche dans la vue.
 *
 * Une union de chaînes plutôt qu'un `enum` : elle s'étend par ajout, et l'ajout se voit.
 * La recherche dans le scrollback (#79) viendra s'y greffer, et le `switch` exhaustif
 * d'`applyKeyAction` refusera alors de compiler tant que la nouvelle action n'aura pas
 * son effet — c'est le point de branchement prévu.
 */
export type KeyAction =
    | "scroll-page-up"
    | "scroll-page-down"
    | "scroll-line-up"
    | "scroll-line-down";

/** Les seules valeurs de `KeyboardEvent.key` que cette table nomme. */
type ScrollKey = "ArrowUp" | "ArrowDown";

/**
 * L'accord, écrit comme on le lit : ses modificateurs puis sa touche.
 *
 * Même forme que dans `key-bindings.ts`, et pour la même raison : la table étant indexée
 * par ce type, une entrée mal orthographiée ne compile pas, et **deux entrées pour le
 * même accord non plus** — un objet littéral n'a pas deux fois la même clé. Le
 * recouvrement silencieux, où la seconde ligne serait morte sans qu'aucun test ne le
 * dise, n'est pas représentable.
 */
type Chord = `${"" | "⇧"}⌘${ScrollKey}`;

/**
 * Les quatre raccourcis de défilement, et rien d'autre.
 *
 * `⌘` seul pagine, `⌘⇧` avance d'une ligne : la modulation par ⇧ suit la convention de
 * Terminal.app et d'iTerm, où le modificateur supplémentaire affine le geste plutôt que
 * de l'inverser.
 *
 * Les flèches **nues** n'y figurent pas, et ne doivent jamais y figurer : ce sont elles
 * qui parcourent l'historique de `zsh`. Ce sont deux navigations différentes — l'une dans
 * le tampon d'affichage, l'autre dans les commandes passées — et la seconde appartient au
 * shell.
 */
const SCROLLING = {
    // Page précédente / suivante.
    "⌘ArrowUp": "scroll-page-up",
    "⌘ArrowDown": "scroll-page-down",
    // Une ligne vers le haut / vers le bas.
    "⇧⌘ArrowUp": "scroll-line-up",
    "⇧⌘ArrowDown": "scroll-line-down",
} satisfies Partial<Record<Chord, KeyAction>>;

/**
 * La même table, vue comme un dictionnaire ouvert.
 *
 * L'affectation élargit le type sans `as` : les clés restent vérifiées à l'écriture, et
 * la résolution peut y chercher n'importe quelle touche du clavier.
 */
const BY_CHORD: Readonly<Record<string, KeyAction | undefined>> = SCROLLING;

/**
 * L'action qu'un accord déclenche, ou `null` s'il ne nous regarde pas.
 *
 * Les modificateurs sont comparés **exactement** : `⌘` est requis, `⌃` et `⌥` doivent être
 * relâchés. `⌥⌘↑` n'est pas un raccourci d'Ash, et l'avaler priverait le shell — ou la
 * composition de macOS — de ce qu'il en aurait fait.
 */
export function resolveKeyAction(chord: KeyChord): KeyAction | null {
    if (chord.type !== "keydown") return null;
    if (chord.ctrlKey || chord.altKey) return null;

    const pressed = `${chord.shiftKey ? "⇧" : ""}${chord.metaKey ? "⌘" : ""}${chord.key}`;
    return BY_CHORD[pressed] ?? null;
}

/**
 * Ce que l'action a besoin de savoir faire du terminal — et rien de plus.
 *
 * Deux méthodes au lieu du `Terminal` de xterm.js : c'est ce qui rend la **convention de
 * signe** vérifiable sans DOM ni WebGL. Un `-1` écrit `+1` fait défiler dans le mauvais
 * sens, et c'est la faute qu'on veut voir tomber dans `bun test` plutôt que sous les
 * doigts. `Terminal` satisfait cette interface structurellement, sans adaptateur.
 */
export interface ScrollSurface {
    scrollPages(pageCount: number): void;
    scrollLines(amount: number): void;
}

/**
 * Exécute l'action sur la surface.
 *
 * Aucune borne n'est calculée ici : `scrollLines` et `scrollPages` bornent `ydisp` entre
 * le haut et le bas du tampon, donc arrivé au bout, l'appel ne fait simplement rien.
 * Recalculer cette limite ici la dupliquerait, et la ferait diverger.
 */
export function applyKeyAction(action: KeyAction, surface: ScrollSurface): void {
    switch (action) {
        case "scroll-page-up":
            surface.scrollPages(-1);
            return;
        case "scroll-page-down":
            surface.scrollPages(1);
            return;
        case "scroll-line-up":
            surface.scrollLines(-1);
            return;
        case "scroll-line-down":
            surface.scrollLines(1);
            return;
    }
}
