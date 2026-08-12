import { ElementBuilder, text } from "./node";

/**
 * Un choix parmi une liste courte et connue — le `<select>` natif.
 *
 * Il manquait au socle : la fenêtre de réglages en a deux (l'adaptateur d'une carte, celui
 * du formulaire d'ajout), écrits chacun à la main, et les deux avaient la même faute
 * possible — une liste d'options dont aucune n'est marquée, donc un menu qui affiche la
 * première valeur en prétendant que c'est la valeur de l'entrée.
 *
 * `options` **exige** la valeur courante pour cette raison : un choix qui ne dit pas ce
 * qui est choisi ment sur ce que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Une valeur absente de la
 * liste ne sélectionne rien plutôt que d'en inventer une.
 *
 * Le natif plutôt qu'une liste dessinée : c'est ce qui donne le clavier, le survol, la
 * molette et le rendu macOS sans une ligne de code — et un menu déroulant réécrit à la main
 * est l'un des composants qu'on rate le plus sûrement.
 */
class ChoiceBuilder extends ElementBuilder {
    constructor(name: string) {
        super("select", "ui-choice");
        // Comme un champ : posé dans une grille, il n'a pas de `<label for>` qui le désigne.
        this.attr("aria-label", name);
    }

    /** Les valeurs proposées, et celle qui est en vigueur. */
    options(values: readonly string[], selected: string): this {
        return this.add(...values.map((value) => new Option_(value, value === selected)));
    }

    onSelect(handler: (value: string) => void): this {
        return this.on("change", (event) => {
            handler(event.value);
        });
    }
}

class Option_ extends ElementBuilder {
    constructor(value: string, selected: boolean) {
        super("option");
        this.attr("value", value).add(text(value));
        // L'attribut, et non la propriété : les vues du dépôt recréent leur DOM à chaque
        // rendu, donc l'élément est toujours neuf et les deux coïncident — c'est le même
        // raisonnement que la valeur d'un champ dans `paint`.
        if (selected) this.attr("selected", "");
    }
}

export function choice(name: string): ChoiceBuilder {
    return new ChoiceBuilder(name);
}

export type { ChoiceBuilder };
