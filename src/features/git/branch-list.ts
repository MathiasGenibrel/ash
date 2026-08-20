/**
 * La liste de la popup, aplatie et filtrée — **une valeur, pas du DOM**.
 *
 * Ce que ce module décide et ce qu'il ne décide **pas** est la frontière d'ADR-0009, et
 * elle est nette :
 *
 * - il ne décide **ni le groupement, ni l'ordre, ni le worktree qui détient une branche**.
 *   Ce sont des faits du dépôt, et ils arrivent déjà tranchés dans le `BranchOverview` que
 *   `features::git` rend. Les redériver ici serait fabriquer une seconde vérité à partir
 *   d'une copie ;
 * - il décide du **filtre** et de la **sélection**. Le filtre change à chaque frappe et ne
 *   demande rien au disque : le faire traverser la frontière coûterait un aller-retour
 *   Tauri par caractère tapé, pour rendre un sous-ensemble d'une liste déjà en main. La
 *   sélection, elle, est une position dans ce que l'écran montre — elle n'existe que là.
 */

import type { Branch, BranchGroup, BranchOverview } from "@/shared/ipc";

/** Une ligne de la liste : une branche, et le groupe sous lequel elle se range. */
export interface BranchRow {
    readonly branch: Branch;
    readonly group: BranchGroup;
    /** Cette ligne ouvre son groupe : c'est elle qui porte le titre. */
    readonly opensGroup: boolean;
}

/**
 * Les lignes que le filtre laisse passer, dans l'ordre que le backend a fixé.
 *
 * Le filtre est une **sous-chaîne insensible à la casse**, cherchée dans le nom de la
 * branche *et* dans celui du worktree qui la détient. Le second n'est pas un extra : la
 * popup existe pour dire « cette branche vit ailleurs », donc taper le nom d'un worktree
 * pour retrouver ce qu'il tient est exactement le geste qu'elle doit servir.
 *
 * Pas de correspondance floue, pas de score : une liste de branches se compte en dizaines,
 * et un classement par pertinence détruirait le seul ordre que la spec exige — la courante
 * en tête, jamais rangée par ordre alphabétique.
 *
 * Un groupe entièrement filtré **disparaît avec son titre** : `opensGroup` est recalculé
 * après le filtre, jamais recopié.
 */
export function visibleRows(overview: BranchOverview | null, query: string): readonly BranchRow[] {
    if (overview === null) return [];
    const needle = query.trim().toLowerCase();

    const rows: BranchRow[] = [];
    for (const section of overview.sections) {
        let first = true;
        for (const branch of section.branches) {
            if (!matches(branch, needle)) continue;
            rows.push({ branch, group: section.group, opensGroup: first });
            first = false;
        }
    }
    return rows;
}

function matches(branch: Branch, needle: string): boolean {
    if (needle === "") return true;
    if (branch.name.toLowerCase().includes(needle)) return true;
    return branch.worktree !== null && branch.worktree.name.toLowerCase().includes(needle);
}

/**
 * La ligne sélectionnée après un déplacement, **en bouclant**.
 *
 * Boucler et non buter : la liste est courte, la courante est en tête, et remonter d'un cran
 * depuis la première pour atteindre la dernière est le geste qu'on fait sans y penser dans
 * une palette. Sur une liste vide, il n'y a pas de sélection — `-1`, et rien à valider.
 */
export function moveSelection(
    rows: readonly BranchRow[],
    selected: number,
    step: number,
): number {
    if (rows.length === 0) return -1;
    const from = selected < 0 ? 0 : selected;
    return (((from + step) % rows.length) + rows.length) % rows.length;
}

/**
 * Où retomber après un changement de filtre.
 *
 * Sur la branche qui était sélectionnée si elle a survécu au filtre, sur la première sinon.
 * C'est ce qui empêche la sélection de **sauter sous les doigts** : en tapant `fea`, la
 * ligne visée doit rester visée entre `f` et `fe`, et non se déplacer parce que la liste a
 * raccourci au-dessus d'elle.
 */
export function keepSelection(rows: readonly BranchRow[], previous: Branch | null): number {
    if (rows.length === 0) return -1;
    if (previous === null) return 0;
    const still = rows.findIndex((row) => row.branch.name === previous.name);
    return still < 0 ? 0 : still;
}

/** La branche sélectionnée, ou `null` — sur une liste vide, ou avant tout choix. */
export function selectedBranch(rows: readonly BranchRow[], selected: number): Branch | null {
    return rows[selected]?.branch ?? null;
}
