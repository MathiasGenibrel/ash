/**
 * La géométrie d'un **bord réglable** : la mesure qu'un glissement le long d'un bord produit,
 * et rien de plus.
 *
 * Deux surfaces d'Ash se règlent ainsi, et elles s'y sont prises deux fois :
 * la colonne de gauche, réglée par son bord droit (`src/features/sidebar/resize.ts`, #129), et
 * le panneau bas, réglé par son bord haut (`src/features/panel/layout.ts`, #24). Elles n'ont ni
 * le même axe, ni les mêmes bornes, ni la même façon de refermer — mais la mesure, elle, est la
 * même arithmétique, et elle avait été écrite deux fois : deux bornes relatives, deux clamps,
 * deux écarts de saisie, deux poignées bornées, deux pourcentages pour `aria-valuenow`. Une
 * correction faite d'un côté ne serait jamais arrivée à l'autre.
 *
 * **Dans `shared/` pour la raison qui y met déjà `agent-state`** : c'est de la géométrie sans
 * un mot du domaine. Rien ici ne sait ce qu'est une colonne, un panneau, un repli ou une vue ;
 * rien ici ne connaît le DOM, ne lit une fenêtre, ni ne décide qu'un relâchement referme. Ce
 * qui reste propre à chaque feature **reste chez elle** : ses fractions, son pas de clavier,
 * le sens de son axe, et la règle qui dit ce que relâcher veut dire.
 *
 * Le partage avec le backend ne bouge pas : la taille réglée est détenue en Rust
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), les bornes **relatives**
 * restent ici parce qu'elles dépendent d'une mise en page que le backend ne mesure pas.
 */

/** Les deux bornes d'un bord, en fraction de ce qui le contient. */
export interface EdgeBounds {
    /** Sous cette fraction, il ne reste plus une surface mais un liseré. */
    readonly min: number;
    /** Au-delà, ce que la surface prend à sa voisine cesse d'être tenable. */
    readonly max: number;
}

/**
 * Les deux bornes en pixels, pour l'étendue du moment.
 *
 * `extent` est ce dans quoi la surface se règle — la fenêtre pour la colonne, la zone
 * terminal pour le panneau. Il est ramené à 1 au minimum : une étendue nulle arrive
 * réellement, le temps d'une mesure faite avant la première mise en page.
 */
export function edgeBounds(extent: number, bounds: EdgeBounds): { min: number; max: number } {
    const usable = Math.max(1, extent);
    return { min: usable * bounds.min, max: usable * bounds.max };
}

/**
 * La taille réellement posée à l'écran, pour l'étendue du moment.
 *
 * Appelée à **chaque** rendu, et pas seulement quand la taille change : c'est ce qui fait que
 * réduire la fenêtre ramène la surface dans ses bornes sans rien réécrire sur le disque, et
 * qu'elle se retrouve intacte quand la fenêtre reprend sa taille.
 */
export function clampedSize(size: number, extent: number, bounds: EdgeBounds): number {
    const { min, max } = edgeBounds(extent, bounds);
    return Math.round(Math.min(Math.max(size, min), max));
}

/**
 * L'écart entre le pointeur et le trait, mesuré à l'instant où l'on attrape.
 *
 * `zoneStart` est le début de la zone d'interaction le long de l'axe ; le trait est `overhang`
 * pixels plus loin. **Le trait suit le pointeur, il ne le rejoint pas** : sans cet écart,
 * attraper la zone à `overhang` pixels du bord ferait sauter la surface d'autant au moment
 * même du clic, et la zone élargie punirait celui qui l'atteint au lieu de l'aider.
 */
export function grabOffset(pointer: number, zoneStart: number, overhang: number): number {
    return pointer - (zoneStart + overhang);
}

/**
 * Où poser la poignée le long du bord, en pixels depuis le début de la zone.
 *
 * Elle suit le pointeur — c'est la variante retenue en #129 —, et se borne à `margin` des deux
 * extrémités pour ne jamais déborder du bord qu'elle annonce.
 */
export function handleOffset(pointer: number, edgeStart: number, edgeExtent: number, margin: number): number {
    const floor = margin;
    const ceiling = Math.max(floor, edgeExtent - margin);
    return Math.min(Math.max(pointer - edgeStart, floor), ceiling);
}

/**
 * La part de l'étendue qu'occupe la surface, pour l'annoncer aux technologies d'assistance.
 *
 * `aria-valuenow` d'un `separator` déplaçable veut un nombre sur une échelle, et la seule
 * échelle qui ait un sens est celle des bornes — que les deux séparateurs annoncent déjà par
 * leurs `aria-valuemin` et `aria-valuemax`.
 */
export function sizePercent(size: number, extent: number, bounds: EdgeBounds): number {
    return Math.round((clampedSize(size, extent, bounds) / Math.max(1, extent)) * 100);
}
