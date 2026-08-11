import { describe, expect, it } from "bun:test";

import { moveSection, SETTINGS_SECTIONS, sectionStep } from "./sections";

describe("la navigation entre sections", () => {
    it("Given a bare arrow key, when the window reads it, then it is not a section move", () => {
        // Given — les flèches nues appartiennent à ce qui a le focus : un champ de chemin
        // resterait impossible à parcourir si la fenêtre les prenait
        const bare = { key: "ArrowDown", altKey: false };

        // When
        const step = sectionStep(bare);

        // Then
        expect(step).toBeNull();
    });

    it("Given an option-arrow, when the window reads it, then it moves one section that way", () => {
        // Given
        const down = { key: "ArrowDown", altKey: true };
        const up = { key: "ArrowUp", altKey: true };

        // When / Then
        expect(sectionStep(down)).toBe(1);
        expect(sectionStep(up)).toBe(-1);
    });

    it("Given the first section, when the move goes up, then it stays where it is", () => {
        // Given — la liste tient à l'écran en entier : repartir à la dernière se lirait
        // comme un saut, pas comme une navigation
        const first = SETTINGS_SECTIONS[0] ?? "tools";

        // When
        const moved = moveSection(first, -1);

        // Then
        expect(moved).toBe(first);
    });

    it("Given the last section, when the move goes down, then it stays where it is", () => {
        // Given
        const last = SETTINGS_SECTIONS[SETTINGS_SECTIONS.length - 1] ?? "notifications";

        // When
        const moved = moveSection(last, 1);

        // Then
        expect(moved).toBe(last);
    });

    it("Given the tools section, when the move goes down twice, then it lands on appearance", () => {
        // Given — l'ordre de la maquette est une exigence, pas un goût : `tools`,
        // `shortcuts`, `appearance`, `notifications`
        // When
        const landed = moveSection(moveSection("tools", 1), 1);

        // Then
        expect(landed).toBe("appearance");
    });
});
