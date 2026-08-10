import { describe, expect, it } from "bun:test";

import { TerminalSession } from "./session";
import type { PtyBridge, PtyFrame, TabId, TabInfo, TerminalSize, TerminalView } from "./ports";

/**
 * Terminal de test : les écritures ne se terminent que lorsqu'on le décide, parce que
 * c'est précisément le moment où l'acquittement se déclenche.
 */
class FakeView implements TerminalView {
    size: TerminalSize = { cols: 80, rows: 24 };
    written: string[] = [];
    disposed = false;
    visible = false;
    cleared = 0;

    private pending: (() => void)[] = [];
    private inputHandler: ((data: string) => void) | undefined;
    private resizeHandler: ((size: TerminalSize) => void) | undefined;

    write(data: string, done: () => void): void {
        this.written.push(data);
        this.pending.push(done);
    }
    onInput(handler: (data: string) => void): void {
        this.inputHandler = handler;
    }
    onResize(handler: (size: TerminalSize) => void): void {
        this.resizeHandler = handler;
    }
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

    /** Simule xterm.js qui finit de consommer les écritures en attente. */
    finishWrites(): void {
        const done = this.pending;
        this.pending = [];
        for (const callback of done) callback();
    }
    type(data: string): void {
        this.inputHandler?.(data);
    }
    resizeTo(size: TerminalSize): void {
        this.resizeHandler?.(size);
    }
}

class FakeBridge implements PtyBridge {
    acks: TabId[] = [];
    writes: string[] = [];
    resizes: TerminalSize[] = [];
    closes: TabId[] = [];
    openedAt: string | null = null;

    private emit: ((frame: PtyFrame) => void) | undefined;

    open(
        _size: TerminalSize,
        cwd: string | null,
        onFrame: (frame: PtyFrame) => void,
    ): Promise<TabId> {
        this.emit = onFrame;
        this.openedAt = cwd;
        return Promise.resolve("01JTAB");
    }
    tabs(): Promise<TabInfo[]> {
        const cwd = this.openedAt ?? "/Users/me";
        return Promise.resolve([
            {
                tabId: "01JTAB",
                cwd,
                process: "zsh",
                state: "idle",
                location: { worktreeRoot: cwd, worktreeName: "me", repo: null },
            },
        ]);
    }
    hasForegroundProcess(): Promise<boolean> {
        return Promise.resolve(false);
    }
    onTabsChanged(): Promise<() => void> {
        // Une session ne s'intéresse pas au répertoire de son onglet : c'est l'atelier
        // qui rend la barre, et lui seul écoute la boucle de sonde.
        return Promise.resolve(() => {});
    }
    write(_tabId: TabId, data: string): Promise<void> {
        this.writes.push(data);
        return Promise.resolve();
    }
    resize(_tabId: TabId, size: TerminalSize): Promise<void> {
        this.resizes.push(size);
        return Promise.resolve();
    }
    ack(tabId: TabId): Promise<void> {
        this.acks.push(tabId);
        return Promise.resolve();
    }
    close(tabId: TabId): Promise<void> {
        this.closes.push(tabId);
        return Promise.resolve();
    }

    send(frame: PtyFrame): void {
        this.emit?.(frame);
    }
}

const chunk = (data: string): PtyFrame => ({ kind: "chunk", data });

describe("TerminalSession", () => {
    it("Given a chunk arrives, when the terminal has finished writing it, then it is acked exactly once", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        await TerminalSession.start(view, bridge);
        bridge.send(chunk("hello"));

        // When
        view.finishWrites();

        // Then
        expect(view.written).toEqual(["hello"]);
        expect(bridge.acks).toEqual(["01JTAB"]);
    });

    it("Given a chunk has been written but not consumed, when nothing else happens, then nothing is acked", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        await TerminalSession.start(view, bridge);

        // When
        bridge.send(chunk("still rendering"));

        // Then — acquitter à l'appel plutôt qu'au rappel annulerait la contre-pression
        expect(bridge.acks).toEqual([]);
    });

    it("Given the tab was closed, when a late write callback fires, then no ack reaches a tab the backend has dropped", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        const session = await TerminalSession.start(view, bridge);
        bridge.send(chunk("in flight"));

        // When
        await session.close();
        view.finishWrites();

        // Then
        expect(bridge.acks).toEqual([]);
        expect(bridge.closes).toEqual(["01JTAB"]);
    });

    it("Given the shell has exited, when a further chunk arrives, then it is neither written nor acked", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        const session = await TerminalSession.start(view, bridge);

        // When
        bridge.send({ kind: "exit", code: 0 });
        bridge.send(chunk("après la mort"));
        view.finishWrites();

        // Then
        expect(session.isClosed).toBe(true);
        expect(view.written).toEqual([]);
        expect(bridge.acks).toEqual([]);
    });

    it("Given the window is resized, when the terminal reports its new grid, then the pty is resized to match", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        await TerminalSession.start(view, bridge);

        // When
        view.resizeTo({ cols: 120, rows: 40 });

        // Then
        expect(bridge.resizes).toEqual([{ cols: 120, rows: 40 }]);
    });

    it("Given the tab is closed, when the user keeps typing, then nothing is sent to a dead shell", async () => {
        // Given
        const view = new FakeView();
        const bridge = new FakeBridge();
        const session = await TerminalSession.start(view, bridge);

        // When
        await session.close();
        view.type("ls\n");

        // Then
        expect(bridge.writes).toEqual([]);
    });
});
