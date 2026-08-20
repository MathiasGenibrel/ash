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
 *
 * **Ce n'est pas un `Mirrors` de `shared/ipc/`, et ça ne peut pas l'être.** Ce filet-là
 * confronte deux écritures d'une même forme JSON, dont `ts-rs` tire la première d'une
 * `struct`. Un identifiant d'action n'est pas une forme : c'est ce que rend une fonction
 * (`Action::id`, qui en `format!` trois familles). Pour le donner à `ts-rs`, il faudrait
 * écrire les identifiants une seconde fois en Rust — la seconde liste que l'issue #32 existe
 * précisément pour ne pas créer. Deux tests jumeaux, un par côté, sont la bonne réponse ici.
 */
describe("les identifiants d'action du menu", () => {
    it("Given every identifier the window is meant to play, when each is read, then it names its own action", () => {
        // Given — la table entière, et pas seulement le groupe git : ce fichier garde la
        // moitié TypeScript du contrat, donc une faute de frappe sur `tab:new-home` doit se
        // voir ici au même titre que sur `git:branch-card`
        const declared: ReadonlyArray<readonly [string, MenuAction]> = [
            ["tab:new", { kind: "new-tab" }],
            ["tab:new-home", { kind: "new-home-tab" }],
            ["tab:close", { kind: "close-tab" }],
            ["tab:clear", { kind: "clear-scrollback" }],
            ["tab:next", { kind: "next-tab" }],
            ["tab:previous", { kind: "previous-tab" }],
            ["view:toggle-sidebar", { kind: "toggle-sidebar" }],
            // Les cinq surfaces git de la spec §7, déclarées par le sous-menu « Git » (#32)
            ["git:branches", { kind: "toggle-branches" }],
            ["git:graph", { kind: "toggle-graph" }],
            ["git:worktrees", { kind: "toggle-worktrees" }],
            ["git:merge", { kind: "open-merge" }],
            ["git:branch-card", { kind: "toggle-branch-card" }],
            // La famille paramétrée, à ses deux bouts — `Cmd+1` … `Cmd+9`
            ["tab:select:1", { kind: "select-tab", position: 1 }],
            ["tab:select:9", { kind: "select-tab", position: 9 }],
        ];

        // When
        const named = declared.map(([id]) => parseMenuAction(id));

        // Then
        expect(named).toEqual(declared.map(([, action]) => action));
    });

    it("Given the identifiers rust keeps for itself, when they reach the window, then none of them is played here", () => {
        // Given — `route` les joue en Rust et ne les fait descendre à aucune fenêtre :
        // ouvrir les réglages, choisir un thème, changer la taille de police. Les ignorer
        // est **voulu**, et sans ce test on ne saurait pas distinguer ce choix d'un oubli
        const kept = ["app:settings", "view:theme:dark", "view:font:bigger"];

        // When
        const played = kept.map(parseMenuAction);

        // Then
        expect(played).toEqual([null, null, null]);
    });

    it("Given a git identifier no surface carries, when it is read, then nothing is played", () => {
        // Given / When — `git:` est un préfixe comme `view:` et `tab:` : un identifiant
        // inconnu ne doit ni ouvrir une vue au hasard, ni retomber sur une position d'onglet
        const unknown = parseMenuAction("git:blame");

        // Then
        expect(unknown).toBeNull();
    });

    it("Given a tab position beyond the nine shortcuts, when it is read, then it is not an action", () => {
        // Given / When — la spec §4.4 s'arrête à `Cmd+9`, et le Rust refuse `tab:select:0`
        // comme `tab:select:10`. La fenêtre ne doit pas être plus permissive que lui, ou une
        // frappe qu'il a refusée sélectionnerait un onglet qui n'a pas de raccourci
        const outside = ["tab:select:0", "tab:select:10", "tab:select:x"].map(parseMenuAction);

        // Then
        expect(outside).toEqual([null, null, null]);
    });
});
