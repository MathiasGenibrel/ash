/**
 * Les composites propres à la fenêtre de réglages, assemblés à partir de `shared/ui/`.
 *
 * Ils vivent ici et non dans `shared/` pour la raison que le socle écrit lui-même : une
 * carte d'outil, une ligne de hooks, une pastille de test portent la règle **d'un** écran.
 * Le socle n'accueille que ce qui sert deux features et ne porte la règle d'aucune.
 *
 * Chacun rend une description, donc chacun a un test qui la lit — sans monter de DOM. C'est
 * tout l'objet de la refonte : ce que cet écran décide était dans 986 lignes de `document`,
 * hors de portée de `bun test`, et trois passes architecturales d'affilée y ont trouvé une
 * règle produit cachée.
 *
 * **Ce qui n'est pas ici** : les règles. Elles sont dans [`model`](../model.ts) et dans
 * [`verification-state`](../verification-state.ts), et les composites les appellent sans
 * jamais les rejouer.
 */

export { addForm, draftFocusKey, type AddFormActions } from "./add-form";
export { appearanceSection, type AppearanceActions } from "./appearance";
export { foot, noToolsYet, scaleNote, sectionHeader } from "./chrome";
export { conflictScreen } from "./conflict";
export { pathFocusKey, toolCard, type CardActions, type CardContext } from "./card";
export { degradedNotice, degradedRow } from "./degraded";
export { diffView } from "./diff-view";
export { duplicateBanner, type DuplicateBannerActions } from "./duplicate-banner";
export { hooksNote, hooksRow, type HooksRowActions } from "./hooks-row";
export { navColumn } from "./nav";
export { notificationsSection } from "./notifications";
export { shortcutsSection } from "./shortcuts";
export { testDetail, testRow, type TestDetailActions } from "./test-row";
export { cell, label, para, spacer, tag } from "./atoms";
