import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild } from "@/shared/ui";

import { aShortcut, aShortcutsReport } from "../builders";
import { shortcutsSection, type ShortcutCapture, type ShortcutsActions } from "./shortcuts";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

const INERT: ShortcutsActions = {
    openCapture: () => undefined,
    resetShortcut: () => undefined,
    resetAll: () => undefined,
    resolveConflict: () => undefined,
};

/** Un bloc de capture ouvert sur `tab:new`, sans frappe encore. */
function aCapture(overrides: Partial<ShortcutCapture> = {}): ShortcutCapture {
    return { action: "tab:new", keys: "", why: null, note: null, ...overrides };
}

describe("la section shortcuts de la fenêtre de réglages", () => {
    it("Given the bindings the backend holds, when the section is composed, then each one is shown with the combination it sent", () => {
        // Given — les liaisons sont détenues en Rust (`features::shortcuts`), le menu natif en
        // dérive, et la section les **lit**. Une table écrite ici aurait fini par annoncer un
        // raccourci que le menu ne joue plus, et c'est cet écran qu'on croit
        const declared = aShortcutsReport([
            aShortcut({ label: "New Tab", keys: "⌘T" }),
            aShortcut({ action: "tab:select:1", label: "Tab 1 … Tab 9", keys: "⌘1 … ⌘9" }),
        ]);

        // When
        const composed = shortcutsSection(declared, null, INERT);
        const rows = composed.flatMap((child) => findAll(child, "settings-shortcut"));

        // Then
        expect(rows.map(plainText)).toEqual(["New Tab⌘T", "Tab 1 … Tab 9⌘1 … ⌘9"]);
    });

    it("Given shortcuts from two submenus, when the section is composed, then each group is titled once, in the order the menu sends them", () => {
        // Given — « groupés » est un critère de l'issue #110, et l'ordre est celui du menu :
        // on retrouve un raccourci dans l'écran là où on l'a vu dans le menu
        const declared = aShortcutsReport([
            aShortcut({ group: "terminal", label: "New Tab" }),
            aShortcut({ group: "view", label: "Toggle Sidebar", keys: "⌘B" }),
            aShortcut({ group: "terminal", label: "Close Tab", keys: "⌘W" }),
        ]);

        // When
        const composed = shortcutsSection(declared, null, INERT);
        const groups = composed
            .flatMap((child) => findAll(child, "settings-shortcut-group"))
            .map(plainText);

        // Then — deux titres, pas trois : un groupe qui revient est replié dans le premier
        expect(groups).toEqual(["terminal", "view"]);
    });

    it("Given one changed row among untouched ones, when the section is composed, then only that row offers the way back", () => {
        // Given — c'est la promesse que le pied écrit : `only appears on changed rows.` Une
        // icône de retour sur une ligne qui n'a pas bougé proposerait un geste sans effet
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:new", label: "New Tab", keys: "⌘J", changed: true }),
            aShortcut({ action: "tab:close", label: "Close Tab", keys: "⌘W" }),
        ]);

        // When
        const composed = shortcutsSection(declared, null, INERT);
        const backs = composed.flatMap((child) => findAll(child, "settings-shortcut-reset"));

        // Then
        expect(backs).toHaveLength(1);
        expect(said(composed)).toContain("1 changed");
    });

    it("Given a row that cannot be rebound, when the section is composed, then it offers nothing to open", () => {
        // Given — la famille `⌘1 … ⌘9` : neuf positions réglables une par une feraient neuf
        // lignes presque identiques, et un bouton qui n'ouvre rien est pire qu'un texte
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:select:1", label: "Tab 1 … Tab 9", rebindable: false }),
        ]);

        // When
        const composed = shortcutsSection(declared, null, INERT);

        // Then
        expect(composed.flatMap((child) => findAll(child, "settings-shortcut-open"))).toEqual([]);
    });

    it("Given a row opened for capture, when the section is composed, then it grows in place with its three ways out and the value it would replace", () => {
        // Given — la planche est explicite : « la ligne en capture s'agrandit au lieu
        // d'ouvrir une modale : le contexte reste lisible pendant qu'on appuie »
        const declared = aShortcutsReport([aShortcut({ label: "New Tab", keys: "⌘T" })]);

        // When
        const composed = shortcutsSection(declared, aCapture(), INERT);
        const said_ = said(composed);

        // Then — les trois issues, et l'ancienne valeur toujours lisible
        expect(composed.flatMap((child) => findAll(child, "settings-capture"))).toHaveLength(1);
        expect(said_).toContain("press a key combination");
        expect(said_).toContain("esc");
        expect(said_).toContain("no shortcut");
        expect(said_).toContain("confirm");
        expect(said_).toContain("was:⌘T");
    });

    it("Given a combination macOS takes, when it is captured, then the warning shows inside the block and nothing stops it from being confirmed", () => {
        // Given — la règle de la planche, et elle n'est pas un détail de rédaction : « une
        // combinaison prise par macOS ou avalée par le terminal n'est pas interdite — elle est
        // annoncée comme inefficace, au moment de la capture »
        const declared = aShortcutsReport([aShortcut({ label: "New Tab", keys: "⌘T" })]);
        const taken = aCapture({
            keys: "⌘⌥⎋",
            note: "is reserved by macOS (force quit) — ash will never receive it",
        });

        // When
        const composed = shortcutsSection(declared, taken, INERT);

        // Then — l'avertissement est là, la combinaison frappée aussi, et l'aide dit toujours
        // que `⏎` confirme : rien dans l'écran ne la refuse
        const said_ = said(composed);
        expect(said_).toContain("is reserved by macOS (force quit)");
        expect(said_).toContain("⌘⌥⎋");
        expect(said_).toContain("confirm");
    });

    it("Given a combination the terminal swallows, when the row is at rest, then it says so next to a muted pill", () => {
        // Given — `⌘K` : le terminal a ses propres raccourcis et les intercepte avant Ash
        // quand on est dans le shell. La ligne reste une ligne, avec sa combinaison
        const declared = aShortcutsReport([
            aShortcut({
                label: "Clear Scrollback",
                keys: "⌘K",
                reservation: {
                    by: "terminal",
                    note: "swallowed by the terminal — never reaches ash",
                },
            }),
        ]);

        // When
        const composed = shortcutsSection(declared, null, INERT);

        // Then
        expect(said(composed)).toContain("swallowed by the terminal — never reaches ash");
        expect(
            composed.flatMap((child) => findAll(child, "settings-shortcut-keys")).map(plainText),
        ).toEqual(["⌘K"]);
    });

    it("Given two actions on one combination, when the section is composed, then both rows sit in a single block with two named ways out", () => {
        // Given — « un conflit interne se résout par un choix explicite : ash ne réattribue
        // jamais en silence ». Les deux lignes fautives se lisent ensemble, sans quoi elles se
        // contrediraient à l'écran sans dire qu'elles se répondent
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:new", label: "New Tab", keys: "⌘T" }),
            aShortcut({ action: "tab:clear", label: "Clear Scrollback", keys: "⌘K" }),
        ]);
        const disputed = {
            ...declared,
            conflict: {
                keys: "⌘K",
                holder: "tab:clear",
                holderLabel: "Clear Scrollback",
                asked: "tab:new",
                askedLabel: "New Tab",
                diagnosis: "two actions on ⌘K — the last one set would silently win",
                give: "give ⌘K to New Tab",
                keep: "keep the old one",
            },
        };

        // When
        const composed = shortcutsSection(disputed, null, INERT);
        const blocks = composed.flatMap((child) => findAll(child, "settings-conflict-block"));

        // Then — un seul bloc, les deux lignes nommées, et les deux issues telles que le
        // backend les écrit
        expect(blocks).toHaveLength(1);
        const inside = plainText(blocks[0] as UiChild);
        expect(inside).toContain("Clear Scrollbackalready assigned");
        expect(inside).toContain("New Tabjust now");
        expect(inside).toContain("two actions on ⌘K — the last one set would silently win");
        expect(inside).toContain("give ⌘K to New Tab");
        expect(inside).toContain("keep the old one");
    });

    it("Given the combination is held by a row that cannot be rebound, when the section is composed, then the block offers one way out and says why", () => {
        // Given — la famille `⌘1 … ⌘9` ne cède rien : lui reprendre `⌘1` ne tiendrait pas une
        // session, puisque la relecture du fichier le lui rendrait (issue #137). Le backend
        // n'offre donc pas de `give`, et l'écran ne doit pas en inventer un
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:new", label: "New Tab", keys: "⌘T" }),
            aShortcut({
                action: "tab:select:1",
                label: "Tab 1 … Tab 9",
                keys: "⌘1 … ⌘9",
                rebindable: false,
            }),
        ]);
        const refused = {
            ...declared,
            conflict: {
                keys: "⌘&",
                holder: "tab:select:1",
                holderLabel: "Tab 1 … Tab 9",
                asked: "tab:new",
                askedLabel: "New Tab",
                diagnosis:
                    "⌘& belongs to Tab 1 … Tab 9 — that row is not rebindable, and ash will not take it away",
                give: null,
                keep: "keep the old one",
            },
        };

        // When
        const composed = shortcutsSection(refused, null, INERT);
        const blocks = composed.flatMap((child) => findAll(child, "settings-conflict-block"));

        // Then — une seule issue, et la raison du refus nomme ce qui tient la touche pressée
        expect(blocks).toHaveLength(1);
        const inside = plainText(blocks[0] as UiChild);
        expect(inside).toContain("⌘& belongs to Tab 1 … Tab 9");
        expect(inside).toContain("keep the old one");
        expect(composed.flatMap((child) => findAll(child, "settings-conflict-give"))).toHaveLength(
            0,
        );
    });

    it("Given the holder has no row of its own, when the block is composed, then it is named as the contract names it", () => {
        // Given — les huit positions d'onglet derrière « Tab 1 … Tab 9 » n'ont pas de ligne :
        // la famille se lit sous une seule. Chercher le détenteur dans la liste affichée
        // rendait alors son identifiant interne dans une fenêtre de réglages (issue #137).
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:new", label: "New Tab", keys: "⌘T" }),
            aShortcut({
                action: "tab:select:1",
                label: "Tab 1 … Tab 9",
                keys: "⌘1 … ⌘9",
                rebindable: false,
            }),
        ]);
        const refused = {
            ...declared,
            conflict: {
                keys: "⌘É",
                holder: "tab:select:2",
                holderLabel: "Tab 1 … Tab 9",
                asked: "tab:new",
                askedLabel: "New Tab",
                diagnosis:
                    "⌘É belongs to Tab 1 … Tab 9 — that row is not rebindable, and ash will not take it away",
                give: null,
                keep: "keep the old one",
            },
        };

        // When
        const composed = shortcutsSection(refused, null, INERT);
        const blocks = composed.flatMap((child) => findAll(child, "settings-conflict-block"));

        // Then — le nom vient du contrat, et aucun identifiant ne fuit à l'écran
        const inside = plainText(blocks[0] as UiChild);
        expect(inside).toContain("Tab 1 … Tab 9");
        expect(inside).not.toContain("tab:select:2");
    });

    it("Given the two conflicting actions sit in two different submenus, when the section is composed, then they still meet in one block, once", () => {
        // Given — rien n'oblige les deux fautives à être voisines : `⌘B` est dans « View »,
        // et une capture peut la viser depuis « Terminal ». Un bloc posé par groupe l'aurait
        // dessiné deux fois, et un libellé cherché dans le seul groupe courant aurait montré
        // un identifiant d'action à la place d'un nom
        const declared = aShortcutsReport([
            aShortcut({ action: "tab:new", group: "terminal", label: "New Tab", keys: "⌘T" }),
            aShortcut({ action: "view:toggle-sidebar", group: "view", label: "Toggle Sidebar" }),
        ]);
        const disputed = {
            ...declared,
            conflict: {
                keys: "⌘B",
                holder: "view:toggle-sidebar",
                holderLabel: "Toggle Sidebar",
                asked: "tab:new",
                askedLabel: "New Tab",
                diagnosis: "two actions on ⌘B — the last one set would silently win",
                give: "give ⌘B to New Tab",
                keep: "keep the old one",
            },
        };

        // When
        const composed = shortcutsSection(disputed, null, INERT);
        const blocks = composed.flatMap((child) => findAll(child, "settings-conflict-block"));

        // Then
        expect(blocks).toHaveLength(1);
        expect(plainText(blocks[0] as UiChild)).toContain("Toggle Sidebar");
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it is waiting for rather than an empty list", () => {
        // Given — la navigation traverse la section (`⌥↓`) : un panneau muet se lirait comme
        // une panne, et « aucun raccourci » serait un mensonge
        const composed = shortcutsSection(null, null, INERT);

        // Then
        expect(said(composed)).toContain("reading them from the menu…");
    });
});
