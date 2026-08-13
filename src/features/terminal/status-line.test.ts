import { describe, expect, it } from "bun:test";

import { MetadataBuilder, TabBuilder } from "@/shared/ipc/builders";
import type { WorktreeMetadata } from "@/shared/ipc";
import { composeStatusLine, elide, type StatusLineModel } from "./status-line";
import type { TabsState } from "./tabs";

/**
 * Un onglet actif dans un worktree, et rien d'autre : le décor de la plupart des cas.
 *
 * `now` est l'époque Unix, comme la date d'entrée par défaut du `TabBuilder` : l'onglet
 * vient donc d'entrer dans son état, et les scénarios qui ne parlent pas de durée lisent
 * `0s` sans avoir à s'en soucier.
 */
function showing(
    tab = TabBuilder.create().running("claude").inFlatWorktree("/dev/omelette-web").build(),
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
    it("Given an expanded sidebar, when the status line is composed, then it only carries the command hint", () => {
        // Given / When — dépliée, la sidebar porte déjà les agents : les répéter serait du
        // bruit
        const line = showing();

        // Then
        expect(line.hint?.text).toBe("⌘K commands");
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

    it("Given a collapsed sidebar and nobody waiting, when the status line is composed, then it falls back to the command hint", () => {
        // Given / When
        const line = showing(undefined, undefined, true);

        // Then
        expect(line.hint?.text).toBe("⌘K commands");
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
