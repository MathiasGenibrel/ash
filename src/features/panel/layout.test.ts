import { describe, expect, it } from "bun:test";

import {
    appliedHeight,
    dragOutcome,
    grabOffset,
    handleOffset,
    resizeByKey,
    GRAB_OVERHANG,
    KEYBOARD_STEP,
    MAX_HEIGHT_FRACTION,
    MIN_HEIGHT_FRACTION,
    type BottomPanelState,
    type PanelArea,
} from "./layout";

/**
 * Une zone terminal de 600 px dont le bas de la partie réglable est à 700 : les nombres sont
 * ronds, et surtout **différents l'un de l'autre** — la hauteur de la zone sert aux bornes,
 * son bord bas sert au glissement, et les confondre est exactement la faute qu'un `bottom`
 * égal à `height` laisserait passer.
 */
function area(overrides: Partial<PanelArea> = {}): PanelArea {
    return { bottom: 700, height: 600, ...overrides };
}

/** Test Data Builder : un panneau ouvert au tiers de la zone, dont on surcharge le nécessaire. */
function panel(overrides: Partial<BottomPanelState> = {}): BottomPanelState {
    return { height: 200, open: true, view: "graph", ...overrides };
}

describe("the bottom panel takes its height from the terminal", () => {
    it("Given a drag that goes on climbing, when the ceiling is reached, then the panel stops there", () => {
        // Given — le panneau prend sa hauteur au terminal : le laisser couvrir la fenêtre
        // ferait redessiner `vim` sur deux lignes (ADR-0003)
        const zone = area();

        // When — le pointeur monte bien au-dessus du haut de la zone
        const outcome = dragOutcome(-400, zone);

        // Then
        expect(outcome.height).toBe(zone.height * MAX_HEIGHT_FRACTION);
        expect(outcome.willCollapse).toBe(false);
    });

    it("Given a drag pushed below the floor, when it is still held, then the panel stops but announces it would close", () => {
        // Given — le geste de la colonne de gauche, à l'horizontale : on montre le plancher,
        // et c'est le relâchement qui décide
        const zone = area();

        // When — le pointeur descend sous le bas de la zone réglable
        const outcome = dragOutcome(zone.bottom + 30, zone);

        // Then — un panneau qui rétrécirait jusqu'à zéro ne dirait plus quand relâcher referme
        expect(outcome.height).toBe(zone.height * MIN_HEIGHT_FRACTION);
        expect(outcome.willCollapse).toBe(true);
    });

    it("Given the edge grabbed beside the line, when the pointer moves, then the edge follows instead of jumping to it", () => {
        // Given — la zone attrapable déborde de 7 px de part et d'autre du trait : on attrape
        // à 5 px au-dessus, le trait lui-même étant à `zoneTop + GRAB_OVERHANG`
        const zone = area();
        const zoneTop = zone.bottom - 250 - GRAB_OVERHANG;
        const grab = grabOffset(zoneTop + GRAB_OVERHANG - 5, zoneTop);

        // When — le pointeur monte de 40 px depuis l'endroit exact où il a saisi
        const outcome = dragOutcome(zoneTop + GRAB_OVERHANG - 5 - 40, zone, grab);

        // Then — glisser de 40 px déplace le bord de 40 px, d'où qu'on soit parti dans les
        // 15 px : sans l'écart, la cible facile à atteindre punirait celui qui l'atteint
        expect(outcome.height).toBe(290);
    });

    it("Given a panel taller than the window can now hold, when the layout is laid out again, then it is brought back without being rewritten", () => {
        // Given — une hauteur réglée sur un grand écran, relue dans une petite fenêtre
        const kept = 500;

        // When
        const shown = appliedHeight(kept, 300);

        // Then — la hauteur gardée reste intacte sur le disque et se retrouve telle quelle
        // quand la fenêtre reprend sa taille : c'est l'affichage qui la borne
        expect(shown).toBe(300 * MAX_HEIGHT_FRACTION);
    });
});

describe("the separator of the bottom panel", () => {
    it("Given a closed panel, when an arrow is pressed on its separator, then nothing is resized", () => {
        // Given — le panneau vient de rendre sa hauteur au terminal
        const closed = panel({ open: false });

        // When
        const asked = resizeByKey("ArrowUp", closed, 600);

        // Then — une flèche qui rouvrirait reprendrait au terminal la place qu'on venait de
        // lui rendre, sur un appui distrait
        expect(asked).toBeNull();
    });

    it("Given an open panel, when an arrow is pressed, then it moves by one step and stays within bounds", () => {
        // Given
        const open = panel({ height: 200 });

        // When
        const asked = resizeByKey("ArrowUp", open, 600);

        // Then
        expect(asked).toEqual({ kind: "height", height: 200 + KEYBOARD_STEP });
    });

    it("Given a panel already at its floor, when the down arrow insists, then it does not close", () => {
        // Given — refermer se demande, ça ne s'obtient pas en insistant
        const zone = 600;
        const grounded = panel({ height: zone * MIN_HEIGHT_FRACTION });

        // When
        const asked = resizeByKey("ArrowDown", grounded, zone);

        // Then
        expect(asked).toEqual({ kind: "height", height: zone * MIN_HEIGHT_FRACTION });
    });

    it("Given a panel that has the keyboard on its separator, when Escape is pressed, then the height goes back to the terminal", () => {
        // Given / When
        const asked = resizeByKey("Escape", panel(), 600);

        // Then
        expect(asked).toEqual({ kind: "close" });
    });
});

describe("the handle of the bottom panel", () => {
    it("Given a pointer near the corner of the edge, when the handle follows it, then it never leaves the edge", () => {
        // Given — la poignée suit le pointeur le long du bord ; aux deux extrémités elle
        // déborderait de la zone terminal
        const edgeLeft = 240;
        const edgeWidth = 800;

        // When
        const offsets = [
            handleOffset(edgeLeft - 100, edgeLeft, edgeWidth),
            handleOffset(edgeLeft + 400, edgeLeft, edgeWidth),
            handleOffset(edgeLeft + edgeWidth + 100, edgeLeft, edgeWidth),
        ];

        // Then
        expect(offsets).toEqual([18, 400, edgeWidth - 18]);
    });
});
