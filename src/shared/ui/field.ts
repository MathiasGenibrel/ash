import { ElementBuilder } from "./node";

/**
 * L'attribut qui identifie un champ **à travers un rendu**.
 *
 * Les vues du dépôt refont tout leur DOM à chaque rendu. Un champ qu'on est en train de
 * remplir est donc détruit puis reconstruit — et le focus, avec le curseur, part avec
 * l'ancien élément. La vue qui monte le DOM retient la clé du champ actif et la position du
 * curseur avant de peindre, puis les rend après : c'est ce qui empêche une relance
 * différée de redessiner une carte au milieu d'un mot.
 *
 * La clé est nommée **ici** parce que c'est le seul endroit qui sait quel élément la porte.
 * Le mécanisme, lui, reste dans la vue : ce socle ne conserve rien.
 */
export const FOCUS_KEY = "data-focus-key";

/**
 * Un champ de saisie.
 *
 * `onInput` reçoit la valeur tapée, pas un `Event` : c'est ce qui permet à une vue de
 * s'écrire — et de se tester — sans jamais nommer le DOM. `paint` fait l'extraction.
 */
class FieldBuilder extends ElementBuilder {
    constructor(name: string) {
        super("input", "ui-field");
        // Le nom sert d'étiquette accessible : un champ posé dans une grille n'a pas de
        // `<label for>` qui le désigne, et un champ sans nom n'est qu'une boîte grise.
        this.attr("type", "text").attr("aria-label", name);
    }

    value(current: string): this {
        return this.attr("value", current);
    }

    placeholder(hint: string): this {
        return this.attr("placeholder", hint);
    }

    /** La clé que la vue retient pour rendre le focus après avoir refait le DOM. */
    focusKey(key: string): this {
        return this.attr(FOCUS_KEY, key);
    }

    onInput(handler: (value: string) => void): this {
        return this.on("input", (event) => {
            handler(event.value);
        });
    }

    /**
     * `⏎` — « j'ai fini de taper », et **rien de plus**.
     *
     * Ash ne valide jamais à la place de l'utilisateur
     * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)) : ce geste
     * n'envoie rien, il abrège seulement une attente. Le nom du gestionnaire dit la
     * frappe, pas la touche : c'est le socle qui sait laquelle, donc un composant ne
     * compare jamais une chaîne à `"Enter"`.
     */
    onSubmit(handler: () => void): this {
        return this.on("keydown", (event) => {
            if (event.key === "Enter") handler();
        });
    }

    /** La perte de focus — l'autre façon de dire qu'on a fini de taper. */
    onBlur(handler: () => void): this {
        return this.on("blur", () => {
            handler();
        });
    }
}

export function field(name: string): FieldBuilder {
    return new FieldBuilder(name);
}

export type { FieldBuilder };
