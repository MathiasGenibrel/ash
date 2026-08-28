import { describe, expect, it } from "bun:test";

import {
    TARGET,
    artifactName,
    bundlePath,
    eventBinaryPath,
    productNameFrom,
} from "./artifact";

describe("artifactName", () => {
    it("Given a tag as git writes it, when naming the archive, then the leading v does not survive in the file name", () => {
        // Given
        const tag = "v1.2.0";
        // When
        const name = artifactName("Ash", tag);
        // Then
        expect(name).toBe("Ash-1.2.0-macos-arm64.zip");
    });

    it("Given the bare version and the tag that carries it, when naming the archive, then both name the same file", () => {
        // Given
        const bare = "1.2.0";
        const tag = "v1.2.0";
        // When
        const fromBare = artifactName("Ash", bare);
        const fromTag = artifactName("Ash", tag);
        // Then
        expect(fromBare).toBe(fromTag);
    });

    it("Given a reference that is not a version, when naming the archive, then it refuses instead of naming a file after it", () => {
        // Given
        const branch = "ci/release-workflow";
        // When
        const name = artifactName("Ash", branch);
        // Then
        expect(name).toBeNull();
    });

    it("Given a target no one has mapped to an architecture, when naming the archive, then it refuses rather than guessing a label", () => {
        // Given
        const target = "x86_64-apple-darwin";
        // When
        const name = artifactName("Ash", "v1.2.0", target);
        // Then
        expect(name).toBeNull();
    });
});

describe("bundlePath", () => {
    it("Given a build asked for an explicit target, when locating the bundle, then the triple is part of the path", () => {
        // Given
        const target = TARGET;
        // When
        const path = bundlePath("Ash", target);
        // Then
        expect(path).toBe("src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Ash.app");
    });
});

describe("eventBinaryPath", () => {
    it("Given the bundle of a build, when locating ash-event, then it sits next to the application inside MacOS", () => {
        // Given
        const target = TARGET;
        // When
        const path = eventBinaryPath("Ash", target);
        // Then
        expect(path).toBe(`${bundlePath("Ash", target)}/Contents/MacOS/ash-event`);
    });
});

describe("productNameFrom", () => {
    it("Given the Tauri configuration of the repository, when reading the product name, then it is the one the bundle will carry", () => {
        // Given
        const conf = JSON.stringify({ productName: "Ash", identifier: "com.mg-studio.ash" });
        // When
        const name = productNameFrom(conf);
        // Then
        expect(name).toBe("Ash");
    });

    it("Given a configuration that declares no product name, when reading it, then it refuses rather than falling back to a hardcoded Ash", () => {
        // Given
        const conf = JSON.stringify({ identifier: "com.mg-studio.ash" });
        // When
        const name = productNameFrom(conf);
        // Then
        expect(name).toBeNull();
    });
});
