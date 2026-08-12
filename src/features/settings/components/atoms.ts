import { ElementBuilder, text, type UiChild } from "@/shared/ui";

/**
 * Les balises que la fenêtre pose et dont le socle n'a pas de primitive : un titre, un
 * paragraphe, un morceau de texte, une cellule de grille vide.
 *
 * Elles sont **ici** et non dans `shared/ui/` parce qu'elles ne portent aucune règle : ce
 * sont des balises. Le socle, lui, n'accueille que ce qu'un composant peut se tromper à
 * écrire à la main — un bouton éteint sans raison, un menu qui ne dit pas ce qui est
 * choisi. Un `<p>` ne se rate pas.
 *
 * Elles restent des **descriptions** : rien ici n'appelle `document`, et c'est ce qui met
 * les composites de ce dossier sous test.
 */
class Tag extends ElementBuilder {
    constructor(tag: string, ...classes: readonly string[]) {
        // Par `class` et non par le constructeur : c'est elle qui découpe une chaîne à
        // plusieurs mots, et les tables de présentation en rendent.
        super(tag);
        this.class(...classes);
    }
}

/** Une balise quelconque, avec ses classes — l'échappatoire, et elle est rare. */
export function tag(name: string, ...classes: readonly string[]): Tag {
    return new Tag(name, ...classes);
}

/** Un morceau de texte qui porte une classe : la moitié des nœuds de cet écran. */
export function label(className: string, content: string): Tag {
    return new Tag("span", className).add(text(content));
}

/** Un paragraphe — la prose de l'écran, qui n'est jamais un `<span>`. */
export function para(className: string, ...children: readonly UiChild[]): Tag {
    return new Tag("p", className).add(...children);
}

/**
 * Le `flex: 1` qui pousse ce qui suit à droite.
 *
 * `Stack.spacer()` du socle en pose un aussi, mais sous la classe `ui-spacer`, qu'aucune
 * feuille de style ne peint encore. Tant que la fenêtre de réglages est la seule vue
 * convertie, l'écart entre les deux serait un `flex: 1` qui ne pousse rien.
 */
export function spacer(): Tag {
    return new Tag("span", "settings-spacer");
}

/**
 * Une cellule de grille vide — la colonne des libellés, quand la ligne n'en a pas.
 *
 * Les corps de carte et le formulaire sont des grilles à deux colonnes : une ligne qui se
 * range **sous** ce qu'elle commente laisse donc sa première cellule vide, plutôt que de
 * s'aligner sur une colonne de libellés à laquelle elle ne répond pas.
 */
export function cell(): Tag {
    return new Tag("span");
}

/** Le retour à la ligne d'une prose en deux temps. */
export function lineBreak(): Tag {
    return new Tag("br");
}

export type { Tag };
