import { describe, expect, it } from "bun:test";

import { banner, emptyState } from "./feedback";
import { button } from "./button";
import { find, plainText } from "./read";

describe("ce qu'une vue dit à la place des données", () => {
    it("Given a banner reporting something that just happened, when it is described, then it announces itself without interrupting", () => {
        // Given — `status` et pas `alert` : une bannière rend compte, elle ne coupe pas la
        // parole à un lecteur d'écran à chaque rendu
        const described = banner("the path was reset", "warning").build();

        // Then
        expect(described.attrs["role"]).toBe("status");
        expect(described.classes).toEqual(["ui-banner", "is-warning"]);
    });

    it("Given a banner with something to undo, when its action is described, then the action lives inside the banner", () => {
        // Given — la bannière de retour en arrière du dépôt porte son `undo`, elle ne le
        // laisse pas à trois lignes de là
        const described = banner("the path was reset").action(button("undo the reset")).build();

        // When
        const action = find(described, "ui-button");

        // Then
        expect(action).not.toBeNull();
        expect(plainText(described)).toBe("the path was resetundo the reset");
    });

    it("Given an empty state, when it is described, then it says what the emptiness costs and not only that it is empty", () => {
        // Given — le titre seul serait un cul-de-sac : l'état vide du dépôt explique ce qui
        // reste inerte tant que rien n'est déclaré
        const described = emptyState("no tools declared")
            .prose("until a tool is declared, everything stays idle — no waiting, no notifications.")
            .build();

        // Then
        expect(find(described, "ui-empty-title")).not.toBeNull();
        expect(plainText(find(described, "ui-empty-prose") ?? described)).toContain("stays idle");
    });
});
