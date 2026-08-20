import { describe, expect, it } from "bun:test";

import type { Tab } from "@/shared/ipc";
import type { PinnedWorktree } from "@/shared/ipc";
import { MergeTabBuilder, PinBuilder, TabBuilder } from "@/shared/ipc/builders";
import { MAX_LABEL } from "./labels";
import { buildSidebar, type SidebarGroup, type SidebarTree } from "./tree";

const build = (tabs: readonly Tab[], activeTabId: string | null = null): SidebarTree =>
    buildSidebar(tabs, {
        activeTabId,
        collapsed: new Set(),
        pinned: [],
    });

const collapsing = (tabs: readonly Tab[], ...roots: string[]): SidebarTree =>
    buildSidebar(tabs, {
        activeTabId: null,
        collapsed: new Set(roots),
        pinned: [],
    });

const pinning = (tabs: readonly Tab[], ...pinned: PinnedWorktree[]): SidebarTree =>
    buildSidebar(tabs, {
        activeTabId: null,
        collapsed: new Set(),
        pinned,
    });

const collapsingGroups = (tabs: readonly Tab[], ...keys: string[]): SidebarTree =>
    buildSidebar(tabs, {
        activeTabId: null,
        collapsed: new Set(keys),
        pinned: [],
    });

const worktreesOf = (group: SidebarGroup | undefined) =>
    group === undefined ? [] : group.kind === "repo" ? group.worktrees : [group.worktree];

describe("la hiérarchie dépôt → worktree → onglets", () => {
    it("Given two tabs in the same worktree, when the sidebar is built, then they hang under a single worktree row", () => {
        // Given
        const tabs = [
            TabBuilder.create().named("A").running("claude").inFlatWorktree("/dev/ash").build(),
            TabBuilder.create()
                .named("B")
                .running("bun", "idle")
                .inFlatWorktree("/dev/ash")
                .workingIn("/dev/ash/src")
                .build(),
        ];

        // When
        const tree = build(tabs);

        // Then — un `cd` dans un sous-dossier ne sort pas l'onglet de son worktree
        expect(tree.groups).toHaveLength(1);
        expect(worktreesOf(tree.groups[0])).toHaveLength(1);
        expect(worktreesOf(tree.groups[0])[0]?.tabs.map((tab) => tab.title)).toEqual([
            "claude",
            "bun",
        ]);
    });

    it("Given a repository without any linked worktree, when the sidebar is built, then it stays flat instead of gaining an intermediate level", () => {
        // Given — c'est le `repo: null` du backend, et rien d'autre, qui le décide
        const tabs = [TabBuilder.create().inFlatWorktree("/dev/solo").build()];

        // When
        const tree = build(tabs);

        // Then
        expect(tree.groups[0]?.kind).toBe("flat");
    });

    it("Given two worktrees of the same repository, when the sidebar is built, then they are grouped under one repository row", () => {
        // Given
        const tabs = [
            TabBuilder.create()
                .named("A")
                .inWorktree("/wt/omelette-sidebar", "omelette-web")
                .build(),
            TabBuilder.create().named("B").inWorktree("/wt/omelette-toc", "omelette-web").build(),
        ];

        // When
        const tree = build(tabs);

        // Then — l'information « ces deux dossiers sont le même projet » est exactement ce
        // qu'ADR-0012 refuse de perdre
        expect(tree.groups).toHaveLength(1);
        expect(tree.groups[0]?.kind).toBe("repo");
        expect(worktreesOf(tree.groups[0]).map((worktree) => worktree.title)).toEqual([
            "omelette-sidebar",
            "omelette-toc",
        ]);
    });

    it("Given two worktrees of the same repository, when their rows are named, then they are told apart by their folder suffix", () => {
        // Given
        const tabs = [
            TabBuilder.create()
                .named("A")
                .inWorktree("/wt/omelette-sidebar", "omelette-web")
                .build(),
            TabBuilder.create().named("B").inWorktree("/wt/omelette-toc", "omelette-web").build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(worktreesOf(tree.groups[0]).map((worktree) => worktree.suffix)).toEqual([
            "·sidebar",
            "·toc",
        ]);
    });

    it("Given two worktrees whose last segment is the same, when their suffixes collide, then the whole group falls back to full folder names", () => {
        // Given — `·sidebar` deux fois ne distinguerait plus rien, ce qui est son seul rôle
        const tabs = [
            TabBuilder.create().named("A").inWorktree("/wt/api-sidebar", "acme").build(),
            TabBuilder.create().named("B").inWorktree("/wt/web-sidebar", "acme").build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(worktreesOf(tree.groups[0]).map((worktree) => worktree.suffix)).toEqual([
            "·api-sidebar",
            "·web-sidebar",
        ]);
    });

    it("Given a flat worktree, when its row is named, then it carries no suffix", () => {
        // Given — seul sous son dépôt, il n'a personne dont se distinguer
        const tabs = [TabBuilder.create().inFlatWorktree("/dev/ash").build()];

        // When
        const tree = build(tabs);

        // Then
        expect(worktreesOf(tree.groups[0])[0]?.suffix).toBeNull();
    });

    it("Given tabs opened in three different projects, when the sidebar is built, then the groups keep the order the backend gave", () => {
        // Given — un tri alphabétique ferait sauter les lignes à chaque ouverture d'onglet
        const tabs = [
            TabBuilder.create().named("A").inFlatWorktree("/dev/zeta").build(),
            TabBuilder.create().named("B").inFlatWorktree("/dev/alpha").build(),
            TabBuilder.create().named("C").inFlatWorktree("/dev/mid").build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(worktreesOf(tree.groups[0])[0]?.title).toBe("zeta");
        expect(worktreesOf(tree.groups[1])[0]?.title).toBe("alpha");
        expect(worktreesOf(tree.groups[2])[0]?.title).toBe("mid");
    });

    it("Given a tab the backend could not locate, when the sidebar is built, then it is still shown, flat, under its directory", () => {
        // Given — un `.git` cassé n'est pas une raison de faire disparaître un onglet vivant
        const tabs = [TabBuilder.create().named("A").unlocated("/dev/broken").build()];

        // When
        const tree = build(tabs);

        // Then
        expect(tree.groups[0]?.kind).toBe("flat");
        expect(worktreesOf(tree.groups[0])[0]?.title).toBe("broken");
    });
});

describe("la migration d'un onglet qui change de dépôt", () => {
    it("Given a tab grouped under one repository, when the backend reports it in another, then it leaves the first group for the second", () => {
        // Given — la sonde a vu le `cd`, le backend a re-situé l'onglet, et la sidebar ne
        // résout rien de son côté
        const before = [
            TabBuilder.create().named("A").inWorktree("/wt/ash-main", "ash").build(),
            TabBuilder.create().named("B").inWorktree("/wt/ash-toc", "ash").build(),
        ];
        const groupedUnderAsh = build(before);

        // When — « B » est reparti dans un autre projet
        const after = [
            before[0] as Tab,
            TabBuilder.create().named("B").inFlatWorktree("/dev/omelette-web").build(),
        ];
        const migrated = build(after);

        // Then
        expect(groupedUnderAsh.groups).toHaveLength(1);
        expect(migrated.groups).toHaveLength(2);
        expect(worktreesOf(migrated.groups[0])[0]?.tabs.map((tab) => tab.tabId)).toEqual(["A"]);
        expect(migrated.groups[1]?.kind).toBe("flat");
        expect(worktreesOf(migrated.groups[1])[0]?.tabs.map((tab) => tab.tabId)).toEqual(["B"]);
    });
});

describe("le repli d'un worktree", () => {
    it("Given a worktree that is collapsed, when the sidebar is built, then its row stays and its tabs are hidden behind it", () => {
        // Given — le repli est une propriété du **worktree**, pas du dépôt (ADR-0012)
        const tabs = [
            TabBuilder.create().named("A").inWorktree("/wt/ash-main", "ash").build(),
            TabBuilder.create().named("B").inWorktree("/wt/ash-toc", "ash").build(),
        ];

        // When
        const tree = collapsing(tabs, "/wt/ash-main");

        // Then
        const worktrees = worktreesOf(tree.groups[0]);
        expect(worktrees.map((worktree) => worktree.collapsed)).toEqual([true, false]);
        expect(worktrees[0]?.tabs).toHaveLength(1);
    });

    it("Given a collapsed repository group, when the sidebar is built, then its row carries the most urgent state of every worktree below it", () => {
        // Given — spec §4.1 : un dépôt est repliable, et sa ligne est alors le seul endroit
        // qui puisse dire ce qui se passe deux niveaux plus bas
        const tabs = [
            TabBuilder.create()
                .named("A")
                .running("claude", "working")
                .inWorktree("/wt/ash-main", "ash")
                .build(),
            TabBuilder.create()
                .named("B")
                .running("codex", "waiting")
                .inWorktree("/wt/ash-toc", "ash")
                .build(),
        ];

        // When
        const tree = collapsingGroups(tabs, "repo:/dev/ash/.git");

        // Then — le groupe garde ses worktrees en mémoire ; c'est la vue qui ne les pose pas
        const group = tree.groups[0];
        expect(group?.kind === "repo" && group.collapsed).toBe(true);
        expect(group?.state).toBe("waiting");
    });
});

describe("la remontée d'état vers la ligne du dessus", () => {
    it("Given a worktree whose agent is waiting while another works, when the row bubbles its state, then it shows waiting", () => {
        // Given — `waiting` est le seul état qui demande quelque chose à l'utilisateur
        const tabs = [
            TabBuilder.create().named("A").running("claude", "working").build(),
            TabBuilder.create().named("B").running("codex", "waiting").build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(tree.groups[0]?.state).toBe("waiting");
        expect(tree.waitingCount).toBe(1);
    });
});

describe("la lisibilité à 240 px", () => {
    it("Given fifteen tabs across three projects, when the sidebar is built, then every tab is still placed and every label fits the column", () => {
        // Given — le critère « 15 onglets restent lisibles à 240 px » se protège par une
        // règle de troncature, pas par une capture d'écran
        const tabs = Array.from({ length: 15 }, (_, index) =>
            TabBuilder.create()
                .named(`T${index}`)
                .running(`agent-with-a-very-long-name-${index}`)
                .inFlatWorktree(`/dev/project-${index % 3}`)
                .build(),
        );

        // When
        const tree = build(tabs);

        // Then
        expect(tree.tabCount).toBe(15);
        expect(tree.groups).toHaveLength(3);
        const labels = tree.groups.flatMap((group) =>
            worktreesOf(group).flatMap((worktree) => worktree.tabs.map((tab) => tab.label)),
        );
        expect(labels).toHaveLength(15);
        expect(labels.every((label) => label.length <= MAX_LABEL)).toBe(true);
    });
});

describe("les worktrees épinglés (spec §5.2)", () => {
    it("Given a pinned worktree that no tab inhabits, when the sidebar is built, then it has a row under its repository", () => {
        // Given — un onglet dans un worktree du dépôt, et une épingle sur un autre, fermé
        const tabs = [TabBuilder.create().inWorktree("/wt/ash-sidebar", "ash").build()];

        // When
        const tree = pinning(tabs, PinBuilder.create("/wt/ash-toc").ofRepo("ash").build());

        // Then — un worktree existe tant qu'il a un onglet **ou** qu'il est épinglé, et les
        // deux se rangent sous le même dépôt
        expect(tree.groups).toHaveLength(1);
        const worktrees = worktreesOf(tree.groups[0]);
        expect(worktrees.map((worktree) => worktree.key)).toEqual([
            "/wt/ash-sidebar",
            "/wt/ash-toc",
        ]);
        expect(worktrees[1]?.tabs).toEqual([]);
        expect(worktrees[1]?.pinned).toBe(true);
    });

    it("Given a worktree that already hosts tabs, when it is pinned, then its row is marked without being duplicated or moved", () => {
        // Given — l'épingle posée sur une ligne déjà là
        const tabs = [
            TabBuilder.create().named("A").inWorktree("/wt/ash-sidebar", "ash").build(),
            TabBuilder.create().named("B").inWorktree("/wt/ash-toc", "ash").build(),
        ];

        // When
        const tree = pinning(tabs, PinBuilder.create("/wt/ash-toc").ofRepo("ash").build());

        // Then — deux lignes, dans l'ordre de première apparition des onglets : épingler ne
        // fait pas sauter une ligne sous les yeux de l'utilisateur
        const worktrees = worktreesOf(tree.groups[0]);
        expect(worktrees.map((worktree) => worktree.key)).toEqual([
            "/wt/ash-sidebar",
            "/wt/ash-toc",
        ]);
        expect(worktrees.map((worktree) => worktree.pinned)).toEqual([false, true]);
        expect(worktrees[1]?.tabs).toHaveLength(1);
    });

    it("Given nothing but a pinned worktree, when the sidebar is built, then the column has a row while counting no agent", () => {
        // Given — la colonne d'un démarrage : aucun onglet ouvert dans ce projet
        const tree = pinning([], PinBuilder.create("/dev/ash").build());

        // Then — la ligne est là, et l'en-tête ne prétend pas qu'un agent tourne
        expect(tree.groups).toHaveLength(1);
        expect(tree.tabCount).toBe(0);
        expect(worktreesOf(tree.groups[0])[0]?.state).toBe("idle");
    });
});

describe("l'onglet de merge dans la colonne", () => {
    it("Given a merge tab in a worktree, when the column is built, then its row carries no agent state at all", () => {
        // Given — un onglet de merge n'a pas de processus. Lui prêter le `idle` d'un shell
        // à son invite serait afficher un état qui n'a **aucune source**
        // ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
        const merge = MergeTabBuilder.create().id("M").inWorktree("/wt/ash-merge", "ash").build();

        // When
        const tree = build([merge]);

        // Then
        const row = tree.groups[0]?.kind === "repo" ? tree.groups[0].worktrees[0]?.tabs[0] : null;
        expect(row?.state).toBeNull();
        expect(row?.label).toBe("rebase feat onto main");
        expect(row?.subagents).toEqual([]);
    });

    it("Given a waiting agent beside a merge tab, when the worktree row bubbles a state, then the merge tab adds nothing to it", () => {
        // Given — la remontée ne doit voir que ce qui a une source. Un `idle` inventé pour
        // la surface d'outil ne changerait rien ici, mais il masquerait un worktree dont le
        // seul onglet est un onglet de merge : sa ligne dirait `idle` sans qu'aucun shell
        // n'existe.
        const waiting = TabBuilder.create()
            .named("A")
            .running("claude", "waiting")
            .inWorktree("/wt/ash-merge", "ash")
            .build();
        const merge = MergeTabBuilder.create().id("M").inWorktree("/wt/ash-merge", "ash").build();

        // When
        const withAgent = build([waiting, merge]);
        const alone = build([merge]);

        // Then
        const stateOf = (tree: SidebarTree): string | undefined =>
            tree.groups[0]?.kind === "repo" ? tree.groups[0].worktrees[0]?.state : undefined;
        expect(stateOf(withAgent)).toBe("waiting");
        expect(stateOf(alone)).toBe("idle");
        expect(withAgent.waitingCount).toBe(1);
    });
});
