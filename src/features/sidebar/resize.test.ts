import { describe, expect, it } from "bun:test";

import {
    appliedWidth,
    dragOutcome,
    handleOffset,
    KEYBOARD_STEP,
    resizeByKey,
    widthPercent,
    type SidebarColumnState,
} from "./resize";

/** La colonne telle que le backend l'annonce. Défauts valides, et surcharge de l'utile. */
function column(overrides: Partial<SidebarColumnState> = {}): SidebarColumnState {
    return { width: 240, collapsed: false, ...overrides };
}

describe("les bornes du glissement", () => {
    it("Given a drag pushed past 80% of the window, when the width is resolved, then it stops at 80%", () => {
        // Given — une fenêtre de 1000 px, et un pointeur emmené bien au-delà du plafond
        const windowWidth = 1000;

        // When
        const outcome = dragOutcome(950, windowWidth);

        // Then — au-delà, le terminal n'a plus de quoi montrer 80 colonnes
        expect(outcome.width).toBe(800);
        expect(outcome.willCollapse).toBe(false);
    });

    it("Given a drag pulled below 10% of the window, when the width is resolved, then the column stops at 10% and stays open", () => {
        // Given — le pointeur est passé sous le plancher, mais le bouton n'est pas relâché
        const windowWidth = 1000;

        // When
        const outcome = dragOutcome(40, windowWidth);

        // Then — la colonne s'immobilise ; rien n'est refermé tant qu'on tient le bord
        expect(outcome.width).toBe(100);
        expect(outcome.willCollapse).toBe(true);
    });

    it("Given a pointer between the two bounds, when the width is resolved, then the column follows it", () => {
        // Given
        const windowWidth = 1200;

        // When
        const outcome = dragOutcome(360, windowWidth);

        // Then
        expect(outcome).toEqual({ width: 360, willCollapse: false });
    });
});

describe("la fenêtre qui rétrécit", () => {
    it("Given a column wider than 80% of a shrunken window, when it is laid out, then it comes back inside its bounds", () => {
        // Given — une colonne de 600 px réglée sur un grand écran, puis une fenêtre réduite
        const kept = column({ width: 600 });

        // When
        const applied = appliedWidth(kept.width, 500);

        // Then — réduire la fenêtre ne fait jamais sortir la colonne de ses bornes
        expect(applied).toBe(400);
    });

    it("Given a column narrowed only by a small window, when the window grows back, then the kept width comes back untouched", () => {
        // Given — la largeur gardée n'est pas réécrite quand l'affichage la ramène : c'est
        // ce qui distingue une contrainte de mise en page d'un réglage
        const kept = column({ width: 600 });

        // When
        const narrow = appliedWidth(kept.width, 500);
        const wide = appliedWidth(kept.width, 1600);

        // Then
        expect(narrow).toBe(400);
        expect(wide).toBe(600);
    });
});

describe("le clavier sur le séparateur", () => {
    it("Given a focused separator, when an arrow key is pressed, then the column moves by one step", () => {
        // Given
        const focused = column({ width: 300 });

        // When
        const wider = resizeByKey("ArrowRight", focused, 1000);
        const narrower = resizeByKey("ArrowLeft", focused, 1000);

        // Then
        expect(wider).toEqual({ kind: "width", width: 300 + KEYBOARD_STEP });
        expect(narrower).toEqual({ kind: "width", width: 300 - KEYBOARD_STEP });
    });

    it("Given a column already at its floor, when the left arrow is pressed again, then it is not closed", () => {
        // Given — insister sur une flèche ne doit pas refermer : refermer se demande
        const atFloor = column({ width: 100 });

        // When
        const asked = resizeByKey("ArrowLeft", atFloor, 1000);

        // Then
        expect(asked).toEqual({ kind: "width", width: 100 });
    });

    it("Given a collapsed column, when an arrow key is pressed, then nothing is resized", () => {
        // Given — une colonne refermée n'a pas de largeur à ajuster à l'aveugle
        const closed = column({ collapsed: true });

        // When
        const asked = resizeByKey("ArrowLeft", closed, 1000);

        // Then
        expect(asked).toBeNull();
    });

    it("Given a focused separator, when Enter or Space is pressed, then the column is toggled", () => {
        // Given — la touche qui replie et déplie, dans les deux sens
        const open = column();
        const closed = column({ collapsed: true });

        // When / Then
        expect(resizeByKey("Enter", open, 1000)).toEqual({ kind: "toggle" });
        expect(resizeByKey(" ", closed, 1000)).toEqual({ kind: "toggle" });
    });

    it("Given a key the separator knows nothing about, when it is pressed, then it is let through", () => {
        // Given — le séparateur est focalisable : `Tab`, `⌘B` et le reste doivent ressortir
        // intacts, sinon le focus s'y piégerait
        const focused = column();

        // When / Then
        expect(resizeByKey("Tab", focused, 1000)).toBeNull();
        expect(resizeByKey("b", focused, 1000)).toBeNull();
    });
});

describe("la poignée", () => {
    it("Given a pointer along the edge, when the handle is placed, then it sits at the pointer's height", () => {
        // Given — un bord de 600 px de haut, posé sous la bande de titre
        const edgeTop = 38;

        // When
        const offset = handleOffset(300, edgeTop, 600);

        // Then
        expect(offset).toBe(262);
    });

    it("Given a pointer at the very top or bottom of the edge, when the handle is placed, then it stays inside the column", () => {
        // Given — la maquette la borne à 18 px des deux extrémités pour qu'elle ne déborde pas
        const edgeTop = 0;

        // When
        const top = handleOffset(2, edgeTop, 600);
        const bottom = handleOffset(598, edgeTop, 600);

        // Then
        expect(top).toBe(18);
        expect(bottom).toBe(582);
    });
});

describe("ce que le séparateur annonce", () => {
    it("Given a column at a third of the window, when it is announced, then its value is that share in percent", () => {
        // Given — `aria-valuenow` d'un séparateur déplaçable veut une valeur sur l'échelle de
        // ses bornes, et la seule qui ait un sens ici est le pourcentage de fenêtre
        const third = column({ width: 400 });

        // When
        const announced = widthPercent(third.width, 1200);

        // Then
        expect(announced).toBe(33);
    });
});
