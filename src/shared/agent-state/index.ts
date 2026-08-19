import type { AgentState } from "@/shared/ipc";
import { SVG_NAMESPACE } from "@/shared/ui";

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
    /**
     * Le glyphe, choisi pour sa **forme** avant sa couleur.
     *
     * C'est un caractère, donc une forme que la police décide et qu'aucune animation ne
     * déforme. Quand un état demande plus que ça — `working`, dont le mouvement *est*
     * l'information —, il porte en plus un [`shape`], et c'est celui-ci que le DOM reçoit :
     * le caractère reste alors le repli des contextes purement textuels (l'énumération des
     * états qui interrompent, dans les réglages).
     */
    readonly glyph: string;
    /**
     * Le tracé d'un arc **incomplet**, quand un caractère ne suffit pas — `working`, et lui
     * seul. `null` partout ailleurs.
     *
     * `◍` était un disque plein presque symétrique par rotation : la rotation tournait, et
     * rien ne bougeait à l'œil — `working` se lisait comme une pastille immobile, donc comme
     * un état de plus au repos (issue #108). Un secteur incomplet n'a aucune symétrie de
     * rotation : c'est la forme qui rend le mouvement visible, pas l'animation.
     *
     * Le tracé vit dans la table et non dans le rendu parce que c'est une **décision de
     * forme**, au même titre que les quatre caractères : `bun test` ne monte pas de DOM,
     * donc une forme posée dans le rendu serait le seul choix de cette table que rien ne
     * pourrait relire. C'est aussi ce qui garde `working` en un seul endroit pour ses trois
     * consommateurs — sidebar, lignes filles, ligne de statut.
     */
    readonly shape: string | null;
    /** Le mot lu par les lecteurs d'écran, et l'infobulle. */
    readonly label: string;
    /** Le seul fond teinté de toute l'interface — `waiting`, et lui seul. */
    readonly tinted: boolean;
    /** Le filet gauche de 2 px : rien, l'accent, ou l'erreur. */
    readonly rail: "none" | "accent" | "error";
    /** Le nom barré : un agent mort ne se lit pas comme un agent vivant. */
    readonly struck: boolean;
    /**
     * Le glyphe tourne — `working`, pour que le mouvement seul le distingue de `done`.
     *
     * Il ne vaut que pour un état **dessiné** : une rotation ne se voit que sur une forme
     * sans symétrie de rotation, et aucun des quatre caractères n'en est une (issue #108).
     * Les deux champs disent donc bien deux choses — l'un la forme, l'autre le mouvement —,
     * mais le second suppose le premier, et c'est un test qui le tient plutôt qu'un
     * commentaire : voir « un état qui bouge est un état dessiné ».
     */
    readonly spinning: boolean;
    /** La classe qui porte la couleur. */
    readonly className: string;
}

/**
 * L'arc de `working` — 120° d'un cercle de rayon 9, dans la boîte 24×24 de Lucide.
 *
 * Trois nombres, et chacun a sa raison :
 *
 * - **120°**, et pas 270 : un anneau presque fermé à 12 px ne se distingue plus du `○`
 *   d'`idle`, et sa rotation ne se voit qu'au déplacement de son trou. Un secteur d'un tiers
 *   déplace toute sa masse, donc il tourne visiblement à un mètre de l'écran — et il ne
 *   ressemble à aucun des quatre autres états, couleur retirée.
 * - **rayon 9**, et pas 10 : le trait est épais (2,75 sur 24), et un rayon de 10 le ferait
 *   mordre le bord de la boîte une fois arrondi.
 * - le tracé **part du haut** (12, 3), donc `prefers-reduced-motion` — qui coupe l'animation
 *   et rien d'autre — rend un arc franchement incliné, jamais une forme ambiguë.
 *
 * La boîte 24×24 est celle des glyphes de la fenêtre de réglages : un tracé d'Ash se lit
 * dans le même repère partout.
 */
const WORKING_ARC = "M12 3a9 9 0 0 1 7.794 13.5";

const PRESENTATIONS: Readonly<Record<AgentState, AgentPresentation>> = {
    working: {
        glyph: "◍",
        shape: WORKING_ARC,
        label: "working",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: true,
        className: "is-working",
    },
    waiting: {
        glyph: "❯",
        shape: null,
        label: "waiting",
        tinted: true,
        rail: "accent",
        struck: false,
        spinning: false,
        className: "is-waiting",
    },
    done: {
        glyph: "✓",
        shape: null,
        label: "done",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: false,
        className: "is-done",
    },
    idle: {
        glyph: "○",
        shape: null,
        label: "idle",
        tinted: false,
        rail: "none",
        struck: false,
        spinning: false,
        className: "is-idle",
    },
    error: {
        glyph: "✕",
        shape: null,
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
 *
 * La boîte est la même pour les cinq — un `<span>` de 12 px, que la rotation fait tourner —
 * et seul son contenu change : un caractère, ou le dessin d'un état qui n'en a pas
 * ([`AgentPresentation.shape`]). C'est ce qui laisse `working` s'aligner sur les quatre
 * autres dans la sidebar comme dans la ligne de statut, sans qu'aucune des deux ne sache
 * qu'il est dessiné.
 *
 * `role="img"` porte le mot : `aria-label` sur un `<span>` nu n'est pas exposé de façon
 * fiable, et un état dessiné n'a plus de texte du tout à lire par défaut.
 */
export function agentGlyph(state: AgentState): HTMLElement {
    const shown = PRESENTATIONS[state];
    const element = document.createElement("span");
    element.className = `ash-glyph ${shown.className}`;
    element.setAttribute("role", "img");
    element.setAttribute("aria-label", shown.label);
    if (shown.shape === null) element.textContent = shown.glyph;
    else element.append(drawing(shown.shape));
    if (shown.spinning) element.classList.add("is-spinning");
    return element;
}

/**
 * Le trait d'un état dessiné — **une seule définition, deux façons de la poser**.
 *
 * La sidebar peint son glyphe en DOM impératif ([`drawing`]), l'aperçu de thème de la
 * fenêtre de réglages le peint comme une valeur de `shared/ui` : deux mécaniques, un seul
 * dessin. Recopiées de part et d'autre, ces cinq lignes divergeraient au premier ajustement
 * — et c'est précisément l'aperçu, dont toute la valeur est de dire la vérité de la colonne,
 * qui montrerait un `working` que la sidebar n'a plus.
 *
 * `stroke-width` est plus épais que celui des glyphes de vérification (1,75) : ceux-là sont
 * lus de près, celui-ci doit se voir au coin de l'œil dans une ligne de 12 px. **Aucune
 * taille en pixels** : le dessin remplit sa boîte, et la boîte est décidée par le CSS.
 */
export const AGENT_GLYPH_STROKE: Readonly<Record<string, string>> = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "2.75",
    "stroke-linecap": "round",
};

/**
 * Le dessin d'un état, dans l'espace de noms qui le rend visible.
 *
 * Un tracé et pas un caractère, pour la raison qui vaut déjà dans la fenêtre de réglages : à
 * 12 px, la forme d'un caractère dépend de la police installée, et il n'existe aucun arc
 * incomplet fiable dans un jeu monospace. `currentColor` garde la couleur là où elle est
 * décidée — `.ash-glyph.is-working` dans `app/styles.css`, donc le thème.
 *
 * `aria-hidden` : le mot est déjà porté par la boîte, et un lecteur d'écran dirait
 * « working » deux fois. C'est la seule chose que ce dessin-ci ajoute au trait commun
 * ([`AGENT_GLYPH_STROKE`]) : la boîte de l'aperçu, elle, est masquée en entier.
 */
function drawing(shape: string): SVGElement {
    const svg = document.createElementNS(SVG_NAMESPACE, "svg");
    for (const [name, value] of Object.entries(AGENT_GLYPH_STROKE)) {
        svg.setAttribute(name, value);
    }
    svg.setAttribute("aria-hidden", "true");
    svg.setAttribute("focusable", "false");

    const path = document.createElementNS(SVG_NAMESPACE, "path");
    path.setAttribute("d", shape);
    svg.append(path);
    return svg;
}
