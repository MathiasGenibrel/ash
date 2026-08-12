import { SearchAddon, type ISearchOptions } from "@xterm/addon-search";
import type { Terminal } from "@xterm/xterm";

import { paint, FOCUS_KEY } from "@/shared/ui";

import {
    composeSearchBox,
    SEARCH_FOCUS_KEY,
    type SearchBoxActions,
    type SearchMatches,
} from "./search-box";
import { readSearchDecorations } from "./theme";

/**
 * La recherche d'**un** onglet : l'addon xterm, la boîte, et le va-et-vient du focus.
 *
 * C'est la moitié qui touche le DOM, et elle est aussi mince que `paint` : elle ne décide
 * rien. Ce qui décide — le compteur, les boutons éteints, la touche qui fait quoi — est
 * dans [`search-box.ts`](./search-box.ts), sous test sans DOM. Ici, il n'y a que du
 * câblage, et il se vérifie à la main : `bun test` n'a ni `document` ni WebGL, et xterm.js
 * ne s'instancie pas hors navigateur.
 *
 * Une instance par `XtermView`, donc par onglet
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)) : la boîte est un enfant de
 * la surface de l'onglet, elle se masque avec lui et meurt avec lui. Rien n'est partagé
 * entre onglets, donc rien ne peut suivre d'un onglet à l'autre.
 *
 * **Aucun octet ne part vers le PTY d'ici.** Cette classe ne connaît ni le pont Tauri ni
 * `emitInput` — c'est aussi ce qui lui évite le retour en bas que `XtermView.emitInput`
 * applique à la saisie : on cherche pour aller à une occurrence, pas pour revenir au prompt.
 */
export class TerminalSearch {
    private readonly addon = new SearchAddon();
    private readonly box: HTMLElement;
    private opened = false;
    private query = "";
    private matches: SearchMatches | null = null;

    private readonly actions: SearchBoxActions = {
        search: (query) => {
            this.query = query;
            // Au fil de la frappe, et vers l'avant : `incremental` étend la sélection tant
            // que le terme tapé correspond encore, au lieu de sauter à l'occurrence
            // suivante à chaque lettre.
            this.run((term, options) =>
                this.addon.findNext(term, { ...options, incremental: true }),
            );
        },
        findNext: () => {
            this.run((term, options) => this.addon.findNext(term, options));
        },
        findPrevious: () => {
            this.run((term, options) => this.addon.findPrevious(term, options));
        },
        close: () => {
            this.close();
        },
    };

    constructor(
        parent: HTMLElement,
        private readonly terminal: Terminal,
    ) {
        this.box = document.createElement("div");
        this.box.className = "terminal-search";
        parent.append(this.box);

        this.terminal.loadAddon(this.addon);
        // Le compteur ne vient pas de nous : c'est l'addon qui sait combien d'occurrences il
        // a surlignées, et il le dit après coup — `findNext` rend un booléen, pas un total.
        this.addon.onDidChangeResults((results) => {
            this.matches = { index: results.resultIndex, count: results.resultCount };
            this.render(false);
        });
    }

    /** `⌘F` : ouvre le champ, ou le remet sous les doigts s'il est déjà ouvert. */
    open(): void {
        this.opened = true;
        this.render(true);
    }

    /**
     * `⎋` ou la croix : referme, efface le surlignage, **et rend le focus au terminal**.
     *
     * Le retour du focus n'est pas une politesse : sans lui, la frappe suivante irait dans
     * un champ qui n'est plus affiché, c'est-à-dire nulle part.
     */
    close(): void {
        this.opened = false;
        this.query = "";
        this.matches = null;
        this.addon.clearDecorations();
        this.render(false);
        this.terminal.focus();
    }

    dispose(): void {
        this.addon.dispose();
        this.box.remove();
    }

    /**
     * Lance la recherche, ou efface ce qui était surligné quand il ne reste rien à chercher.
     *
     * Les couleurs sont relues à chaque appel : une bascule de thème pendant que le champ
     * est ouvert se répercute à la frappe suivante, sans abonnement.
     */
    private run(search: (term: string, options: ISearchOptions) => boolean): void {
        if (this.query === "") {
            this.addon.clearDecorations();
            this.matches = null;
            this.render(false);
            return;
        }

        const decorations = readSearchDecorations();
        search(this.query, decorations === undefined ? {} : { decorations });
        // Sans décoration, `onDidChangeResults` ne se déclenche pas : c'est le seul cas où
        // le rendu ne suivrait pas la frappe.
        if (decorations === undefined) this.render(false);
    }

    /**
     * Refait la boîte, et remet le curseur là où il était.
     *
     * La description est repeinte en entier à chaque changement — le compteur bouge à chaque
     * lettre —, donc le champ est détruit et reconstruit sous les doigts. Le curseur est
     * relevé avant et reposé après, comme dans la fenêtre de réglages : sans ça, taper au
     * milieu d'un terme déplacerait le point d'insertion à la fin du mot.
     */
    private render(takeFocus: boolean): void {
        this.box.classList.toggle("is-open", this.opened);
        if (!this.opened) {
            this.box.replaceChildren();
            return;
        }

        const caret = this.caret();
        const box = composeSearchBox({ query: this.query, matches: this.matches }, this.actions);
        this.box.replaceChildren(paint(box.build()));

        const input = this.box.querySelector<HTMLInputElement>(
            `input[${FOCUS_KEY}="${SEARCH_FOCUS_KEY}"]`,
        );
        if (input === null) return;
        if (takeFocus) {
            input.focus();
            input.select();
            return;
        }
        if (caret === null) return;
        input.focus();
        input.setSelectionRange(caret, caret);
    }

    /** La position du curseur, si c'est bien notre champ qui a le focus. */
    private caret(): number | null {
        const active = document.activeElement;
        if (!(active instanceof HTMLInputElement)) return null;
        if (active.getAttribute(FOCUS_KEY) !== SEARCH_FOCUS_KEY) return null;
        return active.selectionStart;
    }
}
