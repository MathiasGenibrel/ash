import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, text } from "@/shared/ui";

import { aTool, aVerification } from "../builders";
import { countProblems } from "../model";
import { navColumn } from "./nav";

describe("la colonne des sections", () => {
    it("Given a list with one invalid entry, when the column is described, then its count is the very one the header announces", () => {
        // Given — la maquette `3e` met `3 declared · 1 invalid` en tête **et** `1` sur la
        // ligne `tools`. Recompté dans la vue (#15), l'un des deux chiffres finirait par
        // dire autre chose que l'autre, et celui de la colonne n'était pas sous test
        const tools = [
            aTool({ command: "claude" }),
            aTool({ command: "codex", verification: aVerification("invalid") }),
            aTool({ command: "aider", verification: aVerification("caveat") }),
        ];

        // When
        const described = navColumn("tools", tools, () => undefined);
        const count = described
            .map((row) => find(row, "settings-nav-count"))
            .find((found) => found !== null);

        // Then
        expect(plainText(count ?? text(""))).toBe(String(countProblems(tools)));
        expect(plainText(count ?? text(""))).toBe("1");
    });

    it("Given a list where nothing is invalid, when the column is described, then no zero is shown", () => {
        // Given — un `0` permanent apprendrait à ne plus regarder cet endroit
        const described = navColumn("tools", [aTool()], () => undefined);

        // When
        const counts = described.flatMap((row) => findAll(row, "settings-nav-count"));

        // Then
        expect(counts).toEqual([]);
    });

    it("Given the open section, when the column is described, then it is the one a screen reader hears as current", () => {
        // Given — la classe teinte la ligne, `aria-current` la dit. Une seule des deux
        // laisserait un lecteur d'écran sans repère
        const described = navColumn("appearance", [], () => undefined);

        // When
        const current = described
            .map((row) => find(row, "settings-nav-row"))
            .filter((row) => row?.attrs["aria-current"] === "true");

        // Then
        expect(current).toHaveLength(1);
        expect(plainText(current[0] ?? text(""))).toBe("appearance");
        expect(current[0]?.classes).toContain("is-active");
    });

    it("Given a section row, when it is pressed, then the column asks for that section and decides nothing itself", () => {
        // Given — la colonne rend, elle ne détient pas la section ouverte
        const asked: string[] = [];

        // When
        const described = navColumn("tools", [], (section) => asked.push(section));
        find(described[2] ?? text(""), "settings-nav-row")?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(asked).toEqual(["appearance"]);
    });
});
