import { describe, expect, it } from "bun:test";

import { checkVersions, versionFromCargoToml } from "./version";

/** Un `Cargo.toml` réduit à ce qui compte ici : une section `[package]`, et une autre après. */
function cargoTomlWith(version: string): string {
    return [
        "[package]",
        'name = "ash"',
        `version = "${version}"`,
        'edition = "2021"',
        "",
        "[dependencies]",
        'tauri = { version = "2.9.2" }',
    ].join("\n");
}

function packageJsonWith(version: string): string {
    return JSON.stringify({ name: "ash", private: true, version }, null, 2);
}

describe("checkVersions", () => {
    it("Given a tag whose version both files declare, when checking, then it agrees and names the version", () => {
        // Given
        const sources = {
            tag: "v1.2.0",
            cargoToml: cargoTomlWith("1.2.0"),
            packageJson: packageJsonWith("1.2.0"),
        };
        // When
        const result = checkVersions(sources);
        // Then
        expect(result).toEqual({ ok: true, version: "1.2.0" });
    });

    it("Given a package.json left behind at 1.1.0, when checking a v1.2.0 tag, then it names that file and both values", () => {
        // Given
        const sources = {
            tag: "v1.2.0",
            cargoToml: cargoTomlWith("1.2.0"),
            packageJson: packageJsonWith("1.1.0"),
        };
        // When
        const result = checkVersions(sources);
        // Then
        expect(result.ok).toBe(false);
        expect(result.ok ? "" : result.message).toBe(
            "package.json déclare 1.1.0, le tag v1.2.0 attend 1.2.0",
        );
    });

    it("Given a Cargo.toml left behind at 1.1.0, when checking a v1.2.0 tag, then it names that file and both values", () => {
        // Given
        const sources = {
            tag: "v1.2.0",
            cargoToml: cargoTomlWith("1.1.0"),
            packageJson: packageJsonWith("1.2.0"),
        };
        // When
        const result = checkVersions(sources);
        // Then
        expect(result.ok).toBe(false);
        expect(result.ok ? "" : result.message).toBe(
            "src-tauri/Cargo.toml déclare 1.1.0, le tag v1.2.0 attend 1.2.0",
        );
    });

    it("Given a tag that is not shaped like vX.Y.Z, when checking, then it refuses instead of comparing", () => {
        // Given
        const sources = {
            tag: "release-1.2",
            cargoToml: cargoTomlWith("1.2.0"),
            packageJson: packageJsonWith("1.2.0"),
        };
        // When
        const result = checkVersions(sources);
        // Then
        expect(result.ok).toBe(false);
        expect(result.ok ? "" : result.message).toBe("tag « release-1.2 » : format attendu vX.Y.Z");
    });

    it("Given a package.json without any version field, when checking, then it says the version is unreadable", () => {
        // Given
        const sources = {
            tag: "v1.2.0",
            cargoToml: cargoTomlWith("1.2.0"),
            packageJson: JSON.stringify({ name: "ash" }),
        };
        // When
        const result = checkVersions(sources);
        // Then
        expect(result.ok).toBe(false);
        expect(result.ok ? "" : result.message).toBe(
            "package.json : aucune version lisible, le tag v1.2.0 attend 1.2.0",
        );
    });
});

describe("versionFromCargoToml", () => {
    it("Given a dependency that declares a version before the package section, when reading, then the package version wins", () => {
        // Given
        const cargoToml = [
            "[dependencies]",
            'tauri = { version = "2.9.2" }',
            "",
            "[package]",
            'version = "0.1.0"',
        ].join("\n");
        // When
        const version = versionFromCargoToml(cargoToml);
        // Then
        expect(version).toBe("0.1.0");
    });
});
