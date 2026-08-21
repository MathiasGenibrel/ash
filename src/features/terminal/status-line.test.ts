import { describe, expect, it } from "bun:test";

import {
    AccountUsageBuilder,
    MergeTabBuilder,
    MetadataBuilder,
    TabBuilder,
} from "@/shared/ipc/builders";
import type { Tab, WorktreeMetadata } from "@/shared/ipc";
import { DEFAULT_STATUS_BAR_SEGMENTS, type StatusBarSegments } from "./status-bar";
import {
    composeStatusLine,
    elide,
    shownStatusGroups,
    visibilityRows,
    type StatusLineModel,
} from "./status-line";
import type { TabsState } from "./tabs";
import { composeQuotas } from "./usage";

/**
 * Un onglet actif dans un worktree, et rien d'autre : le décor de la plupart des cas.
 *
 * `now` est l'époque Unix, comme la date d'entrée par défaut du `TabBuilder` : l'onglet
 * vient donc d'entrer dans son état, et les scénarios qui ne parlent pas de durée lisent
 * `0s` sans avoir à s'en soucier.
 */
function showing(
    tab: Tab = TabBuilder.create().running("claude").inFlatWorktree("/dev/omelette-web").build(),
    metadata: WorktreeMetadata | null = MetadataBuilder.create().build(),
    sidebarCollapsed = false,
    now = 0,
): StatusLineModel {
    const state: TabsState = { tabs: [tab], activeTabId: tab.tabId };
    return composeStatusLine(state, metadata, sidebarCollapsed, now);
}

/** Ce que la ligne **dit**, segment par segment — ce qu'un utilisateur y lit. */
function words(model: StatusLineModel): string[] {
    return model.git.map((chip) => chip.text);
}

describe("la ligne de statut", () => {
    it("Given a tab on a branch with a dirty tree, when the status line is composed, then it shows the directory, the branch and the counts", () => {
        // Given — le cas de la maquette : `~/dev/omelette-web │ feat/agent-sidebar +3 ~1`
        const metadata = MetadataBuilder.create()
            .onBranch("feat/agent-sidebar")
            .withTree({ added: 3, modified: 1 })
            .build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(line.cwd.text).toBe("/dev/omelette-web");
        expect(words(line)).toEqual(["feat/agent-sidebar", "+3", "~1"]);
        expect(line.agent.text).toBe("claude · working · 0s");
    });

    it("Given a worktree whose tree is clean, when the status line is composed, then nothing is written after the branch", () => {
        // Given — un arbre propre n'a rien à dire ; `+0 ~0` serait du bruit permanent
        const metadata = MetadataBuilder.create().onBranch("main").withUpstream(0, 0).build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["main"]);
    });

    it("Given a worktree whose git status could not be read, when the status line is composed, then the absence is written instead of a clean tree", () => {
        // Given — `git` absent, trop lent, ou en échec : un cas nominal (ADR-0011), et
        // surtout **pas** un arbre propre. Afficher `main` seul mentirait.
        const metadata = MetadataBuilder.create().onBranch("main").withoutStatus().build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["main", "+? ~?"]);
    });

    it("Given a detached HEAD, when the status line is composed, then it names the commit instead of inventing a branch", () => {
        // Given — la maquette ne dessine qu'une branche ; il en existe pourtant une
        // seconde forme, et elle ne doit pas se lire comme un nom de branche
        const metadata = MetadataBuilder.create().detachedAt("a1b2c3d").build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["@a1b2c3d"]);
        expect(line.git[0]?.title).toBe("detached HEAD at a1b2c3d");
    });

    it("Given a rebase stopped on a conflict, when the status line is composed, then it keeps the branch being moved and adds where the rebase stands", () => {
        // Given — pendant un rebase `HEAD` est détaché : c'est `head-name` qui dit encore
        // sur quelle branche on travaille, et le conflit est ce qu'il faut regarder
        const metadata = MetadataBuilder.create()
            .rebasing("feat/agent-sidebar", "main", 2, 5)
            .withTree({ conflicted: 1 })
            .build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["feat/agent-sidebar", "rebasing onto main · 2/5", "!1"]);
        expect(line.git[1]?.tone).toBe("strong");
        // L'accent reste au seul état qui attend une décision — ici le conflit.
        expect(line.git[2]?.tone).toBe("accent");
    });

    it("Given a merge stopped on a conflict, when the status line is composed, then it says which branch is being merged in, not onto", () => {
        // Given — un merge ramène une branche **dans** celle où l'on est : « onto »
        // inverserait le sens de l'opération
        const metadata = MetadataBuilder.create().onBranch("main").merging("feat").build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["main", "merging feat"]);
    });

    it("Given a tab outside any repository, when the status line is composed, then it says so without pretending the tree is clean", () => {
        // Given — un onglet dans `/tmp` est un cas nominal, pas une panne
        const tab = TabBuilder.create().running("zsh", "idle").unlocated("/tmp").build();

        // When
        const line = showing(tab, null);

        // Then
        expect(words(line)).toEqual(["no repo"]);
        expect(line.cwd.text).toBe("/tmp");
        expect(line.agent.text).toBe("zsh · idle");
    });

    it("Given no tab at all, when the status line is composed, then the whole line reads as empty", () => {
        // Given — le bloc `1d` : `~ │ no repo │ no agents`, tout en `faint`
        const state: TabsState = { tabs: [], activeTabId: null };

        // When
        const line = composeStatusLine(state, null, false, 0);

        // Then
        expect([line.cwd.text, ...words(line), line.agent.text]).toEqual([
            "~",
            "no repo",
            "no agents",
        ]);
        expect(line.agent.state).toBeNull();
        expect([line.cwd.tone, line.agent.tone]).toEqual(["faint", "faint"]);
        // Rien à rappeler quand il n'y a rien à faire.
        expect(line.hint).toBeNull();
    });

    it("Given an upstream the branch is ahead of, when the status line is composed, then the divergence is shown", () => {
        // Given
        const metadata = MetadataBuilder.create().onBranch("main").withUpstream(2, 1).build();

        // When
        const line = showing(undefined, metadata);

        // Then
        expect(words(line)).toEqual(["main", "↑2", "↓1"]);
    });
});

describe("la durée de l'état courant", () => {
    it("Given an agent that has been working for a quarter of an hour, when the status line is composed, then it reads the elapsed time from the entry date", () => {
        // Given — le `working · 15m22s` de la maquette. Le backend n'envoie que la **date
        // d'entrée** ; la durée est un fait d'affichage, recalculé à chaque rendu.
        const tab = TabBuilder.create().running("claude").since(1_000_000).build();

        // When — 15 min 22 s plus tard
        const line = showing(tab, undefined, false, 1_000_000 + (15 * 60 + 22) * 1000);

        // Then
        expect(line.agent.text).toBe("claude · working · 15m22s");
    });

    it("Given an agent waiting for less than a minute, when the status line is composed, then only the seconds are shown", () => {
        // Given — sous la minute, écrire `0m45s` ferait lire un zéro pour rien : la ligne
        // fait 25 px et partage sa largeur avec un chemin et un état git.
        const tab = TabBuilder.create().running("claude", "waiting").since(0).build();

        // When
        const line = showing(tab, undefined, false, 45_000);

        // Then
        expect(line.agent.text).toBe("claude · waiting · 45s");
    });

    it("Given an agent that has been working for more than an hour, when the status line is composed, then the seconds give way to the hours", () => {
        // Given — passé l'heure, la seconde n'apprend plus rien et coûte deux caractères.
        const tab = TabBuilder.create().running("claude").since(0).build();

        // When — 2 h 05 min 09 s
        const line = showing(tab, undefined, false, ((2 * 60 + 5) * 60 + 9) * 1000);

        // Then
        expect(line.agent.text).toBe("claude · working · 2h05m");
    });

    it("Given a shell sitting at its prompt, when the status line is composed, then no counter runs on it", () => {
        // Given — `idle` n'est pas une activité : chronométrer un shell vide ferait tourner
        // un compteur là où il n'y a rien à lire.
        const tab = TabBuilder.create().running("zsh", "idle").since(0).build();

        // When — une heure à l'invite
        const line = showing(tab, undefined, false, 3_600_000);

        // Then
        expect(line.agent.text).toBe("zsh · idle");
    });

    it("Given an entry date that is ahead of the display clock, when the status line is composed, then no negative duration is ever shown", () => {
        // Given — le backend date avec l'horloge murale, qui peut reculer : changement de
        // fuseau, recalage `ntp`. Écrire `-3s` serait pire que de ne rien écrire.
        const tab = TabBuilder.create().running("claude").since(10_000).build();

        // When
        const line = showing(tab, undefined, false, 7_000);

        // Then
        expect(line.agent.text).toBe("claude · working");
    });
});

describe("le rappel de droite", () => {
    it("Given an expanded sidebar, when the status line is composed, then it carries no hint at all", () => {
        // Given / When — dépliée, la sidebar porte déjà les agents : les répéter serait du
        // bruit
        const line = showing();

        // Then
        expect(line.hint).toBeNull();
    });

    it("Given a collapsed sidebar and an agent that is waiting, when the status line is composed, then it names the waiting agent and its shortcut", () => {
        // Given — le rail de 46 px ne nomme plus les agents : c'est ce rappel qui rend
        // `⌘B` supportable (bloc `1b`)
        const shell = TabBuilder.create().named("T1").running("zsh", "idle").build();
        const codex = TabBuilder.create()
            .named("T2")
            .running("codex", "waiting")
            .inWorktree("/dev/ash-core", "ash-core")
            .build();
        const state: TabsState = { tabs: [shell, codex], activeTabId: "T1" };

        // When
        const line = composeStatusLine(state, MetadataBuilder.create().build(), true, 0);

        // Then
        expect(line.hint?.text).toBe("1 waiting · ash-core/codex ⌘2");
        expect(line.hint?.tone).toBe("accent");
    });

    it("Given a collapsed sidebar and nobody waiting, when the status line is composed, then it promises no shortcut", () => {
        // Given / When
        const line = showing(undefined, undefined, true);

        // Then — aucune palette de commandes n'existe, et `⌘K` efface le scrollback depuis
        // #159 : annoncer `⌘K commands` promettrait une surface qui n'est pas là.
        expect(line.hint).toBeNull();
    });
});

describe("le répertoire courant", () => {
    it("Given a path longer than the line can hold, when it is shown, then its end is kept", () => {
        // Given — c'est la fin d'un chemin qui dit où l'on est ; garder le début
        // afficherait `/Users/mathias/Doc…` sur toutes les lignes du monde
        const path = "/Users/mathias/dev/omelette-web/src/features/sidebar";

        // When
        const shown = elide(path, 20);

        // Then
        expect(shown).toBe("…rc/features/sidebar");
        expect(shown.length).toBeLessThanOrEqual(20);
    });
});

describe("la ligne de statut d'un onglet sans PTY", () => {
    it("Given a merge tab, when the status line is composed, then it names the operation and times nothing", () => {
        // Given — un onglet de merge n'a ni processus ni état d'agent. Un `idle · 12m` y
        // serait la durée d'un état qui n'existe pas — c'est précisément ce que le typage
        // des onglets rend impossible (ADR-0003, ADR-0007).
        const merge = MergeTabBuilder.create().inFlatWorktree("/dev/ash").build();

        // When — une heure plus tard qu'à l'ouverture, pour qu'un compteur se verrait
        const line = showing(merge, MetadataBuilder.create().build(), false, 3_600_000);

        // Then
        expect(line.agent.state).toBeNull();
        expect(line.agent.text).toBe("rebase feat onto main");
        expect(line.cwd.text).toContain("ash");
    });
});

describe("la jauge de contexte de la ligne", () => {
    it("Given two tabs whose conversations differ, when the selected one changes, then the line shows the context of the tab in front", () => {
        // Given — la jauge suit **l'onglet** : elle arrive avec sa fiche, et c'est ce qui la
        // sépare des deux quotas du compte, qu'un changement d'onglet ne touche pas.
        const light = TabBuilder.create().named("T1").running("claude").consuming(82_000).build();
        const full = TabBuilder.create().named("T2").running("claude").consuming(184_000).build();
        const tabs = [light, full];
        const metadata = MetadataBuilder.create().build();

        // When
        const first = composeStatusLine({ tabs, activeTabId: "T1" }, metadata, false, 0);
        const second = composeStatusLine({ tabs, activeTabId: "T2" }, metadata, false, 0);

        // Then
        expect(first.context?.label).toBe("ctx 41%");
        expect(second.context?.label).toBe("ctx 92%");
        expect(second.context?.share?.level).toBe("compacting");
    });

    it("Given a shell at its prompt, when the status line is composed, then it carries nothing more than it did before the gauge existed", () => {
        // Given — aucun outil reconnu, donc aucun transcript : pas de jauge à zéro, pas de
        // `ctx —`. La ligne doit rester celle d'aujourd'hui, au pixel.
        const shell = TabBuilder.create().running("zsh", "idle").build();

        // When
        const line = showing(shell);

        // Then
        expect(line.context).toBeNull();
        expect(line.agent.text).toBe("zsh · idle");
    });
});

describe("ce que la ligne montre, et ce que le menu en dit", () => {
    /** Le décor de la maquette : un agent qui travaille dans un worktree sale. */
    function line(): StatusLineModel {
        const tab = TabBuilder.create()
            .running("claude")
            .consuming(82_000, 200_000, "Opus 5 1M")
            .inFlatWorktree("/dev/omelette-web")
            .build();
        const metadata = MetadataBuilder.create()
            .onBranch("feat/agent-sidebar")
            .withTree({ added: 3, modified: 1 })
            .build();
        return composeStatusLine({ tabs: [tab], activeTabId: tab.tabId }, metadata, false, 0);
    }

    function shownWords(segments: StatusBarSegments): string[] {
        return shownStatusGroups(line(), segments).flatMap((group) =>
            group.chips.map((chip) => chip.text),
        );
    }

    it("Given every segment shown, when the groups are composed, then the line reads as it always has", () => {
        // Given / When
        const groups = shownStatusGroups(line(), DEFAULT_STATUS_BAR_SEGMENTS);

        // Then — trois groupes, donc deux `│`, et le glyphe d'état sur le seul qui en porte un
        expect(groups.map((group) => group.chips[0]?.text)).toEqual([
            "/dev/omelette-web",
            "feat/agent-sidebar",
            "claude · working · 0s",
        ]);
        expect(groups.map((group) => group.glyph)).toEqual([null, null, "working"]);
    });

    it("Given a hidden cwd, when the groups are composed, then the line opens on the branch instead of on a separator", () => {
        // Given — le trait tombe **entre** deux groupes montrés ; un `cwd` décoché qui
        // laisserait le sien ferait s'ouvrir la ligne sur un `│` orphelin
        const segments = { ...DEFAULT_STATUS_BAR_SEGMENTS, cwd: false };

        // When
        const groups = shownStatusGroups(line(), segments);

        // Then
        expect(groups.length).toBe(2);
        expect(groups[0]?.chips[0]?.text).toBe("feat/agent-sidebar");
    });

    it("Given a hidden context bar, when the groups are composed, then what says where we are stays in place", () => {
        // Given — le scénario de la tâche : décocher la jauge ne touche ni le `cwd`, ni la
        // branche, ni l'état de l'agent. Les deux moitiés de la ligne sont indépendantes
        const segments = { ...DEFAULT_STATUS_BAR_SEGMENTS, context: false };

        // When / Then
        expect(shownWords(segments)).toEqual(shownWords(DEFAULT_STATUS_BAR_SEGMENTS));
    });

    it("Given every segment hidden, when the groups are composed, then nothing is drawn rather than a row of separators", () => {
        // Given — la légende de la vue 5c est formelle : chaque élément de la barre se coupe
        const nothing: StatusBarSegments = {
            session: false,
            weekly: false,
            context: false,
            model: false,
            agent: false,
            branch: false,
            cwd: false,
        };

        // When / Then
        expect(shownStatusGroups(line(), nothing)).toEqual([]);
    });

    it("Given an open menu, when its rows are composed, then each preview is the value the bar shows right now", () => {
        // Given — les aperçus ne sont pas des exemples figés : `63% · 2h14` est la vraie
        // valeur du quota de session à l'instant où le menu s'ouvre
        const quotas = composeQuotas(new AccountUsageBuilder().build(), 1_787_241_600_000);

        // When
        const rows = visibilityRows(DEFAULT_STATUS_BAR_SEGMENTS, line(), quotas);

        // Then
        expect(rows.map((row) => [row.name, row.preview])).toEqual([
            ["session", "63% · 2h14"],
            ["weekly", "28% · 2d 17h"],
            ["context bar", "41%"],
            ["model", "Opus 5 1M"],
            ["agent state", "working"],
            ["branch", "feat/agent-sidebar +3 ~1"],
            ["cwd", "/dev/omelette-web"],
        ]);
        expect(rows.map((row) => row.shown)).toEqual([true, false, true, true, true, true, true]);
    });

    it("Given a tab whose tool says nothing, when the menu rows are composed, then the previews are empty instead of dashed", () => {
        // Given — ni quota, ni jauge, ni modèle : trois absences qu'ADR-0016 interdit de
        // maquiller. Un tiret dirait qu'on attend une valeur qui n'existe pas
        const shell = TabBuilder.create().running("zsh", "idle").build();
        const bare = composeStatusLine({ tabs: [shell], activeTabId: shell.tabId }, null, false, 0);

        // When
        const rows = visibilityRows(DEFAULT_STATUS_BAR_SEGMENTS, bare, []);

        // Then
        const previews = new Map(rows.map((row) => [row.id, row.preview]));
        expect(previews.get("session")).toBe("");
        expect(previews.get("weekly")).toBe("");
        expect(previews.get("context")).toBe("");
        expect(previews.get("model")).toBe("");
        // Ce que la ligne montre quand même, elle, se lit dans le menu comme dans la barre.
        expect(previews.get("branch")).toBe("no repo");
        expect(previews.get("agent")).toBe("idle");
    });

    it("Given the seven segments, when the menu is composed, then the rule falls between the context bar and the agent state", () => {
        // Given / When — l'ordre de la maquette, et son seul trait : d'un côté ce que la
        // conversation consomme, de l'autre où l'on est et ce que l'agent fait
        const rows = visibilityRows(DEFAULT_STATUS_BAR_SEGMENTS, line(), []);

        // Then
        expect(rows.filter((row) => row.separated).map((row) => row.id)).toEqual(["agent"]);
    });
});
