/**
 * La description d'un morceau d'interface — **une valeur, pas un élément**.
 *
 * Le dépôt a un motif, tenu partout sauf à un endroit : on compose un modèle, puis on le
 * peint (`terminal/status-line.ts`, `terminal/tab-bar.ts`, `sidebar/tree.ts`). Ce qui
 * décide vit dans des fonctions pures, testées ; ce qui touche le DOM ne décide rien.
 * `features/settings/view.ts` a rompu ce motif — 986 lignes, 79 `document`, aucune
 * fonction pure — et trois passes architecturales d'affilée y ont trouvé une règle
 * produit cachée, toujours du même type : la vue qui décide de ne pas montrer une
 * information que le backend lui envoie. Rien ne pouvait l'attraper, parce que `bun test`
 * ne monte pas de DOM.
 *
 * Ce module rétablit le motif pour les vues composées de composants : un composant rend
 * une `UiNode`, et un seul module — [`paint`](./paint.ts) — sait la poser dans le DOM. Un
 * test lit alors une structure de données, sans `happy-dom`, sans `jsdom`, sans rien à
 * installer.
 *
 * **Ce que ce module n'est pas** : il n'y a ici ni état, ni cycle de vie, ni diffing, ni
 * réactivité. Une description entre, du DOM sort. Les trois vues du dépôt reconstruisent
 * déjà tout leur DOM à chaque rendu (`replaceChildren`) : il n'y a ni gain à défendre, ni
 * mini-React à écrire.
 */

/**
 * Ce qu'un gestionnaire reçoit quand son événement se produit.
 *
 * C'est une **valeur**, pas un `Event` du DOM : sans ça, `onInput` d'un champ n'aurait
 * aucun moyen de connaître ce qui a été tapé sans lire `event.target`, c'est-à-dire sans
 * ramener le DOM dans le composant — et le test d'un champ redeviendrait intestable.
 * `paint` extrait `value` de la cible ; pour un clic, elle vaut la chaîne vide.
 */
export interface UiEvent {
    readonly value: string;
}

export type UiHandler = (event: UiEvent) => void;

export interface UiTextNode {
    readonly kind: "text";
    readonly text: string;
}

export interface UiElementNode {
    readonly kind: "element";
    readonly tag: string;
    readonly classes: readonly string[];
    readonly attrs: Readonly<Record<string, string>>;
    readonly on: Readonly<Record<string, UiHandler>>;
    readonly children: readonly UiNode[];
}

export type UiNode = UiTextNode | UiElementNode;

/** Tout ce qui sait se réduire à une description. */
export interface UiBuilder {
    build(): UiNode;
}

/**
 * Ce qu'un conteneur accepte : une description, ou un constructeur qui en produit une.
 *
 * C'est ce qui permet d'écrire `row(badge("claude"), button("add"))` sans un `.build()`
 * par enfant — le `.build()` explicite est du bruit, et un oubli ne se voit pas.
 */
export type UiChild = UiNode | UiBuilder;

export function toNode(child: UiChild): UiNode {
    return "kind" in child ? child : child.build();
}

/** Un morceau de texte nu — la seule primitive qui n'a rien à configurer. */
export function text(content: string): UiTextNode {
    return { kind: "text", text: content };
}

/**
 * La base fluide des primitives : des classes, des attributs, des gestionnaires, des
 * enfants.
 *
 * Les méthodes rendent `this`, donc une sous-classe garde son propre type le long d'une
 * chaîne — `button("add").class("is-primary").onClick(f)` reste un `ButtonBuilder`.
 */
export abstract class ElementBuilder implements UiBuilder {
    private readonly classNames: string[];
    private readonly attributes: Record<string, string> = {};
    private readonly handlers: Record<string, UiHandler> = {};
    private readonly kids: UiNode[] = [];

    protected constructor(
        private readonly tag: string,
        ...classes: readonly string[]
    ) {
        this.classNames = [...classes];
    }

    /** L'échappatoire : les classes propres à la feature qui pose le composant. */
    class(...names: readonly string[]): this {
        this.classNames.push(...names.filter((name) => name.length > 0));
        return this;
    }

    attr(name: string, value: string): this {
        this.attributes[name] = value;
        return this;
    }

    /** L'infobulle, quand le mot affiché est plus court que ce qu'il veut dire. */
    title(hint: string): this {
        return this.attr("title", hint);
    }

    on(event: string, handler: UiHandler): this {
        this.handlers[event] = handler;
        return this;
    }

    add(...children: readonly UiChild[]): this {
        this.kids.push(...children.map(toNode));
        return this;
    }

    build(): UiElementNode {
        return {
            kind: "element",
            tag: this.tag,
            classes: [...this.classNames],
            attrs: { ...this.attributes },
            on: { ...this.handlers },
            children: [...this.kids],
        };
    }
}
