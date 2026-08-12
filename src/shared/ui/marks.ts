import { ElementBuilder, text } from "./node";

/**
 * Les deux marques d'une ligne : un mot court, et un signe.
 *
 * Elles se ressemblent dans le DOM — un `<span>` avec du texte — et se distinguent par ce
 * qu'elles promettent. Une pastille se lit ; un glyphe se **reconnaît à sa forme**, donc il
 * doit dire en toutes lettres ce qu'il montre, sans quoi il ne dit rien à un lecteur
 * d'écran.
 */

class BadgeBuilder extends ElementBuilder {
    constructor(label: string) {
        super("span", "ui-badge");
        this.add(text(label));
    }
}

/** Un mot court posé sur une ligne : un libellé d'outil, un compteur, un état. */
export function badge(label: string): BadgeBuilder {
    return new BadgeBuilder(label);
}

class GlyphBuilder extends ElementBuilder {
    constructor(symbol: string, label: string) {
        super("span", "ui-glyph");
        // Le mot est obligatoire, comme la raison d'un bouton éteint : un signe sans nom
        // est invisible pour un lecteur d'écran, et illisible dès qu'on ne connaît pas la
        // convention. `presentAgentState` fait déjà porter un mot à chacun de ses cinq
        // glyphes — c'est la même exigence, rendue non contournable.
        this.add(text(symbol)).attr("aria-label", label);
    }
}

/** Un signe, et le mot qu'il remplace. */
export function glyph(symbol: string, label: string): GlyphBuilder {
    return new GlyphBuilder(symbol, label);
}

export type { BadgeBuilder, GlyphBuilder };
