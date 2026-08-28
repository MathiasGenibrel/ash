/**
 * Le nom de l'archive publiée par une release, et les chemins du bundle qu'elle contient.
 *
 * Tout ce qui est une **règle de nommage** vit ici, et nulle part ailleurs : le nom du
 * `.zip`, la cible Rust qu'on construit, l'étiquette d'architecture qui en découle, le
 * chemin du bundle produit par `tauri build`, et celui d'`ash-event` à l'intérieur. Le
 * workflow ne compose aucune de ces chaînes : il les demande.
 *
 * La raison est la même que pour `version.ts` : une règle recomposée dans une étape shell
 * est une seconde définition, hors des tests, et silencieuse quand elle diverge. Un
 * `Ash-$TAG-macos.zip` écrit dans le YAML publierait `Ash-v1.2.0-macos.zip` sans que rien
 * ne le contredise.
 *
 * Ce fichier ne redécide pas non plus la forme d'un numéro de version : il la demande à
 * `version.ts`, qui la détient. C'est ce qui laisse la CI passer `$GITHUB_REF_NAME` tel
 * quel — `v1.2.0` comme `1.2.0`.
 *
 * Comme les deux autres scripts de release : les fonctions pures prennent des valeurs et
 * rendent une chaîne ou `null`, la CLI en dessous lit le disque, imprime et choisit le code
 * de sortie.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { versionOf } from "./version";

/**
 * La seule cible construite aujourd'hui. `x86_64-apple-darwin` et la cible universelle sont
 * hors périmètre ; les ajouter, c'est une entrée de plus dans `ARCH_LABELS` et un argument
 * de plus au workflow — pas une règle à réécrire ailleurs.
 */
export const TARGET = "aarch64-apple-darwin";

/**
 * Le triplet Rust dit `aarch64` ; Apple, et donc quiconque télécharge, dit `arm64`. Le nom
 * de l'archive parle la langue de celui qui la télécharge, et la table fait la traduction
 * une fois. Une cible inconnue est refusée plutôt qu'étiquetée au jugé : mieux vaut une
 * pipeline qui s'arrête qu'une archive dont le nom ment sur la machine qui l'exécutera.
 */
const ARCH_LABELS: Readonly<Record<string, string>> = {
    "aarch64-apple-darwin": "macos-arm64",
};

/** `productName` est décidé dans ce fichier, et le bundle en porte le nom. */
const TAURI_CONF = "src-tauri/tauri.conf.json";

/**
 * `ash-event` est le client du socket de hooks (ADR-0007). L'application le cherche **à
 * côté d'elle**, dans son propre dossier `MacOS/` : absent du bundle, Ash s'installe, se
 * lance, et n'a plus aucun état d'agent — sans rien dire. D'où le chemin nommé ici et
 * vérifié par le job de build.
 */
const EVENT_BINARY = "ash-event";

export function archLabelOf(target: string): string | null {
    return ARCH_LABELS[target] ?? null;
}

/**
 * `Ash-1.2.0-macos-arm64.zip`. `asked` est le tag ou le numéro nu ; le `v` ne survit jamais
 * dans le nom du fichier.
 */
export function artifactName(
    productName: string,
    asked: string,
    target: string = TARGET,
): string | null {
    const version = versionOf(asked);
    const arch = archLabelOf(target);
    if (version === null || arch === null) return null;
    return `${productName}-${version}-${arch}.zip`;
}

/**
 * Là où `tauri build` dépose le bundle. Avec `--target`, cargo insère le triplet dans le
 * chemin — c'est ce que fait le workflow ; sans lui, la sortie est celle que le README
 * décrit pour un `bun run package` local.
 */
export function bundlePath(productName: string, target?: string): string {
    const prefix = target === undefined ? "src-tauri/target" : `src-tauri/target/${target}`;
    return `${prefix}/release/bundle/macos/${productName}.app`;
}

export function eventBinaryPath(productName: string, target?: string): string {
    return `${bundlePath(productName, target)}/Contents/MacOS/${EVENT_BINARY}`;
}

export function productNameFrom(tauriConf: string): string | null {
    let parsed: unknown;
    try {
        parsed = JSON.parse(tauriConf);
    } catch {
        return null;
    }
    if (typeof parsed !== "object" || parsed === null) return null;
    const name = (parsed as Record<string, unknown>)["productName"];
    return typeof name === "string" && name !== "" ? name : null;
}

const USAGE = [
    "usage :",
    "  bun scripts/release/artifact.ts --name vX.Y.Z   nom de l'archive",
    "  bun scripts/release/artifact.ts --bundle-path   le .app que tauri build produit",
    "  bun scripts/release/artifact.ts --event-binary  ash-event, dans ce bundle",
    "  bun scripts/release/artifact.ts --target        le triplet Rust construit",
].join("\n");

if (import.meta.main) {
    const [mode, asked] = process.argv.slice(2);
    const root = fileURLToPath(new URL("../../", import.meta.url));

    const printed = ((): string | null => {
        if (mode === "--target") return TARGET;

        const productName = productNameFrom(readFileSync(`${root}${TAURI_CONF}`, "utf8"));
        if (productName === null) {
            console.error(`${TAURI_CONF} : aucun productName lisible`);
            process.exit(1);
        }

        switch (mode) {
            case "--name":
                if (asked === undefined) break;
                return artifactName(productName, asked);
            case "--bundle-path":
                return bundlePath(productName, TARGET);
            case "--event-binary":
                return eventBinaryPath(productName, TARGET);
            default:
                break;
        }
        return null;
    })();

    if (printed === null) {
        console.error(
            mode === "--name" && asked !== undefined
                ? `« ${asked} » : format attendu X.Y.Z ou vX.Y.Z`
                : USAGE,
        );
        process.exit(1);
    }
    console.log(printed);
}
