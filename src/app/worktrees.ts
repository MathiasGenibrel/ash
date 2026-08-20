import { invoke } from "@tauri-apps/api/core";

import type { WorktreeRemoval, WorktreeRow } from "@/shared/ipc";

/**
 * Le tableau des worktrees, tel que le backend le compose (spec §7.3).
 *
 * Il vit dans `app/` et non dans la feature, comme `bottom-panel.ts` et `sidebar-rows.ts`, et
 * pour la même raison : la feature ne connaît aucune commande Tauri — c'est le composition
 * root qui relie.
 *
 * **Rien n'est calculé ici.** Les deux colonnes qui font l'écran — `agents now` et
 * `last worked by` — croisent les onglets, le journal d'attribution et l'état git, et ce
 * croisement appartient au backend
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * Il n'y a **aucun event** : le tableau est fermé la plupart du temps, et une vue que
 * personne ne regarde n'a pas à se redessiner à chaque écriture dans `.git`. Il est relu
 * quand il devient visible.
 */
export async function listWorktrees(): Promise<readonly WorktreeRow[]> {
    // Une lecture qui échoue rend un tableau vide plutôt que de casser le panneau : la vue
    // dit alors qu'elle n'a rien à montrer, ce qui est exactement l'état d'une fenêtre dont
    // aucun onglet ne se situe dans un dépôt.
    return await invoke<WorktreeRow[]>("git_worktrees").catch(() => []);
}

/**
 * Ce qu'une suppression de worktree emporterait — et **elle ne supprime rien** (spec §5.4).
 *
 * Il n'y a pas de commande qui supprime, ni ici ni dans le backend : ce qui revient est une
 * fiche, dont la commande est du texte à montrer
 * ([ADR-0015](../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 */
export async function worktreeRemoval(worktreeRoot: string): Promise<WorktreeRemoval | null> {
    return await invoke<WorktreeRemoval | null>("git_worktree_removal", { worktreeRoot }).catch(
        () => null,
    );
}
