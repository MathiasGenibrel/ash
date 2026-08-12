/**
 * `shared/ui/` — une couche de composants où **un composant est une valeur**.
 *
 * Un composant rend une [`UiNode`](./node.ts) ; [`paint`](./paint.ts) est le seul module du
 * dossier qui sait la poser dans le DOM. Une feature compose donc son écran avec des
 * fonctions pures, et le teste en lisant une structure de données — sans `happy-dom`, sans
 * `jsdom`, sans rien à installer.
 *
 * ```ts
 * card(tool.command)
 *     .badge(tool.label)
 *     .rows(field("config").value(tool.config).onInput(actions.retarget));
 * ```
 *
 * **La règle du dépôt s'applique ici comme ailleurs** : un composant ne monte dans
 * `shared/ui/` que s'il sert au moins deux features et ne porte la règle d'aucune. Une
 * carte d'outil, une ligne de hooks, une pastille de test appartiennent à
 * `features/settings/components/`. Ce dossier n'est pas un fourre-tout de composants.
 *
 * Les classes posées (`ui-row`, `ui-button`, …) sont peintes par les feuilles de style des
 * features qui les emploient : aucune n'est encore convertie, donc aucune règle CSS n'est
 * écrite d'avance.
 */

export { button, type ButtonBuilder } from "./button";
export { choice, type ChoiceBuilder } from "./choice";
export {
    banner,
    emptyState,
    type BannerBuilder,
    type EmptyStateBuilder,
    type UiTone,
} from "./feedback";
export { field, FOCUS_KEY, type FieldBuilder, type Submission } from "./field";
export { column, row, type Stack } from "./layout";
export { badge, glyph, type BadgeBuilder, type GlyphBuilder } from "./marks";
export {
    ElementBuilder,
    SVG_NAMESPACE,
    text,
    toNode,
    type UiBuilder,
    type UiChild,
    type UiComponent,
    type UiElementNode,
    type UiEvent,
    type UiHandler,
    type UiNode,
    type UiTextNode,
} from "./node";
export { paint } from "./paint";
export { find, findAll, plainText } from "./read";
