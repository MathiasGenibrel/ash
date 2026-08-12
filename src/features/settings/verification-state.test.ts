import { describe, expect, it } from "bun:test";

import {
    presentVerification,
    testTileClass,
    testTileLabel,
    VERIFICATION_STATES,
} from "./verification-state";

describe("la présentation des cinq états de vérification", () => {
    it("Given the five verification states, when they are presented, then each one has a shape and a word of its own", () => {
        // Given — la discipline de `shared/agent-state` : la forme porte l'état à elle
        // seule, et la couleur ne fait que la doubler. Deux états qui partagent une classe
        // partagent une couleur, donc se confondent dès qu'on ne lit plus la phrase
        const shown = VERIFICATION_STATES.map((state) => presentVerification(state));

        // When
        const words = new Set(shown.map((one) => one.label));
        const classes = new Set(shown.map((one) => one.className));

        // Then
        expect(VERIFICATION_STATES.length).toBe(5);
        expect(words.size).toBe(5);
        expect(classes.size).toBe(5);
    });

    it("Given the five states, when the one that is happening now is looked for, then only verifying moves", () => {
        // Given — le mouvement est ce qui distingue `verifying` d'`unverified` sans lire un
        // mot. Un second état animé lui prendrait cette distinction
        // When
        const moving = VERIFICATION_STATES.filter((state) => presentVerification(state).spinning);

        // Then
        expect(moving).toEqual(["verifying"]);
    });

    it("Given the two states that leave the folder undecided, when a card is drawn, then its border stays neutral", () => {
        // Given — la bordure teintée est la seconde façon de dire l'état, sans texte. La
        // teindre pour `unverified` dirait qu'il s'est passé quelque chose
        // When
        const neutral = VERIFICATION_STATES.filter(
            (state) => presentVerification(state).cardClassName === "",
        );

        // Then
        expect(neutral).toEqual(["unverified", "verifying"]);
    });
});

describe("les pastilles de la rangée de tests", () => {
    it("Given a test that will never run because the chain stopped, when its tile is drawn, then it is not the same as one still waiting", () => {
        // Given — les deux disent « pas lancé », et ne se peignent pas pareil : le premier
        // attend, le second ne viendra jamais. Les confondre ferait attendre une réponse
        // When
        const waiting = testTileClass("pending");
        const abandoned = testTileClass("skipped");

        // Then
        expect(waiting).not.toBe(abandoned);
    });

    it("Given a tile, when a screen reader reads it, then it hears the test and its result rather than a number", () => {
        // Given — un `3` seul ne dit rien : ni de quel test il s'agit, ni ce qu'il a donné
        const test = { number: 3, label: "the command exists in PATH" };

        // When
        const said = testTileLabel("warned", test);

        // Then
        expect(said).toBe("test 3, the command exists in PATH: passed with a caveat");
    });
});
