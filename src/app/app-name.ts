import { invoke } from "@tauri-apps/api/core";

/**
 * Le nom sous lequel l'application se présente — `Ash` installée, `Ash-dev` en
 * développement.
 *
 * **Le nom vit en Rust** (`APP_NAME`, dans `src-tauri/src/lib.rs`), et ce module ne fait
 * que le chercher : c'est la même règle que pour le thème et la taille de police — le
 * frontend affiche, il ne détient pas
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ici la règle est même plus
 * facile à tenir qu'ailleurs, parce que le nom est décidé à la **compilation** par
 * `debug_assertions` : la webview ne pourrait pas le calculer, elle ne sait pas de quel
 * build elle est la webview.
 *
 * D'où la forme : **une lecture, une fois, au démarrage** — pas de `listen`, pas de
 * `subscribe`, contrairement à `theme.ts` et `font-size.ts`. Ces deux-là suivent des
 * préférences qui changent sous les doigts de l'utilisateur ; un nom d'application ne
 * change jamais pendant une session. Un signal qui n'émet jamais est une promesse qu'on
 * fait à ses abonnés et qu'on ne tient pas, et il ferait croire au prochain lecteur qu'il
 * existe un cas où le titre se renomme tout seul.
 *
 * Il vit dans `app/` et non dans `shared/ipc/` pour la même raison que `theme.ts` : ses
 * seuls lecteurs sont les deux composition roots du frontend, qui écrivent les deux bandes
 * de titre.
 */

/**
 * Ce qu'on écrit si le backend ne répond pas.
 *
 * C'est le seul littéral du nom côté TypeScript, et il ne sert que dans une fenêtre déjà
 * cassée : sans backend, il n'y aura ni onglet, ni thème, ni terminal. Le nom de
 * l'application **installée**, parce que c'est celui qui ne surprend personne — un
 * `Ash-dev` affiché par défaut ferait douter du binaire qu'on regarde, ce qui est
 * exactement ce que la séparation des deux noms cherche à éviter.
 */
export const FALLBACK_APP_NAME = "Ash";

/**
 * Le nom tel que le backend l'a sérialisé.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown`. Une chaîne vide est refusée au
 * même titre qu'un `null` : elle donnerait une bande de titre qui commence par un tiret,
 * et un `settings — ` en suspens.
 */
export function parseAppName(value: unknown): string | null {
    return typeof value === "string" && value.length > 0 ? value : null;
}

/**
 * Demande son nom au backend.
 *
 * Ne rejette jamais : un nom manquant ne doit pas empêcher une fenêtre de se monter, et le
 * repli est un mot juste dans le seul cas où il sert.
 */
export async function loadAppName(): Promise<string> {
    try {
        return parseAppName(await invoke<unknown>("app_name")) ?? FALLBACK_APP_NAME;
    } catch {
        return FALLBACK_APP_NAME;
    }
}
