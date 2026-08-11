import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { TERMINAL_THEME_TOKENS } from "@/features/terminal";
import { AGENT_STATES, presentAgentState } from "@/shared/agent-state";

/**
 * Les deux palettes, lues dans la feuille de style elle-même.
 *
 * Le critère d'acceptation porte sur la **lisibilité** — « les cinq états d'agent lisibles
 * dans les deux thèmes » —, pas sur l'existence des thèmes. Deux façons de le rater, et
 * les deux sont mécaniques : ajouter un token à une seule palette, et choisir une couleur
 * qui disparaît sur son fond. Ce sont les deux seules choses vérifiées ici ; rien de ce
 * qui relève du goût.
 */

const STYLES = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** Les tokens d'un bloc `:root`, par son sélecteur. */
function palette(selector: string): Map<string, string> {
    const start = STYLES.indexOf(selector);
    const open = STYLES.indexOf("{", start);
    const close = STYLES.indexOf("}", open);
    const tokens = new Map<string, string>();
    for (const line of STYLES.slice(open + 1, close).split("\n")) {
        const declaration = /^\s*(--[\w-]+):\s*([^;]+);/.exec(line);
        if (declaration !== null) tokens.set(declaration[1] ?? "", (declaration[2] ?? "").trim());
    }
    return tokens;
}

const LIGHT = palette(':root[data-theme="light"]');
const DARK = palette(':root[data-theme="dark"]');

/** Le fond sur lequel un glyphe d'état est peint : la ligne de statut et la sidebar. */
const SURFACES = ["--ash-bg-status", "--ash-bg-sidebar"] as const;

/**
 * La couleur de chaque état, **lue** dans `.ash-glyph.is-… { color: var(--…) }`.
 *
 * Elle est lue et non recopiée : une table écrite ici dirait ce que le CSS *devrait* dire,
 * et laisserait passer exactement la faute qu'on cherche — un état repeint avec le token
 * d'un autre, qui garde son contraste et perd son sens.
 */
const STATE_TOKENS = new Map(
    [...STYLES.matchAll(/\.ash-glyph\.(is-[\w-]+)\s*\{\s*color:\s*var\((--[\w-]+)\)/g)].map(
        (match) => [match[1] ?? "", match[2] ?? ""],
    ),
);

describe("les deux palettes", () => {
    it("Given the light and dark palettes, when their tokens are compared, then neither defines a token the other lacks", () => {
        // Given — un token oublié d'un côté n'est pas une erreur de compilation : il
        // hérite silencieusement de l'autre palette, et se voit le jour où quelqu'un
        // bascule de thème
        const light = [...LIGHT.keys()].sort();
        const dark = [...DARK.keys()].sort();

        // When / Then
        expect(light).toEqual(dark);
        expect(light.length).toBeGreaterThan(0);
    });

    it("Given the five agent states, when their colour is looked up, then the stylesheet paints it in both palettes", () => {
        // Given — un état que `styles.css` ne peint pas est un état qu'on ne distingue
        // plus ; un état peint avec un token qu'une palette ignore aussi
        const painted = AGENT_STATES.map((state) => STATE_TOKENS.get(presentAgentState(state).className));

        // When
        const missing = painted.filter(
            (token) => token === undefined || !LIGHT.has(token) || !DARK.has(token),
        );

        // Then
        expect(missing).toEqual([]);
        expect(new Set(painted).size).toBe(AGENT_STATES.length);
    });

    it("Given the four states that ask for attention, when they are measured against their background, then they clear the non-text contrast threshold in both themes", () => {
        // Given — WCAG 1.4.11 : un glyphe est un objet graphique, son seuil est 3:1. Il
        // vaut pour tous les états qui demandent quelque chose ; `idle` est traité en
        // dessous, parce qu'il est délibérément le plus discret. La liste est dérivée, pour
        // qu'un sixième état soit mesuré sans qu'on ait à y penser.
        const quiet = STATE_TOKENS.get(presentAgentState("idle").className);
        const loud = [...STATE_TOKENS.values()].filter((token) => token !== quiet);

        // When
        const ratios = [LIGHT, DARK].flatMap((tokens) =>
            loud.flatMap((state) =>
                SURFACES.map((surface) => contrast(read(tokens, state), read(tokens, surface))),
            ),
        );

        // Then
        expect(Math.min(...ratios)).toBeGreaterThanOrEqual(3);
    });

    it("Given the five states, when they are measured against their background, then idle is the quietest in both themes", () => {
        // Given — un shell posé à son invite ne doit pas se lire aussi fort qu'un agent en
        // erreur. C'est une propriété de la palette, pas du thème : la perdre d'un seul
        // côté rendrait la hiérarchie visuelle différente en clair et en sombre.
        const measure = (tokens: Map<string, string>, token: string): number =>
            contrast(read(tokens, token), read(tokens, "--ash-bg-status"));

        // When
        const quietest = [LIGHT, DARK].map((tokens) =>
            [...STATE_TOKENS.values()].reduce((a, b) =>
                measure(tokens, a) <= measure(tokens, b) ? a : b,
            ),
        );

        // Then
        expect(quietest).toEqual(["--ash-idle", "--ash-idle"]);
    });
});

describe("la palette du terminal", () => {
    it("Given the colours xterm.js asks for, when they are looked up, then both palettes define every one of them", () => {
        // Given — xterm.js ne résout pas un `var(--ash-…)` : un token absent d'une palette
        // ne se voit pas en CSS, il se voit dans le terminal, sous la forme d'une couleur
        // par défaut de xterm.js au milieu de celles de l'application
        const asked = Object.values(TERMINAL_THEME_TOKENS);

        // When
        const missing = asked.filter((token) => !LIGHT.has(token) || !DARK.has(token));

        // Then
        expect(missing).toEqual([]);
    });

    it("Given the ANSI colours an agent writes with, when they are measured against the terminal background, then they stay readable in both themes", () => {
        // Given — `claude` colore abondamment, et les valeurs par défaut de xterm.js sont
        // pensées pour un fond sombre : du jaune ou du cyan clair sur `--ash-bg` clair ne se
        // lit pas. Les six teintes sont du texte (4,5:1), leurs variantes vives de
        // l'emphase (3:1). `black` et `white` sont hors de la mesure, et ne peuvent pas ne
        // pas l'être : une palette ANSI les place aux deux bouts de l'échelle, donc l'un des
        // deux touche toujours le fond — c'est ce que les applications attendent d'eux.
        const hue = /^(bright)?(red|green|yellow|blue|magenta|cyan)$/i;
        const ansi = Object.entries(TERMINAL_THEME_TOKENS).filter(([key]) => hue.test(key));

        // When
        const failures = [LIGHT, DARK].flatMap((tokens) =>
            ansi.filter(
                ([key, token]) =>
                    contrast(read(tokens, token), read(tokens, "--ash-bg")) <
                    (key.startsWith("bright") ? 3 : 4.5),
            ),
        );

        // Then
        expect(failures).toEqual([]);
        expect(ansi).toHaveLength(12);
    });
});

function read(tokens: Map<string, string>, token: string): string {
    const value = tokens.get(token);
    if (value === undefined) throw new Error(`token absent de la palette : ${token}`);
    return value;
}

/** Le rapport de contraste WCAG entre deux couleurs `#rrggbb`. */
function contrast(a: string, b: string): number {
    const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
    return (high + 0.05) / (low + 0.05);
}

function luminance(color: string): number {
    const hex = color.replace("#", "");
    const channels = [0, 2, 4].map((offset) => {
        const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
        return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
    }) as [number, number, number];
    return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}
