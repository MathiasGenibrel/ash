import { describe, expect, it } from "bun:test";

import type { ComposeOutcome } from "@/shared/ipc";
import { TabBuilder } from "@/shared/ipc/builders";
import type {
    PtyBridge,
    PtyFrame,
    TabId,
    TabInfo,
    TerminalSize,
    TerminalView,
} from "./ports";
import type { TabsState } from "./tabs";
import { TerminalWorkbench } from "./workbench";

/**
 * Backend de test : il tient l'ordre des onglets **et leur répertoire courant** comme le
 * registre Rust les tient.
 *
 * Le `cwd` qu'il rend n'est pas celui du lancement : c'est celui que sa sonde voit, et il
 * bouge quand `cd` bouge. Un faux backend qui rendrait toujours la même valeur ne
 * distinguerait pas « répertoire courant » de « répertoire de départ » — et laisserait
 * passer exactement les bugs qui vivent là.
 */
class FakeBackend implements PtyBridge {
    readonly opened: { tabId: TabId; cwd: string | null }[] = [];
    readonly killed: TabId[] = [];
    readonly acks: TabId[] = [];
    /** Les onglets dont l'avant-plan est tenu par autre chose que le shell. */
    readonly running = new Set<TabId>();

    private order: TabInfo[] = [];
    private frames = new Map<TabId, (frame: PtyFrame) => void>();
    private watchers: ((changed: TabInfo[]) => void)[] = [];
    private next = 1;

    open(_size: TerminalSize, cwd: string | null, onFrame: (frame: PtyFrame) => void) {
        const tabId = `T${this.next++}`;
        this.opened.push({ tabId, cwd });
        this.order.push(describe_tab(tabId, cwd ?? "/Users/me"));
        this.frames.set(tabId, onFrame);
        return Promise.resolve(tabId);
    }

    /** L'atelier ne compose pas : le geste d'ADR-0015 part du panneau des conflits. */
    compose(): Promise<ComposeOutcome> {
        return Promise.resolve("written");
    }

    write() {
        return Promise.resolve();
    }
    resize() {
        return Promise.resolve();
    }
    ack(tabId: TabId) {
        this.acks.push(tabId);
        return Promise.resolve();
    }
    close(tabId: TabId) {
        this.killed.push(tabId);
        this.forget(tabId);
        return Promise.resolve();
    }
    tabs() {
        return Promise.resolve([...this.order]);
    }
    hasForegroundProcess(tabId: TabId) {
        return Promise.resolve(this.running.has(tabId));
    }
    onTabsChanged(handler: (changed: TabInfo[]) => void) {
        this.watchers.push(handler);
        return Promise.resolve(() => {
            this.watchers = this.watchers.filter((watcher) => watcher !== handler);
        });
    }

    /**
     * L'utilisateur a fait un `cd` dans cet onglet — ou un programme y a été lancé
     * ailleurs. Le backend le sait ; personne n'en est encore prévenu.
     */
    moveTo(tabId: TabId, cwd: string): void {
        this.order = this.order.map((tab) =>
            tab.tabId === tabId ? describe_tab(tabId, cwd) : tab,
        );
    }

    /** Une passe de la boucle de sonde du backend annonce ce qui a bougé. */
    probe(): void {
        const changed = this.order.map((tab) => ({ ...tab }));
        for (const watcher of this.watchers) watcher(changed);
    }

    /** Le shell d'un onglet écrit. */
    emit(tabId: TabId, data: string): void {
        this.frames.get(tabId)?.({ kind: "chunk", data });
    }

    /** Le shell d'un onglet sort de lui-même. */
    exit(tabId: TabId): void {
        this.forget(tabId);
        this.frames.get(tabId)?.({ kind: "exit", code: 0 });
    }

    private forget(tabId: TabId): void {
        this.order = this.order.filter((tab) => tab.tabId !== tabId);
    }
}

/** Un onglet tel que le registre Rust le décrirait : situé, avec son avant-plan. */
function describe_tab(tabId: TabId, cwd: string): TabInfo {
    return TabBuilder.create().named(tabId).inFlatWorktree(cwd).build();
}

class FakeView implements TerminalView {
    size: TerminalSize = { cols: 80, rows: 24 };
    visible = false;
    cleared = 0;
    disposed = false;

    private pending: (() => void)[] = [];

    write(_data: string, done: () => void): void {
        this.pending.push(done);
    }
    onInput(): void {}
    onResize(): void {}
    clear(): void {
        this.cleared += 1;
    }
    setVisible(visible: boolean): void {
        this.visible = visible;
    }
    focus(): void {}
    dispose(): void {
        this.disposed = true;
    }

    /** Simule xterm.js qui finit de consommer ce qu'on lui a écrit. */
    finishWrites(): void {
        const done = this.pending;
        this.pending = [];
        for (const callback of done) callback();
    }
}

/** Le banc : un atelier, son backend, ses surfaces, et la réponse aux confirmations. */
function bench(options: { confirm?: boolean } = {}) {
    const backend = new FakeBackend();
    const views: FakeView[] = [];
    const asked: TabInfo[] = [];
    /** Ce que la barre d'onglets a reçu à afficher, dans l'ordre. */
    const rendered: TabsState[] = [];

    const workbench = new TerminalWorkbench({
        bridge: backend,
        createView: () => {
            const view = new FakeView();
            views.push(view);
            return view;
        },
        confirmClose: (tab) => {
            asked.push(tab);
            return Promise.resolve(options.confirm ?? false);
        },
        onRender: (state) => {
            rendered.push(state);
        },
    });

    return {
        backend,
        views,
        asked,
        rendered,
        workbench,
        // Une surface libérée a quitté le DOM : elle ne « reste » pas visible, quoi que
        // dise son dernier `setVisible`.
        visible: () => views.filter((view) => view.visible && !view.disposed).length,
    };
}

describe("un seul terminal visible", () => {
    it("Given a tab is already open, when a second one is opened, then only the new one is visible and the first is not disposed", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");

        // When
        await app.workbench.openTab("home");

        // Then — ADR-0003 : caché, pas démonté. Son shell tourne encore.
        expect(app.visible()).toBe(1);
        expect(app.views[1]?.visible).toBe(true);
        expect(app.views[0]?.disposed).toBe(false);
    });

    it("Given a hidden tab whose shell keeps writing, when its terminal finishes the write, then the chunk is still acked", async () => {
        // Given — un onglet caché qui cesse d'acquitter se fige au bout de huit morceaux
        const app = bench();
        await app.workbench.openTab("home");
        await app.workbench.openTab("home");
        const hidden = app.backend.opened[0]?.tabId ?? "";

        // When
        app.backend.emit(hidden, "sortie d'arrière-plan");
        app.views[0]?.finishWrites();

        // Then
        expect(app.backend.acks).toEqual([hidden]);
    });
});

describe("l'origine d'un nouvel onglet", () => {
    it("Given the active tab has been cd'ed elsewhere since it opened, when Cmd+T opens a tab, then the new shell starts in the directory the tab is in now", async () => {
        // Given — un `cd /tmp` dans l'onglet actif : le backend le sait, l'atelier non
        const app = bench();
        await app.workbench.openTab("home");
        const active = app.backend.opened[0]?.tabId ?? "";
        app.backend.moveTo(active, "/tmp");
        app.backend.opened.length = 0;

        // When
        await app.workbench.openTab("current-worktree");

        // Then — le répertoire *courant* de l'onglet actif, redemandé au backend au
        // moment du `Cmd+T` ; celui d'il y a une ouverture d'onglet n'a plus cours
        expect(app.backend.opened[0]?.cwd).toBe("/tmp");
    });

    it("Given a pinned worktree with no tab, when its sidebar row is clicked, then the new shell starts in that worktree and not where the active tab is", async () => {
        // Given — un onglet ouvert ailleurs : c'est lui que `⌘T` reprendrait
        const app = bench();
        await app.workbench.openTab("home");
        const active = app.backend.opened[0]?.tabId ?? "";
        app.backend.moveTo(active, "/tmp");
        app.backend.opened.length = 0;

        // When — le clic sur la ligne épinglée (spec §5.2)
        await app.workbench.openTab({ directory: "/wt/ash-sidebar" });

        // Then — la ligne dit elle-même d'où part le shell : elle n'a pas d'onglet dont
        // reprendre un répertoire, et l'onglet actif est dans un autre projet
        expect(app.backend.opened[0]?.cwd).toBe("/wt/ash-sidebar");
    });

    it("Given any active tab, when Cmd+Shift+T opens a tab, then the new shell starts at home", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");
        app.backend.opened.length = 0;

        // When
        await app.workbench.openTab("home");

        // Then — `null` est ce que le backend traduit en `~`
        expect(app.backend.opened[0]?.cwd).toBeNull();
    });
});

describe("le répertoire courant d'un onglet", () => {
    it("Given an open tab, when the backend's probe announces a cd, then the tab bar is re-rendered with the new directory without any tab being opened or closed", async () => {
        // Given — le titre d'un onglet ne doit pas attendre le prochain `Cmd+T` pour
        // suivre un `cd` : la boucle de sonde d'ADR-0005 est le seul mécanisme prévu
        const app = bench();
        await app.workbench.openTab("home");
        const tabId = app.backend.opened[0]?.tabId ?? "";
        const before = app.rendered.length;
        app.backend.moveTo(tabId, "/tmp");

        // When
        app.backend.probe();

        // Then
        expect(app.workbench.tabs.tabs.map((tab) => tab.cwd)).toEqual(["/tmp"]);
        expect(app.rendered.slice(before).at(-1)?.tabs.map((tab) => tab.cwd)).toEqual(["/tmp"]);
    });

    it("Given a tab that has not moved, when a probe pass announces its unchanged directory, then the tab bar is not re-rendered", async () => {
        // Given — la boucle passe trois fois par seconde ; reconstruire la barre à chaque
        // passe la ferait clignoter pour rien
        const app = bench();
        await app.workbench.openTab("home");
        const before = app.rendered.length;

        // When
        app.backend.probe();

        // Then
        expect(app.rendered).toHaveLength(before);
    });

    it("Given the active tab has moved, when Cmd+T opens a tab from it, then it starts in the announced directory", async () => {
        // Given — même chemin que `Cmd+T`, mais l'atelier a déjà appris le `cd`
        const app = bench();
        await app.workbench.openTab("home");
        const tabId = app.backend.opened[0]?.tabId ?? "";
        app.backend.moveTo(tabId, "/dev/ash/worktrees/probe");
        app.backend.probe();
        app.backend.opened.length = 0;

        // When
        await app.workbench.openTab("current-worktree");

        // Then
        expect(app.backend.opened[0]?.cwd).toBe("/dev/ash/worktrees/probe");
    });
});

describe("la fermeture d'un onglet", () => {
    it("Given a process is running in the tab, when the confirmation is declined, then the shell is left alone", async () => {
        // Given
        const app = bench({ confirm: false });
        await app.workbench.openTab("home");
        const tabId = app.backend.opened[0]?.tabId ?? "";
        app.backend.running.add(tabId);

        // When
        await app.workbench.closeActive();

        // Then — rien n'est détruit tant que l'utilisateur n'a pas répondu, et un refus
        // laisse l'onglet exactement comme il était
        expect(app.asked).toHaveLength(1);
        expect(app.backend.killed).toEqual([]);
        expect(app.workbench.tabs.tabs.map((tab) => tab.tabId)).toEqual([tabId]);
        expect(app.views[0]?.disposed).toBe(false);
    });

    it("Given a process is running in the tab, when the confirmation is accepted, then the shell is terminated", async () => {
        // Given
        const app = bench({ confirm: true });
        await app.workbench.openTab("home");
        const tabId = app.backend.opened[0]?.tabId ?? "";
        app.backend.running.add(tabId);

        // When
        await app.workbench.closeActive();

        // Then
        expect(app.backend.killed).toEqual([tabId]);
    });

    it("Given nothing is running in the tab, when it is closed, then no confirmation is asked", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");

        // When
        await app.workbench.closeActive();

        // Then — demander à chaque fermeture rendrait la question invisible le jour où
        // elle compte
        expect(app.asked).toEqual([]);
        expect(app.backend.killed).toHaveLength(1);
    });

    it("Given three tabs and the middle one active, when it is closed, then the tab to its right becomes the visible one", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");
        await app.workbench.openTab("home");
        await app.workbench.openTab("home");
        await app.workbench.selectAt(2);

        // When
        await app.workbench.closeActive();

        // Then
        expect(app.workbench.tabs.activeTabId).toBe(app.backend.opened[2]?.tabId ?? "");
        expect(app.visible()).toBe(1);
        expect(app.views[2]?.visible).toBe(true);
    });

    it("Given the last remaining tab, when it is closed, then no terminal is left visible", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");

        // When
        await app.workbench.closeActive();

        // Then — la fenêtre reste, vide, et `⌘T` rouvre un onglet
        expect(app.workbench.tabs.tabs).toEqual([]);
        expect(app.workbench.tabs.activeTabId).toBeNull();
        expect(app.visible()).toBe(0);
    });

    it("Given a shell that exits on its own, when its frame arrives, then its tab leaves the bar and its neighbour takes over", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");
        await app.workbench.openTab("home");
        const first = app.backend.opened[0]?.tabId ?? "";
        await app.workbench.select(first);

        // When
        app.backend.exit(first);
        await app.workbench.settled();

        // Then
        expect(app.workbench.tabs.tabs.map((tab) => tab.tabId)).toEqual([
            app.backend.opened[1]?.tabId ?? "",
        ]);
        expect(app.workbench.tabs.activeTabId).toBe(app.backend.opened[1]?.tabId ?? "");
    });
});

describe("Cmd+K", () => {
    it("Given two tabs, when the scrollback is cleared, then only the active terminal is cleared", async () => {
        // Given
        const app = bench();
        await app.workbench.openTab("home");
        await app.workbench.openTab("home");

        // When
        await app.workbench.clearActive();

        // Then
        expect(app.views[0]?.cleared).toBe(0);
        expect(app.views[1]?.cleared).toBe(1);
    });

    it("Given no tab at all, when the scrollback is cleared, then nothing happens", async () => {
        // Given
        const app = bench();

        // When
        await app.workbench.clearActive();

        // Then — ni exception, ni surface inventée pour recevoir l'effacement
        expect(app.views).toEqual([]);
    });
});
