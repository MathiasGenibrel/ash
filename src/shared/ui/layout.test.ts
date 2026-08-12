import { describe, expect, it } from "bun:test";

import { badge } from "./marks";
import { find, findAll } from "./read";
import { row } from "./layout";

describe("les conteneurs", () => {
    it("Given a row whose end is pushed away, when it is described, then the spacer sits between the two sides", () => {
        // Given — les trois vues du dépôt poussent leurs actions à droite avec un `flex: 1`
        const described = row(badge("tools")).spacer().add(badge("2")).build();

        // When
        const kinds = described.children.map((child) =>
            child.kind === "element" ? child.classes[0] : "text",
        );

        // Then
        expect(kinds).toEqual(["ui-badge", "ui-spacer", "ui-badge"]);
    });

    it("Given nested containers, when a class is looked up, then it is found at any depth", () => {
        // Given — un test de feature descendra par classe, pas par position : sinon il
        // casserait au premier nœud ajouté au milieu
        const described = row(row(row(badge("claude").class("is-active")))).build();

        // When / Then
        expect(find(described, "is-active")).not.toBeNull();
        expect(findAll(described, "ui-row")).toHaveLength(3);
    });
});
