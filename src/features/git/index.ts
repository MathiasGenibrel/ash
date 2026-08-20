/**
 * API publique de la feature git côté fenêtre.
 *
 * **Elle ne porte aujourd'hui que la fiche de branche** (#31, spec §7.5,
 * [ADR-0013](../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md)). La popup de
 * branches (#25), le graphe (#27), le tableau des worktrees (#28) et l'onglet de merge
 * (#30) sont en vol dans d'autres worktrees, et se poseront dans ce même dossier : ce
 * fichier est **volontairement minimal**, pour qu'une fusion n'ait que des lignes à
 * ajouter.
 *
 * Le reste du frontend n'importe que ce fichier : ni `card`, ni `markdown`, ni `tag` ne sont
 * des points d'entrée.
 */

export { mountBranchCard, view as branchCardView, type BranchCardPorts, type BranchCardView } from "./card";
export { markdown, progressOf, readCard, type CardContent, type Meta, type TaskProgress } from "./markdown";
