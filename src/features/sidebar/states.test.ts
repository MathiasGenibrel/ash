import { describe, expect, it } from "bun:test";

import { AGENT_STATES, bubbleState, presentAgentState } from "./states";

describe("les cinq états d'une ligne d'agent", () => {
    it("Given the five agent states, when they are presented, then each one gets its own shape", () => {
        // Given — le design le formule comme un test : la forme porte l'état à elle seule,
        // sans la couleur. C'est ce qui tient sous daltonisme, écran mat, ou du coin de l'œil.
        const states = AGENT_STATES;

        // When
        const glyphs = states.map((state) => presentAgentState(state).glyph);

        // Then
        expect(new Set(glyphs).size).toBe(states.length);
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
});

describe("la remontée d'état vers la ligne du dessus", () => {
    it("Given only idle shells, when the row bubbles its state, then it stays idle", () => {
        // Given / When
        const bubbled = bubbleState(["idle", "idle"]);

        // Then
        expect(bubbled).toBe("idle");
    });

    it("Given an error next to a done agent, when the row bubbles its state, then the error wins", () => {
        // Given / When
        const bubbled = bubbleState(["done", "error", "idle"]);

        // Then — une ligne repliée qui montrerait `done` cacherait exactement ce qu'il
        // faut regarder
        expect(bubbled).toBe("error");
    });

    it("Given an error next to a waiting agent, when the row bubbles its state, then waiting wins", () => {
        // Given / When
        const bubbled = bubbleState(["error", "waiting", "working"]);

        // Then — une erreur attendra ; une question bloque un agent
        expect(bubbled).toBe("waiting");
    });
});
