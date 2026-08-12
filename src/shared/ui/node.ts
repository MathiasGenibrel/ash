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
    /**
     * La touche, quand l'événement en porte une — `""` sinon.
     *
     * Elle est ici pour la même raison que `value` : un champ qui veut reconnaître `⏎` ne
     * doit pas avoir à fouiller un `KeyboardEvent`, sans quoi il redevient intestable.
     * `paint` est le seul extracteur, et [`FieldBuilder.onSubmit`](./field.ts) le seul
     * lecteur — un composant ne filtre pas une touche à la main.
     */
    readonly key: string;
}

export type UiHandler = (event: UiEvent) => void;

export interface UiTextNode {
    readonly kind: "text";
    readonly text: string;
}

/**
 * L'espace de noms d'un `<svg>` et de ses tracés.
 *
 * Un document HTML n'a qu'un seul cas où le nom d'une balise ne suffit pas à la créer, et
 * c'est celui-là : `createElement("svg")` produit un élément HTML inconnu, muet et
 * invisible. Les icônes de `features/settings/verification-state.ts` sont des `<svg>` parce
 * qu'à 13 px la forme d'un état ne peut pas dépendre de la police installée.
 */
export const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

export interface UiElementNode {
    readonly kind: "element";
    readonly tag: string;
    /** `null` pour du HTML — le cas de tout le dépôt sauf les icônes. */
    readonly namespace: string | null;
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
 * Un composant : ce qui rend un **élément**, et pas un morceau de texte nu.
 *
 * C'est le type que rendent les composites d'une feature, et il n'existe que pour leurs
 * tests : `build()` y donne un `UiElementNode`, donc un test lit `attrs`, `classes` et
 * `children` sans avoir à écarter d'abord le cas d'un nœud texte qui ne peut pas se
 * produire.
 */
export interface UiComponent extends UiBuilder {
    build(): UiElementNode;
}

/**
 * Ce qu'un conteneur accepte : une description, ou un constructeur qui en produit une.
 *
 * C'est ce qui permet d'écrire `row(badge("claude"), button("add"))` sans un `.build()`
 * par enfant — le `.build()` explicite est du bruit, et un oubli ne se voit pas.
 */
export type UiChild = UiNode | UiBuilder;

export function toNode(child: UiChild): UiNode {
    // On reconnaît le constructeur à ce qu'il sait faire, pas la description à ce qu'elle
    // porte : une sous-classe de `ElementBuilder` posée dans `features/x/components/` a le
    // droit de nommer un champ `kind` — une carte a un genre d'outil — et un test négatif
    // l'aurait alors prise pour une description, silencieusement, en produisant un arbre
    // faux plutôt qu'une erreur.
    return "build" in child ? child.build() : child;
}

/**
 * Les noms d'attributs dont la règle vit ailleurs dans ce dossier.
 *
 * `attr` est l'échappatoire du socle, et une échappatoire qui peut réécrire un invariant
 * n'en est plus une : `button("install").attr("disabled", "")` rendrait exactement le
 * bouton éteint et muet que [`disabled(reason)`](./button.ts) a été conçu pour rendre
 * impossible. Les gestionnaires ont de la même façon leur propre canal — un `on…` posé en
 * attribut serait du code transporté par une valeur, alors que tout l'intérêt de cette
 * couche est qu'une description ne s'exécute pas.
 */
type ReservedAttribute = "disabled" | "aria-disabled" | `on${string}`;

/** Le nom refusé devient `never`, donc l'appel ne compile pas. */
type FreeAttribute<Name extends string> = Name extends ReservedAttribute ? never : string;

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
    private ns: string | null = null;

    protected constructor(
        private readonly tag: string,
        ...classes: readonly string[]
    ) {
        this.classNames = [...classes];
    }

    /**
     * La porte intérieure de l'espace de noms — `svg`, `path`.
     *
     * Elle est `protected` comme [`mark`](#mark) : le namespace n'est pas une décoration
     * qu'une vue pose au passage, c'est ce qui distingue un élément dessiné d'un élément de
     * texte, et la primitive qui produit l'un des deux est seule à le savoir.
     */
    protected inNamespace(uri: string): this {
        this.ns = uri;
        return this;
    }

    /**
     * L'échappatoire : les classes propres à la feature qui pose le composant.
     *
     * Une chaîne à plusieurs mots vaut plusieurs classes — les tables de présentation du
     * dépôt en rendent (`settings-tile is-passed`), et une classe composée serait posée
     * telle quelle dans la liste : le DOM la découperait à la peinture, mais `find` et
     * `findAll`, eux, chercheraient un nom qui n'y est pas. Le test croirait alors à une
     * absence.
     */
    class(...names: readonly string[]): this {
        const split = names.flatMap((name) => name.split(" "));
        this.classNames.push(...split.filter((name) => name.length > 0));
        return this;
    }

    /**
     * L'échappatoire des attributs — sauf ceux dont la règle appartient à une primitive.
     *
     * Un nom réservé ne compile pas ; un nom calculé (`data-${key}`) reste permis, parce
     * qu'il n'est jamais un `on…` ni un `disabled` en pratique et que l'interdire coûterait
     * plus que ce qu'il protège.
     */
    attr<Name extends string>(name: Name & FreeAttribute<Name>, value: string): this {
        return this.mark(name, value);
    }

    /**
     * La porte intérieure : les noms réservés, posés par la primitive qui en porte la règle.
     *
     * `ButtonBuilder.disabled(reason)` passe par ici — c'est lui qui exige la raison, donc
     * c'est lui qui a le droit d'écrire l'attribut.
     */
    protected mark(name: string, value: string): this {
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
            namespace: this.ns,
            classes: [...this.classNames],
            attrs: { ...this.attributes },
            on: { ...this.handlers },
            children: [...this.kids],
        };
    }
}
