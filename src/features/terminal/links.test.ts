import { describe, expect, it } from "bun:test";

import { TerminalLinks, type LinkBridge, type LinkLine } from "./links";
import type { ILink } from "@xterm/xterm";

/**
 * Un backend qui reconnaît ce qu'on lui a dit de reconnaître, et qui **retient** ses
 * réponses jusqu'à ce qu'un test les libère.
 *
 * La retenue est le cœur du sujet : le critère dit qu'un candidat pas encore vérifié reste
 * du texte, et il n'y a pas d'autre façon de le mettre en scène.
 */
class FakeLinks implements LinkBridge {
    readonly asked: { cwd: string; candidates: string[] }[] = [];
    readonly opened: { cwd: string; candidate: string }[] = [];
    private readonly known: Set<string>;
    private answer: (() => void) | null = null;

    constructor(known: string[] = []) {
        this.known = new Set(known);
    }

    openable(cwd: string, candidates: string[]): Promise<string[]> {
        this.asked.push({ cwd, candidates });
        const recognised = candidates.filter((candidate) => this.known.has(candidate));
        return new Promise((resolve) => {
            this.answer = () => {
                resolve(recognised);
            };
        });
    }

    open(cwd: string, candidate: string): Promise<void> {
        this.opened.push({ cwd, candidate });
        return Promise.resolve();
    }

    /** Le backend répond, et le rendu apprend. */
    async settle(): Promise<void> {
        this.answer?.();
        this.answer = null;
        await Promise.resolve();
        await Promise.resolve();
    }
}

interface Built {
    links: TerminalLinks;
    bridge: FakeLinks;
    cwd: { current: string };
}

function build(line: string, known: string[] = []): Built {
    const bridge = new FakeLinks(known);
    const cwd = { current: "/dev/ash" };
    const held: LinkLine = { text: line, startRow: 7, cols: 80 };
    const links = new TerminalLinks({
        bridge,
        cwd: () => cwd.current,
        lines: () => held,
    });
    return { links, bridge, cwd };
}

function provided(links: TerminalLinks): ILink[] {
    let given: ILink[] | undefined;
    links.provider.provideLinks(7, (found) => {
        given = found;
    });
    return given ?? [];
}

/** Un clic, avec ou sans `Cmd` — il n'y a pas de DOM ici, et il n'en faut pas. */
function click(meta: boolean): MouseEvent {
    return { metaKey: meta, preventDefault: () => undefined } as unknown as MouseEvent;
}

describe("TerminalLinks", () => {
    it("Given cmd is not held, when the mouse moves over a path, then nothing is underlined and the backend is never asked", () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        // When
        const [link] = provided(links);
        link?.hover?.(click(false), link.text);
        // Then
        expect(link?.decorations).toEqual({ pointerCursor: false, underline: false });
        expect(bridge.asked).toEqual([]);
    });

    it("Given cmd is held over a path the backend recognises, when the answer comes back, then the link lights up under the mouse", async () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        // When
        expect(link?.decorations).toEqual({ pointerCursor: false, underline: false });
        await bridge.settle();
        // Then
        expect(link?.decorations).toEqual({ pointerCursor: true, underline: true });
    });

    it("Given a word that looks like a path but the backend does not recognise, when it is hovered under cmd, then it stays text and the click opens nothing", async () => {
        // Given
        const { links, bridge } = build("see src/gone.rs now", []);
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        await bridge.settle();
        // When
        link?.activate(click(true), link.text);
        // Then
        expect(link?.decorations).toEqual({ pointerCursor: false, underline: false });
        expect(bridge.opened).toEqual([]);
        expect(links.claimsTheClick).toBe(false);
    });

    it("Given a link lit under cmd and a still mouse, when cmd is released, then the decorations go out", async () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        await bridge.settle();
        expect(link?.decorations).toEqual({ pointerCursor: true, underline: true });
        // When
        links.setCmdHeld(false);
        // Then
        expect(link?.decorations).toEqual({ pointerCursor: false, underline: false });
        expect(links.claimsTheClick).toBe(false);
    });

    it("Given a recognised link, when it is clicked without cmd, then nothing is opened", async () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        await bridge.settle();
        // When
        link?.activate(click(false), link.text);
        // Then
        expect(bridge.opened).toEqual([]);
    });

    it("Given a recognised link, when it is clicked with cmd, then the word goes back to the backend — never a resolved path", async () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        await bridge.settle();
        // When
        link?.activate(click(true), link.text);
        // Then
        expect(bridge.opened).toEqual([{ cwd: "/dev/ash", candidate: "src/main.rs" }]);
        expect(links.claimsTheClick).toBe(true);
    });

    it("Given the tab changed directory since it was opened, when a relative path is hovered, then it is verified against the current cwd", () => {
        // Given
        const { links, bridge, cwd } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        // When
        cwd.current = "/dev/other";
        provided(links);
        // Then
        expect(bridge.asked).toEqual([{ cwd: "/dev/other", candidates: ["src/main.rs"] }]);
    });

    it("Given the same word in two different directories, when both are hovered, then each is asked for on its own", async () => {
        // Given
        const { links, bridge, cwd } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        provided(links);
        await bridge.settle();
        // When
        cwd.current = "/dev/other";
        provided(links);
        // Then
        expect(bridge.asked.length).toBe(2);
    });

    it("Given a line the backend has already answered for, when it is hovered again, then it is not asked twice", async () => {
        // Given
        const { links, bridge } = build("see src/main.rs now", ["src/main.rs"]);
        links.setCmdHeld(true);
        provided(links);
        await bridge.settle();
        // When
        provided(links);
        // Then
        expect(bridge.asked.length).toBe(1);
    });

    it("Given a tab whose directory has a space in its name, when a path is hovered under cmd, then it is still recognised", async () => {
        // Given
        const { links, bridge, cwd } = build("see src/main.rs now", ["src/main.rs"]);
        cwd.current = "/Users/moi/Mes projets";
        links.setCmdHeld(true);
        const [link] = provided(links);
        link?.hover?.(click(true), link.text);
        // When
        await bridge.settle();
        // Then
        expect(bridge.asked).toEqual([
            { cwd: "/Users/moi/Mes projets", candidates: ["src/main.rs"] },
        ]);
        expect(link?.decorations).toEqual({ pointerCursor: true, underline: true });
    });

    it("Given a session that has hovered far more words than the cache holds, when the oldest is hovered again, then it has been forgotten and is asked for anew", async () => {
        // Given — la mémoire du survol est bornée, comme tout le reste de la fonctionnalité
        const bridge = new FakeLinks([]);
        let line: LinkLine = { text: "a/0", startRow: 7, cols: 80 };
        const links = new TerminalLinks({
            bridge,
            cwd: () => "/dev/ash",
            lines: () => line,
        });
        links.setCmdHeld(true);
        provided(links);
        await bridge.settle();
        // When — une longue session, mot après mot
        for (let index = 1; index <= 4096; index += 1) {
            line = { text: `a/${index}`, startRow: 7, cols: 80 };
            provided(links);
            await bridge.settle();
        }
        const before = bridge.asked.length;
        line = { text: "a/0", startRow: 7, cols: 80 };
        provided(links);
        // Then
        expect(bridge.asked.length).toBe(before + 1);
        expect(bridge.asked.at(-1)).toEqual({ cwd: "/dev/ash", candidates: ["a/0"] });
    });

    it("Given a candidate that the terminal wrapped onto the next row, when it is provided, then its range spans both rows", () => {
        // Given — 80 colonnes, et le mot commence à la colonne 78
        const bridge = new FakeLinks(["/tmp/x"]);
        const links = new TerminalLinks({
            bridge,
            cwd: () => "/dev/ash",
            lines: () => ({ text: `${" ".repeat(77)}/tmp/x`, startRow: 7, cols: 80 }),
        });
        // When
        const [link] = provided(links);
        // Then
        expect(link?.range).toEqual({ start: { x: 78, y: 7 }, end: { x: 3, y: 8 } });
    });
});
