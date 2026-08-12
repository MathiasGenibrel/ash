import { describe, expect, it } from "bun:test";

import { badge } from "./marks";
import { column, row } from "./layout";
import { plainText } from "./read";
import { text, toNode } from "./node";

describe("une description d'interface", () => {
    it("Given a container built from other builders, when it is described, then the children are resolved without an explicit build", () => {
        // Given — l'oubli d'un `.build()` sur un enfant ne se voit pas à la lecture ; un
        // conteneur qui accepte un constructeur enlève la question.
        const built = row(badge("claude"), text(" · "), column(badge("codex")));

        // When
        const described = built.build();

        // Then
        expect(described.children.map((child) => child.kind)).toEqual([
            "element",
            "text",
            "element",
        ]);
        expect(plainText(described)).toBe("claude · codex");
    });

    it("Given a builder, when it is described twice, then the second description is not the first one's array", () => {
        // Given — une description est une valeur : si `build()` rendait ses tableaux
        // internes, un composant pourrait modifier après coup ce qu'un autre a déjà rendu.
        const built = row(badge("claude"));

        // When
        const first = built.build();
        built.add(badge("codex"));
        const second = built.build();

        // Then
        expect(first.children).toHaveLength(1);
        expect(second.children).toHaveLength(2);
    });

    it("Given an empty class name, when it is added to a builder, then it does not become a stray space in the class list", () => {
        // Given — les variantes du dépôt s'écrivent `button(label, variant)` avec un
        // variant vide par défaut ; une classe vide produirait `class="ui-button "`.
        const built = badge("claude").class("", "is-primary");

        // When
        const described = built.build();

        // Then
        expect(described.classes).toEqual(["ui-badge", "is-primary"]);
    });

    it("Given a plain description, when it is passed where a builder is expected, then it is taken as is", () => {
        // Given
        const node = text("claude");

        // When
        const resolved = toNode(node);

        // Then
        expect(resolved).toBe(node);
    });
});
