import { ElementBuilder, SVG_NAMESPACE, type UiBuilder } from "@/shared/ui";

import type { HookState, TestOutcome, VerificationState } from "./contract";

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

/**
 * Le glyphe d'un état — **une description**, comme le reste de l'écran.
 *
 * Il vit ici plutôt que dans la vue parce que quatre décisions y tiennent ensemble : la
 * forme, la classe qui la peint, le mot que lit un lecteur d'écran, et le mouvement qui
 * distingue `verifying` du reste. Écrit à plusieurs endroits, il finit par ne plus dire la
 * même chose des deux côtés.
 */
export function verificationGlyph(state: VerificationState, size = 14): UiBuilder {
    const shown = PRESENTATIONS[state];
    const svg = glyph(SHAPES[state], size, shown.className, shown.label);
    // Le mouvement n'appartient qu'à `verifying`, et c'est ce qui le distingue
    // d'`unverified` sans lire un mot.
    return shown.spinning ? svg.class("is-spinning") : svg;
}

/**
 * La présentation des cinq états de la ligne `hooks` — même discipline, autre question.
 *
 * **Le couple `missing` / `blocked` est le point délicat**, et il est tranché ici. La
 * maquette les dessine en cercle vide contre cercle barré : deux gris que seule une
 * diagonale sépare, à 13 px. Ash exige qu'un état soit distinguable **sans la couleur**
 * (`shared/agent-state`), et une barre d'un pixel sur un cercle de treize n'y suffit pas —
 * elle disparaît au premier écran non-Retina et en vision périphérique.
 *
 * `blocked` est donc un **cadenas** : une silhouette rectangulaire à anse, qui ne partage
 * aucune forme avec le cercle de `missing`. Le sens y gagne aussi — « pas possible » n'est
 * pas « pas encore fait », et un cadenas dit exactement lequel des deux.
 */
export interface HookPresentation {
    /** Le mot lu par un lecteur d'écran, et l'infobulle. */
    readonly label: string;
    /** La classe qui porte la couleur du glyphe. */
    readonly className: string;
    /** La classe qui teinte la bordure de la ligne — la seconde façon de dire l'état. */
    readonly rowClassName: string;
    /**
     * La ligne montre-t-elle le fichier en pastille, à côté de sa phrase ?
     *
     * **Non pour `blocked`, et c'est une décision, pas un oubli** : les trois blocages qui
     * portent un fichier sont des refus qui le nomment déjà dans leur phrase — « ash can't
     * read /home/… ». La pastille l'écrirait une seconde fois sur la même ligne. Les trois
     * autres blocages, eux, n'ont pas de fichier du tout : ils précèdent la lecture.
     *
     * Elle vit dans la table et non dans la vue parce que c'est le backend qui décide de
     * nommer le fichier, et que `bun test` ne monte pas de DOM : posée dans le rendu, la
     * règle serait le seul endroit du chemin où une information venue du backend disparaît
     * sans que rien ne le vérifie.
     */
    readonly showsFile: boolean;
}

const HOOK_PRESENTATIONS: Readonly<Record<HookState, HookPresentation>> = {
    installed: {
        label: "hooks installed",
        className: "is-valid",
        rowClassName: "is-installed",
        showsFile: true,
    },
    missing: {
        label: "hooks missing",
        className: "is-unverified",
        rowClassName: "",
        showsFile: true,
    },
    outdated: {
        label: "hooks from an older version",
        className: "is-caveat",
        rowClassName: "is-outdated",
        showsFile: true,
    },
    conflict: {
        label: "hook block edited by hand",
        className: "is-invalid",
        rowClassName: "is-conflict",
        showsFile: true,
    },
    blocked: {
        label: "hooks unavailable",
        className: "is-blocked",
        rowClassName: "is-blocked",
        showsFile: false,
    },
};

/**
 * Les cinq états, dérivés de la table et non recopiés à côté d'elle — comme
 * [`VERIFICATION_STATES`].
 */
export const HOOK_STATES = Object.keys(HOOK_PRESENTATIONS) as readonly HookState[];

export function presentHooks(state: HookState): HookPresentation {
    return HOOK_PRESENTATIONS[state];
}

/**
 * Les cinq tracés de la ligne `hooks`, dans le vocabulaire de Lucide.
 *
 * `outdated` est une **flèche vers le haut** et non un point d'exclamation : c'est une
 * direction, pas un statut — il y a quelque chose vers quoi aller.
 */
const HOOK_SHAPES: Readonly<Record<HookState, readonly string[]>> = {
    installed: ["M20 6 9 17l-5-5"],
    missing: ["M12 2a10 10 0 1 0 0 20 10 10 0 1 0 0-20"],
    outdated: ["m5 12 7-7 7 7", "M12 19V5"],
    conflict: ["M18 6 6 18", "m6 6 12 12"],
    blocked: [
        "M5 11h14a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2z",
        "M7 11V7a5 5 0 0 1 10 0v4",
    ],
};

/**
 * Les tracés d'un état, **sans DOM**.
 *
 * Ils sont lisibles hors de `hooksGlyph` parce que le choix des formes est une décision et
 * non un détail de rendu : c'est elle qui garantit que deux états gris ne se confondent
 * pas. Depuis que le glyphe est une description, un test pourrait aussi bien lire ses
 * tracés dans son arbre ; cette porte reste la façon la plus directe de comparer deux
 * états entre eux, ce qui est précisément ce que la règle demande.
 */
export function hookShapes(state: HookState): readonly string[] {
    return HOOK_SHAPES[state];
}

/** Le glyphe d'un état de hooks — une description, elle aussi. */
export function hooksGlyph(state: HookState, size = 13): UiBuilder {
    const shown = HOOK_PRESENTATIONS[state];
    return glyph(HOOK_SHAPES[state], size, shown.className, shown.label);
}

/**
 * Un `<svg>` et ses tracés, dans l'espace de noms qui les rend visibles.
 *
 * C'est le seul endroit de la fenêtre qui sorte du HTML : à 13 px, la forme d'un état ne
 * peut pas dépendre de la police installée, et c'est justement la forme qui porte l'état.
 */
class Glyph extends ElementBuilder {
    constructor(shapes: readonly string[], size: number, className: string, label: string) {
        super("svg", "settings-verify-glyph", className);
        this.inNamespace(SVG_NAMESPACE)
            .attr("viewBox", "0 0 24 24")
            .attr("width", String(size))
            .attr("height", String(size))
            .attr("fill", "none")
            .attr("stroke", "currentColor")
            .attr("stroke-width", "1.75")
            .attr("stroke-linecap", "round")
            .attr("stroke-linejoin", "round")
            // Une image qui se nomme : sans le mot, un lecteur d'écran ne lit qu'un dessin.
            .attr("role", "img")
            .attr("aria-label", label)
            .add(...shapes.map((shape) => new Path(shape)));
    }
}

class Path extends ElementBuilder {
    constructor(shape: string) {
        super("path");
        this.inNamespace(SVG_NAMESPACE).attr("d", shape);
    }
}

function glyph(
    shapes: readonly string[],
    size: number,
    className: string,
    label: string,
): Glyph {
    return new Glyph(shapes, size, className, label);
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
