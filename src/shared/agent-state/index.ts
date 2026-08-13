import type { AgentState } from "@/shared/ipc";

export { elapsedSince, formatElapsed } from "./elapsed";

/**
 * La présentation des cinq états d'un agent — glyphe, mot, teinte.
 *
 * Elle vit dans `shared/` et non dans une feature parce qu'elle en sert **deux** : la
 * sidebar, qui range une ligne par agent, et la ligne de statut de la zone terminal, qui
 * montre l'état de l'onglet actif (spec §4.2). Elle ne porte la règle d'aucune des deux —
 * le choix de l'état qu'une ligne repliée remonte, lui, reste à la sidebar
 * (`features/sidebar/states.ts`).
 *
 * Une fonction pure, et pas une feuille de style : le design en fait deux exigences
 * vérifiables, pas deux goûts.
 *
 * 1. **La forme porte l'état à elle seule.** Les cinq glyphes doivent rester distinguables
 *    sans couleur — daltonisme, écran mat, coin de l'œil.
 * 2. **`waiting` est le seul état teinté.** C'est ce qui le rend identifiable en vision
 *    périphérique et sous flou (le « test du flou » à 1,6 px de la planche `1e`). Un
 *    second fond coloré ferait perdre ce test à l'interface entière.
 *
 * Rien ici n'en **produit** : les états viennent du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et trois d'entre eux des
 * hooks d'[ADR-0007](../../../docs/adr/0007-etats-par-hooks.md), qui n'existent pas encore.
 *
 * Les classes posées ici (`ash-glyph`, `is-working`…) sont peintes par `app/styles.css`, à
 * côté des deux palettes : la couleur d'un état est du thème, pas de la mise en forme
 * d'une feature.
 */
export interface AgentPresentation {
    /** Le glyphe, choisi pour sa **forme** avant sa couleur. */
    readonly glyph: string;
    /** Le mot lu par les lecteurs d'écran, et l'infobulle. */
    readonly label: string;
    /** Le seul fond teinté de toute l'interface — `waiting`, et lui seul. */
    readonly tinted: boolean;
    /** Le filet gauche de 2 px : rien, l'accent, ou l'erreur. */
    readonly rail: "none" | "accent" | "error";
    /** Le nom barré : un agent mort ne se lit pas comme un agent vivant. */
    readonly struck: boolean;
    /** Le glyphe tourne — `working`, pour que le mouvement seul le distingue de `done`. */
    readonly spinning: boolean;
    /** La classe qui porte la couleur. */
    readonly className: string;
}

const PRESENTATIONS: Readonly<Record<AgentState, AgentPresentation>> = {
    working: {
        glyph: "◍",
        label: "working",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: true,
        className: "is-working",
    },
    waiting: {
        glyph: "❯",
        label: "waiting",
        tinted: true,
        rail: "accent",
        struck: false,
        spinning: false,
        className: "is-waiting",
    },
    done: {
        glyph: "✓",
        label: "done",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: false,
        className: "is-done",
    },
    idle: {
        glyph: "○",
        label: "idle",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: false,
        className: "is-idle",
    },
    error: {
        glyph: "✕",
        label: "error",
        tinted: false,
        rail: "error",
        struck: true,
        spinning: false,
        className: "is-error",
    },
};

/**
 * Tous les états, dans l'ordre de la planche `1e`.
 *
 * Dérivé de la table, et non recopié à côté d'elle : `PRESENTATIONS` est un
 * `Record<AgentState, …>`, donc le compilateur en garantit l'exhaustivité. Une seconde
 * liste écrite à la main oublierait un jour un état, et les tests qui parcourent « les
 * cinq états » passeraient en en regardant quatre.
 */
export const AGENT_STATES = Object.keys(PRESENTATIONS) as readonly AgentState[];

export function presentAgentState(state: AgentState): AgentPresentation {
    return PRESENTATIONS[state];
}

/**
 * Le glyphe d'un état, prêt à poser dans le DOM.
 *
 * Il vit ici plutôt que dans chaque feature parce que quatre décisions y tiennent
 * ensemble : la forme, la classe qui la peint, le mot que lit un lecteur d'écran, et le
 * mouvement qui distingue `working` de `done`. Écrit une fois par feature — c'était le
 * cas —, il finit par ne plus dire la même chose des deux côtés : un glyphe sans
 * `aria-label` ici, un `working` immobile là. Aucun test ne rattraperait la divergence, le
 * dépôt ne montant pas de DOM dans `bun test`.
 *
 * `.ash-glyph` et les classes d'état sont peintes par `app/styles.css`, à côté des deux
 * palettes : la couleur d'un état est du thème, pas de la mise en forme d'une feature.
 */
export function agentGlyph(state: AgentState): HTMLElement {
    const shown = PRESENTATIONS[state];
    const element = document.createElement("span");
    element.className = `ash-glyph ${shown.className}`;
    element.textContent = shown.glyph;
    element.setAttribute("aria-label", shown.label);
    if (shown.spinning) element.classList.add("is-spinning");
    return element;
}
