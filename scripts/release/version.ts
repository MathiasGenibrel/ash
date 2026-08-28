/**
 * La version d'Ash est décidée à un seul endroit : `src-tauri/Cargo.toml`.
 *
 * `src-tauri/tauri.conf.json` ne déclare plus de champ `version` — Tauri v2 retombe alors
 * sur celle du `Cargo.toml` (schéma de configuration : « If removed the version number from
 * `Cargo.toml` is used »), qui devient sur macOS le `CFBundleShortVersionString` du bundle.
 * Le `package.json`, lui, n'est plus lu par aucun build : il n'est que **vérifié**, parce
 * qu'un dépôt dont les deux fichiers divergent finit par publier un numéro qui ne veut rien
 * dire.
 *
 * Ce fichier sépare la décision de l'effet : les fonctions pures prennent le *contenu* des
 * fichiers et rendent un verdict, la CLI en dessous lit le disque, imprime et choisit le
 * code de sortie.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/**
 * Les deux fichiers, nommés une seule fois : le message de discordance et la lecture sur
 * disque désignent alors forcément le même, et un renommage ne peut pas rendre un message
 * qui mentirait sur le fichier fautif.
 */
const CARGO_TOML = "src-tauri/Cargo.toml";
const PACKAGE_JSON = "package.json";

export type VersionCheck =
    | { readonly ok: true; readonly version: string }
    | { readonly ok: false; readonly message: string };

export interface VersionSources {
    /** Le tag tel qu'il est écrit dans git, avec son `v` — par exemple `v1.2.0`. */
    readonly tag: string;
    /** Le contenu de `src-tauri/Cargo.toml`. */
    readonly cargoToml: string;
    /** Le contenu de `package.json`. */
    readonly packageJson: string;
}

/**
 * La forme d'un numéro de version, écrite **une seule fois** : les deux lectures ci-dessous
 * la demandent, et personne d'autre ne la reconnaît à sa façon.
 */
const VERSION = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

/**
 * Le tag porte un `v`, les fichiers non. Une entrée qui ne ressemble pas à `vX.Y.Z` est un
 * échec explicite : comparer silencieusement `1.2` ou `release-1.2.0` laisserait passer un
 * tag mal formé jusqu'au bundle.
 */
export function versionFromTag(tag: string): string | null {
    const bare = tag.startsWith("v") ? tag.slice(1) : null;
    return bare !== null && VERSION.test(bare) ? bare : null;
}

/**
 * Le `v` est ici **facultatif** : `1.2.0` comme `v1.2.0` nomment la même version. C'est ce
 * qui permet à un appelant — la CI, qui n'a que `$GITHUB_REF_NAME` — de passer le tag tel
 * quel à tous les scripts de release plutôt que d'en retirer le `v` lui-même, ce qui ferait
 * de son `${TAG#v}` une seconde définition de la forme d'un tag.
 */
export function versionOf(input: string): string | null {
    return input.startsWith("v") ? versionFromTag(input) : VERSION.test(input) ? input : null;
}

/**
 * La version du `[package]`, et elle seule : un `version = "1.0"` sous `[dependencies.…]`
 * n'est pas la version de l'application.
 */
export function versionFromCargoToml(text: string): string | null {
    let inPackage = false;
    for (const rawLine of text.split("\n")) {
        const line = rawLine.trim();
        if (line.startsWith("[")) {
            inPackage = line === "[package]";
            continue;
        }
        if (!inPackage) continue;
        const matched = /^version\s*=\s*"([^"]*)"/.exec(line);
        if (matched?.[1] !== undefined) return matched[1];
    }
    return null;
}

export function versionFromPackageJson(text: string): string | null {
    let parsed: unknown;
    try {
        parsed = JSON.parse(text);
    } catch {
        return null;
    }
    if (typeof parsed !== "object" || parsed === null) return null;
    const version = (parsed as Record<string, unknown>)["version"];
    return typeof version === "string" ? version : null;
}

/**
 * Le message de discordance nomme le fichier fautif **et** les deux valeurs : « les versions
 * ne concordent pas » n'apprend rien à qui doit corriger.
 */
export function checkVersions(sources: VersionSources): VersionCheck {
    const expected = versionFromTag(sources.tag);
    if (expected === null) {
        return {
            ok: false,
            message: `tag « ${sources.tag} » : format attendu vX.Y.Z`,
        };
    }

    const declared: readonly (readonly [string, string | null])[] = [
        [CARGO_TOML, versionFromCargoToml(sources.cargoToml)],
        [PACKAGE_JSON, versionFromPackageJson(sources.packageJson)],
    ];

    for (const [file, version] of declared) {
        if (version === null) {
            return {
                ok: false,
                message: `${file} : aucune version lisible, le tag ${sources.tag} attend ${expected}`,
            };
        }
        if (version !== expected) {
            return {
                ok: false,
                message: `${file} déclare ${version}, le tag ${sources.tag} attend ${expected}`,
            };
        }
    }

    return { ok: true, version: expected };
}

const USAGE = "usage : bun scripts/release/version.ts --check vX.Y.Z";

if (import.meta.main) {
    const [flag, tag] = process.argv.slice(2);
    if (flag !== "--check" || tag === undefined) {
        console.error(USAGE);
        process.exit(1);
    }

    const root = fileURLToPath(new URL("../../", import.meta.url));
    const result = checkVersions({
        tag,
        cargoToml: readFileSync(`${root}${CARGO_TOML}`, "utf8"),
        packageJson: readFileSync(`${root}${PACKAGE_JSON}`, "utf8"),
    });

    if (!result.ok) {
        console.error(result.message);
        process.exit(1);
    }
    console.log(`version ${result.version} : ${CARGO_TOML} et ${PACKAGE_JSON} concordent`);
}
