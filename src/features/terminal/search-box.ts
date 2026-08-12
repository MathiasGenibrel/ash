/**
 * Le champ de recherche d'un onglet — sa **description**, pas son DOM.
 *
 * Il appartient à l'onglet et non à la fenêtre
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)) : un onglet porte au plus un
 * PTY, donc au plus un scrollback, donc au plus une recherche. Passer d'un onglet à l'autre
 * ne peut pas emporter la recherche de l'un chez l'autre, parce qu'il y a autant de boîtes
 * que de terminaux et qu'aucune n'est partagée.
 *
 * Ce module ne peint rien et ne connaît ni xterm.js, ni l'addon de recherche : il rend une
 * [`UiNode`](../../shared/ui/node.ts), et `terminal-search.ts` la pose. C'est ce qui met
 * sous test ce qui décide — le compteur d'occurrences, les boutons éteints et leur raison,
 * `⏎`/`⇧⏎`/`⎋` — sans monter de DOM.
 *
 * **Rien ici n'écrit dans le PTY.** Aucune action de cette boîte n'appelle `emitInput` :
 * chercher est un geste d'affichage, et il ne doit ni faire sauter l'affichage en bas
 * (`emitInput` le fait, à raison, pour ce qu'on tape) ni écrire un octet dans le shell
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 */

import { button, field, row, text, type UiComponent } from "@/shared/ui";

/**
 * Ce que l'addon sait des occurrences, quand il le sait.
 *
 * `index` vaut `-1` quand l'addon a dépassé son seuil de surlignage : il connaît alors le
 * nombre d'occurrences mais plus laquelle est active. Ce n'est pas une erreur, c'est une
 * dégradation prévue — et elle a son rendu.
 */
export interface SearchMatches {
    readonly index: number;
    readonly count: number;
}

/**
 * L'état **d'affichage** de la recherche.
 *
 * Il vit côté TypeScript, et c'est régulier : ce n'est ni un état d'agent ni un état de
 * session ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le scrollback
 * est dans la webview — c'est xterm.js qui le tient —, donc ce qu'on y cherche n'a rien à
 * faire dans le backend, au même titre que la position de défilement et la sélection
 * d'onglet.
 */
export interface SearchBoxState {
    readonly query: string;
    /** `null` tant qu'aucune recherche n'a rendu de résultat — pas « zéro occurrence ». */
    readonly matches: SearchMatches | null;
}

/**
 * Ce que la boîte sait demander. Le contrôleur les branche sur l'addon.
 *
 * Des propriétés et non des méthodes : ce sont des rappels, que la composition passe tels
 * quels au socle. Les déclarer en méthodes ferait dépendre leur `this` de l'objet dont on
 * les a détachées, ce qui n'a aucun sens pour un rappel — et le lint le refuse.
 */
export interface SearchBoxActions {
    /** La saisie a changé : on cherche au fil de la frappe. */
    readonly search: (query: string) => void;
    readonly findNext: () => void;
    readonly findPrevious: () => void;
    /** `⎋` ou la croix : on referme, et le focus revient au terminal. */
    readonly close: () => void;
}

/**
 * La clé du champ, retenue par le contrôleur pour lui rendre le focus après un rendu.
 *
 * Le compteur d'occurrences change à chaque frappe, donc la boîte est repeinte pendant
 * qu'on tape : sans cette clé, le champ serait détruit et reconstruit sous les doigts, et
 * le curseur partirait avec l'ancien élément. Même mécanisme que la fenêtre de réglages.
 */
export const SEARCH_FOCUS_KEY = "terminal-search";

/** La classe de la rangée, lue par `terminal.css` — et par personne d'autre. */
const SEARCH_BOX_CLASS = "terminal-search-box";

/** Le compteur, tel qu'un œil doit le lire — et `""` quand il n'y a rien à dire. */
export function searchTally(state: SearchBoxState): string {
    if (state.query === "" || state.matches === null) return "";

    const { index, count } = state.matches;
    if (count === 0) return "no match";
    // Seuil de surlignage dépassé : l'addon compte encore, mais ne sait plus laquelle est
    // active. Afficher `0/2483` mentirait ; le nombre seul reste vrai.
    if (index < 0) return `${count} matches`;
    return `${index + 1}/${count}`;
}

/**
 * Pourquoi la navigation est éteinte, ou `null` si elle ne l'est pas.
 *
 * Le bouton reste **visible avec sa raison** plutôt que masqué : c'est la règle que le
 * socle rend non contournable ([`disabled`](../../shared/ui/button.ts)), et elle vaut ici
 * comme ailleurs — un `↓` qui disparaît quand le champ est vide fait croire qu'il n'existe
 * pas.
 */
function whyNoNavigation(state: SearchBoxState): string | null {
    if (state.query === "") return "type something to search the scrollback";
    if (state.matches !== null && state.matches.count === 0) return "no match in this tab";
    return null;
}

/** La boîte : un champ, un compteur, deux flèches et une croix. */
export function composeSearchBox(
    state: SearchBoxState,
    actions: SearchBoxActions,
): UiComponent {
    const reason = whyNoNavigation(state);
    const tally = searchTally(state);

    const input = field("search the scrollback")
        .class("terminal-search-field")
        .focusKey(SEARCH_FOCUS_KEY)
        .placeholder("search")
        .value(state.query)
        .onInput(actions.search)
        // `⏎` et `⇧⏎` sont le même geste pris dans les deux sens — c'est le socle qui
        // reconnaît la touche, la boîte ne compare aucune chaîne.
        .onSubmit(({ reversed }) => {
            if (reversed) actions.findPrevious();
            else actions.findNext();
        })
        .onCancel(actions.close);

    return row(
        input,
        row(text(tally)).class("terminal-search-tally"),
        step("↑", "previous match", reason, actions.findPrevious),
        step("↓", "next match", reason, actions.findNext),
        button("✕")
            .class("terminal-search-close")
            .attr("aria-label", "close search")
            .title("close search")
            .onClick(actions.close),
    ).class(SEARCH_BOX_CLASS);
}

/** Une flèche de navigation : un signe, le mot qu'il remplace, et sa raison d'être éteint. */
function step(
    symbol: string,
    label: string,
    reason: string | null,
    onClick: () => void,
): UiComponent {
    const control = button(symbol)
        .class("terminal-search-step")
        // Un signe seul est muet pour un lecteur d'écran : il dit en toutes lettres ce
        // qu'il fait, comme `glyph` l'exige de ses symboles.
        .attr("aria-label", label)
        .onClick(onClick);
    return reason === null ? control.title(label) : control.disabled(reason);
}
