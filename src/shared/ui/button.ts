import { ElementBuilder, text } from "./node";

/**
 * Un bouton — et la seule règle produit que ce socle rend impossible à violer.
 *
 * La maquette la répète trois fois : **le bouton reste visible, éteint, avec sa raison —
 * jamais masqué**. « Le masquer ferait croire que ça n'existe pas. » Écrite à la main,
 * cette règle tient une revue de code et se perd à la suivante : `element.disabled = true`
 * ne demande rien à personne.
 *
 * Ici, elle est dans la signature : [`disabled`](#disabled) **exige** sa raison, et un
 * appel sans raison ne compile pas. Le composant n'a plus le droit d'éteindre un bouton en
 * silence.
 */
class ButtonBuilder extends ElementBuilder {
    constructor(label: string) {
        super("button", "ui-button");
        // Un vrai `<button type="button">` : c'est ce qui le met sur le chemin de `tab` et
        // dans l'arbre d'accessibilité sans une ligne de code — et le `type` explicite
        // l'empêche de soumettre le formulaire qui l'entoure.
        this.attr("type", "button").add(text(label));
    }

    onClick(handler: () => void): this {
        return this.on("click", () => {
            handler();
        });
    }

    /**
     * Éteint le bouton, **et dit pourquoi**.
     *
     * La raison n'est pas décorative : elle devient l'infobulle du bouton. `aria-disabled`
     * double l'attribut natif parce qu'un `disabled` seul sort l'élément du chemin de
     * `tab` : la raison reste alors visible mais muette pour un lecteur d'écran qui
     * parcourt les commandes. Une vue reste libre — et la maquette le fait — d'écrire la
     * même raison en clair à côté du bouton.
     */
    disabled(reason: string): this {
        return this.mark("disabled", "").mark("aria-disabled", "true").title(reason);
    }
}

export function button(label: string): ButtonBuilder {
    return new ButtonBuilder(label);
}

export type { ButtonBuilder };
