/**
 * Les règles du panneau bas — et rien que les règles.
 *
 * Elles sont ici, séparées du DOM qui les joue (`resizer.ts`, `index.ts`), pour la raison qui
 * a déjà sorti `resize.ts` de la sidebar : `bun test` n'a pas de DOM, et ce qui mérite d'être
 * protégé ici est arithmétique — la butée à 70 %, le refus de descendre sous 15 % tant qu'on
 * n'a pas relâché, le repli au relâchement, et le fait qu'un panneau fermé ne se
 * redimensionne pas.
 *
 * **Les bornes sont ici et pas en Rust**, alors que la hauteur, elle, y est
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : elles sont relatives à la
 * **zone terminal**, et le backend ne connaît pas la fenêtre. Le partage est exactement celui
 * de la colonne de gauche — le backend détient le choix, la webview sait ce que ce choix
 * donne à l'écran. Une hauteur gardée reste donc telle quelle sur le disque quand on réduit
 * la fenêtre : c'est l'affichage qui la ramène dans les bornes, et elle se retrouve intacte
 * quand la fenêtre reprend sa taille.
 *
 * Ce fichier ne sait **rien de git**, et il n'a pas à en savoir : le panneau est une surface
 * de mise en page, ses vues sont des noms, et leur contenu appartient à #27, #28, #30 et #31.
 */

import {
    clampedSize,
    edgeBounds,
    grabOffset as edgeGrabOffset,
    handleOffset as edgeHandleOffset,
    sizePercent,
    type EdgeBounds,
} from "@/shared/resizable-edge";

/** Les quatre vues du panneau (spec §4.3, ADR-0003). Miroir de `PanelView` côté Rust. */
export type PanelView = "graph" | "worktrees" | "conflicts" | "branch";

/**
 * L'ordre de la barre d'onglets, tel que le schéma d'interface de la spec §4 le dessine :
 * `graph │ worktrees │ conflicts`, et la fiche de branche à la suite.
 */
export const PANEL_VIEWS: readonly PanelView[] = ["graph", "worktrees", "conflicts", "branch"];

/** La hauteur, l'ouverture et la vue, telles que le backend les annonce. */
export interface BottomPanelState {
    /** En pixels, ouvert. Elle ne change pas quand le panneau se referme. */
    readonly height: number;
    readonly open: boolean;
    readonly view: PanelView;
}

/**
 * Le plancher : sous 15 % de la zone terminal, il ne reste plus un panneau mais un liseré.
 *
 * On n'y **descend** pas en glissant — le panneau s'y arrête —, mais relâcher en dessous
 * referme : c'est le geste que la colonne de gauche a déjà, et il n'y a aucune raison qu'il
 * s'apprenne deux fois.
 *
 * **Baisser cette fraction se vérifie côté Rust.** `PanelHeight` borne aussi la hauteur, à
 * 40 px, pour qu'un `theme.json` édité à la main n'ouvre rien d'absurde ; ces bornes-là ne
 * doivent jamais mordre sur celles-ci, sinon la hauteur montrée cesse d'être la hauteur
 * gardée. Le test qui tient ce lien est dans
 * `src-tauri/src/features/theme/bottom_panel.rs`, et il recopie cette fraction.
 */
export const MIN_HEIGHT_FRACTION = 0.15;

/**
 * Le plafond : au-delà de 70 % de la zone terminal, le terminal n'a plus de quoi montrer
 * une TUI.
 *
 * Ce n'est pas une préférence esthétique. Le panneau prend sa hauteur **au terminal**, donc
 * chaque pixel qu'il gagne est un `SIGWINCH` de plus vers ce qui tourne dedans
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)) : un panneau qui pourrait
 * couvrir la fenêtre entière laisserait `vim` se redessiner sur deux lignes.
 */
export const MAX_HEIGHT_FRACTION = 0.7;

/** Ce qu'une flèche déplace, comme sur le séparateur de la colonne : on ajuste, on ne balaie pas. */
export const KEYBOARD_STEP = 16;

/** Ce dont la zone d'interaction déborde de chaque côté du trait — `panel.css` pose la même valeur. */
export const GRAB_OVERHANG = 7;

/** Ce que la poignée garde comme marge aux deux extrémités du bord. */
export const HANDLE_MARGIN = 18;

/** Ce sur quoi le panneau s'ouvre tant que le backend n'a pas répondu. Miroir de `PanelHeight::DEFAULT`. */
export const DEFAULT_PANEL_HEIGHT = 220;

/** La barre d'onglets, toujours visible — voir [`BottomPanel`](./index.ts). */
export const STRIP_HEIGHT = 26;

/**
 * Les deux bornes du panneau, sur l'échelle que `shared/resizable-edge` attend.
 *
 * Les fractions restent **ici** — ce sont les siennes, et elles se justifient par ce que le
 * terminal doit garder de place ; c'est la géométrie qui les applique qui est partagée avec
 * la colonne de gauche.
 */
const HEIGHT_BOUNDS: EdgeBounds = { min: MIN_HEIGHT_FRACTION, max: MAX_HEIGHT_FRACTION };

/** Les deux bornes en pixels, pour la zone terminal du moment. */
function bounds(areaHeight: number): { min: number; max: number } {
    return edgeBounds(areaHeight, HEIGHT_BOUNDS);
}

/**
 * La hauteur réellement posée à l'écran, pour une zone terminal donnée.
 *
 * Appelée à chaque rendu **et** à chaque redimensionnement de la fenêtre : c'est ce qui fait
 * qu'une fenêtre réduite ne laisse jamais le panneau couvrir le terminal, sans rien réécrire
 * sur le disque.
 */
export function appliedHeight(height: number, areaHeight: number): number {
    return clampedSize(height, areaHeight, HEIGHT_BOUNDS);
}

/**
 * La zone dans laquelle le panneau se règle, mesurée par qui a le DOM sous la main.
 *
 * `bottom` est l'ordonnée du bas de la partie réglable — le haut de la barre d'onglets —,
 * et `height` la hauteur totale de la zone terminal, d'où se déduisent les bornes.
 */
export interface PanelArea {
    readonly bottom: number;
    readonly height: number;
}

/** Ce qu'un glissement en cours donne : ce qu'on montre, et ce qu'on ferait en relâchant. */
export interface DragOutcome {
    /** La hauteur à montrer maintenant — jamais sous le plancher, jamais au-dessus du plafond. */
    readonly height: number;
    /** Relâcher ici refermerait le panneau. */
    readonly willCollapse: boolean;
}

/**
 * L'écart entre le pointeur et le trait, mesuré à l'instant où l'on attrape.
 *
 * `zoneTop` est le bord haut de la zone d'interaction ; le trait est [`GRAB_OVERHANG`] pixels
 * plus bas. Sans cet écart, attraper la zone 7 px au-dessus du bord ferait sauter le panneau
 * de 7 px au moment même du clic : la zone élargie rendrait la cible facile à atteindre et
 * punirait celui qui l'atteint.
 */
export function grabOffset(pointerY: number, zoneTop: number): number {
    return edgeGrabOffset(pointerY, zoneTop, GRAB_OVERHANG);
}

/**
 * Le résultat d'un pointeur posé à `pointerY`, sachant l'écart `grab` retenu à la saisie.
 *
 * Le bord **monte quand le pointeur monte** : la hauteur est ce qui sépare le trait du bas de
 * la zone réglable. Le panneau **s'arrête** au plancher au lieu de suivre le pointeur, et
 * c'est le fait de relâcher plus bas qui referme — la même règle que la colonne de gauche,
 * pour la même raison : un panneau qui rétrécirait jusqu'à zéro en glissant ne dirait plus à
 * quel moment le relâchement referme.
 */
export function dragOutcome(pointerY: number, area: PanelArea, grab = 0): DragOutcome {
    const { min } = bounds(area.height);
    const asked = area.bottom - (pointerY - grab);
    return {
        height: appliedHeight(asked, area.height),
        willCollapse: asked < min,
    };
}

/**
 * Où poser la poignée le long du bord, en pixels depuis la gauche de la zone.
 *
 * Elle suit le pointeur — c'est la variante retenue pour la colonne de gauche, et le panneau
 * n'a aucune raison de se comporter autrement —, et se borne à [`HANDLE_MARGIN`] des deux
 * extrémités pour ne pas déborder.
 */
export function handleOffset(pointerX: number, edgeLeft: number, edgeWidth: number): number {
    return edgeHandleOffset(pointerX, edgeLeft, edgeWidth, HANDLE_MARGIN);
}

/** Ce qu'une frappe sur le séparateur demande, ou `null` — et `null` veut dire « laisse passer ». */
export type ResizeCommand = { readonly kind: "height"; readonly height: number } | { readonly kind: "close" };

/**
 * Traduit une frappe faite sur le séparateur focalisé.
 *
 * Les flèches redimensionnent, `Échap` referme. Une flèche sur un panneau **fermé** ne fait
 * rien : ouvrir est un geste à part — un raccourci de vue, ou un clic sur la barre —, et
 * laisser une flèche rouvrir ferait reprendre au terminal une hauteur qu'on venait de lui
 * rendre.
 *
 * Une flèche vers le bas arrivée au plancher ne referme pas non plus. Refermer se demande, ça
 * ne s'obtient pas en insistant — c'est la même règle que pour le glissement, où seul le
 * relâchement décide.
 */
export function resizeByKey(
    key: string,
    panel: BottomPanelState,
    areaHeight: number,
): ResizeCommand | null {
    if (key === "Escape") return { kind: "close" };
    if (!panel.open) return null;

    const applied = appliedHeight(panel.height, areaHeight);
    if (key === "ArrowUp") return { kind: "height", height: appliedHeight(applied + KEYBOARD_STEP, areaHeight) };
    if (key === "ArrowDown") return { kind: "height", height: appliedHeight(applied - KEYBOARD_STEP, areaHeight) };
    return null;
}

/**
 * Le pourcentage de la zone terminal qu'occupe le panneau, pour l'annoncer aux technologies
 * d'assistance : `aria-valuenow` d'un `separator` déplaçable veut un nombre sur une échelle,
 * et la seule échelle qui ait un sens ici est celle des bornes — 15 à 70.
 */
export function heightPercent(height: number, areaHeight: number): number {
    return sizePercent(height, areaHeight, HEIGHT_BOUNDS);
}
