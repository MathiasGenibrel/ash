/**
 * API publique de la feature git côté fenêtre.
 *
 * Elle est **volontairement minuscule** : le jalon J5 pose quatre vues dans le corps du
 * panneau bas — le graphe (#27), ce tableau (#28), l'onglet de merge (#30) et la fiche de
 * branche (#31) —, écrites en parallèle. Ce fichier est leur point de rencontre, et le seul
 * que `app/` importe.
 *
 * Ce qui vaut ici comme ailleurs : la fenêtre **rend** ce que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et aucune de ces vues ne
 * contient de terminal ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
 */

export {
    mountWorktreeTable,
    type WorktreeTable,
    type WorktreeTablePorts,
} from "./worktree-table";
