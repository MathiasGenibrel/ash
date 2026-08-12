import { describe, expect, it } from "bun:test";

import {
    SEARCH_DECORATION_TOKENS,
    TERMINAL_THEME_TOKENS,
    toSearchDecorations,
    toXtermTheme,
} from "./theme";

/**
 * La traduction des tokens en thème xterm, et elle seule.
 *
 * xterm.js ne s'instancie pas hors navigateur, et `bun test` tourne sans DOM : ce qui est
 * vérifiable ici est la fonction pure — que les couleurs viennent bien de la table, et ce
 * qui arrive quand un token manque. Le reste — que le terminal se repeigne vraiment à la
 * bascule, sans perdre son scrollback — relève de la vérification manuelle.
 */

/** Une palette résolue, comme `getComputedStyle` la rendrait. Défauts déterministes. */
class PaletteBuilder {
    private readonly tokens = new Map<string, string>(
        Object.values(TERMINAL_THEME_TOKENS).map((token) => [token, "#010203"]),
    );

    static create(): PaletteBuilder {
        return new PaletteBuilder();
    }

    with(token: string, color: string): this {
        this.tokens.set(token, color);
        return this;
    }

    /** Un token que la feuille de style ne définit pas : `getComputedStyle` rend `""`. */
    without(token: string): this {
        this.tokens.set(token, "");
        return this;
    }

    build(): (token: string) => string | undefined {
        return (token) => this.tokens.get(token);
    }
}

describe("la palette du terminal", () => {
    it("Given a stylesheet whose tokens resolve, when the xterm theme is built, then the colours are those of the table", () => {
        // Given — le terminal n'écrit aucune couleur de son côté : celles qu'il peint
        // doivent être exactement celles que la table définit, fond, texte et ANSI compris
        const palette = PaletteBuilder.create()
            .with("--ash-bg", "#fcfcfb")
            .with("--ash-fg", "#24262a")
            .with("--ash-term-yellow", "#8a6100")
            .with("--ash-term-bright-cyan", "#12879b")
            .build();

        // When
        const theme = toXtermTheme(palette);

        // Then
        expect(theme.background).toBe("#fcfcfb");
        expect(theme.foreground).toBe("#24262a");
        expect(theme.yellow).toBe("#8a6100");
        expect(theme.brightCyan).toBe("#12879b");
    });

    it("Given a colour written with the spaces getComputedStyle leaves around it, when the theme is built, then xterm receives a value it can parse", () => {
        // Given — `getPropertyValue` rend la déclaration telle quelle, espace de tête
        // compris ; xterm.js reconnaît ses couleurs à l'expression exacte, et retomberait
        // sur sa valeur par défaut pour ` #fcfcfb`
        const palette = PaletteBuilder.create().with("--ash-bg", " #fcfcfb ").build();

        // When
        const theme = toXtermTheme(palette);

        // Then
        expect(theme.background).toBe("#fcfcfb");
    });

    it("Given a token the stylesheet does not define, when the theme is built, then the key is left out instead of being invented", () => {
        // Given — un token manquant, ou mal orthographié, ne doit pas faire peindre le
        // terminal avec une couleur écrite en TypeScript : elle survivrait à tout
        // ajustement de la table, et le terminal mentirait sur la palette
        const palette = PaletteBuilder.create().without("--ash-term-cursor").build();

        // When
        const theme = toXtermTheme(palette);

        // Then
        expect("cursor" in theme).toBe(false);
        expect(theme.background).toBe("#010203");
    });
});

describe("le surlignage des occurrences", () => {
    /** Une palette où seuls les deux tokens du surlignage sont définis. */
    const decorationPalette = (rule: string, accent: string): ((token: string) => string) => {
        const tokens = new Map<string, string>([
            [SEARCH_DECORATION_TOKENS.match, rule],
            [SEARCH_DECORATION_TOKENS.active, accent],
        ]);
        return (token) => tokens.get(token) ?? "";
    };

    it("Given the two search tokens, when the decorations are built, then the current match differs by its border and not by its background", () => {
        // Given — le terminal garde la couleur de son texte, et Ash ne peut pas la
        // connaître : repeindre le fond de l'occurrence courante avec l'accent la rendrait
        // illisible dans au moins un des deux thèmes
        const palette = decorationPalette("#c9c7c2", "#c8481f");

        // When
        const decorations = toSearchDecorations(palette);

        // Then
        expect(decorations?.activeMatchBackground).toBe("#c9c7c2");
        expect(decorations?.matchBackground).toBe("#c9c7c2");
        expect(decorations?.activeMatchBorder).toBe("#c8481f");
    });

    it("Given a token the stylesheet does not define, when the decorations are built, then nothing is decorated instead of a colour written here", () => {
        // Given — les deux couleurs de la règle de défilement sont obligatoires pour
        // l'addon : combler un token manquant par un défaut écrit en TypeScript survivrait
        // à tout ajustement de la palette, et le surlignage mentirait sur le thème
        const palette = decorationPalette("", "#c8481f");

        // When
        const decorations = toSearchDecorations(palette);

        // Then — la recherche continue de sélectionner l'occurrence, elle ne la surligne
        // plus : une dégradation lisible, pas une panne
        expect(decorations).toBeUndefined();
    });
});
