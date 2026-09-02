import { describe, expect, it } from "bun:test";

import { aDraft, aShortcut, aSnapshot, aSuggestion, aTool, aVerification } from "./builders";
import {
    ADAPTER_DEFAULT,
    countProblems,
    degradedFixSubject,
    degradedModeSubject,
    describeAddAction,
    describeDuplicates,
    describeReset,
    describeStop,
    describeTool,
    describeToolCount,
    captureIntent,
    emptyToolsProse,
    focusedDraft,
    groupShortcuts,
    needsVerifying,
    NOTHING_VERIFIED_YET,
    parseDiff,
    pendingSuggestions,
    readStroke,
} from "./model";

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
        expect(heading.config).toBe(ADAPTER_DEFAULT);
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
        const action = describeAddAction(
            draft,
            [aTool({ command: "claude" })],
            null,
            aVerification(),
        );

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
        const action = describeAddAction(draft, [], "claude is already declared", aVerification());

        // Then
        expect(action.reason).toBe("name the command first");
    });

    it("Given a backend refusal and nothing wrong with the draft, when the action bar is described, then it shows the refusal and still lets you try again", () => {
        // Given — un refus que le frontend ne sait pas prévoir (le registre a changé sous
        // lui) : le masquer perdrait la seule explication, éteindre `add` interdirait de
        // réessayer
        const draft = aDraft({ command: "codex" });

        // When
        const action = describeAddAction(draft, [], "tool registry poisoned", aVerification());

        // Then
        expect(action).toEqual({ reason: "tool registry poisoned", enabled: true });
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
            '    "hooks": {',
            '-     "Stop": "ash-event waiting"',
            '+     "Stop": "mon script"',
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

describe("les raccourcis groupés", () => {
    it("Given shortcuts from two submenus in the menu's order, when they are grouped, then the groups keep that order", () => {
        // Given — l'ordre est celui du menu natif, et c'est tout l'intérêt : on retrouve un
        // raccourci dans l'écran là où on l'a vu dans le menu. Trier ici donnerait à cette
        // fenêtre un second avis sur une question dont le menu a déjà décidé
        const declared = [
            aShortcut({ group: "application", label: "Settings…" }),
            aShortcut({ group: "terminal", label: "New Tab" }),
            aShortcut({ group: "view", label: "Toggle Sidebar" }),
        ];

        // When
        const grouped = groupShortcuts(declared);

        // Then
        expect(grouped.map((one) => one.group)).toEqual(["application", "terminal", "view"]);
    });

    it("Given a group the backend sends twice, when they are grouped, then the second batch joins the first instead of opening a twin", () => {
        // Given — deux titres identiques dans une liste se lisent comme un bug d'affichage
        const declared = [
            aShortcut({ group: "terminal", label: "New Tab" }),
            aShortcut({ group: "view", label: "Toggle Sidebar" }),
            aShortcut({ group: "terminal", label: "Close Tab" }),
        ];

        // When
        const grouped = groupShortcuts(declared);

        // Then
        expect(grouped.map((one) => one.group)).toEqual(["terminal", "view"]);
        expect(grouped[0]?.shortcuts.map((one) => one.label)).toEqual(["New Tab", "Close Tab"]);
    });
});

describe("le bloc de capture d'une combinaison", () => {
    it("Given the three keys the plate gives the capture, when each is pressed, then each one is its own way out", () => {
        // Given — `esc` annule, `⌫` retire le raccourci, `⏎` confirme. Le bloc consomme
        // **toutes** les frappes tant qu'il est ouvert, donc se tromper d'issue signifie ne
        // plus pouvoir en sortir : c'est exactement ce qui ne s'essaie pas à la main
        const issues = [{ key: "Escape" }, { key: "Backspace" }, { key: "Enter" }];

        // When
        const intents = issues.map(captureIntent);

        // Then
        expect(intents).toEqual(["cancel", "clear", "confirm"]);
    });

    it("Given a modifier held down before the real key, when its own keydown arrives, then nothing is read from it", () => {
        // Given — on tient `⌘` avant de frapper la lettre, et chacun de ces `keydown` arrive
        // au bloc. Les traiter comme une frappe ferait clignoter un refus entre le moment où
        // l'on presse le modificateur et celui où l'on presse la touche
        const held = [{ key: "Meta" }, { key: "Shift" }, { key: "Alt" }, { key: "Control" }];

        // When
        const intents = held.map(captureIntent);

        // Then
        expect(intents).toEqual(["ignore", "ignore", "ignore", "ignore"]);
    });

    it("Given a key whose character and position disagree, when the stroke is read, then both are reported and the character comes first", () => {
        // Given — la touche marquée `W` d'un AZERTY est à la position `KeyZ` d'un clavier
        // US. macOS apparie un équivalent clavier par **caractère** : envoyer la seule
        // position posait `⌘Z` sur une touche qui joue `⌘W`, et l'action devenait
        // injoignable (issue #133). Rien n'est jugé ici — c'est le backend qui tranche
        const pressed = {
            key: "w",
            code: "KeyZ",
            metaKey: true,
            ctrlKey: false,
            altKey: false,
            shiftKey: false,
        };

        // When
        const stroke = readStroke(pressed);

        // Then — les deux faits traversent la frontière, et le caractère en tête
        expect(captureIntent({ key: "w" })).toBe("stroke");
        expect(stroke).toEqual({
            key: "w",
            code: "KeyZ",
            command: true,
            control: false,
            option: false,
            shift: false,
        });
    });
});

describe("l'outil que la sidebar désigne", () => {
    it("Given a recognized tool ash does not know yet, when the sidebar points at it, then the add form opens filled in without writing anything", () => {
        // Given — le geste du marqueur « non instrumenté » (ADR-0006) mène au flux
        // d'ajout qui existe déjà : vérification, puis bouton. Rien n'est écrit d'ici là
        const focused = { command: "claude", adapter: "claude-code" };

        // When
        const prefilled = focusedDraft(focused, aSnapshot());

        // Then — la commande et l'adaptateur viennent du geste ; le dossier reste vide ici,
        // et c'est l'écran qui le propose ensuite, pour cet adaptateur (ADR-0006)
        expect(prefilled).toEqual({
            command: "claude",
            label: "",
            adapter: "claude-code",
            config: "",
        });
    });

    it("Given a tool already declared, when the sidebar points at it, then no second entry is proposed", () => {
        // Given — sa carte est déjà là, avec sa ligne `hooks` et son bouton. Une saisie de
        // plus montrerait deux fois le même outil, dont une que l'ajout refuserait
        const snapshot = aSnapshot({ tools: [aTool({ command: "claude" })] });

        // When
        const prefilled = focusedDraft({ command: "claude", adapter: "claude-code" }, snapshot);

        // Then — et c'est ce `null` qui fait qu'aucun dossier n'est même demandé au
        // backend : pas de saisie à remplir, donc pas de lecture de disque pour la remplir
        expect(prefilled).toBeNull();
    });

    it("Given an adapter this build does not embed, when the sidebar points at a tool, then the form falls back to one it offers", () => {
        // Given — `claude-code` disparaît de la liste quand `ash-event` est introuvable :
        // un menu posé sur une valeur qu'il ne contient pas n'afficherait rien de vrai
        const snapshot = aSnapshot({ adapters: ["generic"] });

        // When
        const prefilled = focusedDraft({ command: "claude", adapter: "claude-code" }, snapshot);

        // Then
        expect(prefilled?.adapter).toBe("generic");
    });
});

describe("ce que la fenêtre relance en s'ouvrant", () => {
    it("Given a list read back from ~/.ash/tools.json, when the window asks whether it must verify, then it says yes", () => {
        // Given — au redémarrage, la déclaration revient du fichier sans ce qu'elle avait
        // prouvé : la vérification est un fait daté sur la machine (ADR-0007). Sans cette
        // relance, la ligne `hooks` d'un outil instrumenté depuis des mois resterait éteinte
        // jusqu'à ce que quelqu'un pense à cliquer `re-verify all`
        const restored = [
            aTool({ command: "claude", verification: aVerification("unverified") }),
            aTool({ command: "claude-perso", verification: aVerification("valid") }),
        ];

        // When
        const relaunch = needsVerifying(restored);

        // Then
        expect(relaunch).toBe(true);
    });

    it("Given a list every entry of which has already been judged, when the window asks whether it must verify, then nothing is relaunched", () => {
        // Given — le test 4 **lance une commande** : la relancer sur une liste que la
        // séquence vient de juger ferait partir un processus par entrée pour un verdict
        // qu'on a déjà sous les yeux
        const judged = [
            aTool({ command: "claude", verification: aVerification("valid") }),
            aTool({ command: "kimi", verification: aVerification("invalid") }),
        ];

        // When
        const relaunch = needsVerifying(judged);

        // Then
        expect(relaunch).toBe(false);
    });
});

describe("ce qu'ash a vu tourner", () => {
    it("Given a tool ash saw running that no one declared, when the section filters what it shows, then it stays proposed", () => {
        // Given — la fenêtre ouvrait sur « no tools declared » pendant qu'ash savait très
        // bien que `claude` tenait l'avant-plan d'un onglet (ADR-0006)
        const suggestions = [aSuggestion({ command: "claude" })];

        // When
        const shown = pendingSuggestions(suggestions, []);

        // Then
        expect(shown.map((one) => one.command)).toEqual(["claude"]);
    });

    it("Given a tool that has just been declared, when the list comes back before the suggestions do, then it is not shown twice", () => {
        // Given — les deux valeurs n'arrivent pas par le même aller-retour : la carte est
        // déjà là quand la suggestion l'est encore. Sans cette garde, déclarer laisserait le
        // même outil deux fois à l'écran, dont une sous un geste que le backend refuserait
        const suggestions = [aSuggestion({ command: "claude" }), aSuggestion({ command: "codex" })];

        // When
        const shown = pendingSuggestions(suggestions, [aTool({ command: "claude" })]);

        // Then
        expect(shown.map((one) => one.command)).toEqual(["codex"]);
    });

    it("Given nothing declared but a tool seen running, when the empty state speaks, then it names what ash saw instead of only what is missing", () => {
        // Given — « no tools declared » reste vrai et devient trompeur : il fallait deviner
        // qu'on passait par le marqueur de la sidebar pour déclarer un outil
        const suggestions = [aSuggestion({ command: "claude" })];

        // When
        const prose = emptyToolsProse(suggestions);

        // Then — ce qu'ash a vu, et la promesse que le clic ne pose rien (ADR-0007)
        expect(prose).toContain("claude");
        expect(prose).toContain("writes nothing");
    });

    it("Given nothing seen running, when the empty state speaks, then it keeps saying what the emptiness costs", () => {
        // Given — la machine où aucun agent n'a jamais été lancé. Inventer une phrase sur ce
        // qu'ash aurait vu serait un mensonge, et il n'a rien à proposer
        // When
        const prose = emptyToolsProse([]);

        // Then — `null` : l'appelant garde la phrase d'origine
        expect(prose).toBeNull();
    });
});
