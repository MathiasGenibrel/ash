import { ElementBuilder, text } from "@/shared/ui";

/**
 * Le panneau du milieu : une zone de saisie **multiligne**.
 *
 * Elle n'est pas dans `shared/ui/` et n'a pas à y monter : la règle du dépôt est qu'un
 * composant n'y va que s'il sert au moins deux features, et l'onglet de merge est le seul
 * écran d'Ash où l'on tape plusieurs lignes. `field()` reste ce qu'il est — un `<input>`
 * d'une ligne, sur lequel un hunk de trois lignes serait illisible.
 *
 * Le contenu est un **enfant texte** et non un attribut `value` : c'est ainsi qu'un
 * `<textarea>` porte sa valeur, et `paint` ne recopie `value` que sur un `<input>`. La
 * lecture, elle, marche déjà — `paint` sait extraire la valeur d'un `HTMLTextAreaElement`.
 */
class EditorBuilder extends ElementBuilder {
    constructor(name: string, content: string) {
        super("textarea", "merge-editor");
        this.attr("aria-label", name).attr("spellcheck", "false").add(text(content));
    }

    onInput(handler: (value: string) => void): this {
        return this.on("input", (event) => {
            handler(event.value);
        });
    }
}

/** Le panneau central, éditable — le critère d'acceptation de la spec §7.4. */
export function editor(name: string, content: string): EditorBuilder {
    return new EditorBuilder(name, content);
}

export type { EditorBuilder };
