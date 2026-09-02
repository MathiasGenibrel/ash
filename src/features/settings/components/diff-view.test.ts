import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import { diffView } from "./diff-view";

describe("le diff de ce qu'Ash écrirait", () => {
    it("Given the diff the backend sends, when it is displayed, then the legend reads it in the backend's direction", () => {
        // Given — `−` est le fichier tel qu'il est, `+` tel qu'Ash le laisserait : c'est le
        // sens d'un diff qu'on s'apprête à appliquer. Légender l'inverse ferait lire chaque
        // ligne à l'envers, la seule faute qu'un diff ne pardonne pas
        const diff =
            '--- file\n+++ ash\n-  "rtk hook claude"\n+  "ash-event waiting"\n   "hooks": {';

        // When
        const legends = findAll(diffView(diff).build(), "settings-diff-legend").map(plainText);

        // Then
        expect(legends).toEqual(["− the file as it is", "+ what ash would write"]);
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

    it("Given a write to come, when the diff is displayed, then it says that nothing happens until the user chooses", () => {
        // Given — le diff précède l'écriture, et ne la déclenche pas. « Jamais silencieux »
        // veut dire que c'est le clic de l'utilisateur qui écrit, et l'écran doit le dire
        // avant qu'il ne clique (ADR-0007, amendement du 2026-08-12)
        const said = plainText(diffView("-a\n+b"));

        // Then
        expect(said).toContain("nothing is written until you choose");
        expect(said).toContain("entries carrying its own marker");
    });
});
