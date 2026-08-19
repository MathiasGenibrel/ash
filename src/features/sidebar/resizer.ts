import {
    dragOutcome,
    grabOffset,
    handleOffset,
    type DragOutcome,
    resizeByKey,
    widthPercent,
    MAX_WIDTH_FRACTION,
    MIN_WIDTH_FRACTION,
    type SidebarColumnState,
} from "./resize";

/**
 * Le séparateur du bord droit : la zone qu'on attrape, et la poignée qui l'annonce.
 *
 * **Le sujet est la cible, pas le trait.** La bordure fait un pixel, et l'épaissir alourdirait
 * une interface qui tient sur des traits fins ; la zone d'interaction déborde donc de 7 px de
 * chaque côté — 15 px attrapables — et **elle n'est jamais dessinée**. Ce qui se voit, c'est
 * une poignée qui se déploie depuis la bordure **à la hauteur du curseur**, dès que celui-ci
 * entre dans la zone élargie, donc avant qu'il n'ait atteint le trait.
 *
 * Ce fichier ne décide rien : il pose des événements et lit `resize.ts`. La règle est là-bas,
 * où elle se teste sans DOM.
 *
 * **Ce qui se voit pendant le glissement n'est pas ce qui est gardé.** La largeur suit le
 * pointeur image par image — un fait d'affichage, comme le compteur de durée de la ligne de
 * statut —, et seul le **relâchement** part au backend : un aller-retour Tauri par image
 * réécrirait `~/.ash/theme.json` soixante fois par seconde.
 */
export interface SidebarResizerPorts {
    /** La colonne telle que le backend l'a annoncée — jamais un état gardé ici. */
    column(): SidebarColumnState;
    /** La largeur de la fenêtre, d'où se déduisent les bornes. */
    viewportWidth(): number;
    /** Montrer une largeur sans rien décider — le temps du glissement. */
    preview(width: number): void;
    /** Le glissement s'est arrêté sur une largeur : elle part au backend. */
    commitWidth(width: number): void;
    /** Le glissement s'est arrêté sous le plancher : la colonne se referme. */
    collapse(): void;
    /** La touche du séparateur : replier ou déplier, comme `⌘B`. */
    toggle(): void;
}

export interface SidebarResizer {
    readonly element: HTMLElement;
    /** Réaccorde ce que le séparateur annonce à ce que le backend vient de dire. */
    update(): void;
}

/** La classe posée sur la racine le temps d'un glissement — voir `sidebar.css`. */
const DRAGGING_CLASS = "ash-resizing";

export function createSidebarResizer(ports: SidebarResizerPorts): SidebarResizer {
    const element = document.createElement("div");
    element.className = "ash-sidebar-resizer";
    // `separator` avec `tabindex` est le rôle d'un séparateur **déplaçable** : il accepte un
    // nom, une orientation et une valeur, et c'est ce qui le rend atteignable au clavier.
    element.setAttribute("role", "separator");
    element.setAttribute("aria-orientation", "vertical");
    element.setAttribute("aria-label", "Largeur de la colonne");
    element.setAttribute("aria-valuemin", String(MIN_WIDTH_FRACTION * 100));
    element.setAttribute("aria-valuemax", String(MAX_WIDTH_FRACTION * 100));
    element.tabIndex = 0;

    const handle = document.createElement("span");
    handle.className = "ash-sidebar-handle";
    // La poignée est un pur signal : elle n'a rien à dire à un lecteur d'écran, qui entend
    // déjà le séparateur qui la porte.
    handle.setAttribute("aria-hidden", "true");
    element.append(handle);

    /**
     * Le geste en cours : l'écart au trait retenu à la saisie, et ce que le dernier
     * `pointermove` a donné.
     *
     * `outcome` reste `null` tant que rien n'a bougé, et c'est ce qui distingue un glissement
     * d'un **simple clic** : cliquer le bord sans glisser ne change aucune largeur et
     * n'annonce rien au backend.
     */
    let dragging: { grab: number; outcome: DragOutcome | null } | null = null;

    // Le séparateur ne porte **pas** de classe de repli : la colonne repliée se dessine par
    // sa propre `is-collapsed` (`view.ts`), et le séparateur, lui, se place sur le bord par la
    // seule variable de largeur — il n'a rien à savoir de plus, et une classe qu'aucune règle
    // ne lit ferait croire à un point d'accroche qui n'existe pas.
    const update = (): void => {
        const column = ports.column();
        element.setAttribute("aria-valuenow", String(widthPercent(column.width, ports.viewportWidth())));
    };

    /** La poignée suit la hauteur du curseur tant qu'il longe le bord. */
    const followPointer = (event: PointerEvent): void => {
        const box = element.getBoundingClientRect();
        handle.style.top = `${handleOffset(event.clientY, box.top, box.height)}px`;
    };

    element.addEventListener("pointermove", (event) => {
        followPointer(event);
        if (dragging === null) return;

        // L'écart au trait est celui de la saisie, jamais celui de l'image en cours : c'est ce
        // qui fait que le bord suit le pointeur au lieu de le rejoindre d'un saut.
        const outcome = dragOutcome(event.clientX, ports.viewportWidth(), dragging.grab);
        dragging = { grab: dragging.grab, outcome };
        ports.preview(outcome.width);
    });

    element.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        // La capture est ce qui garde la poignée visible et le curseur en `col-resize` quand
        // le pointeur sort de la zone — c'est-à-dire pendant tout glissement un peu vif.
        element.setPointerCapture(event.pointerId);
        // Mesuré une fois, au contact, et sur la boîte réelle de la zone : le trait est à
        // `GRAB_OVERHANG` de son bord gauche, quelle que soit la largeur du moment — repliée
        // comprise. Rien n'est montré ni annoncé tant que le pointeur n'a pas bougé.
        dragging = { grab: grabOffset(event.clientX, element.getBoundingClientRect().left), outcome: null };
        element.classList.add("is-dragging");
        document.documentElement.classList.add(DRAGGING_CLASS);
    });

    const finish = (event: PointerEvent): void => {
        const outcome = dragging?.outcome ?? null;
        dragging = null;
        element.classList.remove("is-dragging");
        document.documentElement.classList.remove(DRAGGING_CLASS);
        if (element.hasPointerCapture(event.pointerId)) element.releasePointerCapture(event.pointerId);
        // Un clic sans glissement : rien n'a été montré, rien n'est décidé. Commettre la
        // largeur d'un pointeur immobile est exactement le saut que l'écart de saisie évite.
        if (outcome === null) return;

        // Relâcher sous le plancher referme la colonne, **sans toucher à la largeur** : c'est
        // ce qui la restitue telle quelle au prochain `⌘B`.
        if (outcome.willCollapse) ports.collapse();
        else ports.commitWidth(outcome.width);
    };

    element.addEventListener("pointerup", finish);
    // Un glissement peut aussi finir sans `pointerup` — un `⌘Tab`, une notification qui prend
    // le pointeur. La colonne doit alors garder ce qu'on voyait, pas revenir en arrière.
    element.addEventListener("pointercancel", finish);

    element.addEventListener("keydown", (event) => {
        const asked = resizeByKey(event.key, ports.column(), ports.viewportWidth());
        if (asked === null) return;
        event.preventDefault();
        if (asked.kind === "toggle") ports.toggle();
        else ports.commitWidth(asked.width);
    });

    update();
    return { element, update };
}
