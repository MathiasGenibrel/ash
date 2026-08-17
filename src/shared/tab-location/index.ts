import type { TabInfo } from "@/shared/ipc";

/**
 * Comment on **nomme** l'endroit où travaille un onglet.
 *
 * C'est la ligne que la sidebar montrerait tout en haut de la pile de cet onglet : le
 * **dépôt** quand il y en a un, sinon le worktree — la forme à plat d'
 * [ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) —, sinon le dernier
 * segment du répertoire, pour un onglet que le backend n'a pas su situer.
 *
 * Dans `shared/` parce que deux consommateurs en ont besoin, et qu'aucune règle propre à
 * l'un ne s'y glisse : la bande de titre de la fenêtre (`app/window-title.ts`) et le rappel
 * de la ligne de statut sidebar repliée (`features/terminal/status-line.ts`). La règle
 * vivait dans la barre d'onglets, qui la portait pour les deux ; la barre est partie, la
 * règle est restée.
 *
 * Rien n'est deviné ici : la localisation vient du backend, qui seul la résout
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). On ne choisit que le nom
 * à écrire.
 */
export function locationLabel(tab: TabInfo): string {
    return tab.location?.repo?.name ?? tab.location?.worktreeName ?? basename(tab.cwd);
}

/** Dernier segment d'un chemin — `~` reste `~`, `/` reste `/`. */
function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
