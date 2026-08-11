import type { ITheme } from "@xterm/xterm";

/**
 * La palette du terminal, lue dans la table de tokens de l'application.
 *
 * xterm.js ne peint pas en CSS : il compose ses cellules lui-même — sur un canevas WebGL,
 * de surcroît — et n'a donc aucun moyen de résoudre un `var(--ash-…)`. Il lui faut des
 * couleurs concrètes, et il les faut **à chaque changement de thème**, parce qu'une
 * palette posée à la construction resterait celle du thème de départ.
 *
 * D'où ce module : la table reste unique et vit dans `app/styles.css` (c'est elle qui
 * porte les deux paliers), et c'est ici qu'on la lit au moment de peindre. Aucun
 * hexadécimal n'est écrit de ce côté-ci — un seul suffirait à faire diverger le terminal
 * du reste de la fenêtre au premier ajustement de la palette.
 */

/**
 * Ce que le terminal demande à la table de tokens, clé xterm par clé xterm.
 *
 * Publié par la feature parce que c'est un **contrat** : `app/styles.css` doit définir ces
 * tokens dans ses deux paliers, et `app/styles.test.ts` s'en sert pour le vérifier. Le
 * fond et le texte ne sont pas propres au terminal — ce sont ceux de la fenêtre.
 */
export const TERMINAL_THEME_TOKENS = {
    background: "--ash-bg",
    foreground: "--ash-fg",
    cursor: "--ash-term-cursor",
    cursorAccent: "--ash-term-cursor-text",
    selectionBackground: "--ash-term-selection",
    selectionInactiveBackground: "--ash-term-selection-idle",
    black: "--ash-term-black",
    red: "--ash-term-red",
    green: "--ash-term-green",
    yellow: "--ash-term-yellow",
    blue: "--ash-term-blue",
    magenta: "--ash-term-magenta",
    cyan: "--ash-term-cyan",
    white: "--ash-term-white",
    brightBlack: "--ash-term-bright-black",
    brightRed: "--ash-term-bright-red",
    brightGreen: "--ash-term-bright-green",
    brightYellow: "--ash-term-bright-yellow",
    brightBlue: "--ash-term-bright-blue",
    brightMagenta: "--ash-term-bright-magenta",
    brightCyan: "--ash-term-bright-cyan",
    brightWhite: "--ash-term-bright-white",
} as const satisfies Readonly<Record<string, string>>;

/** De quoi lire un token, sans que la traduction ait besoin du DOM. */
export type TokenReader = (token: string) => string | undefined;

/**
 * Traduit les tokens résolus en thème xterm.
 *
 * Un token que la feuille de style ne définit pas est **omis**, et non remplacé par une
 * couleur écrite ici : xterm.js applique alors sa propre valeur par défaut, ce qui donne
 * un terminal aux couleurs discutables plutôt qu'un terminal qui ment sur la palette de
 * l'application. C'est aussi ce qui arrive à un token mal orthographié —
 * `getComputedStyle` rend la chaîne vide pour une propriété qu'il ne connaît pas.
 */
export function toXtermTheme(read: TokenReader): ITheme {
    const theme: ITheme = {};
    for (const [key, token] of Object.entries(TERMINAL_THEME_TOKENS) as [
        keyof typeof TERMINAL_THEME_TOKENS,
        string,
    ][]) {
        const color = read(token)?.trim();
        if (color !== undefined && color !== "") theme[key] = color;
    }
    return theme;
}

/**
 * La palette telle que le document la porte **en ce moment**.
 *
 * Lue sur la racine, où `app/theme.ts` pose `data-theme` et où les deux paliers de tokens
 * sont déclarés — et pas sur la surface du terminal : `getComputedStyle` d'un élément
 * détaché du document ne rend rien sous WebKit, et un onglet dont la surface n'est pas
 * encore posée naîtrait alors aux couleurs par défaut de xterm.js.
 */
export function readTerminalTheme(): ITheme {
    const style = getComputedStyle(document.documentElement);
    return toXtermTheme((token) => style.getPropertyValue(token));
}
