import { invoke } from "@tauri-apps/api/core";

import type { CommitGraph } from "@/features/git";

/**
 * Le raccordement du graphe de commits à la commande qui le rend.
 *
 * Il vit dans `app/` et non dans la feature, comme `bottom-panel.ts` et `sidebar-column.ts` :
 * une feature ne va pas chercher Tauri, c'est le composition root qui relie.
 *
 * **Une commande, aucun event.** Le graphe est relu sur un geste — ouvrir la vue, changer
 * d'onglet, demander une fenêtre plus grande — et non poussé : un `git log` par écriture
 * observée dans `.git` relancerait un processus à chaque frappe d'un agent, pour un écran que
 * personne ne regarde la plupart du temps.
 */

/**
 * Ce que le backend a sérialisé, ou `null`.
 *
 * Ce qui traverse est du JSON, donc `unknown` : une réponse qui n'est pas un graphe — un
 * backend plus récent, un répertoire hors de tout dépôt — ne doit pas faire peindre une
 * fenêtre à moitié. Elle est alors rendue comme « rien à montrer », ce qui est exactement ce
 * qu'un répertoire hors dépôt donne.
 */
export function parseCommitGraph(value: unknown): CommitGraph | null {
    if (typeof value !== "object" || value === null) return null;
    const { rows, lanes, folded, window, hasMore } = value as Record<string, unknown>;
    if (!Array.isArray(rows) || !Array.isArray(folded)) return null;
    if (typeof lanes !== "number" || typeof window !== "number") return null;
    if (typeof hasMore !== "boolean") return null;
    return value as CommitGraph;
}

/** Demande la fenêtre du graphe d'un worktree. */
export async function readCommitGraph(
    worktreeRoot: string,
    window: number,
): Promise<CommitGraph | null> {
    return parseCommitGraph(await invoke<unknown>("git_commit_graph", { worktreeRoot, window }));
}
