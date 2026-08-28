import { describe, expect, it } from "bun:test";

import { releaseNotesFor } from "./release-notes";

describe("releaseNotesFor", () => {
    it("Given a changelog whose section is followed by an older one, when asking for its notes, then only its own body comes out", () => {
        // Given
        const changelog = [
            "# Changelog",
            "",
            "## [1.2.0] - 2026-08-28",
            "",
            "### Ajouté",
            "",
            "- Les notes de release sortent du CHANGELOG.",
            "",
            "## [1.1.0] - 2026-08-01",
            "",
            "### Corrigé",
            "",
            "- Une autre version, qui ne doit pas déborder.",
        ].join("\n");
        // When
        const notes = releaseNotesFor(changelog, "1.2.0");
        // Then
        expect(notes).toEqual({
            ok: true,
            body: "### Ajouté\n\n- Les notes de release sortent du CHANGELOG.",
        });
    });

    it("Given a changelog whose last section is the one asked for, when asking for its notes, then the body runs to the end of the file", () => {
        // Given
        const changelog = ["# Changelog", "", "## [0.1.0]", "", "### Ajouté", "", "- Le début.", ""].join(
            "\n",
        );
        // When
        const notes = releaseNotesFor(changelog, "0.1.0");
        // Then
        expect(notes).toEqual({ ok: true, body: "### Ajouté\n\n- Le début." });
    });

    it("Given a changelog without the asked version, when asking for its notes, then it fails by naming the missing section", () => {
        // Given
        const changelog = ["# Changelog", "", "## [1.1.0]", "", "- Rien pour 1.2.0."].join("\n");
        // When
        const notes = releaseNotesFor(changelog, "1.2.0");
        // Then
        expect(notes).toEqual({
            ok: false,
            message: "CHANGELOG.md : aucune section [1.2.0]",
        });
    });

    it("Given a section that holds nothing but blank lines, when asking for its notes, then it fails instead of returning an empty string", () => {
        // Given
        const changelog = ["# Changelog", "", "## [1.2.0]", "", "   ", "", "## [1.1.0]", "", "- Le reste."].join(
            "\n",
        );
        // When
        const notes = releaseNotesFor(changelog, "1.2.0");
        // Then
        expect(notes).toEqual({
            ok: false,
            message: "CHANGELOG.md : la section [1.2.0] est vide",
        });
    });

    it("Given the tag rather than the bare version, when asking for its notes, then the same section comes out", () => {
        // Given
        const changelog = ["# Changelog", "", "## [1.2.0]", "", "- Une entrée."].join("\n");
        // When
        const notes = releaseNotesFor(changelog, "v1.2.0");
        // Then
        expect(notes).toEqual({ ok: true, body: "- Une entrée." });
    });

    it("Given something that is neither a version nor a tag, when asking for its notes, then it blames the argument and not the changelog", () => {
        // Given
        const changelog = ["# Changelog", "", "## [1.2.0]", "", "- Une entrée."].join("\n");
        // When
        const notes = releaseNotesFor(changelog, "release-1.2");
        // Then
        expect(notes).toEqual({
            ok: false,
            message: "« release-1.2 » : format attendu X.Y.Z ou vX.Y.Z",
        });
    });

    it("Given a section whose last entry is a level three heading, when asking for its notes, then the heading does not close the section", () => {
        // Given
        const changelog = [
            "## [1.2.0]",
            "",
            "### Ajouté",
            "",
            "- Une entrée.",
            "",
            "### Corrigé",
            "",
            "- Une autre.",
        ].join("\n");
        // When
        const notes = releaseNotesFor(changelog, "1.2.0");
        // Then
        expect(notes.ok && notes.body.includes("### Corrigé")).toBe(true);
    });
});
