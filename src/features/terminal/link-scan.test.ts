import { describe, expect, it } from "bun:test";

import { scanLine } from "./link-scan";

/** Les mots trouvés, sans leurs bornes — la plupart des cas ne parlent que de ça. */
function words(line: string): string[] {
    return scanLine(line).map((candidate) => candidate.text);
}

describe("scanLine", () => {
    it("Given a compiler line pointing at a file and a line number, when scanning it, then the file is the candidate and the numbers are dropped", () => {
        // Given
        const line = "error[E0308]: src/features/terminal/index.ts:120:5 mismatched types";
        // When
        const found = words(line);
        // Then
        expect(found).toContain("src/features/terminal/index.ts");
    });

    it("Given a url inside a sentence, when scanning it, then the closing bracket and the full stop stay with the sentence", () => {
        // Given
        const line = "see (https://example.com/a/b).";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["https://example.com/a/b"]);
    });

    it("Given a url whose own parenthesis is balanced, when scanning it, then it keeps it", () => {
        // Given
        const line = "https://fr.wikipedia.org/wiki/Ash_(logiciel)";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["https://fr.wikipedia.org/wiki/Ash_(logiciel)"]);
    });

    it("Given a line painted with a scheme ash never opens, when scanning it, then it is still handed over — the backend is the one that refuses", () => {
        // Given
        const line = "javascript:alert(1) file:///etc/passwd";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["javascript:alert(1)", "file:///etc/passwd"]);
    });

    it("Given ordinary english words, when scanning them, then nothing is a candidate", () => {
        // Given
        const line = "the agent finished and wrote a summary";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual([]);
    });

    it("Given a bare file name with an extension, when scanning it, then it is a candidate — a compiler prints those", () => {
        // Given
        const line = "Cargo.toml changed";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["Cargo.toml"]);
    });

    it("Given a quoted path, when scanning it, then the quotes are not part of the candidate", () => {
        // Given
        const line = `cp "src/a b.txt" /tmp`;
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["src/a", "b.txt", "/tmp"]);
    });

    it("Given a candidate, when scanning the line, then its bounds point back at the line it came from", () => {
        // Given
        const line = "  see ~/.ash/theme.json now";
        // When
        const [candidate] = scanLine(line);
        // Then
        expect(candidate).toBeDefined();
        expect(line.slice(candidate?.start ?? 0, candidate?.end ?? 0)).toBe("~/.ash/theme.json");
    });

    it("Given a line a hostile output painted with thousands of words, when scanning it, then the harvest is bounded", () => {
        // Given
        const line = Array.from({ length: 5_000 }, (_, index) => `f${index}/x`).join(" ");
        // When
        const found = scanLine(line);
        // Then
        expect(found.length).toBeLessThanOrEqual(64);
    });

    it("Given a url carrying a port, when scanning it, then the port is not read as a line number", () => {
        // Given
        const line = "listening on http://localhost:1420";
        // When
        const found = words(line);
        // Then
        expect(found).toEqual(["http://localhost:1420"]);
    });
});
