/**
 * API publique de la feature panneau bas.
 *
 * Le reste du frontend n'importe que ce fichier : ni `layout`, ni `resizer`, ni `strip` ne
 * sont des points d'entrée.
 *
 * **Le panneau ne contient jamais de terminal, et ne prend jamais le focus clavier par
 * lui-même** ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md), reformulation du
 * 2026-08-10). Ce sont les deux règles qui bornent cette surface, et elles se lisent dans le
 * code : rien ici n'appelle `focus()`, la barre d'onglets annule le `mousedown` qui aurait
 * déplacé les doigts, et le corps du panneau est un élément vide que d'autres features
 * rempliront.
 *
 * **Il ne détient rien non plus.** Sa hauteur, son ouverture et sa vue vivent en Rust avec
 * les autres préférences d'apparence
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) ; le panneau les rend, et
 * les gestes partent au backend. Ce qui suit le pointeur pendant un glissement est un fait
 * d'affichage, et le reste.
 *
 * Il ne sait **rien de git** : il nomme quatre surfaces, dont le contenu appartient à #27,
 * #28, #30 et #31.
 */

import "./panel.css";

import { paint } from "@/shared/ui";

import {
    appliedHeight,
    DEFAULT_PANEL_HEIGHT,
    type BottomPanelState,
    type PanelArea,
    type PanelView,
} from "./layout";
import { createPanelResizer } from "./resizer";
import { panelStrip } from "./strip";

export { DEFAULT_PANEL_HEIGHT, PANEL_VIEWS, type BottomPanelState, type PanelView } from "./layout";

/** Ce que le panneau sait demander, et qu'il ne sait pas faire lui-même. */
export interface BottomPanelPorts {
    /**
     * Une vue est demandée — le clic sur une entrée de la barre, et demain un raccourci.
     *
     * Le panneau demande une **vue**, jamais un état : c'est le backend qui décide que
     * redemander celle qui est montrée referme le panneau (ADR-0009).
     */
    showView(view: PanelView): void;
    /** La hauteur réglée au relâchement du bord, ou par une flèche sur le séparateur. */
    setHeight(height: number): void;
    /** Refermer — un glissement relâché sous le plancher, ou `Échap` sur le séparateur. */
    close(): void;
    /**
     * La hauteur de la **zone terminal**, d'où se déduisent les bornes de 15 % et 70 %.
     *
     * Injectée comme la largeur de fenêtre l'est pour la colonne, et pour la même raison :
     * c'est une lecture du monde, et le panneau n'a pas à savoir dans quelle mise en page il
     * a été posé.
     */
    areaHeight(): number;
}

export interface BottomPanel {
    readonly element: HTMLElement;
    /**
     * Là où les vues poseront leur contenu — le graphe (#27), le tableau des worktrees
     * (#28), les conflits (#30), la fiche de branche (#31).
     *
     * Exposé plutôt que rempli ici : le panneau est une surface de mise en page, et il n'a
     * pas à connaître git. Ce qu'il garantit à ce qui s'y posera, c'est une boîte dont la
     * hauteur est réglée et qui ne prend pas le clavier.
     */
    readonly body: HTMLElement;
    /**
     * Le panneau que le backend vient d'annoncer.
     *
     * C'est cette annonce, et elle seule, qui remplace la hauteur montrée pendant un
     * glissement.
     */
    setPanel(panel: BottomPanelState): void;
    /** La fenêtre a changé de taille : la hauteur montrée revient dans ses bornes. */
    layOut(): void;
}

/** Ce sur quoi la fenêtre s'ouvre tant que le backend n'a pas répondu. */
export const CLOSED_PANEL: BottomPanelState = {
    height: DEFAULT_PANEL_HEIGHT,
    open: false,
    view: "graph",
};

export function mountBottomPanel(ports: BottomPanelPorts): BottomPanel {
    let panel: BottomPanelState = CLOSED_PANEL;

    /**
     * La hauteur qui suit le pointeur pendant un glissement, et rien d'autre.
     *
     * Elle n'est pas un second détenteur : elle est effacée par la première annonce du
     * backend. Elle survit au relâchement le temps de l'aller-retour, sans quoi le panneau
     * reviendrait d'une image à sa hauteur précédente avant de repartir à la bonne — et
     * chacun de ces deux sauts referait la grille du terminal.
     */
    let dragged: number | null = null;

    const element = document.createElement("div");
    element.className = "ash-panel";

    const strip = document.createElement("div");
    strip.className = "ash-panel-strip-host";
    // **Le seul geste de tout le fichier qui touche au focus, et il le préserve.** Sans lui,
    // cliquer un onglet du panneau retirerait le clavier au terminal : la frappe suivante
    // n'irait nulle part, et l'utilisateur ne verrait aucune raison à ça. Posé sur l'hôte
    // plutôt que sur chaque bouton — `mousedown` remonte, et la barre est reconstruite à
    // chaque rendu.
    strip.addEventListener("mousedown", (event) => {
        event.preventDefault();
    });

    const body = document.createElement("div");
    body.className = "ash-panel-body";

    const resizer = createPanelResizer({
        panel: () => panel,
        area: (): PanelArea => ({
            bottom: element.getBoundingClientRect().bottom,
            height: ports.areaHeight(),
        }),
        preview: (height) => {
            dragged = height;
            layOut();
        },
        commitHeight: (height) => {
            dragged = height;
            layOut();
            ports.setHeight(height);
        },
        close: () => {
            dragged = null;
            ports.close();
        },
    });

    element.append(resizer.element, strip, body);

    /**
     * Pose la hauteur du panneau, en une seule propriété, sur la racine du document.
     *
     * Même chemin que `--ash-sidebar-width`, et pour la même raison : une valeur de mise en
     * page, lue par du CSS. `appliedHeight` est rappelée à **chaque** pose, et pas seulement
     * quand la hauteur change : c'est ce qui fait que réduire la fenêtre rend ses lignes au
     * terminal sans jamais réécrire la hauteur qu'on a réglée.
     */
    const layOut = (): void => {
        const shown = appliedHeight(dragged ?? panel.height, ports.areaHeight());
        document.documentElement.style.setProperty("--ash-panel-height", `${shown}px`);
        // Pendant un glissement, le panneau est ouvert quoi qu'en dise l'état annoncé : c'est
        // ce qu'on voit qui doit suivre le pointeur.
        element.classList.toggle("is-open", panel.open || dragged !== null);
        resizer.update();
    };

    const draw = (): void => {
        strip.replaceChildren(
            paint(
                panelStrip(panel, (view) => {
                    ports.showView(view);
                }).build(),
            ),
        );
        // Le corps est vide, et il le dit. Le texte est porté par un attribut plutôt qu'écrit
        // dans le DOM : `panel.css` ne l'affiche que sur un `:empty`, donc la première vue qui
        // posera son contenu (#27, #28, #30, #31) le fera disparaître sans avoir à savoir
        // qu'il existait — et sans qu'un rendu du panneau puisse l'effacer.
        body.dataset["empty"] = `${panel.view} — nothing here yet`;
        layOut();
    };

    draw();

    return {
        element,
        body,
        setPanel(next) {
            panel = next;
            // L'annonce du backend **remplace** ce que le glissement montrait : c'est elle
            // qui fait autorité, et le seul endroit où la hauteur montrée redevient la
            // hauteur gardée.
            dragged = null;
            draw();
        },
        layOut,
    };
}
