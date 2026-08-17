import { describe, expect, it } from "bun:test";

import { parseAppName } from "./app-name";

describe("le nom que le backend rend", () => {
    it("Given a backend that answers something that is not a name, when it is parsed, then nothing is taken for one", () => {
        // Given — ce qu'une frontière JSON peut rendre quand les deux côtés ne sont pas du
        // même build : la chaîne vide compte, elle écrirait une bande commençant par un
        // tiret et un `settings — ` en suspens
        const answers: unknown[] = [null, undefined, "", 0, { name: "Ash" }];

        // When
        const parsed = answers.map(parseAppName);

        // Then — aucune ne passe, et l'appelant se repliera sur un nom lisible
        expect(parsed).toEqual([null, null, null, null, null]);
    });

    it("Given the name of a development build, when it is parsed, then it comes through untouched", () => {
        // Given / When
        const parsed = parseAppName("Ash-dev");

        // Then — pas de mise en minuscules, pas de troncature : la bande écrit ce que
        // `APP_NAME` vaut
        expect(parsed).toBe("Ash-dev");
    });
});
