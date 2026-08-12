import { describe, expect, it } from "bun:test";

import { badge } from "./marks";
import { button } from "./button";
import { column, row } from "./layout";
import { plainText } from "./read";
import { ElementBuilder, text, toNode } from "./node";

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

    it("Given a component that carries a field named kind, when a container takes it as a child, then it is still built", () => {
        // Given — une carte d'outil de `features/settings/components/` a le droit de nommer
        // un champ `kind` ; un conteneur qui reconnaîtrait la description à ce champ la
        // prendrait pour une description et produirait un arbre faux, sans rien signaler.
        class ToolCard extends ElementBuilder {
            readonly kind = "claude-code";

            constructor() {
                super("div", "tool-card");
                this.add(text("claude"));
            }
        }

        // When
        const described = row(new ToolCard()).build();

        // Then
        expect(described.children.map((child) => child.kind)).toEqual(["element"]);
        expect(plainText(described)).toBe("claude");
    });

    it("Given an attribute whose rule belongs to a primitive, when a component writes it by hand, then it does not compile", () => {
        // Given / When — `attr` est l'échappatoire du socle : si elle peut réécrire
        // `disabled`, la raison obligatoire d'un bouton éteint n'est plus qu'une convention.
        // `@ts-expect-error` casse le jour où le nom redevient permis.
        // @ts-expect-error un bouton ne s'éteint que par `disabled(reason)`
        const muted = button("install").attr("disabled", "");
        // @ts-expect-error un gestionnaire passe par `on`, jamais par un attribut
        const inline = badge("claude").attr("onclick", "alert(1)");

        // Then — un nom calculé, lui, reste permis
        const key = `data-${"tool"}`;
        const tagged = badge("claude").attr(key, "claude-code");
        expect(tagged.build().attrs[key]).toBe("claude-code");
        expect([muted, inline]).toHaveLength(2);
    });
});
