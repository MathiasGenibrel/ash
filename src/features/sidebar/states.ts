import type { AgentState } from "@/shared/ipc";

/**
 * La présentation des cinq états d'une ligne d'agent.
 *
 * Une fonction pure, et pas une feuille de style : le design en fait deux exigences
 * vérifiables, pas deux goûts.
 *
 * 1. **La forme porte l'état à elle seule.** Les cinq glyphes doivent rester distinguables
 *    sans couleur — daltonisme, écran mat, coin de l'œil.
 * 2. **`waiting` est le seul état teinté.** C'est ce qui le rend identifiable en vision
 *    périphérique et sous flou (le « test du flou » à 1,6 px de la planche `1e`). Un
 *    second fond coloré ferait perdre ce test à la sidebar entière.
 *
 * Cette feature **présente** les états ; elle n'en produit aucun. Ils viennent du backend
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et pour trois d'entre
 * eux des hooks d'[ADR-0007](../../../docs/adr/0007-etats-par-hooks.md), qui n'existent pas
 * encore.
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

/** Tous les états, dans l'ordre de la planche `1e`. */
export const AGENT_STATES: readonly AgentState[] = [
    "working",
    "waiting",
    "done",
    "idle",
    "error",
];

export function presentAgentState(state: AgentState): AgentPresentation {
    return PRESENTATIONS[state];
}
