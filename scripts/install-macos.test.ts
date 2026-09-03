/**
 * Ce que ce fichier garde, c'est la duplication assumée de `scripts/install-macos.sh`.
 *
 * Le nom de l'archive d'une release est décidé dans `scripts/release/artifact.ts`, et
 * nulle part ailleurs (#186). L'installeur, lui, est servi seul par
 * `raw.githubusercontent.com` à une machine qui n'a ni le dépôt ni bun : il ne peut pas
 * importer, donc il redit la règle. La duplication est acceptée, à une condition — que
 * personne ne puisse la faire dériver sans que ce test le dise.
 *
 * D'où la forme retenue : on **exécute** le script sur son point d'entrée `--artifact-name`
 * et on confronte sa sortie à celle d'`artifactName()`. Vérifier que le source contient
 * `macos-arm64` ne prouverait rien — ni que la version y est insérée au bon endroit, ni que
 * le `v` d'un tag disparaît, ni que le nom du produit est celui du bundle.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "bun:test";

import { artifactName, productNameFrom } from "./release/artifact";

const ROOT = fileURLToPath(new URL("../", import.meta.url));
const SCRIPT = `${ROOT}scripts/install-macos.sh`;

/** Le nom du bundle, lu là où il est décidé — jamais retapé « Ash » dans ce fichier. */
function productName(): string {
    const name = productNameFrom(readFileSync(`${ROOT}src-tauri/tauri.conf.json`, "utf8"));
    if (name === null) throw new Error("src-tauri/tauri.conf.json : aucun productName lisible");
    return name;
}

function runScript(...args: readonly string[]): { stdout: string; exitCode: number } {
    const result = Bun.spawnSync(["bash", SCRIPT, ...args]);
    return { stdout: result.stdout.toString().trim(), exitCode: result.exitCode };
}

/** Le nom que la règle du dépôt donne à l'archive. Un refus ici est un test qui ne dit rien. */
function decidedName(version: string): string {
    const name = artifactName(productName(), version);
    if (name === null) throw new Error(`artifactName a refusé « ${version} »`);
    return name;
}

/** Le corps du script : ses lignes de commentaire retirées, sa documentation ne l'est pas. */
function bodyOf(script: string): string {
    return script
        .split("\n")
        .filter((line) => !line.trimStart().startsWith("#"))
        .join("\n");
}

describe("install-macos.sh — le nom de l'archive", () => {
    it("Given a bare version, when the shell script names the archive, then it names the same file as artifactName", () => {
        // Given
        const version = "1.2.0";
        // When
        const fromShell = runScript("--artifact-name", version).stdout;
        // Then
        expect(fromShell).toBe(decidedName(version));
    });

    it("Given a tag as the releases API returns it, when the shell script names the archive, then the leading v does not survive", () => {
        // Given
        const tag = "v0.1.0";
        // When
        const fromShell = runScript("--artifact-name", tag).stdout;
        // Then
        expect(fromShell).toBe("Ash-0.1.0-macos-arm64.zip");
        expect(fromShell).toBe(decidedName(tag));
    });
});

describe("install-macos.sh — ce qu'il ne fait jamais", () => {
    const body = bodyOf(readFileSync(SCRIPT, "utf8"));

    it("Given an installer meant to run unattended, when reading its body, then it never escalates privileges nor prompts", () => {
        // Given
        const forbidden = ["sudo", "osascript", "read"];
        // When
        const found = forbidden.filter((word) => new RegExp(`\\b${word}\\b`).test(body));
        // Then
        expect(found).toEqual([]);
    });

    it("Given a bundle whose extended attributes matter, when reading its body, then it unpacks with ditto and never with unzip", () => {
        // Given
        const unpacker = /\bunzip\b/;
        // When
        const usesUnzip = unpacker.test(body);
        // Then
        expect(usesUnzip).toBe(false);
        expect(body).toContain("/usr/bin/ditto");
    });
});

describe("install-macos.sh — les codes de retour", () => {
    it("Given an option nobody defined, when running the installer, then it exits 2 without touching the network", () => {
        // Given
        const option = "--everything";
        // When
        const { exitCode } = runScript(option);
        // Then
        expect(exitCode).toBe(2);
    });

    it("Given a destination that does not exist, when running the installer, then it exits 2 instead of creating one", () => {
        // Given
        const destination = "/tmp/ash-install-does-not-exist";
        // When
        const { exitCode } = runScript("--dir", destination);
        // Then
        expect(exitCode).toBe(2);
    });
});

/**
 * Ces deux-là passent par `source` : le script ne lance `main` que s'il est exécuté ou lu
 * sur l'entrée standard (`curl … | bash`), donc le sourcer donne accès à ses fonctions sans
 * rien installer. C'est la seule façon d'exercer la remise en place sans provoquer une
 * panne réelle entre l'écartement et la pose.
 */
function sourceAndRun(script: string): { stderr: string; exitCode: number } {
    const result = Bun.spawnSync(["bash", "-c", `set -e\nsource "${SCRIPT}"\n${script}`]);
    return { stderr: result.stderr.toString(), exitCode: result.exitCode };
}

describe("install-macos.sh — la remise en place", () => {
    it("Given an install that failed after setting the previous application aside, when cleaning up, then the previous one is put back", () => {
        // Given
        const scenario = `
            dir="$(mktemp -d)"
            mkdir -p "$dir/precedent-Ash.app"
            TARGET_APP="$dir/Ash.app"
            SET_ASIDE="$dir/precedent-Ash.app"
            # When
            restore_set_aside
            # Then
            test -d "$TARGET_APP" || exit 1
            test -e "$SET_ASIDE" && exit 1
            rm -rf "$dir"
        `;
        // When
        const { exitCode, stderr } = sourceAndRun(scenario);
        // Then
        expect(exitCode).toBe(0);
        expect(stderr).toContain("remise en place");
    });

    it("Given an install that reached its end, when cleaning up, then the freshly posted application is never overwritten by the previous one", () => {
        // Given
        const scenario = `
            dir="$(mktemp -d)"
            mkdir -p "$dir/precedent-Ash.app" "$dir/Ash.app"
            printf 'neuve' > "$dir/Ash.app/marqueur"
            TARGET_APP="$dir/Ash.app"
            SET_ASIDE="$dir/precedent-Ash.app"
            # When
            restore_set_aside
            # Then
            test -f "$TARGET_APP/marqueur" || exit 1
            rm -rf "$dir"
        `;
        // When
        const { exitCode, stderr } = sourceAndRun(scenario);
        // Then
        expect(exitCode).toBe(0);
        expect(stderr).not.toContain("remise en place");
    });
});
