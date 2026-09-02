import { describe, expect, it } from "bun:test";

import {
    AGENT_ROW_CHANNELS,
    AGENT_STATES,
    agentRowClasses,
    decorateAgentRow,
    presentAgentState,
} from "./index";

describe("les cinq états d'un agent", () => {
    it("Given the five agent states, when they are presented, then each one gets its own shape", () => {
        // Given — le design le formule comme un test : la forme porte l'état à elle seule,
        // sans la couleur. C'est ce qui tient sous daltonisme, écran mat, ou du coin de l'œil.
        const states = AGENT_STATES;

        // When — la forme *rendue* : un tracé quand l'état en a un, son caractère sinon.
        // Comparer les seuls caractères laisserait deux états se rejoindre à l'écran tout en
        // gardant deux glyphes de repli distincts dans la table.
        const shapes = states.map((state) => {
            const shown = presentAgentState(state);
            return shown.shape ?? shown.glyph;
        });

        // Then
        expect(new Set(shapes).size).toBe(states.length);
    });

    it("Given the five agent states, when they are presented, then waiting is the only one with a tinted background", () => {
        // Given — c'est ce qui fait passer le « test du flou » : à 1,6 px, seule la ligne
        // waiting reste identifiable. Un second fond coloré le ferait perdre.
        const tinted = AGENT_STATES.filter((state) => presentAgentState(state).tinted);

        // When / Then
        expect(tinted).toEqual(["waiting"]);
    });

    it("Given an agent that failed, when it is presented, then its name is struck through and its state badge is bordered with the error colour", () => {
        // Given / When
        const error = presentAgentState("error");

        // Then — un agent mort ne doit pas se lire comme un agent vivant, et il lui reste
        // deux canaux non chromatiques (le glyphe, le nom barré) le jour où la couleur
        // manque : la bordure de l'étiquette ajoute, elle ne porte pas seule
        expect(error.struck).toBe(true);
        expect(error.badge).toBe("error");
        expect(error.tinted).toBe(false);
    });

    it("Given an agent that is working, when it is presented, then only its glyph moves", () => {
        // Given / When
        const moving = AGENT_STATES.filter((state) => presentAgentState(state).spinning);

        // Then — le mouvement seul suffit à distinguer `working` de `done`, et il ne doit
        // pas devenir un bruit de fond
        expect(moving).toEqual(["working"]);
    });

    it("Given the state that moves, when its shape is read, then it is an open arc that never closes back on itself", () => {
        // Given — c'est la forme qui rend la rotation visible, pas l'animation : `◍` était un
        // disque presque invariant par rotation, donc `working` se lisait comme une pastille
        // immobile malgré son `animation: ash-spin` (issue #108). Un état dessiné est
        // exactement celui qui tourne, et son tracé doit rester un secteur incomplet.
        const drawn = AGENT_STATES.filter((state) => presentAgentState(state).shape !== null);

        // When
        const shape = presentAgentState("working").shape ?? "";

        // Then — un anneau fermé revient à son point de départ ; un arc n'y revient pas
        expect(drawn).toEqual(["working"]);
        expect(shape).toMatch(ONLY_MOVES_AND_ARCS);
        expect(travel(shape)).toBeGreaterThan(1);
    });

    it("Given the five states, when a state moves, then it is a drawn state — a spun character says nothing", () => {
        // Given — la règle que les deux champs `shape` et `spinning` doivent respecter
        // ensemble, et que `styles.css` n'écrit qu'en prose (« ne remets jamais une forme
        // ronde et pleine derrière cette classe »). Elle est énoncée sur les cinq états, et
        // non sur `working` nommément : un sixième état y passerait aussi.
        const states = AGENT_STATES.map((state) => presentAgentState(state));

        // When — les états qui bougent sans porter de tracé : un caractère mis en rotation,
        // dont la police décide s'il se voit tourner. C'est exactement la panne d'#108.
        const spunCharacters = states.filter((shown) => shown.spinning && shown.shape === null);

        // Then
        expect(spunCharacters).toEqual([]);
    });
});

/**
 * Un tracé qui ne fait que partir d'un point et suivre des arcs.
 *
 * Ce qu'elle exclut est ce qui rendrait la rotation muette : un `Z` qui referme, une ligne
 * droite, une courbe de Bézier dont on ne saurait plus dire si la forme est un secteur.
 */
const ONLY_MOVES_AND_ARCS =
    /^M[\d.-]+ [\d.-]+(a[\d.]+ [\d.]+ [\d.]+ [01] [01] -?[\d.]+ -?[\d.]+)+$/;

/**
 * La distance entre le début et la fin d'un tracé d'arcs, en unités de sa boîte.
 *
 * Elle se calcule en sommant les déplacements **relatifs** des arcs, ce que la forme
 * `a rx ry rotation grand-arc sens dx dy` donne directement. Un cercle complet — deux
 * demi-arcs — revient à zéro, un secteur non.
 */
function travel(shape: string): number {
    const arcs = [...shape.matchAll(/a[\d.]+ [\d.]+ [\d.]+ [01] [01] (-?[\d.]+) (-?[\d.]+)/g)];
    const dx = arcs.reduce((sum, arc) => sum + Number(arc[1]), 0);
    const dy = arcs.reduce((sum, arc) => sum + Number(arc[2]), 0);
    return Math.hypot(dx, dy);
}

describe("la ligne d'un agent : l'état d'un côté, la sélection de l'autre", () => {
    it("Given each of the five states, when a row is selected, then it differs from the same row unselected by a channel that survives without colour", () => {
        // Given — cinq agents `waiting` dans la colonne, et rien qui dise lequel est sous les
        // doigts : c'est le bug d'#181, et il ne se voyait pas parce que la différence était
        // nulle, pas parce qu'elle était pâle. Ce que le test exige est donc plus fort qu'un
        // écart : un écart qu'une capture en niveaux de gris montre encore
        const colourless = AGENT_ROW_CHANNELS.filter((channel) => !channel.chromatic);

        for (const state of AGENT_STATES) {
            // When
            const selected = decorateAgentRow(state, true);
            const plain = decorateAgentRow(state, false);

            // Then
            const told = colourless.filter(({ channel }) => selected[channel] !== plain[channel]);
            expect(told.length).toBeGreaterThan(0);
        }
    });

    it("Given the channels of a row, when both the state and the selection change, then no channel answers to the two of them", () => {
        // Given — l'invariant qui aurait attrapé le bug : `.ash-agent.is-selected` posait le
        // fond et le filet gauche, `.ash-agent.is-tinted` — déclarée après, à spécificité
        // égale — reposait les deux, et l'état gagnait parce qu'il était écrit en second. Un
        // canal à deux propriétaires rend muette celle des deux informations qui perd
        for (const { channel } of AGENT_ROW_CHANNELS) {
            // When — le canal bouge-t-il avec la sélection, à état fixé ? avec l'état, à
            // sélection fixée ?
            const movesWithSelection = AGENT_STATES.some(
                (state) =>
                    decorateAgentRow(state, true)[channel] !==
                    decorateAgentRow(state, false)[channel],
            );
            const movesWithState = [true, false].some((selected) =>
                differs(AGENT_STATES.map((state) => decorateAgentRow(state, selected)[channel])),
            );

            // Then — un propriétaire, ou aucun ; jamais deux
            expect({ channel, shared: movesWithSelection && movesWithState }).toEqual({
                channel,
                shared: false,
            });
        }
    });

    it("Given each of the five states, when the classes of a row are composed, then the selected row carries a class the unselected one does not", () => {
        // Given — les deux invariants ci-dessus se tiennent sur la décoration, et une
        // décoration juste peut encore n'être rendue par personne : c'est la traduction en
        // classes qui atteint l'écran. Une ligne `waiting` sélectionnée doit donc porter à la
        // fois la teinte de son état et le filet de la sélection — le contraire, c'est #181
        for (const state of AGENT_STATES) {
            // When
            const selected = agentRowClasses(state, true);
            const plain = agentRowClasses(state, false);

            // Then — tout ce que l'état posait est encore là, et la sélection s'y ajoute
            expect(selected).toEqual([...plain, "is-selected"]);
        }
    });
});

/** Une suite de valeurs qui n'est pas constante — deux valeurs suffisent à la distinguer. */
function differs(values: readonly unknown[]): boolean {
    return new Set(values).size > 1;
}
