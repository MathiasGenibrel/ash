import { describe, expect, it } from "bun:test";

import { FOCUS_KEY, find, findAll, plainText, text, type UiChild } from "@/shared/ui";

import { aDraft, aSnapshot, aTool, aVerification } from "../builders";
import { addForm, draftFocusKey, type AddFormActions } from "./add-form";

function recorder(): { asked: string[] } & AddFormActions {
    const asked: string[] = [];
    return {
        asked,
        cancelAdding: () => asked.push("cancel"),
        editDraft: (patch) => asked.push(`edit ${JSON.stringify(patch)}`),
        submitDraft: () => asked.push("submit"),
    };
}

/** Le bouton `add` de la barre d'action — le dernier bouton du formulaire. */
function addButton(described: readonly UiChild[]) {
    const buttons = described.flatMap((child) => findAll(child, "ui-button"));
    return buttons.at(-1);
}

describe("le formulaire d'ajout", () => {
    it("Given a draft the four tests have not answered on, when the form is described, then add is off and says what is being waited for", () => {
        // Given — la patience de la maquette (§3.8) : `add` éteint tant que les tests n'ont
        // pas **répondu**, pas tant qu'ils n'ont pas réussi. La règle est dans
        // `describeAddAction` et n'est pas rejouée ici
        const described = addForm(aDraft(), aSnapshot(), null, null, recorder());

        // When
        const add = addButton(described);

        // Then
        expect(plainText(add ?? text(""))).toBe("add");
        expect(add?.attrs["disabled"]).toBe("");
        expect(described.map(plainText).join("")).toContain("waiting on the four tests");
    });

    it("Given a name the backend has already refused and a draft that is locally wrong, when the form is described, then it is the local refusal that is read", () => {
        // Given — la précédence est une règle (#14) : un refus local décrit la saisie qu'on
        // a sous les yeux, celui du backend décrit celle qu'on lui a envoyée
        const described = addForm(
            aDraft({ command: "" }),
            aSnapshot({ tools: [aTool()] }),
            aVerification(),
            "claude is already declared",
            recorder(),
        );

        // When
        const reason = plainText(described[1] ?? text(""));

        // Then
        expect(reason).toContain("name the command first");
        expect(reason).not.toContain("already declared");
    });

    it("Given a draft the tests have answered on, when add is pressed, then the form asks for it and judges nothing itself", () => {
        // Given — c'est le backend qui juge à nouveau, et c'est lui qui tranche (ADR-0009)
        const actions = recorder();
        const described = addForm(aDraft(), aSnapshot(), aVerification("invalid"), null, actions);

        // When
        addButton(described)?.on["click"]?.({ value: "", key: "" });

        // Then — une entrée invalide se déclare : Ash n'empêche pas de déclarer, il refuse
        // d'écrire
        expect(addButton(described)?.attrs["disabled"]).toBeUndefined();
        expect(actions.asked).toEqual(["submit"]);
    });

    it("Given the fallback adapter, when the form is described, then what it costs is written before anything is added", () => {
        // Given — `generic` est un mode dégradé, et l'écran le dit **avant** (§3.8) : sans
        // adaptateur dédié, l'outil n'aura jamais `waiting`
        const described = addForm(
            aDraft({ command: "aider", adapter: "generic" }),
            aSnapshot(),
            null,
            null,
            recorder(),
        );

        // When
        const said = described.map(plainText).join("");

        // Then
        expect(said).toContain("degraded mode");
        expect(said).toContain("aider will show as ");
        expect(said).toContain("never ");
    });

    it("Given a dedicated adapter, when the form is described, then nothing is warned about", () => {
        // Given — un adaptateur dédié n'a rien à annoncer, et un avertissement permanent
        // cesse d'être lu
        const described = addForm(
            aDraft({ adapter: "claude-code" }),
            aSnapshot(),
            null,
            null,
            recorder(),
        );

        // When
        const warnings = described.flatMap((child) => findAll(child, "settings-degraded"));

        // Then
        expect(warnings).toEqual([]);
    });

    it("Given nothing verified yet, when the test row of the form is described, then it shows the empty verification of the model and no invented one", () => {
        // Given — la vue fabriquait ce `Verification` elle-même (#15), donc hors de portée
        // de tout test : une saisie que rien n'a jugée n'autorise jamais une écriture
        const described = addForm(aDraft(), aSnapshot(), null, null, recorder());

        // When
        const summary = described
            .map((child) => find(child, "settings-test-summary"))
            .find((found) => found !== null);

        // Then
        expect(plainText(summary ?? text(""))).toBe("nothing verified yet");
    });

    it("Given a field being typed into, when the form is redrawn, then each field carries the key that gets the cursor back", () => {
        // Given — le formulaire se redessine à chaque frappe, et la vérification de la
        // saisie est différée de 400 ms : sans clé, le curseur partirait à chaque relance
        const described = addForm(aDraft(), aSnapshot(), null, null, recorder());

        // When
        const keys = described
            .flatMap((child) => findAll(child, "settings-input"))
            .map((input) => input.attrs[FOCUS_KEY]);

        // Then — le menu d'adaptateur n'en a pas : on n'y tape pas
        expect(keys).toEqual([
            draftFocusKey("command"),
            draftFocusKey("label"),
            undefined,
            draftFocusKey("config"),
        ]);
    });
});
