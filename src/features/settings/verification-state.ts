import type { TestOutcome, VerificationState } from "./contract";

/**
 * La présentation des cinq états de vérification — forme, mot, teinte.
 *
 * Elle suit la discipline de `shared/agent-state/index.ts`, et pour les mêmes raisons :
 *
 * 1. **La forme porte l'état à elle seule.** Cercle vide, anneau tournant, coche, triangle,
 *    croix : les cinq restent distinguables en niveaux de gris, en vision périphérique, et
 *    pour un œil qui ne sépare pas le rouge du vert. La couleur double l'information, elle
 *    ne la porte jamais.
 * 2. **Le mouvement n'appartient qu'à `verifying`.** C'est le seul état qui dit « ça se
 *    passe maintenant », et c'est ce qui le distingue d'`unverified` sans lire un mot.
 * 3. **La couleur est du thème.** Les classes posées ici sont peintes par
 *    `app/styles.css`, à côté des deux palettes, comme celles des états d'agent.
 *
 * Elle vit dans la feature et non dans `shared/` : `shared/` demande deux lecteurs, et la
 * fenêtre de réglages est aujourd'hui le seul. Le jour où la sidebar dira qu'un outil
 * déclaré n'est pas vérifié, elle en aura un second — c'est ce jour-là que le module
 * déménagera.
 *
 * **Aucun état n'est produit ici.** Les cinq viennent du backend, qui seul a lu le dossier
 * et lancé la commande ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface VerificationPresentation {
    /** Le mot lu par un lecteur d'écran, et l'infobulle. */
    readonly label: string;
    /** La classe qui porte la couleur du glyphe. */
    readonly className: string;
    /** La classe qui teinte la bordure de la carte — la seconde façon de dire l'état. */
    readonly cardClassName: string;
    /** Le glyphe tourne — `verifying`, et lui seul. */
    readonly spinning: boolean;
    /** Ce que fait le bouton de la ligne `test`. */
    readonly action: "verify" | "cancel" | "re-verify";
}

const PRESENTATIONS: Readonly<Record<VerificationState, VerificationPresentation>> = {
    unverified: {
        label: "unverified",
        className: "is-unverified",
        cardClassName: "",
        spinning: false,
        action: "verify",
    },
    verifying: {
        label: "verifying",
        className: "is-verifying",
        cardClassName: "",
        spinning: true,
        action: "cancel",
    },
    valid: {
        label: "valid",
        className: "is-valid",
        cardClassName: "is-valid",
        spinning: false,
        action: "re-verify",
    },
    caveat: {
        label: "valid with a caveat",
        className: "is-caveat",
        cardClassName: "is-caveat",
        spinning: false,
        action: "re-verify",
    },
    invalid: {
        label: "invalid",
        className: "is-invalid",
        cardClassName: "is-invalid",
        spinning: false,
        action: "re-verify",
    },
};

/**
 * Les cinq états, dérivés de la table et non recopiés à côté d'elle.
 *
 * `PRESENTATIONS` est un `Record<VerificationState, …>` : le compilateur en garantit
 * l'exhaustivité, alors qu'une seconde liste écrite à la main oublierait un jour un état,
 * et les tests qui parcourent « les cinq » passeraient en en regardant quatre.
 */
export const VERIFICATION_STATES = Object.keys(PRESENTATIONS) as readonly VerificationState[];

export function presentVerification(state: VerificationState): VerificationPresentation {
    return PRESENTATIONS[state];
}

/**
 * Les cinq tracés, dans le vocabulaire de Lucide.
 *
 * Un `<svg>` et pas un caractère : à cette taille, un glyphe typographique dépend de la
 * police installée, et la forme est précisément ce qui doit rester identique partout.
 */
const SHAPES: Readonly<Record<VerificationState, readonly string[]>> = {
    // Un cercle fermé et vide : rien n'a été tenté.
    unverified: ["M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20"],
    // Un arc, donc un cercle **incomplet** : la forme dit déjà « en cours », et la rotation
    // le confirme pour ceux qui la voient.
    verifying: ["M12 2a10 10 0 0 1 8.66 5"],
    valid: ["M20 6 9 17l-5-5"],
    caveat: [
        "m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3",
        "M12 9v4",
        "M12 17h.01",
    ],
    invalid: ["M18 6 6 18", "m6 6 12 12"],
};

const SVG = "http://www.w3.org/2000/svg";

/**
 * Le glyphe d'un état, prêt à poser dans le DOM.
 *
 * Il vit ici plutôt que dans la vue parce que quatre décisions y tiennent ensemble : la
 * forme, la classe qui la peint, le mot que lit un lecteur d'écran, et le mouvement qui
 * distingue `verifying` du reste. Écrit à plusieurs endroits, il finit par ne plus dire la
 * même chose des deux côtés — et aucun test ne rattraperait la divergence, le dépôt ne
 * montant pas de DOM dans `bun test`.
 */
export function verificationGlyph(state: VerificationState, size = 14): SVGElement {
    const shown = PRESENTATIONS[state];
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.75");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("class", `settings-verify-glyph ${shown.className}`);
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", shown.label);
    for (const shape of SHAPES[state]) {
        const path = document.createElementNS(SVG, "path");
        path.setAttribute("d", shape);
        svg.append(path);
    }
    if (shown.spinning) svg.classList.add("is-spinning");
    return svg;
}

/**
 * Le glyphe de la ligne `hooks` **quand elle est bloquée**, et rien d'autre.
 *
 * Les cinq états de cette ligne appartiennent à l'issue #16 ; celui-ci est là parce que la
 * planche `3e` — l'entrée invalide en contexte, qui est de cette issue-ci — le montre :
 * *« le bouton installer reste à sa place, éteint, avec sa raison à gauche. le masquer
 * ferait croire que les hooks n'existent pas pour cet outil. »*
 *
 * **Cercle barré**, et pas cercle vide : le second dira `missing` chez #16, c'est-à-dire
 * « rien n'est encore installé, et vous pouvez le faire ». C'est la barre diagonale, et
 * elle seule, qui distingue « pas fait » de « pas possible » — les deux sont gris, et
 * remplacer l'un par une nuance de l'autre effacerait la différence.
 */
export function blockedHooksGlyph(size = 13): SVGElement {
    const svg = document.createElementNS(SVG, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("width", String(size));
    svg.setAttribute("height", String(size));
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.75");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("class", "settings-verify-glyph is-blocked");
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", "hooks unavailable");
    for (const shape of ["M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20", "m4.9 4.9 14.2 14.2"]) {
        const path = document.createElementNS(SVG, "path");
        path.setAttribute("d", shape);
        svg.append(path);
    }
    return svg;
}

/**
 * La classe d'une pastille de la rangée.
 *
 * `pending` et `skipped` disent tous deux « pas lancé », et ne se peignent pourtant pas
 * pareil : le premier attend, le second ne viendra jamais parce que la chaîne s'est
 * arrêtée avant lui. Les confondre ferait croire qu'un test va encore répondre.
 */
export function testTileClass(outcome: TestOutcome): string {
    return `settings-tile is-${outcome}`;
}

/** Ce qu'un lecteur d'écran entend d'une pastille — le chiffre seul ne dit rien. */
export function testTileLabel(outcome: TestOutcome, test: { number: number; label: string }): string {
    const said: Record<TestOutcome, string> = {
        pending: "not run yet",
        running: "running",
        passed: "passed",
        warned: "passed with a caveat",
        failed: "failed",
        skipped: "not run — an earlier test stopped the sequence",
    };
    return `test ${test.number}, ${test.label}: ${said[outcome]}`;
}
