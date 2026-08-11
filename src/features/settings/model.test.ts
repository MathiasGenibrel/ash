import { describe, expect, it } from "bun:test";

import type { ToolDeclaration, ToolDraft } from "./contract";
import { addBlockedReason, degradedModeSubject, describeTool, describeToolCount } from "./model";

/**
 * Test Data Builders : une entrée déclarée, et une saisie de formulaire.
 *
 * Les défauts sont valides et déterministes — une entrée `claude` sur l'adaptateur de
 * repli, non vérifiée, comme toute entrée à ce jalon. Un scénario ne surcharge que ce
 * qu'il regarde.
 */
function aTool(overrides: Partial<ToolDeclaration> = {}): ToolDeclaration {
    return {
        command: "claude",
        label: null,
        adapter: "generic",
        config: null,
        verified: false,
        ...overrides,
    };
}

function aDraft(overrides: Partial<ToolDraft> = {}): ToolDraft {
    return { command: "claude", label: "", adapter: "generic", config: "", ...overrides };
}

describe("ce qu'une carte d'outil dit", () => {
    it("Given a tool with a display label, when its card is described, then the command stays the name and the label rides beside it", () => {
        // Given — `label = "Perso"` de la spec §9. C'est l'écran où l'on déclare la
        // commande : la masquer derrière son libellé cacherait ce qu'on règle
        const tool = aTool({ command: "claude-perso", label: "Perso" });

        // When
        const heading = describeTool(tool);

        // Then
        expect([heading.name, heading.badge]).toEqual(["claude-perso", "Perso"]);
    });

    it("Given a tool with no configuration folder, when its card is described, then it says the adapter default rather than nothing", () => {
        // Given — l'absence de dossier n'est pas un dossier vide : c'est celui de
        // l'adaptateur, que l'adaptateur est seul à connaître
        const tool = aTool({ config: null });

        // When
        const heading = describeTool(tool);

        // Then
        expect(heading.config).toBe("adapter default");
    });
});

describe("le compteur de la section", () => {
    it("Given no declared tool, when the header is counted, then it says none rather than zero", () => {
        // Given — l'état vide se dit d'un mot : il n'y a rien à compter
        // When
        const counted = describeToolCount([]);

        // Then
        expect(counted).toBe("none");
    });

    it("Given three tools none of which passed the four tests, when the header is counted, then it says how many are verified", () => {
        // Given — le format `<n> declared · <n> verified` est normatif. Rien n'est vérifié
        // à ce jalon, et c'est précisément ce que le compteur doit dire
        const tools = [aTool(), aTool({ command: "codex" }), aTool({ command: "kimi" })];

        // When
        const counted = describeToolCount(tools);

        // Then
        expect(counted).toBe("3 declared · 0 verified");
    });
});

describe("la règle d'ajout d'une entrée", () => {
    it("Given a draft that names a fresh command, when the add button is judged, then nothing blocks it", () => {
        // Given
        const draft = aDraft({ command: "codex" });

        // When
        const reason = addBlockedReason(draft, [aTool()]);

        // Then
        expect(reason).toBeNull();
    });

    it("Given a draft with no command yet, when the add button is judged, then it is blocked with its reason", () => {
        // Given — le bouton reste à sa place, éteint, avec sa raison : le masquer ferait
        // croire que l'ajout n'existe pas
        const draft = aDraft({ command: "  " });

        // When
        const reason = addBlockedReason(draft, []);

        // Then
        expect(reason).toBe("name the command first");
    });

    it("Given a command already declared, when the add button is judged, then it is blocked and names the collision", () => {
        // Given — `match` est la clé de la spec §9 : deux entrées homonymes désigneraient
        // le même processus
        const draft = aDraft({ command: " claude " });

        // When
        const reason = addBlockedReason(draft, [aTool({ command: "claude" })]);

        // Then
        expect(reason).toBe("claude is already declared");
    });

    it("Given a draft that carries a path instead of a command name, when the add button is judged, then it is blocked", () => {
        // Given — la sonde compare un nom de processus (ADR-0005/0006) : un chemin ne
        // correspondrait jamais, tout en se lisant comme une entrée valide
        const draft = aDraft({ command: "/usr/local/bin/claude" });

        // When
        const reason = addBlockedReason(draft, []);

        // Then
        expect(reason).toBe("/usr/local/bin/claude is not a command name");
    });
});

describe("l'avertissement de mode dégradé", () => {
    it("Given a draft on the generic adapter, when the form is drawn, then the warning names the tool it concerns", () => {
        // Given — un outil sans adaptateur dédié n'émettra jamais `waiting` (ADR-0007/0008),
        // et l'écran le dit **avant** l'ajout, pas après
        const draft = aDraft({ command: "kimi", adapter: "generic" });

        // When
        const subject = degradedModeSubject(draft);

        // Then
        expect(subject).toBe("kimi");
    });

    it("Given a draft on a dedicated adapter, when the form is drawn, then there is nothing to warn about", () => {
        // Given
        const draft = aDraft({ adapter: "claude-code" });

        // When
        const subject = degradedModeSubject(draft);

        // Then
        expect(subject).toBeNull();
    });
});
