import { describe, expect, it } from "bun:test";

import { findAll, plainText, type UiChild, type UiElementNode } from "@/shared/ui";

import { anAppearance } from "../builders";
import { appearanceSection, type AppearanceActions } from "./appearance";

function said(children: readonly UiChild[]): string {
    return children.map(plainText).join("");
}

/** Les boutons de la section, avec ce qu'ils portent — leur libellé et leur état. */
function buttons(children: readonly UiChild[]): UiElementNode[] {
    return children.flatMap((child) => findAll(child, "ui-button"));
}

const IDLE: AppearanceActions = { chooseTheme: () => undefined, stepFontSize: () => undefined };

describe("la section appearance de la fenêtre de réglages", () => {
    it("Given a session set to dark, when the section is composed, then the three themes are offered and the one in force is marked", () => {
        // Given — le critère de l'issue #110 : « le choix courant y est visible sans avoir à
        // le deviner ». Un menu déroulant n'aurait montré que lui, et les deux autres
        // seraient restées à deviner
        const composed = appearanceSection(anAppearance({ mode: "dark" }), IDLE);

        // When
        const themes = buttons(composed).filter((one) => one.attrs["aria-pressed"] !== undefined);

        // Then
        expect(themes.map(plainText)).toEqual(["light", "dark", "system"]);
        expect(themes.map((one) => one.attrs["aria-pressed"])).toEqual(["false", "true", "false"]);
    });

    it("Given a theme clicked in the screen, when the click is played, then it asks the backend and changes nothing itself", () => {
        // Given — la section est la **seconde** surface d'un état détenu par
        // `features::theme` (ADR-0009). Une bascule posée ici afficherait un choix que le
        // backend n'a peut-être pas retenu, et la coche du menu Vue dirait l'autre
        const asked: string[] = [];
        const shown = anAppearance({ mode: "system" });
        const composed = appearanceSection(shown, { ...IDLE, chooseTheme: (m) => asked.push(m) });

        // When
        const light = buttons(composed).find((one) => plainText(one) === "light");
        light?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked).toEqual(["light"]);
        expect(shown.mode).toBe("system");
    });

    it("Given a font size the backend holds, when the section is composed, then it shows that size and offers steps rather than a number to type", () => {
        // Given — les bornes sont en Rust (`FontSize`), et une taille saisie ici en ferait un
        // second détenteur : la section demande un pas, comme le menu Vue
        const composed = appearanceSection(anAppearance({ fontSize: 15 }), IDLE);

        // When
        const steps = buttons(composed)
            .filter((one) => one.attrs["aria-pressed"] === undefined)
            .map(plainText);

        // Then
        expect(said(composed)).toContain("15 pt");
        expect(steps).toEqual(["smaller", "bigger", "default"]);
        expect(composed.flatMap((child) => findAll(child, "settings-input"))).toEqual([]);
    });

    it("Given the backend has not answered yet, when the section is composed, then it says what it waits for rather than a default it does not hold", () => {
        // Given — afficher `system` et `13 pt` avant la réponse ferait lire à une session
        // réglée en sombre un choix qui n'est pas le sien
        const composed = appearanceSection(null, IDLE);

        // Then
        expect(said(composed)).toContain("asking ash what it is set to…");
        expect(buttons(composed)).toEqual([]);
    });

    it("Given any state of the section, when it is composed, then its content starts under the title instead of being centred", () => {
        // Given — le corps centré était celui des sections vides, et le critère de l'issue
        // demande que le contenu commence sous son titre
        const composed = appearanceSection(anAppearance(), IDLE);

        // When
        const bodies = composed.flatMap((child) => findAll(child, "settings-body"));

        // Then
        expect(bodies).toHaveLength(1);
        expect(bodies[0]?.classes).not.toContain("is-empty");
    });
});
