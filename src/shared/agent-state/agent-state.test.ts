import { describe, expect, it } from "bun:test";

import { AGENT_STATES, presentAgentState } from "./index";

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

    it("Given an agent that failed, when it is presented, then its name is struck through and its row takes the error rail", () => {
        // Given / When
        const error = presentAgentState("error");

        // Then — un agent mort ne doit pas se lire comme un agent vivant
        expect(error.struck).toBe(true);
        expect(error.rail).toBe("error");
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
const ONLY_MOVES_AND_ARCS = /^M[\d.-]+ [\d.-]+(a[\d.]+ [\d.]+ [\d.]+ [01] [01] -?[\d.]+ -?[\d.]+)+$/;

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
