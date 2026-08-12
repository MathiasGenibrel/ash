import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import { diffView } from "./diff-view";

describe("le diff d'un conflit", () => {
    it("Given the diff the backend sends, when it is displayed, then the legend reads it in the backend's direction", () => {
        // Given — `−` est ce qu'Ash écrirait, `+` ce que le fichier porte. La maquette
        // légende l'inverse ; suivre sa légende ferait lire chaque ligne à l'envers, la
        // seule faute qu'un diff ne pardonne pas
        const diff = "--- ash\n+++ file\n-  \"ash-event done\"\n+  \"ash-event done --quiet\"\n   \"hooks\": {";

        // When
        const legends = findAll(diffView(diff).build(), "settings-diff-legend").map(plainText);

        // Then
        expect(legends).toEqual(["− the ash block", "+ this file"]);
    });

    it("Given a diff whose header names two files, when it is displayed, then the header is not shown as a change", () => {
        // Given — `---` et `+++` sont l'en-tête du format, pas des lignes du fichier. Les
        // peindre en rouge et vert ferait croire à deux lignes supprimées et ajoutées
        const diff = "--- ash\n+++ file\n-old\n+new";

        // When
        const lines = findAll(diffView(diff).build(), "settings-diff-line").map(plainText);

        // Then
        expect(lines).toEqual(["− old", "+ new"]);
    });

    it("Given a conflict, when the diff is displayed, then it says that ash touches nothing until it is settled", () => {
        // Given — l'écran de conflit est le refus lui-même : il n'écrit rien, et il le dit
        // (ADR-0007)
        const said = plainText(diffView("-a\n+b"));

        // Then
        expect(said).toContain("outside the ash block the file is untouched");
    });
});
