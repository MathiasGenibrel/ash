import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { AGENT_STATES, presentAgentState } from "@/shared/agent-state";
import { find, findAll, plainText, type UiChild, type UiElementNode } from "@/shared/ui";

import { anAppearance } from "../builders";
import { appearanceSection, SIDEBAR_ROW_HEIGHTS, type AppearanceActions } from "./appearance";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

/** Les boutons de la section, avec ce qu'ils portent — leur libellé et leur état. */
function buttons(children: readonly UiChild[]): UiElementNode[] {
    return children.flatMap((child) => findAll(child, "ui-button"));
}

/** Les tuiles de thème, dans l'ordre où elles sont proposées. */
function tiles(children: readonly UiChild[]): UiElementNode[] {
    return children.flatMap((child) => findAll(child, "settings-theme-tile"));
}

/**
 * Les lignes d'agent de la **première** miniature peinte dans une palette donnée.
 *
 * La première, parce qu'il y en a deux par palette une fois la tuile `system` composée : la
 * sienne superpose les deux rendus, et c'est précisément ce que le test de la diagonale
 * vérifie ailleurs.
 */
function previewRows(children: readonly UiChild[], palette: string): readonly UiElementNode[] {
    const preview = children.flatMap((child) => findAll(child, `ash-palette-${palette}`)).at(0);
    return preview === undefined ? [] : findAll(preview, "settings-preview-row");
}

const FONTS = ["JetBrains Mono", "Menlo", "SF Mono"];

const IDLE: AppearanceActions = {
    chooseTheme: () => undefined,
    stepFontSize: () => undefined,
    chooseFont: () => undefined,
    chooseDensity: () => undefined,
};

describe("la section appearance de la fenêtre de réglages", () => {
    it("Given a session set to dark, when the section is composed, then the three themes are offered as previews and the one in force is marked", () => {
        // Given — le critère de l'issue #22 : les trois thèmes se choisissent sur un
        // **aperçu** de la sidebar, pas sur trois boutons radio. Le choix courant doit
        // rester visible sans avoir à le deviner
        const composed = appearanceSection(anAppearance({ mode: "dark" }), FONTS, IDLE);

        // When
        const offered = tiles(composed);

        // Then
        expect(offered.map((one) => one.attrs["aria-label"])).toEqual([
            "theme: light",
            "theme: dark",
            "theme: system",
        ]);
        expect(offered.map((one) => one.attrs["aria-pressed"])).toEqual(["false", "true", "false"]);
        expect(offered.filter((one) => one.classes.includes("is-chosen"))).toHaveLength(1);
    });

    it("Given a theme preview, when its rows are read, then it shows the five agent states with what shared/agent-state says of each", () => {
        // Given — l'aperçu n'a de valeur que s'il dit la vérité de la colonne : c'est la
        // même table qui décide du glyphe, du fond teinté, du rail et du nom barré. Une
        // seconde table ici finirait par montrer un `working` que la sidebar n'a plus
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // When
        const rows = previewRows(composed, "dark");

        // Then — un état par ligne, dans l'ordre, et chacun avec son traitement
        expect(rows).toHaveLength(AGENT_STATES.length);
        for (const state of AGENT_STATES) {
            const shown = presentAgentState(state);
            const row = rows.find((one) => one.classes.includes(shown.className));
            expect(row).toBeDefined();
            expect(row?.classes.includes("is-tinted")).toBe(shown.tinted);
            expect(row?.classes.includes(`has-${shown.rail}-rail`)).toBe(shown.rail !== "none");
        }
    });

    it("Given the error row of a preview, when its name is read, then it is struck through as the sidebar strikes it", () => {
        // Given — la planche de design dessine cette ligne **non barrée**, et
        // `presentAgentState` pose `struck: true` sur `error`. C'est la colonne qui gagne :
        // un aperçu qui ment sur ce qu'il montre est pire qu'une absence d'aperçu
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // When
        const rows = previewRows(composed, "dark");
        const error = rows.find((one) => one.classes.includes(presentAgentState("error").className));
        const name = error === undefined ? null : find(error, "settings-preview-name");

        // Then
        expect(presentAgentState("error").struck).toBe(true);
        expect(name?.classes).toContain("is-struck");
    });

    it("Given the system tile, when it is composed, then it shows both palettes at once instead of naming a third one", () => {
        // Given — `system` n'est pas une troisième palette : c'est l'absence de choix, donc
        // celui de macOS. La diagonale porte le message, et aucun texte ne l'explique
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // When
        const tile = tiles(composed).at(2);
        const palettes = tile === null || tile === undefined ? [] : findAll(tile, "settings-preview");

        // Then — deux miniatures superposées, une par palette, et la claire découpée
        expect(palettes.map((one) => one.classes.includes("ash-palette-light"))).toEqual([
            false,
            true,
        ]);
        expect(palettes.filter((one) => one.classes.includes("is-clipped"))).toHaveLength(1);
    });

    it("Given the two palettes of a preview, when their rows are compared, then only the colours differ", () => {
        // Given — c'est la démonstration que la section veut faire : le thème clair ne perd
        // ni la hiérarchie ni l'urgence de `waiting`, parce que l'urgence tient au rail et
        // au fond teinté, pas à la luminosité
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // When
        const dark = previewRows(composed, "dark").map((row) => row.classes.join(" "));
        const light = previewRows(composed, "light").map((row) => row.classes.join(" "));

        // Then — mêmes formes, mêmes retraits, même teinte : seule la palette change, et
        // elle est portée par la classe de la miniature, pas par ses lignes
        expect(light).toEqual(dark);
    });

    it("Given a theme clicked in the screen, when the click is played, then it asks the backend and changes nothing itself", () => {
        // Given — la section est la **seconde** surface d'un état détenu par
        // `features::theme` (ADR-0009). Une bascule posée ici afficherait un choix que le
        // backend n'a peut-être pas retenu, et la coche du menu Vue dirait l'autre
        const asked: string[] = [];
        const shown = anAppearance({ mode: "system" });
        const composed = appearanceSection(shown, FONTS, {
            ...IDLE,
            chooseTheme: (mode) => asked.push(mode),
        });

        // When
        const light = tiles(composed).find((one) => one.attrs["aria-label"] === "theme: light");
        light?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual(["light"]);
        expect(shown.mode).toBe("system");
    });

    it("Given the monospace fonts the backend read, when one is picked, then it asks the backend with that family and says how many are installed", () => {
        // Given — la liste est celle du système, lue par `features::theme::FontCatalog` : la
        // fenêtre ne la calcule pas, elle la rend, et le choix repart tel quel
        const asked: string[] = [];
        const composed = appearanceSection(anAppearance({ font: "Menlo" }), FONTS, {
            ...IDLE,
            chooseFont: (family) => asked.push(family),
        });

        // When
        const menu = composed.flatMap((child) => findAll(child, "ui-choice")).at(0);
        menu?.on["change"]?.({ value: "SF Mono", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual(["SF Mono"]);
        expect(said(composed)).toContain("3 monospace fonts installed");
        // Ce qui est en vigueur est marqué, sans quoi le menu afficherait la première valeur
        // en prétendant que c'est celle de la session
        const selected = menu?.children.filter(
            (option) => option.kind === "element" && option.attrs["selected"] !== undefined,
        );
        expect(selected).toHaveLength(1);
    });

    it("Given the backend has not listed the fonts yet, when the section is composed, then it says what it waits for instead of offering a menu of one", () => {
        // Given — un menu qui ne proposerait que la police en vigueur laisserait croire
        // qu'il n'y en a pas d'autre sur la machine
        const composed = appearanceSection(anAppearance({ font: "Menlo" }), null, IDLE);

        // Then
        expect(said(composed)).toContain("asking macOS which monospace fonts are installed…");
        expect(composed.flatMap((child) => findAll(child, "ui-choice"))).toEqual([]);
        expect(said(composed)).toContain("Menlo");
    });

    it("Given a font size the backend holds, when the section is composed, then it shows that size, offers steps rather than a number to type, and renders a sample at that size", () => {
        // Given — les bornes sont en Rust (`FontSize`), et une taille saisie ici en ferait un
        // second détenteur : la section demande un pas, comme le menu Vue. L'échantillon,
        // lui, montre la conséquence au lieu de l'annoncer
        const composed = appearanceSection(anAppearance({ fontSize: 15 }), FONTS, IDLE);

        // When
        const steps = buttons(composed)
            .filter((one) => one.classes.includes("settings-button"))
            .map(plainText);
        const sample = composed
            .flatMap((child) => findAll(child, "settings-appearance-sample"))
            .at(0);

        // Then
        expect(said(composed)).toContain("15 px");
        expect(steps).toEqual(["smaller", "bigger", "default"]);
        expect(composed.flatMap((child) => findAll(child, "settings-input"))).toEqual([]);
        expect(sample?.attrs["style"]).toBe("font-size: 15px");
    });

    it("Given a sidebar density, when the other one is clicked, then it asks the backend and the segmented control marks what the backend holds", () => {
        // Given — quatrième préférence détenue par `features::theme`, même chemin que les
        // trois autres : la fenêtre demande, elle ne pose pas
        const asked: string[] = [];
        const composed = appearanceSection(anAppearance({ density: "comfortable" }), FONTS, {
            ...IDLE,
            chooseDensity: (density) => asked.push(density),
        });

        // When
        const segments = composed.flatMap((child) => findAll(child, "settings-segment"));
        segments.find((one) => plainText(one) === "compact")?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(asked).toEqual(["compact"]);
        expect(segments.map((one) => one.attrs["aria-pressed"])).toEqual(["true", "false"]);
    });

    it("Given the note that measures a sidebar row, when it is compared to the stylesheet, then it says the heights the stylesheet actually paints", () => {
        // Given — c'est la seule mesure recopiée du dépôt : une note qui annonce 24 px
        // pendant que `styles.css` en pose 22 est pire que pas de note du tout. Le même
        // dispositif que `app/styles.test.ts` pour les palettes, et pour la même raison
        const styles = readFileSync(new URL("../../../app/styles.css", import.meta.url), "utf8");
        const painted = new Map(
            [
                ...styles.matchAll(
                    /:root\[data-density="(\w+)"\]\s*\{[^}]*?--ash-row-height:\s*(\d+)px/g,
                ),
            ].map((match) => [match[1] ?? "", Number(match[2])]),
        );

        // When
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // Then
        expect(painted.get("comfortable")).toBe(SIDEBAR_ROW_HEIGHTS.comfortable);
        expect(painted.get("compact")).toBe(SIDEBAR_ROW_HEIGHTS.compact);
        expect(said(composed)).toContain("24 px / row · 18 px when compact");
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it waits for rather than a default it does not hold", () => {
        // Given — afficher `system` et `13 pt` avant la réponse ferait lire à une session
        // réglée en sombre un choix qui n'est pas le sien
        const composed = appearanceSection(null, FONTS, IDLE);

        // Then
        expect(said(composed)).toContain("asking ash what it is set to…");
        expect(buttons(composed)).toEqual([]);
        expect(tiles(composed)).toEqual([]);
    });

    it("Given any state of the section, when it is composed, then its content starts under the title instead of being centred", () => {
        // Given — le corps centré était celui des sections vides, et le critère de l'issue
        // demande que le contenu commence sous son titre
        const composed = appearanceSection(anAppearance(), FONTS, IDLE);

        // When
        const bodies = composed.flatMap((child) => findAll(child, "settings-body"));

        // Then
        expect(bodies).toHaveLength(1);
        expect(bodies[0]?.classes).not.toContain("is-empty");
    });
});
