import { describe, expect, it } from "bun:test";

import { bubbleState } from "./states";

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
