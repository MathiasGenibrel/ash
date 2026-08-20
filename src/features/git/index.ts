/**
 * API publique de la feature git côté fenêtre.
 *
 * Elle porte aujourd'hui **le graphe de commits** (#27, spec §7.2), et elle en portera
 * d'autres : la popup de branches (#25), le tableau des worktrees (#28), l'onglet de merge
 * (#30), la fiche de branche (#31). Le reste du frontend n'importe que ce fichier.
 *
 * **Cette feature ne détient aucun état du produit** : les couloirs, la colonne `by` et le
 * repli des branches inactives sont décidés en Rust
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et ce qu'elle garde — la
 * ligne dont le détail est ouvert, la fenêtre déjà demandée — sont des faits d'affichage.
 *
 * **Elle ne lance aucune action git.** Le graphe lit, et rien de plus : aucun verbe ne touche
 * l'arbre depuis cet écran.
 */

export type { CommitGraph, CommitRow, FoldedBranch, GraphLink } from "./contract";
export { mountCommitGraph, type CommitGraphPanel, type CommitGraphPorts } from "./graph";
export { WINDOW_STEP } from "./graph-view";
