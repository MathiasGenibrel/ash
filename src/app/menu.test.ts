import { describe, expect, it } from "bun:test";

import { parseMenuAction, type MenuAction } from "./menu";

/**
 * L'identifiant d'action est le contrat entre `src-tauri/src/menu.rs` et cette fenêtre : une
 * chaîne que **rien ne vérifie à la compilation**, des deux côtés. Le Rust garde sa moitié
 * par un aller-retour `id` / `from_id` ; ce fichier garde la nôtre.
 *
 * Ce qui se casserait sans lui est silencieux : une faute de frappe dans `git:branch-card`
 * laisse l'entrée de menu s'afficher, `⌘⌃I` être consommé par AppKit — donc ne pas descendre
 * dans le shell — et la fiche de branche ne jamais s'ouvrir. Rien n'échoue, rien ne se dit.
 */
describe("les identifiants d'action du menu", () => {
    it("Given the five git menu identifiers, when they are read, then each names its own surface", () => {
        // Given — les cinq surfaces de la spec §7, déclarées par le sous-menu « Git » (#32)
        const declared = [
            "git:branches",
            "git:graph",
            "git:worktrees",
            "git:merge",
            "git:branch-card",
        ];

        // When
        const named = declared.map(parseMenuAction);

        // Then
        expect(named).toEqual([
            { kind: "toggle-branches" },
            { kind: "toggle-graph" },
            { kind: "toggle-worktrees" },
            { kind: "open-merge" },
            { kind: "toggle-branch-card" },
        ] satisfies MenuAction[]);
    });

    it("Given a git identifier no surface carries, when it is read, then nothing is played", () => {
        // Given / When — `git:` est un préfixe comme `view:` et `tab:` : un identifiant
        // inconnu ne doit ni ouvrir une vue au hasard, ni retomber sur une position d'onglet
        const unknown = parseMenuAction("git:blame");

        // Then
        expect(unknown).toBeNull();
    });
});
