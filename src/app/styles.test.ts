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
        const painted = AGENT_STATES.map((state) =>
            STATE_TOKENS.get(presentAgentState(state).className),
        );

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

describe("les couleurs de la fenêtre de réglages", () => {
    it("Given the warning colour the settings window introduced, when it is measured on a card, then it stays readable in both themes", () => {
        // Given — c'est la seule **couleur** que la fenêtre de réglages a ajoutée à la
        // table : le cran manquant entre `--ash-done` et `--ash-error`. Elle porte du
        // texte de 11 px et plus (le mode dégradé, `valide avec réserve`), donc le seuil
        // est celui du texte, pas celui d'un objet graphique.
        const measure = (tokens: Map<string, string>): number =>
            contrast(read(tokens, "--ash-warning"), read(tokens, "--ash-bg-card"));

        // When
        const ratios = [LIGHT, DARK].map(measure);

        // Then
        expect(Math.min(...ratios)).toBeGreaterThanOrEqual(4.5);
    });

    it("Given the colour the settings window writes its paragraphs in, when it is measured on the surfaces it is written on, then it clears the body-text threshold in both themes", () => {
        // Given — la maquette écrit des paragraphes entiers en `--ash-fg-dim`, qui mesure
        // 2,9:1 sur une carte : c'est un gris de métadonnée, tenable sur trois mots dans la
        // sidebar, illisible sur trois lignes. La prose longue est donc montée d'un cran, à
        // `--ash-fg-subtle`, et ce test est ce qui empêche de la redescendre sans le voir.
        const surfaces = ["--ash-bg", "--ash-bg-card"] as const;

        // When
        const ratios = [LIGHT, DARK].flatMap((tokens) =>
            surfaces.map((surface) =>
                contrast(read(tokens, "--ash-fg-subtle"), read(tokens, surface)),
            ),
        );

        // Then
        expect(Math.min(...ratios)).toBeGreaterThanOrEqual(4.5);
    });
});

describe("le diff d'un conflit de hooks", () => {
    it("Given the diff colours, when they are measured on the tinted line they are written on, then both sides stay readable in both themes", () => {
        // Given — la maquette ne dessine le diff **qu'en thème sombre** : les quatre valeurs
        // claires sont dérivées, donc rien ne les a jamais vues. C'est exactement le genre
        // de couleur qu'on choisit à l'œil sur un écran et qui disparaît sur un autre. Les
        // fonds sont des teintes à 8–10 % : on les compose sur le fond réel du panneau avant
        // de mesurer, sinon on mesurerait un contraste que personne ne voit.
        const panel = "--ash-bg-disabled";
        const sides = [
            ["--ash-diff-removed-fg", "--ash-diff-removed-bg"],
            ["--ash-diff-added-fg", "--ash-diff-added-bg"],
        ] as const;

        // When
        const ratios = [LIGHT, DARK].flatMap((tokens) =>
            sides.map(([ink, wash]) =>
                contrast(read(tokens, ink), blend(read(tokens, wash), read(tokens, panel))),
            ),
        );

        // Then — du texte de 11,5 px : le seuil est celui du corps de texte
        expect(Math.min(...ratios)).toBeGreaterThanOrEqual(4.5);
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

/**
 * Une teinte `rgb(r g b / a%)` posée sur un fond opaque, rendue en `#rrggbb`.
 *
 * Sans ça, mesurer un fond de diff reviendrait à mesurer une couleur pleine que personne
 * n'affiche : ce que l'œil voit est la composition, et c'est elle qui décide de la
 * lisibilité.
 */
function blend(wash: string, behind: string): string {
    const parsed = /rgb\((\d+) (\d+) (\d+) \/ (\d+)%\)/.exec(wash);
    if (parsed === null) return wash;
    const alpha = Number(parsed[4]) / 100;
    const front = [1, 2, 3].map((index) => Number(parsed[index]));
    const back = [0, 2, 4].map((offset) =>
        Number.parseInt(behind.replace("#", "").slice(offset, offset + 2), 16),
    );
    const mixed = front.map((value, index) =>
        Math.round(alpha * value + (1 - alpha) * (back[index] ?? 0)),
    );
    return `#${mixed.map((value) => value.toString(16).padStart(2, "0")).join("")}`;
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
