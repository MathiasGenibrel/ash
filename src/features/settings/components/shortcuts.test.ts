import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild } from "@/shared/ui";

import { aShortcut } from "../builders";
import { shortcutsSection } from "./shortcuts";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

describe("la section shortcuts de la fenêtre de réglages", () => {
    it("Given the shortcuts the menu declares, when the section is composed, then each one is shown with the combination the backend sent", () => {
        // Given — les accélérateurs sont déclarés en Rust (`src-tauri/src/menu.rs`), et la
        // section les **lit**. Une table écrite ici aurait fini par annoncer un raccourci que
        // le menu ne déclare plus, et c'est cet écran qu'on croit
        const declared = [
            aShortcut({ label: "New Tab", keys: "⌘T" }),
            aShortcut({ label: "Tab 1 … Tab 9", keys: "⌘1 … ⌘9" }),
        ];

        // When
        const composed = shortcutsSection(declared);
        const rows = composed.flatMap((child) => findAll(child, "settings-shortcut"));

        // Then
        expect(rows.map(plainText)).toEqual(["New Tab⌘T", "Tab 1 … Tab 9⌘1 … ⌘9"]);
    });

    it("Given shortcuts from two submenus, when the section is composed, then each group is titled once, in the order the menu sends them", () => {
        // Given — « groupés » est un critère de l'issue #110, et l'ordre est celui du menu :
        // on retrouve un raccourci dans l'écran là où on l'a vu dans le menu
        const declared = [
            aShortcut({ group: "terminal", label: "New Tab" }),
            aShortcut({ group: "view", label: "Toggle Sidebar", keys: "⌘B" }),
            aShortcut({ group: "terminal", label: "Close Tab", keys: "⌘W" }),
        ];

        // When
        const composed = shortcutsSection(declared);
        const groups = composed
            .flatMap((child) => findAll(child, "settings-shortcut-group"))
            .map(plainText);

        // Then — deux titres, pas trois : un groupe qui revient est replié dans le premier
        expect(groups).toEqual(["terminal", "view"]);
    });

    it("Given the list is read-only, when the section is composed, then it says so and offers nothing to click", () => {
        // Given — Ash ne sait pas encore rebinder (issue #22). Un contrôle posé là en
        // attendant promettrait un geste qui ne mène nulle part
        const composed = shortcutsSection([aShortcut()]);

        // Then
        expect(composed.flatMap((child) => findAll(child, "ui-button"))).toEqual([]);
        expect(composed.flatMap((child) => findAll(child, "settings-input"))).toEqual([]);
        expect(said(composed)).toContain("read-only");
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it is waiting for rather than an empty list", () => {
        // Given — la navigation traverse la section (`⌥↓`) : un panneau muet se lirait comme
        // une panne, et « aucun raccourci » serait un mensonge
        const composed = shortcutsSection(null);

        // Then
        expect(said(composed)).toContain("reading them from the menu…");
    });
});
