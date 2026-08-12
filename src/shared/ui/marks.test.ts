import { describe, expect, it } from "bun:test";

import { badge, glyph } from "./marks";
import { plainText } from "./read";

describe("les marques d'une ligne", () => {
    it("Given a glyph, when it is described, then it always carries the word it stands for", () => {
        // Given — un signe sans nom est muet pour un lecteur d'écran, et illisible pour qui
        // ne connaît pas la convention. Le mot n'est pas optionnel : il est dans la
        // signature, comme la raison d'un bouton éteint.
        const described = glyph("❯", "waiting").build();

        // Then
        expect(plainText(described)).toBe("❯");
        expect(described.attrs["aria-label"]).toBe("waiting");
    });

    it("Given a badge with a tooltip, when it is described, then the short word and its explanation travel together", () => {
        // Given — la pastille montre un mot court ; l'infobulle dit ce qu'il abrège
        const described = badge("generic").title("mode dégradé : trois états sur cinq").build();

        // Then
        expect(plainText(described)).toBe("generic");
        expect(described.attrs["title"]).toBe("mode dégradé : trois états sur cinq");
    });
});
