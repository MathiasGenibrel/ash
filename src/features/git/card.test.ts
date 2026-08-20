import { describe, expect, it } from "bun:test";

import type { BranchCard, CardLogState } from "@/shared/ipc";
import { find, plainText } from "@/shared/ui";

import { view, type BranchCardPorts } from "./card";

/** Test Data Builder : la fiche telle que le backend la rend. */
class CardBuilder {
    private card: BranchCard = {
        worktreeRoot: "/dev/ash",
        path: "/dev/ash/.ash/worktree.md",
        otherPath: "/Users/moi/.ash/worktrees/ash-0000000000000000.md",
        mode: "repo",
        ignoredByTheRepo: false,
        exists: true,
        source: "---\ntype: feat\n---\n\n# why\n\n- [x] one\n- [ ] two\n",
        log: {
            state: "stale",
            table: "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n",
            diff: "",
            note: "ash would refresh the ash:log block, and touch nothing else.",
            writable: true,
        },
    };

    refusing(state: CardLogState, note: string, diff: string): this {
        this.card = { ...this.card, log: { ...this.card.log, state, note, diff, writable: false } };
        return this;
    }

    local(): this {
        this.card = { ...this.card, mode: "local", path: this.card.otherPath, otherPath: "/dev/ash/.ash/worktree.md" };
        return this;
    }

    gitignored(): this {
        this.card = { ...this.card, ignoredByTheRepo: true };
        return this;
    }

    build(): BranchCard {
        return this.card;
    }
}

/** Les gestes, enregistrés — la vue ne fait rien elle-même. */
function ports(): BranchCardPorts & { readonly asked: string[] } {
    const asked: string[] = [];
    return {
        asked,
        writeLog: () => asked.push("write"),
        place: (local) => asked.push(`place:${String(local)}`),
    };
}

describe("la fiche de branche", () => {
    it("Given a card and its source, when the view is drawn, then the rendering and the source are both there", () => {
        // Given — spec §7.5 : « rendu à gauche et source à droite ». Les deux volets sont le
        // dispositif : on vérifie d'un coup d'œil que ce qu'Ash affiche est ce que le
        // fichier dit, pour un fichier qu'Ash écrit en partie.
        const card = new CardBuilder().build();

        // When
        const drawn = view(card, ports());

        // Then
        expect(plainText(find(drawn, "ash-card-rendered") ?? drawn)).toContain("why");
        expect(plainText(find(drawn, "ash-card-source") ?? drawn)).toBe(card.source);
    });

    it("Given a block ash refuses to touch, when the view is drawn, then the button stays visible, off, and says why", () => {
        // Given — les deux refus qu'ADR-0013 et la spec §10 nomment. Masquer le bouton
        // ferait croire que la fiche n'a pas de journal, alors que c'est le bloc qui est en
        // conflit ; l'éteindre sans raison ferait croire à une panne.
        const card = new CardBuilder()
            .refusing(
                "conflicted",
                "the ash:log block is in conflict. ash never resolves it",
                "--- the file as it is\n+++ what ash would write\n",
            )
            .build();

        // When
        const drawn = view(card, ports());

        // Then
        const button = find(drawn, "ash-card-write");
        expect(button?.attrs["disabled"]).toBe("");
        expect(button?.attrs["title"]).toContain("never resolves");
        expect(plainText(drawn)).toContain("conflicted");
        // …et le diff est montré, parce que « il signale, propose le diff, et demande »
        expect(find(drawn, "ash-card-diff")).not.toBeNull();
    });

    it("Given a writable block, when the button is pressed, then the view asks and writes nothing itself", () => {
        // Given — la vue ne détient rien et n'écrit rien : elle demande, le backend décide
        // (ADR-0009). C'est ce qui garantit qu'il n'y a qu'un seul chemin vers le fichier.
        const asked = ports();
        const drawn = view(new CardBuilder().build(), asked);

        // When
        find(drawn, "ash-card-write")?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(asked.asked).toEqual(["write"]);
    });

    it("Given a card that lives in a gitignored .ash, when the view is drawn, then it says the card will not travel", () => {
        // Given — ADR-0013 : le mode local « perd son unique avantage ». Le taire laisserait
        // croire qu'une fiche versionnée voyage avec la branche alors que git l'ignore.
        const card = new CardBuilder().gitignored().build();

        // When
        const drawn = view(card, ports());

        // Then
        expect(plainText(find(drawn, "ash-card-warning") ?? drawn)).toContain("gitignored");
    });

    it("Given a card kept out of the repository, when the placement is switched back, then the view says where it would go", () => {
        // Given — l'interrupteur du mode local. Il ne déplace rien : il dit où Ash regarde,
        // et l'écran doit nommer l'autre emplacement pour que le geste soit compréhensible.
        const asked = ports();
        const drawn = view(new CardBuilder().local().build(), asked);

        // When
        const place = find(drawn, "ash-card-place");
        place?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(place?.attrs["title"]).toContain("/dev/ash/.ash/worktree.md");
        expect(asked.asked).toEqual(["place:false"]);
    });
});
