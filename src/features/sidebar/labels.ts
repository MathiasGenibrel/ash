/**
 * Faire tenir un nom dans la colonne.
 *
 * Trois règles, et une seule question derrière les trois : à 240 px — 46 px repliée — que
 * reste-t-il d'un nom quand il n'y a plus la place de l'écrire en entier ? Elles sont ici
 * plutôt que dans [`./tree`] parce qu'elles ne connaissent rien de la hiérarchie
 * d'ADR-0012 : elles prennent une chaîne et en rendent une plus courte. Les lignes
 * d'agents, de sous-agents et d'épinglage qui viendront s'en serviront sans passer par
 * l'arbre.
 */

/** Au-delà, un nom est coupé : à 240 px, la colonne ne montre pas plus. */
export const MAX_LABEL = 26;

/** Coupe un nom trop long, en gardant le début — c'est lui qui identifie. */
export function truncate(text: string, max: number = MAX_LABEL): string {
    return text.length <= max ? text : `${text.slice(0, max - 1)}…`;
}

/**
 * Le suffixe qui distingue deux worktrees d'un même dépôt : `omelette-web` → `web`.
 *
 * Le design ne prend que le **dernier segment** du nom de dossier — c'est ce qui rend
 * `·sidebar` et `·toc` lisibles côte à côte là où `omelette-sidebar` et `omelette-toc` se
 * ressemblent trop pour être distingués du coin de l'œil.
 */
export function shortSuffix(worktreeName: string): string {
    const segments = worktreeName.split("-").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? worktreeName;
}

/** Deux lettres pour le rail replié : `omelette-web` → `ow`, `ash` → `as`. */
export function abbreviate(name: string): string {
    const segments = name.split(/[-_. ]/).filter((segment) => segment.length > 0);
    const initials = segments
        .slice(0, 2)
        .map((segment) => segment[0] ?? "")
        .join("");
    return (initials.length === 2 ? initials : name.slice(0, 2)).toLowerCase();
}

/** Dernier segment d'un chemin — `/` reste `/`. */
export function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
