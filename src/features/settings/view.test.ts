import { describe, expect, it } from "bun:test";

import { find, findAll, plainText, text, type UiChild } from "@/shared/ui";

import {
    anAppearance,
    aDraft,
    aHooksReport,
    aNotificationsReport,
    aShortcut,
    aSnapshot,
    aTool,
    aVerification,
} from "./builders";
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
        removal: null,
        fonts: null,
        notifications: aNotificationsReport(),
        appearance: anAppearance(),
        shortcuts: [aShortcut()],
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

    it("Given the appearance section, when the panel is composed, then it settles the theme here instead of sending the user to the menu", () => {
        // Given — c'était la section qui renvoyait à `View ▸ Theme`, et la fenêtre de réglages
        // est censée être l'autre façon d'éditer `~/.ash/config.toml` (spec §9) : envoyer
        // l'utilisateur ailleurs est un aveu, pas une réponse (#110)
        const composed = settingsPanel(scene({ section: "appearance" }), IDLE_ACTIONS);

        // Then
        expect(said(composed)).not.toContain("View ▸ Theme");
        expect(said(composed)).toContain("light");
        expect(said(composed)).toContain("13 px");
    });

    it("Given the shortcuts section, when the panel is composed, then it lists what the menu declares rather than pointing at it", () => {
        // Given — même aveu, et le point dur du critère : la combinaison vient du menu, elle
        // n'est pas recopiée en TypeScript
        const composed = settingsPanel(scene({ section: "shortcuts" }), IDLE_ACTIONS);

        // Then
        expect(said(composed)).toContain("New Tab");
        expect(said(composed)).toContain("⌘T");
        expect(said(composed)).not.toContain("changing them here comes later");
    });

    it("Given the notifications section, when the panel is composed, then it shows what the backend says rather than a placeholder", () => {
        // Given — c'est la section qui porte la dernière puce de la spec §8, et elle était
        // un texte de remplissage. Un panneau qui garderait sa prose d'attente ferait lire
        // « rien n'est notifié » à un utilisateur qu'Ash interrompt déjà
        const composed = settingsPanel(scene({ section: "notifications" }), IDLE_ACTIONS);

        // Then
        expect(said(composed)).toContain("System Settings ▸ Notifications ▸ ash");
        expect(said(composed)).not.toContain("nothing is notified yet");
    });

    it("Given a removal that has been announced, when the panel is composed, then it replaces the list instead of floating over it", () => {
        // Given — ce qui va toucher plusieurs fichiers de l'utilisateur se lit en entier
        // (spec §10), comme l'écran du diff : ni modale, ni panneau qu'un `esc` chasse
        const plan = {
            files: [
                {
                    file: "/home/someone/.claude/settings.json",
                    commands: ["claude"],
                    entries: 5,
                    deletesTheFile: false,
                    handEdited: false,
                    diff: "",
                },
            ],
            summary: "5 entries in /home/someone/.claude/settings.json",
            handEdited: false,
            kept: ["the .bak copies stay where they are."],
        };

        // When
        const composed = settingsPanel(
            scene({
                snapshot: aSnapshot({ tools: [aTool()] }),
                removal: { step: "asked", plan },
            }),
            IDLE_ACTIONS,
        );

        // Then
        expect(composed.flatMap((child) => findAll(child, "settings-card"))).toHaveLength(0);
        expect(said(composed)).toContain("/home/someone/.claude/settings.json");
        expect(said(composed)).toContain("the .bak copies stay where they are.");
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

        // Then — l'en-tête, la note, la bannière, le corps, la désinstallation, le pied
        expect(shapes).toEqual([false, false, true, false, false, false]);
        expect(composed.flatMap((child) => findAll(child, "settings-card"))).toHaveLength(2);
    });
});
