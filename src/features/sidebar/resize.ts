/**
 * Les règles du redimensionnement de la colonne — et rien que les règles.
 *
 * Elles sont ici, séparées du DOM qui les joue (`resizer.ts`), pour la raison qui a déjà
 * sorti `header.ts` et `visible.ts` de `view.ts` : `bun test` n'a pas de DOM, et ce qui
 * mérite d'être protégé ici est arithmétique — la butée à 80 %, le refus de descendre sous
 * 10 % tant qu'on n'a pas relâché, le repli au relâchement, et la poignée qui ne déborde
 * jamais du bord.
 *
 * **Les bornes sont ici et pas en Rust**, alors que la largeur, elle, y est
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : elles sont relatives à la
 * fenêtre, et le backend ne connaît pas la fenêtre. Le partage est le même que pour le thème
 * — le backend détient le **choix**, la webview sait ce que ce choix donne à l'écran. Une
 * largeur gardée reste donc telle quelle sur le disque quand on rétrécit la fenêtre : c'est
 * l'affichage qui la ramène dans les bornes, et elle se retrouve intacte quand la fenêtre
 * reprend sa taille.
 */

/** La largeur et le repli, tels que le backend les annonce. */
export interface SidebarColumnState {
    /** En pixels, dépliée. Elle ne change pas quand la colonne se replie. */
    readonly width: number;
    readonly collapsed: boolean;
}

/**
 * Le plancher : sous 10 % de la fenêtre, il ne reste plus une colonne mais un bord.
 *
 * On n'y **descend** pas en glissant — la colonne s'y arrête —, mais relâcher en dessous
 * referme : c'est le seul geste de la maquette qui distingue ce qu'on montre pendant le
 * glissement de ce qu'on décide en le finissant.
 *
 * **Baisser cette fraction se vérifie côté Rust.** `SidebarWidth` borne aussi la largeur, à
 * 46 px, pour qu'un `theme.json` édité à la main n'ouvre rien d'absurde ; ces bornes-là ne
 * doivent jamais mordre sur celles-ci, sinon la largeur montrée cesse d'être la largeur
 * gardée. Le test qui tient ce lien est dans
 * `src-tauri/src/features/theme/sidebar_column.rs`, et il recopie cette fraction.
 */
export const MIN_WIDTH_FRACTION = 0.1;

/** Le plafond : au-delà de 80 %, le terminal n'a plus de quoi montrer 80 colonnes. */
export const MAX_WIDTH_FRACTION = 0.8;

/**
 * Ce qu'une flèche déplace. Seize pixels, pas un : au clavier on ajuste, on ne balaie pas —
 * et il faut pouvoir traverser la fenêtre en un nombre de frappes qui reste tenable.
 */
export const KEYBOARD_STEP = 16;

/**
 * Ce dont la zone d'interaction déborde de chaque côté du trait — 7 px, donc 15 px
 * attrapables (`sidebar.css` pose la même valeur).
 *
 * C'est aussi ce qui rend [`grabOffset`] nécessaire : on attrape le bord **à côté** du trait,
 * et c'est tout l'intérêt de la zone élargie.
 */
export const GRAB_OVERHANG = 7;

/** Ce que la poignée garde comme marge aux deux extrémités du bord (maquette validée). */
export const HANDLE_MARGIN = 18;

/** Ce sur quoi la colonne s'ouvre tant que le backend n'a pas répondu — les 240 px du design. */
export const DEFAULT_SIDEBAR_WIDTH = 240;

/** Ce qu'il reste de la colonne une fois repliée : le rail des écrans de design. */
export const RAIL_WIDTH = 46;

/** Les deux bornes en pixels, pour la fenêtre du moment. */
function bounds(windowWidth: number): { min: number; max: number } {
    const usable = Math.max(1, windowWidth);
    return { min: usable * MIN_WIDTH_FRACTION, max: usable * MAX_WIDTH_FRACTION };
}

/**
 * La largeur réellement posée à l'écran, pour une fenêtre donnée.
 *
 * Appelée à chaque rendu **et** à chaque redimensionnement de la fenêtre : c'est ce qui fait
 * que réduire la fenêtre ne laisse jamais la colonne hors de ses bornes, sans rien réécrire
 * sur le disque.
 */
export function appliedWidth(width: number, windowWidth: number): number {
    const { min, max } = bounds(windowWidth);
    return Math.round(Math.min(Math.max(width, min), max));
}

/** Ce qu'un glissement en cours donne : ce qu'on montre, et ce qu'on ferait en relâchant. */
export interface DragOutcome {
    /** La largeur à montrer maintenant — jamais sous le plancher, jamais au-dessus du plafond. */
    readonly width: number;
    /** Relâcher ici refermerait la colonne. */
    readonly willCollapse: boolean;
}

/**
 * L'écart entre le pointeur et le trait, mesuré à l'instant où l'on attrape.
 *
 * `zoneLeft` est le bord gauche de la zone d'interaction ; le trait est [`GRAB_OVERHANG`]
 * pixels plus loin. L'écart va donc de −7 à +8, et il est **retenu pour tout le geste**.
 */
export function grabOffset(pointerX: number, zoneLeft: number): number {
    return pointerX - (zoneLeft + GRAB_OVERHANG);
}

/**
 * Le résultat d'un pointeur posé à `pointerX` dans une fenêtre de `windowWidth`, sachant
 * l'écart `grab` retenu à la saisie.
 *
 * **Le trait suit le pointeur, il ne le rejoint pas.** Sans cet écart, attraper la zone à 7 px
 * du bord ferait sauter la colonne de 7 px au moment même du clic : la zone élargie rendrait
 * la cible facile à atteindre et punirait celui qui l'atteint. Avec lui, glisser de N pixels
 * déplace le bord de N pixels, d'où qu'on soit parti dans les 15 px.
 *
 * La colonne **s'arrête** au plancher au lieu de suivre le pointeur : continuer à rétrécir
 * jusqu'à zéro donnerait une colonne illisible avant de la refermer, et on ne saurait plus à
 * quel moment le relâchement referme. Ici, la colonne s'immobilise, et c'est le fait de
 * relâcher plus à gauche qui décide.
 */
export function dragOutcome(pointerX: number, windowWidth: number, grab = 0): DragOutcome {
    const { min } = bounds(windowWidth);
    const edge = pointerX - grab;
    return {
        width: appliedWidth(edge, windowWidth),
        willCollapse: edge < min,
    };
}

/**
 * Où poser la poignée le long du bord, en pixels depuis le haut de la zone.
 *
 * Elle suit la hauteur du pointeur — c'est tout le propos de la variante retenue —, et se
 * borne à [`HANDLE_MARGIN`] des deux extrémités pour ne pas déborder de la colonne.
 */
export function handleOffset(pointerY: number, edgeTop: number, edgeHeight: number): number {
    const floor = HANDLE_MARGIN;
    const ceiling = Math.max(floor, edgeHeight - HANDLE_MARGIN);
    return Math.min(Math.max(pointerY - edgeTop, floor), ceiling);
}

/** Ce qu'une frappe sur le séparateur demande, ou `null` — et `null` veut dire « laisse passer ». */
export type ResizeCommand = { readonly kind: "width"; readonly width: number } | { readonly kind: "toggle" };

/**
 * Traduit une frappe faite sur le séparateur focalisé.
 *
 * Les flèches redimensionnent, `Enter` et `Espace` replient et déplient. Une flèche sur une
 * colonne **repliée** ne fait rien : ouvrir est un geste à part, et laisser une flèche
 * rouvrir ferait perdre le repli au premier appui distrait sur une colonne fermée.
 *
 * Une flèche gauche arrivée au plancher ne referme pas non plus. Refermer se demande, ça ne
 * s'obtient pas en insistant — c'est la même règle que pour le glissement, où seul le
 * relâchement décide.
 */
export function resizeByKey(
    key: string,
    column: SidebarColumnState,
    windowWidth: number,
): ResizeCommand | null {
    if (key === "Enter" || key === " ") return { kind: "toggle" };
    if (column.collapsed) return null;

    const applied = appliedWidth(column.width, windowWidth);
    if (key === "ArrowLeft") return { kind: "width", width: appliedWidth(applied - KEYBOARD_STEP, windowWidth) };
    if (key === "ArrowRight") return { kind: "width", width: appliedWidth(applied + KEYBOARD_STEP, windowWidth) };
    return null;
}

/**
 * Le pourcentage de fenêtre qu'occupe la colonne, pour l'annoncer aux technologies
 * d'assistance : `aria-valuenow` d'un `separator` déplaçable veut un nombre sur une échelle,
 * et la seule échelle qui ait un sens ici est celle des bornes — 10 à 80.
 */
export function widthPercent(width: number, windowWidth: number): number {
    return Math.round((appliedWidth(width, windowWidth) / Math.max(1, windowWidth)) * 100);
}
