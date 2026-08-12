import { describe, expect, it } from "bun:test";

import { find, findAll, plainText } from "@/shared/ui";

import { FOUR_TESTS } from "../builders";
import { foot, noToolsYet, scaleNote, sectionHeader } from "./chrome";

describe("l'en-tête d'une section", () => {
    it("Given a section with a count, when its header is described, then the actions are pushed past it rather than beside the title", () => {
        // Given — la maquette pousse `re-verify all` et `add` à droite ; sans l'espaceur ils
        // se colleraient au compteur et la barre perdrait sa lecture
        const described = sectionHeader("tools", "3 declared · 1 invalid", []).build();

        // When
        const order = described.children.map((child) =>
            child.kind === "element" ? child.classes[0] : "text",
        );

        // Then
        expect(plainText(find(described, "settings-count") ?? described)).toBe(
            "3 declared · 1 invalid",
        );
        expect(order.at(-1)).toBe("settings-spacer");
    });

    it("Given a section that counts nothing, when its header is described, then no empty counter is left behind", () => {
        // Given — les trois sections sans contenu n'ont rien à compter
        const described = sectionHeader("appearance", null, []).build();

        // Then
        expect(findAll(described, "settings-count")).toEqual([]);
        expect(plainText(described)).toBe("appearance");
    });
});

describe("la note de barème", () => {
    it("Given the four tests the backend names, when the note is described, then it uses their labels and never a copy of them", () => {
        // Given — les tests existent en Rust, donc ils s'y nomment. Une liste recopiée dans
        // l'écran finirait par décrire un test que la séquence ne lance plus
        const said = plainText(scaleNote(FOUR_TESTS));

        // Then
        expect(said).toContain("tests · 1 folder · 2 readable · 3 in PATH · 4 answers");
    });
});

describe("l'état vide de la liste", () => {
    it("Given no tool declared, when the empty state is described, then it says what the emptiness costs and not only that it is empty", () => {
        // Given — le titre seul serait un cul-de-sac : l'écran dit ce qu'on n'a pas encore
        const said = plainText(noToolsYet());

        // Then
        expect(said).toContain("no tools declared");
        expect(said).toContain("everything stays idle — no waiting, no notifications");
    });
});

describe("le pied de la section", () => {
    it("Given a list of entries, when the foot is described, then it repeats that ash writes to no file on its own", () => {
        // Given — c'est la promesse que toute la fenêtre tient (ADR-0007)
        const said = plainText(foot("ash writes to no file until an entry is verified."));

        // Then
        expect(said).toBe("ash writes to no file until an entry is verified.");
    });
});
