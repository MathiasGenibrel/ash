import { describe, expect, it } from "bun:test";

import type { ToolDeclaration, ToolDraft, Verification, VerificationState } from "./contract";
import {
    degradedModeSubject,
    describeAddAction,
    describeHooksAvailability,
    describeTool,
    describeToolCount,
} from "./model";

/**
 * Test Data Builders : une vérification, une entrée déclarée, et une saisie de formulaire.
 *
 * Les défauts sont valides et déterministes — une entrée `claude` sur l'adaptateur de
 * repli, dont les quatre tests sont passés. Un scénario ne surcharge que ce qu'il regarde.
 *
 * `aVerification` dérive `allowsHooks` de l'état plutôt que de le laisser surcharger : la
 * règle est celle du backend, et un test qui la contredirait dans son `Given` prouverait
 * quelque chose qui ne peut pas arriver.
 */
function aVerification(
    state: VerificationState = "valid",
    overrides: Partial<Verification> = {},
): Verification {
    return {
        state,
        tests: ["passed", "passed", "passed", "passed"],
        summary: "folder recognised · claude answers with this folder",
        stoppedAt: null,
        detail: null,
        fix: null,
        launched: null,
        allowsHooks: state === "valid" || state === "caveat" || state === "verifying",
        ...overrides,
    };
}

function aTool(overrides: Partial<ToolDeclaration> = {}): ToolDeclaration {
    const verification = overrides.verification ?? aVerification();
    return {
        command: "claude",
        label: null,
        adapter: "generic",
        config: null,
        ...overrides,
        verification,
        verified: overrides.verified ?? verification.allowsHooks,
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

    it("Given three tools of which one has never proved anything, when the header is counted, then it says how many are verified", () => {
        // Given — le format `<n> declared · <n> verified` est normatif
        const tools = [
            aTool(),
            aTool({ command: "codex" }),
            aTool({ command: "kimi", verification: aVerification("unverified") }),
        ];

        // When
        const counted = describeToolCount(tools);

        // Then
        expect(counted).toBe("3 declared · 2 verified");
    });

    it("Given one entry ash refuses to write to, when the header is counted, then the problem is what it announces", () => {
        // Given — la maquette `3e` : le compteur passe à `<n> declared · <n> invalid`, et
        // la ligne `tools` de la navigation porte le même chiffre. Compter les vérifiées à
        // côté ferait chercher lesquelles manquent
        const tools = [
            aTool(),
            aTool({ command: "codex", verification: aVerification("invalid") }),
            aTool({ command: "kimi" }),
        ];

        // When
        const counted = describeToolCount(tools);

        // Then
        expect(counted).toBe("3 declared · 1 invalid");
    });
});

describe("la barre d'action du formulaire d'ajout", () => {
    it("Given a draft that names a fresh command, when the action bar is described, then add is on and the bar says what adding will do", () => {
        // Given
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [aTool()], null, aVerification());

        // Then — la barre n'est jamais muette : sans refus, elle annonce la suite
        expect(action).toEqual({
            reason: "hooks install after adding, once the four tests pass",
            enabled: true,
        });
    });

    it("Given a draft with no command yet, when the action bar is described, then add is off with its reason", () => {
        // Given — le bouton reste à sa place, éteint, avec sa raison : le masquer ferait
        // croire que l'ajout n'existe pas
        const draft = aDraft({ command: "  " });

        // When
        const action = describeAddAction(draft, [], null, aVerification());

        // Then
        expect(action).toEqual({ reason: "name the command first", enabled: false });
    });

    it("Given a command already declared, when the action bar is described, then add is off and names the collision", () => {
        // Given — `match` est la clé de la spec §9 : deux entrées homonymes désigneraient
        // le même processus
        const draft = aDraft({ command: " claude " });

        // When
        const action = describeAddAction(draft, [aTool({ command: "claude" })], null, aVerification());

        // Then
        expect(action).toEqual({ reason: "claude is already declared", enabled: false });
    });

    it("Given a draft that carries a path instead of a command name, when the action bar is described, then add is off", () => {
        // Given — la sonde compare un nom de processus (ADR-0005/0006) : un chemin ne
        // correspondrait jamais, tout en se lisant comme une entrée valide
        const draft = aDraft({ command: "/usr/local/bin/claude" });

        // When
        const action = describeAddAction(draft, [], null, aVerification());

        // Then
        expect(action).toEqual({
            reason: "/usr/local/bin/claude is not a command name",
            enabled: false,
        });
    });

    it("Given a backend refusal the draft has since been corrected past, when the action bar is described, then the local reason wins", () => {
        // Given — le refus du backend décrit la saisie qu'on lui a envoyée ; le refus local
        // décrit celle qu'on a sous les yeux. Lire le premier ferait reprocher à l'écran
        // une saisie qui n'existe plus
        const draft = aDraft({ command: "" });

        // When
        const action = describeAddAction(draft, [], "« claude » est déjà déclarée", aVerification());

        // Then
        expect(action.reason).toBe("name the command first");
    });

    it("Given a backend refusal and nothing wrong with the draft, when the action bar is described, then it shows the refusal and still lets you try again", () => {
        // Given — un refus que le frontend ne sait pas prévoir (le registre a changé sous
        // lui) : le masquer perdrait la seule explication, éteindre `add` interdirait de
        // réessayer
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [], "registre des outils empoisonné", aVerification());

        // Then
        expect(action).toEqual({ reason: "registre des outils empoisonné", enabled: true });
    });

    it("Given a draft whose tests have not answered yet, when the action bar is described, then add stays visible, off, with what it is waiting for", () => {
        // Given — « add est désactivé tant que les quatre tests n'ont pas répondu » (§3.8).
        // C'est de la patience, pas un jugement : rien ne dit encore que la saisie est
        // mauvaise
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [], null, null);

        // Then
        expect(action).toEqual({ reason: "waiting on the four tests", enabled: false });
    });

    it("Given a draft whose first three tests have passed while the command is still answering, when the action bar is described, then it names the test it waits on", () => {
        // Given — le second temps : les tests 1 à 3 ont parlé, le quatrième lance un
        // programme. Le mot affiché est `test 4 of 4`, jamais un pourcentage
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [], null, aVerification("verifying"));

        // Then
        expect(action).toEqual({ reason: "waiting on test 4 of 4", enabled: false });
    });

    it("Given a draft the tests found invalid, when the action bar is described, then adding is still allowed", () => {
        // Given — la maquette `3e` montre une entrée invalide **dans la liste**, avec sa
        // correction à portée : Ash n'empêche pas de déclarer, il refuse d'écrire. Éteindre
        // `add` ici ferait disparaître l'entrée qu'on cherche justement à corriger
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [], null, aVerification("invalid"));

        // Then
        expect(action.enabled).toBe(true);
    });
});

describe("ce qui autorise l'écriture des hooks", () => {
    it("Given an entry the tests found invalid, when the hooks line is described, then the button stays visible, off, with its reason", () => {
        // Given — « the block stays visible: button present, disabled, with its reason —
        // never hidden ». Le masquer ferait croire que les hooks n'existent pas pour cet
        // outil
        const verification = aVerification("invalid");

        // When
        const hooks = describeHooksAvailability(verification);

        // Then
        expect(hooks).toEqual({
            reason: "unavailable until the path is verified",
            enabled: false,
        });
    });

    it("Given an entry nothing has verified yet, when the hooks line is described, then it is off for a different reason than an invalid one", () => {
        // Given — les deux sont éteintes et ne disent pas la même chose : l'une attend, à
        // l'autre on a répondu non
        const verification = aVerification("unverified");

        // When
        const hooks = describeHooksAvailability(verification);

        // Then
        expect(hooks).toEqual({ reason: "install unavailable", enabled: false });
    });

    it("Given the first three tests passing while the command is still answering, when the hooks line is described, then it already lights up", () => {
        // Given — c'est la conséquence fonctionnelle du résultat en deux temps : « as soon
        // as tests 1–3 pass it lights up — without waiting for test 4 ». La règle est
        // calculée en Rust ; ce qui est vérifié ici est que la fenêtre l'annonce sans la
        // rejouer
        const verification = aVerification("verifying");

        // When
        const hooks = describeHooksAvailability(verification);

        // Then
        expect(hooks.enabled).toBe(true);
    });

    it("Given an entry valid with a caveat, when the hooks line is described, then ash still writes if you insist", () => {
        // Given — « the folder is right, the pair isn't. ash still writes if you insist,
        // and says so »
        const verification = aVerification("caveat");

        // When
        const hooks = describeHooksAvailability(verification);

        // Then
        expect(hooks.enabled).toBe(true);
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
