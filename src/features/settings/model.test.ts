import { describe, expect, it } from "bun:test";

import type {
    HooksReport,
    ToolDeclaration,
    ToolDraft,
    Verification,
    VerificationState,
} from "./contract";
import {
    countProblems,
    degradedFixSubject,
    degradedModeSubject,
    describeAddAction,
    describeDuplicates,
    describeReset,
    describeStop,
    describeTool,
    describeToolCount,
    NOTHING_VERIFIED_YET,
    parseDiff,
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
        lastValidConfig: null,
        resetFrom: null,
        duplicates: [],
        hooks: aHooksReport(),
        ...overrides,
        verification,
        verified: overrides.verified ?? verification.allowsHooks,
    };
}

/** Une ligne `hooks` posée et à jour — l'état nominal, dont on ne surcharge que le reste. */
function aHooksReport(overrides: Partial<HooksReport> = {}): HooksReport {
    return {
        state: "installed",
        summary: "installed · v1",
        note: "remove deletes the block and its markers.",
        file: "/home/someone/.claude/settings.json",
        action: "remove",
        enabled: true,
        diff: null,
        backup: "/home/someone/.claude/settings.json.bak",
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

describe("le chiffre que l'en-tête et la colonne montrent ensemble", () => {
    it("Given a list whose entries are unverified, valid with a caveat and invalid, when its problems are counted, then only the invalid one counts", () => {
        // Given — la colonne de navigation et l'en-tête de section montrent le même chiffre
        // au même instant (maquette `3e`). Une réserve n'en est pas un : ash y écrit quand
        // même. Comptés séparément, les deux finiraient par ne plus dire la même chose
        const tools = [
            aTool({ command: "kimi", verification: aVerification("unverified") }),
            aTool({ command: "codex", verification: aVerification("caveat") }),
            aTool({ command: "claude", verification: aVerification("invalid") }),
        ];

        // When
        const problems = countProblems(tools);

        // Then
        expect([problems, describeToolCount(tools)]).toEqual([1, "3 declared · 1 invalid"]);
    });
});

describe("où la chaîne s'est arrêtée", () => {
    it("Given an invalid entry, when its test line is described, then it names the test the sequence stopped at", () => {
        // Given — « l'erreur nomme le test échoué » : c'est le numéro qui désigne la chose à
        // corriger
        const verification = aVerification("invalid", { stoppedAt: 2 });

        // When
        const stop = describeStop(verification);

        // Then
        expect(stop).toBe("stopped at test 2");
    });

    it("Given an entry valid with a caveat, when its test line is described, then it says nothing about stopping", () => {
        // Given — la séquence pose `stoppedAt` sur une réserve aussi, et son résumé dit déjà
        // ce qui manque. Répéter `stopped at test 3` à côté ferait lire un échec là où le
        // dossier a été reconnu
        const verification = aVerification("caveat", { stoppedAt: 3 });

        // When
        const stop = describeStop(verification);

        // Then
        expect(stop).toBeNull();
    });
});

describe("la saisie que rien n'a encore jugée", () => {
    it("Given a form whose tests have not answered, when its test line is drawn, then nothing it shows authorises a write", () => {
        // Given — c'est la vue qui fabriquait cette vérification, hors de portée de tout
        // test. `allowsHooks` y est faux comme partout ailleurs : une saisie que rien n'a
        // jugée n'autorise jamais ash à écrire chez l'utilisateur
        // When
        const shown = NOTHING_VERIFIED_YET;

        // Then
        expect([shown.state, shown.allowsHooks, shown.tests]).toEqual([
            "unverified",
            false,
            ["pending", "pending", "pending", "pending"],
        ]);
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

describe("le doublon de dossier", () => {
    it("Given two entries aiming at the same folder, when the section is described, then the banner names both of them", () => {
        // Given — « le doublon est signalé sur les deux lignes, pas seulement sur celle
        // qu'on vient de toucher » (spec §9.1). Le registre a posé le drapeau sur chacune
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({ command: "claude-perso", duplicates: ["claude"] }),
        ];

        // When
        const banner = describeDuplicates(tools);

        // Then
        expect(banner?.sentence).toBe(
            "claude and claude-perso point at the same folder — one of them will do nothing",
        );
    });

    it("Given a duplicate a reset produced, when the banner is described, then undoing that reset is offered", () => {
        // Given — « ash n'empêche rien, il refuse seulement de poser deux fois les hooks
        // dans le même fichier, et laisse rétablir à portée »
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({
                command: "claude-perso",
                duplicates: ["claude"],
                resetFrom: "~/.claude-perso",
            }),
        ];

        // When
        const banner = describeDuplicates(tools);

        // Then
        expect(banner?.undo).toBe("claude-perso");
    });

    it("Given a duplicate nobody reset, when the banner is described, then it offers no undo", () => {
        // Given — deux entrées peuvent collisionner sans qu'aucun geste ne l'ait causé.
        // Proposer « annuler la réinitialisation » ferait alors chercher laquelle a eu lieu
        const tools = [
            aTool({ command: "claude", duplicates: ["claude-perso"] }),
            aTool({ command: "claude-perso", duplicates: ["claude"] }),
        ];

        // When
        const banner = describeDuplicates(tools);

        // Then
        expect(banner?.undo).toBeNull();
    });

    it("Given a list where nothing collides, when the banner is described, then there is none", () => {
        // Given
        const tools = [aTool({ command: "claude" }), aTool({ command: "codex" })];

        // When / Then
        expect(describeDuplicates(tools)).toBeNull();
    });
});

describe("la réinitialisation d'une entrée", () => {
    it("Given an entry that never passed the four tests, when its reset button is described, then it stays visible, off, with its reason", () => {
        // Given — « réinitialiser ramène à la dernière valeur valide » (spec §9.1) : sans
        // mémoire, il n'y a nulle part où revenir, et la même règle que les hooks s'applique
        const tool = aTool({ lastValidConfig: null, config: "~/dev/notes" });

        // When
        const reset = describeReset(tool);

        // Then
        expect(reset).toEqual({ reason: "no verified folder to go back to yet", enabled: false });
    });

    it("Given an entry that moved away from a folder that worked, when its reset button is described, then it names that folder", () => {
        // Given — c'est **son** dossier, pas le défaut de son adaptateur : deux entrées
        // `claude-code` qui reviendraient au même défaut deviendraient identiques
        const tool = aTool({ lastValidConfig: "~/.claude-perso", config: "~/dev/notes" });

        // When
        const reset = describeReset(tool);

        // Then
        expect(reset).toEqual({ reason: "back to ~/.claude-perso", enabled: true });
    });

    it("Given an entry already sitting on the folder that worked, when its reset button is described, then there is nothing to do", () => {
        // Given — un geste qui ne changerait rien doit se lire comme tel avant d'être tenté
        const tool = aTool({ lastValidConfig: "~/.claude", config: "~/.claude" });

        // When
        const reset = describeReset(tool);

        // Then
        expect(reset.enabled).toBe(false);
    });
});

describe("le diff d'un conflit", () => {
    it("Given the diff the backend produced, when it is parsed, then each line carries its side and its header is dropped", () => {
        // Given — la première décision de l'écran de conflit : reconnaître un préfixe. Elle
        // est ici parce que la vue n'est pas sous test, et qu'un diff lu à l'envers est la
        // seule faute qu'un diff ne pardonne pas
        const diff = [
            "--- ce qu'Ash écrirait",
            "+++ ce que le fichier porte",
            "    \"hooks\": {",
            "-     \"Stop\": \"ash-event waiting\"",
            "+     \"Stop\": \"mon script\"",
        ].join("\n");

        // When
        const lines = parseDiff(diff);

        // Then
        expect(lines).toEqual([
            { kind: "context", text: '  "hooks": {' },
            { kind: "removed", text: '     "Stop": "ash-event waiting"' },
            { kind: "added", text: '     "Stop": "mon script"' },
        ]);
    });
});

describe("le mode dégradé, dit avant qu'on l'applique", () => {
    it("Given an invalid entry whose suggested fix switches to generic, when the card is drawn, then the warning is shown before apply is pressed", () => {
        // Given — « generic est un mode dégradé, et l'écran le dit **avant** qu'on
        // l'applique : l'outil apparaîtra en idle / done / error, jamais en waiting »
        const tool = aTool({
            command: "claude",
            verification: aVerification("invalid", {
                fix: {
                    question: "use the generic adapter instead?",
                    apply: { kind: "useAdapter", adapter: "generic" },
                },
            }),
        });

        // When
        const subject = degradedFixSubject(tool);

        // Then
        expect(subject).toBe("claude");
    });

    it("Given a fix that only repoints the folder, when the card is drawn, then nothing is degraded and nothing is said", () => {
        // Given — un conseil générique là où rien ne change de mode ferait douter du seul
        // avertissement qui compte
        const tool = aTool({
            verification: aVerification("invalid", {
                fix: {
                    question: "use the adapter default ~/.claude instead?",
                    apply: { kind: "useFolder", path: "~/.claude" },
                },
            }),
        });

        // When / Then
        expect(degradedFixSubject(tool)).toBeNull();
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
