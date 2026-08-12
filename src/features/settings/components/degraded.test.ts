import { describe, expect, it } from "bun:test";

import { findAll, plainText, text } from "@/shared/ui";

import { degradedNotice, degradedRow } from "./degraded";

describe("l'avertissement du mode dégradé", () => {
    it("Given the fallback adapter, when the warning is written, then it never says ash reads what the process prints", () => {
        // Given — la maquette écrit « ash reads the process output ». Les états d'agent
        // viennent des hooks, **jamais** d'une analyse de la sortie du PTY
        // ([ADR-0007](../../../../docs/adr/0007-etats-par-hooks.md)) : la sonde observe le
        // processus, elle ne le lit pas
        const said = plainText(degradedNotice("aider"));

        // Then
        expect(said).toContain("ash watches the process, not its hooks");
        expect(said).not.toContain("output");
    });

    it("Given the four states a degraded tool can show, when the warning is written, then each one is tinted by the palette the rest of ash uses", () => {
        // Given — c'est le seul endroit de l'interface où du texte courant est teint par
        // état : les mots portent les classes de `app/styles.css`, donc les mêmes couleurs
        // que la sidebar et la ligne de statut, définies au même endroit
        const words = findAll(degradedNotice("aider"), "ash-state-word");

        // Then — et `waiting` est bien nommé comme celui qu'on n'aura jamais
        expect(words.map(plainText)).toEqual(["idle", "done", "error", "waiting"]);
        expect(words.map((word) => word.classes.at(-1))).toEqual([
            "is-idle",
            "is-done",
            "is-error",
            "is-waiting",
        ]);
    });

    it("Given a grid row, when the warning is placed in it, then it keeps an empty label cell so it sits under what it comments", () => {
        // Given — les corps de carte et le formulaire sont des grilles à deux colonnes :
        // l'avertissement répond au menu, pas à la colonne des libellés
        const placed = degradedRow("aider");

        // Then
        expect(placed).toHaveLength(2);
        expect(plainText(placed[0] ?? text(""))).toBe("");
    });
});
