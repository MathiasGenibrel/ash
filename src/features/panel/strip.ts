import { button, row, type UiComponent } from "@/shared/ui";

import { PANEL_VIEWS, type BottomPanelState, type PanelView } from "./layout";

/**
 * Le nom affiché de chaque vue — le `graph │ worktrees │ conflicts` du schéma d'interface
 * de la spec §4, la fiche de branche à la suite.
 *
 * Les libellés sont en anglais, comme le reste du chrome du produit.
 */
const LABELS: Readonly<Record<PanelView, string>> = {
    graph: "graph",
    worktrees: "worktrees",
    conflicts: "conflicts",
    branch: "branch",
};

/**
 * La barre d'onglets du panneau — **toujours visible**, panneau ouvert ou fermé.
 *
 * C'est ce qui rend le panneau atteignable à la souris sans raccourci : les liaisons `⌘⌃G`,
 * `⌘⌃W`, `⌘⌃M` et `⌘⌃I` sont détenues par le magasin de `features::shortcuts` et restent à
 * déclarer (#32). Une barre qui n'apparaîtrait qu'avec le panneau aurait laissé la surface
 * sans porte.
 *
 * **La barre demande une vue, jamais un état.** Recliquer la vue montrée referme le
 * panneau — mais c'est le backend qui le décide, sous son verrou
 * (`features::theme::ThemeState::show_panel_view`). Une bascule calculée ici ferait de la
 * webview le second détenteur de l'ouverture, et le raccourci et le clic finiraient par ne
 * plus dire la même chose
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * Elle ne sait rien de git : elle nomme quatre surfaces dont le contenu appartient à #27,
 * #28, #30 et #31.
 */
export function panelStrip(panel: BottomPanelState, ask: (view: PanelView) => void): UiComponent {
    const tabs = PANEL_VIEWS.map((view) => {
        const showing = panel.open && panel.view === view;
        return button(LABELS[view])
            .class("ash-panel-tab", ...(showing ? ["is-active"] : []))
            .attr("aria-pressed", showing ? "true" : "false")
            .attr("data-view", view)
            .onClick(() => {
                ask(view);
            });
    });

    return row(...tabs).class("ash-panel-strip");
}
