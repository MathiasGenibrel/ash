import {
    dragOutcome,
    grabOffset,
    handleOffset,
    heightPercent,
    resizeByKey,
    MAX_HEIGHT_FRACTION,
    MIN_HEIGHT_FRACTION,
    type BottomPanelState,
    type DragOutcome,
    type PanelArea,
} from "./layout";

/**
 * Le séparateur du bord haut du panneau : la zone qu'on attrape, et la poignée qui l'annonce.
 *
 * C'est le même objet que celui de la colonne de gauche, tourné d'un quart de tour, et c'est
 * délibéré : la forme du glissement, les bornes, le repli au relâchement et la persistance au
 * seul relâchement ont déjà été tranchés une fois (#129). Une seconde invention aurait donné
 * deux gestes à apprendre pour la même chose.
 *
 * **Le sujet est la cible, pas le trait.** La bordure fait un pixel ; la zone d'interaction
 * déborde de 7 px de part et d'autre, et elle n'est jamais dessinée. Ce qui se voit est une
 * poignée qui se déploie depuis la bordure à l'abscisse du curseur.
 *
 * **Ce qui se voit pendant le glissement n'est pas ce qui est gardé.** La hauteur suit le
 * pointeur image par image — un fait d'affichage —, et seul le **relâchement** part au
 * backend. Ici, l'enjeu est plus lourd que pour la colonne : chaque hauteur commise fait
 * refaire sa grille au terminal, donc poste un `SIGWINCH` à la TUI qui y tourne
 * ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)).
 *
 * Le séparateur ne prend le focus qu'au clavier : `pointerdown` appelle `preventDefault`, donc
 * attraper le bord à la souris ne retire pas le clavier au terminal (ADR-0003, ADR-0010).
 */
export interface PanelResizerPorts {
    /** Le panneau tel que le backend l'a annoncé — jamais un état gardé ici. */
    panel(): BottomPanelState;
    /** La zone dans laquelle le panneau se règle, mesurée par qui possède la mise en page. */
    area(): PanelArea;
    /** Montrer une hauteur sans rien décider — le temps du glissement. */
    preview(height: number): void;
    /** Le glissement s'est arrêté sur une hauteur : elle part au backend. */
    commitHeight(height: number): void;
    /** Le glissement s'est arrêté sous le plancher, ou `Échap` : le panneau se referme. */
    close(): void;
}

export interface PanelResizer {
    readonly element: HTMLElement;
    /** Réaccorde ce que le séparateur annonce à ce que le backend vient de dire. */
    update(): void;
}

/** La classe posée sur la racine le temps d'un glissement — voir `panel.css`. */
const DRAGGING_CLASS = "ash-resizing-panel";

export function createPanelResizer(ports: PanelResizerPorts): PanelResizer {
    const element = document.createElement("div");
    element.className = "ash-panel-resizer";
    element.setAttribute("role", "separator");
    element.setAttribute("aria-orientation", "horizontal");
    element.setAttribute("aria-label", "Hauteur du panneau");
    element.setAttribute("aria-valuemin", String(MIN_HEIGHT_FRACTION * 100));
    element.setAttribute("aria-valuemax", String(MAX_HEIGHT_FRACTION * 100));
    element.tabIndex = 0;

    const handle = document.createElement("span");
    handle.className = "ash-panel-handle";
    handle.setAttribute("aria-hidden", "true");
    element.append(handle);

    /**
     * Le geste en cours : l'écart au trait retenu à la saisie, et ce que le dernier
     * `pointermove` a donné. `outcome` reste `null` tant que rien n'a bougé — cliquer le bord
     * sans glisser ne change aucune hauteur et n'annonce rien au backend.
     */
    let dragging: { grab: number; outcome: DragOutcome | null } | null = null;

    const update = (): void => {
        const { height } = ports.panel();
        element.setAttribute("aria-valuenow", String(heightPercent(height, ports.area().height)));
    };

    /** La poignée suit l'abscisse du curseur tant qu'il longe le bord. */
    const followPointer = (event: PointerEvent): void => {
        const box = element.getBoundingClientRect();
        handle.style.left = `${handleOffset(event.clientX, box.left, box.width)}px`;
    };

    element.addEventListener("pointermove", (event) => {
        followPointer(event);
        if (dragging === null) return;

        const outcome = dragOutcome(event.clientY, ports.area(), dragging.grab);
        dragging = { grab: dragging.grab, outcome };
        ports.preview(outcome.height);
    });

    element.addEventListener("pointerdown", (event) => {
        if (event.button !== 0) return;
        // `preventDefault` fait deux choses ici, et les deux comptent : il empêche la
        // sélection de texte pendant le glissement, et il **laisse le clavier au terminal** —
        // le panneau ne prend jamais le focus de lui-même (ADR-0003).
        event.preventDefault();
        element.setPointerCapture(event.pointerId);
        dragging = {
            grab: grabOffset(event.clientY, element.getBoundingClientRect().top),
            outcome: null,
        };
        element.classList.add("is-dragging");
        document.documentElement.classList.add(DRAGGING_CLASS);
    });

    const finish = (event: PointerEvent): void => {
        const outcome = dragging?.outcome ?? null;
        dragging = null;
        element.classList.remove("is-dragging");
        document.documentElement.classList.remove(DRAGGING_CLASS);
        if (element.hasPointerCapture(event.pointerId))
            element.releasePointerCapture(event.pointerId);
        if (outcome === null) return;

        // Relâcher sous le plancher referme, **sans toucher à la hauteur** : c'est ce qui la
        // restitue telle quelle à la prochaine ouverture.
        if (outcome.willCollapse) ports.close();
        else ports.commitHeight(outcome.height);
    };

    element.addEventListener("pointerup", finish);
    // Un glissement peut aussi finir sans `pointerup` — un `⌘Tab`, une notification qui prend
    // le pointeur. Le panneau doit alors garder ce qu'on voyait, pas revenir en arrière.
    element.addEventListener("pointercancel", finish);

    element.addEventListener("keydown", (event) => {
        const asked = resizeByKey(event.key, ports.panel(), ports.area().height);
        if (asked === null) return;
        event.preventDefault();
        if (asked.kind === "close") ports.close();
        else ports.commitHeight(asked.height);
    });

    update();
    return { element, update };
}
