import { describe, expect, it } from "bun:test";

import { moveSection, SETTINGS_SECTIONS, sectionStep, type SettingsSection } from "./sections";

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
        const last = SETTINGS_SECTIONS[SETTINGS_SECTIONS.length - 1] ?? "usage";

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

    it("Given the four sections the mockup drew, when usage joins them, then none of them moved", () => {
        // Given — la section `usage` est arrivée après coup (ADR-0016). Elle est en dernier
        // pour une raison qui se perdrait sans ce test : réarranger la liste pour loger un
        // nouveau venu déplacerait quatre positions déjà apprises, et `⌥↓` ne mènerait plus
        // où l'habitude le dit
        const drawnByTheMockup: SettingsSection[] = [
            "tools",
            "shortcuts",
            "appearance",
            "notifications",
        ];

        // When
        const order = [...SETTINGS_SECTIONS];

        // Then
        expect(order).toEqual([...drawnByTheMockup, "usage"]);
    });
});
