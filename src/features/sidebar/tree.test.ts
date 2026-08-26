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

/** Ce que la ligne unique d'un groupe à plat écrit — `null` si le groupe n'est pas à plat. */
const rowOf = (group: SidebarGroup | undefined) =>
    group !== undefined && group.kind === "flat" ? group.row : null;

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

describe("un dépôt qui n'héberge qu'un worktree", () => {
    it("Given a repository whose only worktree is its main tree, when the sidebar is built, then one row carries the repository name and its tabs sit right under it", () => {
        // Given — le cas nominal de l'usage réel : l'orchestrateur vit dans l'arbre
        // principal, et ses enfants partent en worktree sans onglet à eux
        const tabs = [
            TabBuilder.create().named("A").running("claude").inWorktree("/dev/ash", "ash").build(),
            TabBuilder.create().named("B").running("bun").inWorktree("/dev/ash", "ash").build(),
        ];

        // When
        const tree = build(tabs);

        // Then — une ligne, pas deux : `ash` → `ash ·ash` → `claude` dépensait un niveau
        // entier pour une seule vérité
        expect(tree.groups[0]?.kind).toBe("flat");
        expect(rowOf(tree.groups[0])?.label).toBe("ash");
        expect(rowOf(tree.groups[0])?.suffix).toBeNull();
        expect(worktreesOf(tree.groups[0])[0]?.tabs.map((tab) => tab.tabId)).toEqual(["A", "B"]);
    });

    it("Given a repository whose only worktree is a linked one, when its row is named, then it keeps the suffix that tells it from the repository", () => {
        // Given — le dossier du worktree n'est pas celui du dépôt : le suffixe dit lequel
        const tabs = [
            TabBuilder.create().inWorktree("/wt/backoffice", "democratic-backoffice").build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(rowOf(tree.groups[0])?.label).toBe("democratic-backoffice");
        expect(rowOf(tree.groups[0])?.suffix).toBe("·backoffice");
    });

    it("Given a directory outside any repository, when its row is named, then it still carries the folder name", () => {
        // Given — l'autre famille de la forme à plat, qui n'a aucun nom de dépôt à montrer
        const tabs = [TabBuilder.create().inFlatWorktree("/dev/solo").build()];

        // When
        const tree = build(tabs);

        // Then
        expect(rowOf(tree.groups[0])?.label).toBe("solo");
        expect(rowOf(tree.groups[0])?.suffix).toBeNull();
    });

    it("Given a repository shown flat, when a tab opens a second worktree of it, then the intermediate level comes back", () => {
        // Given — un seul worktree habité
        const before = [TabBuilder.create().named("A").inWorktree("/dev/ash", "ash").build()];

        // When — un agent part sur une branche, dans son propre dossier
        const after = [
            before[0] as Tab,
            TabBuilder.create().named("B").inWorktree("/wt/ash-sidebar", "ash").build(),
        ];
        const grown = build(after);

        // Then — le niveau revient dès qu'il porte deux vérités : deux worktrees ont deux
        // états d'arbre (ADR-0012, alternative écartée)
        expect(build(before).groups[0]?.kind).toBe("flat");
        expect(grown.groups[0]?.kind).toBe("repo");
        expect(worktreesOf(grown.groups[0]).map((worktree) => worktree.suffix)).toEqual([
            "·ash",
            "·sidebar",
        ]);
    });

    it("Given a repository with two worktrees, when one of them loses its last tab, then the column falls back to the flat form", () => {
        // Given
        const both = [
            TabBuilder.create().named("A").inWorktree("/dev/ash", "ash").build(),
            TabBuilder.create().named("B").inWorktree("/wt/ash-sidebar", "ash").build(),
        ];

        // When — l'agent de `·sidebar` a fini, son onglet est fermé, et rien ne l'épingle
        const alone = build([both[0] as Tab]);

        // Then — la bascule joue dans les deux sens, sans redémarrage
        expect(build(both).groups[0]?.kind).toBe("repo");
        expect(alone.groups[0]?.kind).toBe("flat");
        expect(rowOf(alone.groups[0])?.label).toBe("ash");
    });

    it("Given a collapsed worktree, when its repository falls back to the flat form, then the row is still collapsed", () => {
        // Given — le repli vise le **worktree** (ADR-0012), et sa clé ne change pas quand la
        // forme du groupe change : un dépôt qui gagne un second worktree perdrait sinon
        // silencieusement son état replié
        const both = [
            TabBuilder.create().named("A").inWorktree("/dev/ash", "ash").build(),
            TabBuilder.create().named("B").inWorktree("/wt/ash-sidebar", "ash").build(),
        ];

        // When
        const grouped = collapsing(both, "/dev/ash");
        const flattened = collapsing([both[0] as Tab], "/dev/ash");

        // Then
        expect(worktreesOf(grouped.groups[0])[0]?.collapsed).toBe(true);
        expect(worktreesOf(flattened.groups[0])[0]?.collapsed).toBe(true);
        expect(worktreesOf(flattened.groups[0])[0]?.key).toBe("/dev/ash");
    });

    it("Given a pinned worktree alone under its repository, when the sidebar is built, then its row stays, flat, and the pin still targets the worktree", () => {
        // Given — un worktree sans onglet n'existe dans la colonne que par son épingle
        // (spec §4.1) ; la mise à plat ne lui retire pas sa ligne
        const tree = pinning([], PinBuilder.create("/wt/ash-toc").ofRepo("ash").build());

        // When
        const worktree = worktreesOf(tree.groups[0])[0];

        // Then
        expect(tree.groups[0]?.kind).toBe("flat");
        expect(rowOf(tree.groups[0])?.label).toBe("ash");
        expect(rowOf(tree.groups[0])?.suffix).toBe("·toc");
        expect(worktree?.key).toBe("/wt/ash-toc");
        expect(worktree?.pinned).toBe(true);
    });

    it("Given a repository shown flat whose agent waits while another works, when its row bubbles a state, then it shows waiting", () => {
        // Given — la remontée est inchangée : c'est la ligne aplatie qui la porte désormais
        const tabs = [
            TabBuilder.create()
                .named("A")
                .running("claude", "working")
                .inWorktree("/dev/ash", "ash")
                .build(),
            TabBuilder.create()
                .named("B")
                .running("codex", "waiting")
                .inWorktree("/dev/ash", "ash")
                .build(),
        ];

        // When
        const tree = build(tabs);

        // Then
        expect(tree.groups[0]?.state).toBe("waiting");
        expect(worktreesOf(tree.groups[0])[0]?.state).toBe("waiting");
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
        const row = worktreesOf(tree.groups[0])[0]?.tabs[0];
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
            worktreesOf(tree.groups[0])[0]?.state;
        expect(stateOf(withAgent)).toBe("waiting");
        expect(stateOf(alone)).toBe("idle");
        expect(withAgent.waitingCount).toBe(1);
    });
});
