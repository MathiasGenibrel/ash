import { type UiChild, type UiElementNode, type UiNode, toNode } from "./node";

/**
 * Lire une description — ce que fait un test, et parfois une vue.
 *
 * Sans ces deux fonctions, chaque test réécrirait sa propre descente d'arbre, et
 * `expect(node.children[1]?.children[0])` finirait par vérifier une position plutôt qu'un
 * comportement. Elles n'appellent rien du DOM : elles parcourent une valeur.
 */

/** Tout le texte d'une description, dans l'ordre — ce qu'un œil y lirait. */
export function plainText(child: UiChild): string {
    const node = toNode(child);
    return node.kind === "text" ? node.text : node.children.map(plainText).join("");
}

/** Le premier élément qui porte cette classe, la racine comprise. */
export function find(child: UiChild, className: string): UiElementNode | null {
    const node = toNode(child);
    if (node.kind === "text") return null;
    if (node.classes.includes(className)) return node;

    for (const kid of node.children) {
        const found = find(kid, className);
        if (found !== null) return found;
    }
    return null;
}

/** Tous les éléments qui portent cette classe. */
export function findAll(child: UiChild, className: string): readonly UiElementNode[] {
    const node = toNode(child);
    if (node.kind === "text") return [];

    const here = node.classes.includes(className) ? [node] : [];
    return [...here, ...node.children.flatMap((kid: UiNode) => findAll(kid, className))];
}
