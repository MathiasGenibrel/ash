import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import {
    DEFAULT_STATUS_BAR_SEGMENTS,
    composeVisibilityMenu,
    parseStatusBarSegments,
    type StatusBarSegmentId,
    type VisibilityRow,
} from "./status-bar";

/**
 * Le menu « show in the status bar » (spec §4.2, vue 5c) : ce qu'il liste, et ce qu'il
 * **envoie**.
 *
 * Deux règles y valent d'être tenues. La première est que rien n'est détenu ici : un clic
 * part en bascule vers le backend, et la coche suit ce qui revient — un menu qui appliquerait
 * son propre clic serait le second détenteur d'une préférence (ADR-0009), et divergerait de
 * la barre au premier échec d'écriture. La seconde est qu'un élément **masqué reste dans la
 * liste** : c'est le seul endroit d'où on peut le rallumer, et l'en retirer serait un aller
 * sans retour.
 */

/** La feuille de la feature, lue comme un texte : c'est tout ce que `bun test` peut en faire. */
const SHEET = readFileSync(new URL("./terminal.css", import.meta.url), "utf8");

function line(id: StatusBarSegmentId, shown: boolean, preview = ""): VisibilityRow {
    return { id, name: id, preview, shown, separated: false };
}

describe("ce que le backend annonce", () => {
    it("Given a backend that answers nothing understandable, when the segments are read, then the line keeps the defaults", () => {
        // Given — une réponse qui n'aboutit pas, un fichier de préférence absent ou illisible :
        // trois façons de ne rien dire, et une seule conduite acceptable — la ligne d'avant
        const nonsense = [null, undefined, "dark", 3, []];

        // When
        const read = nonsense.map((value) => parseStatusBarSegments(value));

        // Then — surtout pas sept `false` : une ligne de statut vide serait la façon la plus
        // spectaculaire de rater un fichier manquant
        expect(read).toEqual(nonsense.map(() => DEFAULT_STATUS_BAR_SEGMENTS));
    });

    it("Given a backend that names only some segments, when they are read, then the missing ones keep their default", () => {
        // Given — un Ash plus ancien que le menu, ou un `theme.json` édité à la main
        const partial = { weekly: true, cwd: false, cursor: "bar" };

        // When
        const read = parseStatusBarSegments(partial);

        // Then — un champ absent vaut « montré », jamais « masqué »
        expect(read).toEqual({ ...DEFAULT_STATUS_BAR_SEGMENTS, weekly: true, cwd: false });
    });
});

describe("le panneau du menu contextuel", () => {
    it("Given a hidden segment, when the menu is composed, then it is still listed, greyed and without its tick", () => {
        // Given — le weekly, masqué par défaut
        const rows = [line("session", true), line("weekly", false)];

        // When
        const menu = composeVisibilityMenu(rows, () => undefined);

        // Then — il reste une ligne du menu, et elle dit qu'elle est décochée à l'œil comme
        // au lecteur d'écran
        const lines = findAll(menu, "status-menu-line");
        expect(lines.length).toBe(2);
        expect(findAll(menu, "status-menu-name").map(plainText)).toEqual(["session", "weekly"]);
        expect(lines[1]?.classes).toContain("is-hidden");
        expect(lines[1]?.attrs["aria-checked"]).toBe("false");
        expect(findAll(menu, "status-menu-check").map(plainText)).toEqual(["✓", ""]);
    });

    it("Given a menu line, when it is clicked, then what leaves is the segment and nothing else", () => {
        // Given — le clic ne dit pas ce que le segment devient : c'est le backend qui décide,
        // et c'est ce qui empêche le menu d'en devenir le second détenteur (ADR-0009)
        const toggled: StatusBarSegmentId[] = [];
        const menu = composeVisibilityMenu([line("cwd", true)], (id) => {
            toggled.push(id);
        });

        // When
        findAll(menu, "status-menu-line")[0]?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(toggled).toEqual(["cwd"]);
    });

    it("Given a row that opens a group, when the menu is composed, then a rule is drawn above it and nowhere else", () => {
        // Given — le trait de la maquette sépare ce que la conversation consomme de ce qui dit
        // où l'on est ; posé partout, il ferait une liste illisible
        const rows: VisibilityRow[] = [
            line("context", true),
            { ...line("agent", true), separated: true },
            line("cwd", true),
        ];

        // When
        const menu = composeVisibilityMenu(rows, () => undefined);

        // Then — un seul trait, et il précède `agent`
        const children = menu.build().children;
        expect(findAll(menu, "status-menu-rule").length).toBe(1);
        const ruleAt = children.findIndex(
            (node) => node.kind === "element" && node.classes.includes("status-menu-rule"),
        );
        const agentAt = children.findIndex(
            (node) => node.kind === "element" && plainText(node).includes("agent"),
        );
        expect(ruleAt).toBe(agentAt - 1);
    });

    it("Given the menu panel, when its colours are read, then each names a token instead of a hexadecimal", () => {
        // Given — la même exigence que pour les segments d'usage : un hexadécimal écrit ici
        // sortirait des trois thèmes sans qu'aucun test de modèle ne bronche
        const rules = [...SHEET.matchAll(/^\.[^\n{]*status-(?:bar-)?menu[^\n{]*\{([^}]*)\}/gm)];

        // When
        const painted = rules.map((rule) => rule[1] ?? "").join("\n");

        // Then
        expect(rules.length).toBeGreaterThan(0);
        expect(painted).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    });
});
