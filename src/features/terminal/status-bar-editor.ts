import { paint } from "@/shared/ui";

import {
    appendSpacer,
    composeDrawer,
    dropIndex,
    editorPills,
    moveItem,
    removeAt,
    type DrawerActions,
    type StatusBarLayout,
    type StatusBarSegmentId,
} from "./status-bar";

/**
 * Le mode édition de la ligne de statut — la vue 5e : des pastilles qu'on glisse, un `×` qui
 * jette, un tiroir qui rend, et l'élastique devenu un objet.
 *
 * Ce module ne décide rien : il pose les pastilles, écoute le pointeur, et rend l'indice où
 * l'on a lâché. Ce qui **décide** — quelle place tombe sous le pointeur, ce que contient le
 * tiroir, ce que devient la barre après un déplacement — est dans `status-bar.ts`, en
 * fonctions pures. C'est la seule façon d'en tester quoi que ce soit : `bun test` ne monte
 * pas de DOM.
 *
 * ## Le geste d'entrée, et la sélection de texte
 *
 * Un clic gauche maintenu **430 ms** ouvre le mode ; le maintien se voit par un trait de 2 px
 * qui file sur le bord haut de la barre. [`LongPress`] arme le compteur, et **le désarme dès
 * que le pointeur bouge de plus de [`SLIP`] pixels** — c'est ce qui laisse la sélection de
 * texte intacte : sélectionner, c'est presser puis glisser, et un glissement n'est jamais un
 * maintien. Les deux gestes ne se disputent donc rien, et la spec §4.2 le dit ainsi plutôt
 * que de trancher en faveur de l'un.
 *
 * Seul le bouton **gauche** arme le compteur : un maintien du bouton droit précède un menu
 * contextuel, et le mode édition lui volerait son geste.
 *
 * ## Pourquoi le pointeur et non le glisser-déposer HTML5
 *
 * La maquette utilise `draggable` et `dragstart`/`dragenter`/`dragover`. Trois raisons de ne
 * pas la suivre ici :
 *
 * - le socle `shared/ui` ne transporte **ni** `dataTransfer` **ni** `preventDefault` — un
 *   `dragover` non annulé refuse tout dépôt, et WKWebView exige un `setData` dans
 *   `dragstart` pour que le glissement démarre. Les deux demanderaient d'élargir `UiEvent`
 *   pour une seule surface ;
 * - un glissement HTML5 dans une fenêtre macOS peut **sortir** de la webview et devenir un
 *   dépôt système : on ne veut pas qu'une pastille de barre de statut atterrisse dans le
 *   Finder ;
 * - le geste d'entrée est **déjà** un `pointerdown` maintenu. Enchaîner sur des événements de
 *   pointeur garde une seule mécanique là où deux se marcheraient dessus.
 *
 * Les mouvements sont écoutés sur le **document** et non sur la pastille, avec ou sans
 * capture : la barre se repeint à chaque réordonnancement, donc la pastille sous le doigt est
 * détruite en cours de geste — une capture posée dessus partirait avec elle.
 */

/** Le maintien qui ouvre le mode édition (vue 5e). */
export const HOLD_MS = 430;

/**
 * Ce qu'un doigt a le droit de trembler sans annuler le maintien, en pixels.
 *
 * Au-delà, le geste est une **sélection de texte** et le compteur se désarme. Quelques pixels
 * plutôt que zéro : une souris bouge d'un ou deux pixels pendant qu'on presse, et exiger
 * l'immobilité parfaite rendrait le mode édition inatteignable au trackpad.
 */
const SLIP = 4;

/**
 * Le compteur du clic maintenu, et le trait qui le montre.
 *
 * Il ne connaît ni la barre ni le mode édition : il dit qu'un maintien est allé au bout. Ce
 * qu'on en fait est décidé par la ligne de statut.
 */
export class LongPress {
    private timer: number | null = null;
    private from: { x: number; y: number } | null = null;

    /**
     * `armed` décide si le geste vaut la peine d'être compté — la ligne de statut y refuse le
     * maintien quand elle est **déjà** en édition, sans quoi chaque pression sur une pastille
     * relancerait un compteur qui n'a plus rien à ouvrir.
     */
    constructor(
        private readonly surface: HTMLElement,
        private readonly progress: HTMLElement,
        private readonly armed: () => boolean,
        private readonly onHeld: () => void,
    ) {
        this.surface.addEventListener("pointerdown", this.onDown);
        // `pointercancel` autant que `pointerup` : c'est lui qui arrive quand le système
        // reprend le pointeur — un défilement, un geste à trois doigts. Sans lui, le trait
        // resterait figé sur la barre.
        for (const name of ["pointerup", "pointercancel", "pointerleave"] as const) {
            this.surface.addEventListener(name, this.cancel);
        }
        this.surface.addEventListener("pointermove", this.onMove);
    }

    private readonly onDown = (event: PointerEvent): void => {
        // `event.button === 0` : le bouton droit ouvre le menu contextuel, et le compteur ne
        // doit pas courir sous lui.
        if (event.button !== 0 || !this.armed()) return;

        this.from = { x: event.clientX, y: event.clientY };
        this.progress.classList.add("is-running");
        this.timer = window.setTimeout(() => {
            this.reset();
            this.onHeld();
        }, HOLD_MS);
    };

    /**
     * Le geste a bougé : c'est une sélection de texte, pas un maintien.
     *
     * Relâcher avant la fin ne fait donc **rien** — le clic garde son comportement d'avant,
     * la branche ancre son popup et la pastille de quota ouvre son popover. C'est le critère
     * de la tâche, et il tient parce que ce module ne consomme aucun événement : il n'appelle
     * ni `preventDefault`, ni `stopPropagation`.
     */
    private readonly onMove = (event: PointerEvent): void => {
        const from = this.from;
        if (from === null) return;
        if (Math.abs(event.clientX - from.x) > SLIP || Math.abs(event.clientY - from.y) > SLIP) {
            this.cancel();
        }
    };

    private readonly cancel = (): void => {
        this.reset();
    };

    private reset(): void {
        if (this.timer !== null) window.clearTimeout(this.timer);
        this.timer = null;
        this.from = null;
        this.progress.classList.remove("is-running");
    }
}

/** Ce que l'éditeur demande au backend. Aucun geste n'est appliqué localement, sauf un. */
export interface EditorBridge {
    /**
     * La barre que le glissement vient de composer — au **relâchement** seulement.
     *
     * Le chemin de `set_bottom_panel_height` : la webview applique la règle de manipulation
     * directe et annonce le résultat, le backend le retient
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    readonly arrange: (layout: StatusBarLayout) => void;
    /** Un segment quitte la barre, ou y revient — la même bascule que le menu contextuel. */
    readonly toggle: (id: StatusBarSegmentId) => void;
    /** La barre reprend sa disposition d'origine. */
    readonly reset: () => void;
    /** `terminé`, `Échap`, un clic ailleurs. */
    readonly done: () => void;
}

/**
 * La surface d'édition : les pastilles dans la barre, le tiroir contre elle.
 *
 * Elle tient **une** chose que le backend ne tient pas : la disposition en cours de
 * glissement. C'est une image intermédiaire d'un geste continu, pas un état — elle disparaît
 * au relâchement, remplacée par ce que le backend annonce. Sans elle, réordonner demanderait
 * un aller-retour Tauri par mouvement de souris.
 */
export class StatusBarEditor {
    /** La rangée de pastilles, posée dans la ligne de statut à la place de son contenu. */
    readonly element: HTMLElement;

    private drawer: HTMLElement | null = null;
    /** La barre telle que le backend l'a annoncée — le point de départ de tout geste. */
    private announced: StatusBarLayout = [];
    /** La barre en cours de composition, pendant un glissement seulement. */
    private dragging: { layout: StatusBarLayout; index: number } | null = null;

    constructor(
        private readonly bar: HTMLElement,
        private readonly bridge: EditorBridge,
    ) {
        this.element = document.createElement("div");
        this.element.className = "status-editor";
        this.element.hidden = true;
    }

    get open(): boolean {
        return this.drawer !== null;
    }

    /** Le clic long, ou la ligne `réorganiser la barre…` du menu. */
    show(layout: StatusBarLayout): void {
        this.announced = layout;
        if (this.drawer === null) {
            const drawer = document.createElement("div");
            drawer.className = "status-drawer-panel";
            drawer.setAttribute("role", "toolbar");
            drawer.setAttribute("aria-label", "arrange the status bar");
            document.body.append(drawer);
            this.drawer = drawer;

            this.element.hidden = false;
            this.bar.dataset["edit"] = "true";
            document.addEventListener("keydown", this.onKeyDown, true);
            document.addEventListener("pointerdown", this.onOutside, true);
        }
        this.repaint();
        this.anchor();
    }

    /** `terminé`, `Échap`, un clic ailleurs — et la fermeture de tout ce qui l'accompagne. */
    close(): void {
        if (this.drawer === null) return;
        document.removeEventListener("keydown", this.onKeyDown, true);
        document.removeEventListener("pointerdown", this.onOutside, true);
        this.endDrag();
        this.drawer.remove();
        this.drawer = null;
        this.element.hidden = true;
        delete this.bar.dataset["edit"];
    }

    /** La barre a changé pendant l'édition — une bascule, un spacer, un retour aux défauts. */
    refresh(layout: StatusBarLayout): void {
        this.announced = layout;
        if (this.drawer !== null) {
            this.repaint();
            this.anchor();
        }
    }

    /** Ce que l'écran montre : la barre en cours de glissement, ou celle du backend. */
    private get shown(): StatusBarLayout {
        return this.dragging?.layout ?? this.announced;
    }

    private repaint(): void {
        const layout = this.shown;

        const pills = editorPills(layout).map((pill) => {
            const element = document.createElement("span");
            element.className = "status-pill";
            element.dataset["beat"] = String(pill.beat);
            if (pill.item === "spacer") element.classList.add("is-spacer");
            if (this.dragging?.index === pill.index) element.classList.add("is-dragging");

            const label = document.createElement("span");
            label.className = "status-pill-label";
            label.textContent = pill.label;

            const drop = document.createElement("button");
            drop.type = "button";
            drop.className = "status-pill-drop";
            drop.textContent = "×";
            drop.title = `remove ${pill.label}`;
            drop.addEventListener("pointerdown", (event) => {
                // Sans ça, le `×` armerait un glissement en même temps qu'il jette.
                event.stopPropagation();
            });
            drop.addEventListener("click", () => {
                this.throwAway(pill.index);
            });

            element.append(label, drop);
            element.addEventListener("pointerdown", (event) => {
                this.startDrag(event, pill.index);
            });
            return element;
        });

        const done = document.createElement("button");
        done.type = "button";
        done.className = "status-editor-done";
        done.textContent = "terminé";
        done.addEventListener("click", () => {
            this.bridge.done();
        });

        this.element.replaceChildren(...pills, done);

        const actions: DrawerActions = {
            onPick: (id) => {
                this.bridge.toggle(id);
            },
            onSpacer: () => {
                this.bridge.arrange(appendSpacer(this.announced));
            },
            onReset: () => {
                this.bridge.reset();
            },
        };
        this.drawer?.replaceChildren(paint(composeDrawer(layout, actions).build()));
    }

    /**
     * Le `×` d'une pastille.
     *
     * Un **segment** part en bascule, comme depuis le menu : c'est le backend qui sait où il
     * reviendra. Un **élastique**, lui, n'a pas d'identité à basculer — c'est une place qui
     * disparaît, donc une disposition nouvelle.
     */
    private throwAway(index: number): void {
        const item = this.announced[index];
        if (item === undefined) return;
        if (item === "spacer") this.bridge.arrange(removeAt(this.announced, index));
        else this.bridge.toggle(item);
    }

    private startDrag(event: PointerEvent, index: number): void {
        if (event.button !== 0) return;
        // Sans ça, le glissement sélectionnerait le libellé de la pastille et laisserait une
        // traînée bleue derrière le doigt.
        event.preventDefault();
        this.dragging = { layout: this.announced, index };
        document.addEventListener("pointermove", this.onDragMove, true);
        document.addEventListener("pointerup", this.onDragEnd, true);
        document.addEventListener("pointercancel", this.onDragEnd, true);
        this.repaint();
    }

    /**
     * Le réordonnancement se fait **pendant** le glissement, et c'est un critère : la barre
     * montre l'ordre nouveau avant qu'on lâche.
     *
     * Les milieux sont relus à chaque mouvement plutôt que mesurés une fois au départ : les
     * pastilles n'ont pas la même largeur, donc déplacer l'une déplace les milieux de toutes
     * les autres.
     */
    private readonly onDragMove = (event: PointerEvent): void => {
        const dragging = this.dragging;
        if (dragging === null) return;

        const centers = [...this.element.querySelectorAll(".status-pill")].map((pill) => {
            const box = pill.getBoundingClientRect();
            return box.left + box.width / 2;
        });
        const to = dropIndex(centers, event.clientX);
        const layout = moveItem(dragging.layout, dragging.index, Math.min(to, centers.length - 1));
        if (layout === dragging.layout) return;

        this.dragging = { layout, index: Math.min(to, centers.length - 1) };
        this.repaint();
    };

    private readonly onDragEnd = (): void => {
        const dragging = this.dragging;
        this.endDrag();
        if (dragging === null) return;

        // Un glissement qui se termine là où il a commencé n'a rien à annoncer — le backend
        // le verrait de toute façon, mais l'aller-retour ferait repeindre la barre pour rien.
        if (dragging.layout !== this.announced) this.bridge.arrange(dragging.layout);
        else this.repaint();
    };

    private endDrag(): void {
        if (this.dragging === null) return;
        this.dragging = null;
        document.removeEventListener("pointermove", this.onDragMove, true);
        document.removeEventListener("pointerup", this.onDragEnd, true);
        document.removeEventListener("pointercancel", this.onDragEnd, true);
    }

    private readonly onKeyDown = (event: KeyboardEvent): void => {
        if (event.key !== "Escape") return;
        event.preventDefault();
        this.bridge.done();
    };

    /**
     * Un clic hors de la barre et hors du tiroir sort du mode.
     *
     * Le tiroir en est exclu pour la raison qui saute aux yeux une fois qu'on l'a vue :
     * cliquer une pastille du tiroir la remet dans la barre, et refermer sur ce clic-là
     * fermerait le mode à chaque ajout.
     */
    private readonly onOutside = (event: Event): void => {
        const target = event.target;
        if (!(target instanceof Node)) return;
        if (this.bar.contains(target) || this.drawer?.contains(target) === true) return;
        this.bridge.done();
    };

    /**
     * Le tiroir est ancré **au-dessus** de la ligne, sur toute sa largeur.
     *
     * La maquette le pose dessous ; dans Ash, la ligne de statut est la dernière rangée de la
     * fenêtre, et « dessous » est hors de l'écran. Il prend donc la place que prennent déjà
     * le popover d'usage et le menu contextuel — et pour la même raison qu'eux : la ligne
     * coupe ce qui la dépasse (`overflow: hidden`), donc rien d'ancré à elle ne peut vivre
     * dedans.
     */
    private anchor(): void {
        const drawer = this.drawer;
        if (drawer === null) return;
        const bounds = this.bar.getBoundingClientRect();
        drawer.style.left = `${String(Math.round(bounds.left))}px`;
        drawer.style.width = `${String(Math.round(bounds.width))}px`;
        drawer.style.bottom = `${String(Math.round(window.innerHeight - bounds.top))}px`;
    }
}
