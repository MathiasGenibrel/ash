import { paint } from "@/shared/ui";

import {
    composeVisibilityMenu,
    type StatusBarSegmentId,
    type VisibilityRow,
} from "./status-bar";

/**
 * Le panneau du menu contextuel, posé dans le document.
 *
 * Il ne décide rien : `status-bar.ts` compose les lignes, ce module les pose, les ancre et
 * les referme. La séparation est celle de tout le dépôt — ce qui décide est pur et testé, ce
 * qui touche au DOM ne décide pas.
 *
 * **Ancré au pied de la fenêtre**, exactement comme le popover d'usage et la popup de
 * branches : la ligne de statut coupe ce qui la dépasse (`overflow: hidden`), donc un
 * panneau ouvert vers le haut ne peut pas y vivre, et l'ouvrir vers le bas le ferait sortir
 * de l'écran.
 */
export class StatusBarMenu {
    private panel: HTMLElement | null = null;

    /**
     * `rows` est relu **à chaque peinture** et non capturé à l'ouverture : les aperçus sont
     * les valeurs courantes, et elles s'égrènent — le `2h14` d'un quota avance pendant que
     * le menu est ouvert.
     */
    constructor(
        private readonly anchorTo: HTMLElement,
        private readonly rows: () => readonly VisibilityRow[],
        private readonly onToggle: (id: StatusBarSegmentId) => void,
    ) {}

    get open(): boolean {
        return this.panel !== null;
    }

    /** Le clic droit sur la ligne : ouvre le menu à cet endroit, ou le referme s'il l'était. */
    toggle(x: number): void {
        if (this.panel !== null) {
            this.close();
            return;
        }

        const panel = document.createElement("div");
        panel.className = "status-bar-menu";
        panel.setAttribute("role", "menu");
        panel.setAttribute("aria-label", "show in the status bar");

        document.body.append(panel);
        this.panel = panel;
        this.repaint();
        this.anchor(panel, x);

        document.addEventListener("pointerdown", this.onPointerDown, true);
        document.addEventListener("contextmenu", this.onContextMenu, true);
    }

    close(): void {
        if (this.panel === null) return;
        document.removeEventListener("pointerdown", this.onPointerDown, true);
        document.removeEventListener("contextmenu", this.onContextMenu, true);
        this.panel.remove();
        this.panel = null;
    }

    /**
     * Redessine le menu **s'il est ouvert** — le battement de la seconde, et le retour d'une
     * bascule.
     *
     * C'est ce qui fait qu'une coche suit le backend plutôt que le clic : le geste part en
     * bascule, l'event revient, et c'est lui qui repeint. Le menu ne détient donc rien
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    refresh(): void {
        if (this.panel !== null) this.repaint();
    }

    private repaint(): void {
        this.panel?.replaceChildren(paint(composeVisibilityMenu(this.rows(), this.onToggle).build()));
    }

    /**
     * Au-dessus de la ligne, à l'abscisse du clic, et **ramené dans la fenêtre s'il
     * déborde** — la règle du popover d'usage, prise dans l'autre sens : celui-ci s'aligne
     * sur un point, pas sur un bord.
     */
    private anchor(panel: HTMLElement, x: number): void {
        const bounds = this.anchorTo.getBoundingClientRect();
        const width = panel.getBoundingClientRect().width;
        const left = Math.min(Math.max(8, x), Math.max(8, window.innerWidth - width - 8));
        panel.style.left = `${String(Math.round(left))}px`;
        panel.style.bottom = `${String(Math.round(window.innerHeight - bounds.top + 6))}px`;
    }

    /**
     * Un clic ailleurs referme.
     *
     * Le clic **secondaire** est laissé passer, et c'est ce qui fait tenir le critère « un
     * second clic droit referme » : un `pointerdown` de bouton droit précède toujours son
     * `contextmenu`, et refermer ici ferait rouvrir le menu un battement plus tard.
     */
    private readonly onPointerDown = (event: Event): void => {
        if (event instanceof PointerEvent && event.button === 2) return;
        const target = event.target;
        if (target instanceof Node && this.panel?.contains(target) === true) return;
        this.close();
    };

    /**
     * Un clic droit **hors de la ligne de statut** referme ; sur la ligne, c'est elle qui
     * bascule — sans quoi les deux gestes s'annuleraient, le menu se refermant puis se
     * rouvrant dans le même battement.
     */
    private readonly onContextMenu = (event: Event): void => {
        const target = event.target;
        if (target instanceof Node && this.anchorTo.contains(target)) return;
        this.close();
    };
}
