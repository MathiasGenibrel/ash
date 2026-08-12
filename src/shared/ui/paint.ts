import type { UiNode } from "./node";

/**
 * Le **seul** fichier de `shared/ui/` qui appelle `document` — un test le vérifie
 * (`discipline.test.ts`).
 *
 * C'est ce qui donne sa valeur au reste : tant que la peinture tient dans une trentaine de
 * lignes qui ne décident rien, tout ce qui décide vit dans des descriptions, et une
 * description se lit dans un test sans monter de DOM.
 *
 * Il n'y a ici ni cycle de vie, ni diffing, ni réactivité : une description entre, un nœud
 * sort. Les vues du dépôt reconstruisent déjà leur DOM à chaque rendu.
 *
 * Ce fichier n'est pas couvert par `bun test`, et il ne peut pas l'être : il n'y a pas de
 * `document` dans le runtime des tests. C'est le prix — et la raison — de la frontière.
 */
export function paint(node: UiNode): Node {
    if (node.kind === "text") return document.createTextNode(node.text);

    const element = document.createElement(node.tag);
    if (node.classes.length > 0) element.className = node.classes.join(" ");

    for (const [name, value] of Object.entries(node.attrs)) {
        element.setAttribute(name, value);
    }

    // `value` posé en attribut ne fixe que la valeur *par défaut* d'un champ. Sur un
    // élément neuf les deux coïncident — et les vues du dépôt en créent un neuf à chaque
    // rendu —, mais la propriété est ce qui est réellement affiché, et c'est elle que la
    // restauration du curseur mesure.
    const value = node.attrs["value"];
    if (value !== undefined && element instanceof HTMLInputElement) element.value = value;

    for (const [name, handler] of Object.entries(node.on)) {
        element.addEventListener(name, (event) => {
            handler({ value: valueOf(event.currentTarget) });
        });
    }

    element.append(...node.children.map(paint));
    return element;
}

/**
 * Ce que le gestionnaire reçoit — la valeur de la cible, ou rien.
 *
 * C'est le seul endroit du socle qui lit le DOM : sans cette extraction, `onInput` devrait
 * fouiller un `Event` depuis le composant, et le composant redeviendrait intestable.
 */
function valueOf(target: EventTarget | null): string {
    if (target instanceof HTMLInputElement) return target.value;
    if (target instanceof HTMLSelectElement) return target.value;
    if (target instanceof HTMLTextAreaElement) return target.value;
    return "";
}
