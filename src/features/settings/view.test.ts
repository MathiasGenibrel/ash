import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, text, type UiChild } from "@/shared/ui";

import { aDraft, aHooksReport, aSnapshot, aTool, aVerification } from "./builders";
import { describeToolCount } from "./model";
import { settingsNav, settingsPanel, type SettingsRendering, type SettingsScene } from "./view";

/**
 * L'assemblage de la fenêtre, lu comme une valeur.
 *
 * C'est ce qui n'existait pas : `view.ts` faisait 986 lignes de `document`, et `bun test`
 * ne monte pas de DOM. Trois passes architecturales d'affilée y ont trouvé une règle
 * produit cachée — toujours la même famille, la vue qui supprime une information que le
 * backend envoie. Ces tests-là sont la garde qui manquait.
 */
function scene(overrides: Partial<SettingsScene> = {}): SettingsScene {
    return {
        section: "tools",
        snapshot: aSnapshot(),
        draft: null,
        draftVerification: null,
        failure: null,
        edits: new Map(),
        conflict: null,
        ...overrides,
    };
}

/** Aucune de ces actions n'est appelée par la composition — seulement par un geste. */
const IDLE_ACTIONS: SettingsRendering = new Proxy({} as SettingsRendering, {
    get: () => () => undefined,
});

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

describe("le panneau de la fenêtre de réglages", () => {
    it("Given no tool declared, when the panel is composed, then it says what the emptiness costs and offers only to add one", () => {
        // Given — `re-verify all` sur une liste vide ne vérifierait rien : le bouton
        // n'apparaît qu'à partir de la première entrée
        const composed = settingsPanel(scene(), IDLE_ACTIONS);

        // When
        const buttons = composed.flatMap((child) => findAll(child, "ui-button")).map(plainText);

        // Then
        expect(buttons).toEqual(["add"]);
        expect(said(composed)).toContain("no tools declared");
        expect(said(composed)).toContain(
            "ash writes to no file until you declare a tool and install its hooks.",
        );
    });

    it("Given a list with an invalid entry, when the header and the column are composed, then both announce the same number of problems", () => {
        // Given — la maquette `3e` montre les deux chiffres au même instant. Comptés à deux
        // endroits (#15), ils finissent par ne plus dire la même chose, et celui de la
        // colonne n'était sous aucun test
        const tools = [
            aTool({ command: "claude" }),
            aTool({ command: "codex", verification: aVerification("invalid") }),
        ];
        const current = scene({ snapshot: aSnapshot({ tools }) });

        // When
        const header = settingsPanel(current, IDLE_ACTIONS)[0];
        const column = settingsNav(current, IDLE_ACTIONS);
        const badge = column
            .map((row) => find(row, "settings-nav-count"))
            .find((found) => found !== null);

        // Then
        expect(plainText(find(header ?? text(""), "settings-count") ?? text(""))).toBe(
            describeToolCount(tools),
        );
        expect(plainText(header ?? text(""))).toContain("1 invalid");
        expect(plainText(badge ?? text(""))).toBe("1");
    });

    it("Given an entry being added, when the panel is composed, then the form replaces the list rather than floating above it", () => {
        // Given — le formulaire remplace le contenu de la section (§3.8) : ni modale, ni
        // panneau latéral
        const composed = settingsPanel(scene({ draft: aDraft() }), IDLE_ACTIONS);

        // When
        const cards = composed.flatMap((child) => findAll(child, "settings-card"));

        // Then
        expect(said(composed)).toContain("new tool");
        expect(cards).toEqual([]);
    });

    it("Given a diff being looked at, when the panel is composed, then it replaces the list and offers exactly what the backend allows", () => {
        // Given — l'écran du diff remplace la liste (§4.4). Les issues qu'il propose sont
        // celles du backend, et rien d'autre : un bouton que l'écran ajouterait de lui-même
        // écrirait dans un fichier que le backend n'a pas autorisé à toucher (ADR-0009)
        const tool = aTool({
            hooks: aHooksReport({
                state: "conflict",
                action: "seeTheDiff",
                diff: "-a\n+b",
                choices: [
                    {
                        action: "install",
                        label: "merge, keeping every hook",
                        note: "ash adds its entries next to yours.",
                    },
                ],
            }),
        });
        const composed = settingsPanel(
            scene({ snapshot: aSnapshot({ tools: [tool] }), conflict: "claude" }),
            IDLE_ACTIONS,
        );

        // When
        const buttons = composed.flatMap((child) => findAll(child, "ui-button")).map(plainText);

        // Then
        expect(buttons).toEqual(["← back to the list", "merge, keeping every hook"]);
        expect(composed.flatMap((child) => findAll(child, "settings-card"))).toEqual([]);
    });

    it("Given a section that has no content yet, when the panel is composed, then it says where the thing lives today", () => {
        // Given — la navigation les traverse : un panneau muet ferait croire à une panne
        const composed = settingsPanel(scene({ section: "appearance" }), IDLE_ACTIONS);

        // Then
        expect(said(composed)).toContain("the theme is chosen in View ▸ Theme");
    });

    it("Given two entries pointing at the same folder, when the panel is composed, then the banner sits between the header and the list", () => {
        // Given — elle ne décrit aucune des deux cartes en particulier (§3.7) : posée dans
        // l'une d'elles, elle accuserait celle qu'on regarde
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({ command: "claude-perso", duplicates: ["claude"] }),
        ];

        // When
        const composed = settingsPanel(
            scene({ snapshot: aSnapshot({ tools }) }),
            IDLE_ACTIONS,
        );
        const shapes = composed.map((child) => find(child, "settings-banner") !== null);

        // Then — l'en-tête, la note, la bannière, le corps, le pied
        expect(shapes).toEqual([false, false, true, false, false]);
        expect(composed.flatMap((child) => findAll(child, "settings-card"))).toHaveLength(2);
    });
});
