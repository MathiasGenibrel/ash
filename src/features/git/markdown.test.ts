import { describe, expect, it } from "bun:test";

import { find, findAll, plainText } from "@/shared/ui";

import { markdown, progressOf, readCard } from "./markdown";

/**
 * Test Data Builder : une fiche telle qu'ADR-0013 la décrit — front matter, cases à cocher,
 * tableau, clôture `mermaid`, et le bloc d'Ash.
 *
 * Les défauts sont valides et déterministes ; chaque test ne surcharge que ce qui l'intéresse.
 */
class CardSource {
    private meta = "type: feat\nissue: 31\nbranch: feat/branch-card\n";
    private body = "# why\n\n- [x] read the adr\n- [ ] write the block\n";

    withoutFrontMatter(): this {
        this.meta = "";
        return this;
    }

    saying(body: string): this {
        this.body = body;
        return this;
    }

    build(): string {
        return this.meta === "" ? this.body : `---\n${this.meta}---\n\n${this.body}`;
    }
}

describe("la fiche lue", () => {
    it("Given a card that opens with front matter, when it is read, then the metadata leaves the body", () => {
        // Given — ADR-0013 : « front matter YAML pour les métadonnées ». Les laisser dans le
        // corps les ferait rendre comme un paragraphe de texte, sous une ligne horizontale.
        const source = new CardSource().build();

        // When
        const card = readCard(source);

        // Then
        expect(card.meta).toEqual([
            { key: "type", value: "feat" },
            { key: "issue", value: "31" },
            { key: "branch", value: "feat/branch-card" },
        ]);
        expect(card.body.startsWith("\n# why")).toBe(true);
    });

    it("Given a card whose first line is a horizontal rule, when it is read, then nothing is taken for metadata", () => {
        // Given — un `---` isolé n'ouvre pas un front matter : sans fermeture, tout le
        // document deviendrait des métadonnées, et le corps disparaîtrait de l'écran.
        const source = new CardSource().withoutFrontMatter().saying("---\n\n# why\n").build();

        // When
        const card = readCard(source);

        // Then
        expect(card.meta).toEqual([]);
        expect(card.body).toBe(source);
    });
});

describe("la barre de progression", () => {
    it("Given a card with ticked and unticked boxes, when its progress is counted, then it says how many are done", () => {
        // Given — « les cases `- [ ]` deviennent la barre de progression » (ADR-0013)
        const body = "- [x] one\n- [X] two\n- [ ] three\n- not a task\n";

        // When
        const progress = progressOf(body);

        // Then
        expect(progress).toEqual({ done: 2, total: 3 });
    });

    it("Given a card that documents the checkbox syntax in a code fence, when its progress is counted, then the example does not move the bar", () => {
        // Given — une fiche qui explique comment on l'écrit. Compter son exemple ferait
        // avancer la barre en parlant d'elle-même.
        const body = "- [x] done\n\n```markdown\n- [ ] à faire\n- [ ] à faire\n```\n";

        // When
        const progress = progressOf(body);

        // Then
        expect(progress).toEqual({ done: 1, total: 1 });
    });
});

describe("le rendu du corps", () => {
    it("Given a card carrying angle brackets, when it is rendered, then they stay text and never become markup", () => {
        // Given — la fiche est écrite par des **agents**, dans un dépôt qu'on vient
        // peut-être de cloner. La réponse à « et si elle contient une balise ? » doit être
        // ennuyeuse : rien du rendu ne passe par `innerHTML`.
        const body = '<img src=x onerror="alert(1)"> et <b>gras</b>\n';

        // When
        const rendered = markdown(body);

        // Then — le texte est là, en toutes lettres, et aucun élément `img` n'a été créé
        expect(plainText(rendered)).toContain('<img src=x onerror="alert(1)">');
        const tags = JSON.stringify(rendered.build());
        expect(tags).not.toContain('"tag":"img"');
        expect(tags).not.toContain('"tag":"b"');
    });

    it("Given a task list, when it is rendered, then each item shows whether it is ticked", () => {
        // Given — les cases se rendent, elles ne se cliquent pas : cocher écrirait dans un
        // fichier dont Ash ne possède que le bloc `ash:log` (ADR-0013).
        const body = "- [x] read the adr\n- [ ] write the block\n";

        // When
        const rendered = markdown(body);

        // Then
        const items = findAll(rendered, "ash-md-task");
        expect(items.map((item) => item.classes.includes("is-done"))).toEqual([true, false]);
        expect(items.every((item) => item.on["click"] === undefined)).toBe(true);
    });

    it("Given a table, when it is rendered, then it stays a table", () => {
        // Given — « un tableau reste un tableau » (ADR-0013). C'est aussi la forme du bloc
        // `ash:log` lui-même : le rendre en paragraphe rendrait le journal illisible.
        const body = "| agent | work | when |\n|---|---|---|\n| claude | 4 commits · 15m22s | now |\n";

        // When
        const rendered = markdown(body);

        // Then
        const table = find(rendered, "ash-md-table");
        expect(table?.tag).toBe("table");
        expect(findAll(rendered, "ash-md-row")).toHaveLength(2);
        expect(plainText(rendered)).toContain("4 commits · 15m22s");
    });

    it("Given a mermaid fence, when it is rendered, then its source is shown rather than a diagram invented", () => {
        // Given — dessiner du mermaid impose une dépendance de rendu qu'ADR-0013 nomme
        // comme une conséquence à peser. Tant qu'elle n'est pas prise, l'écran doit dire ce
        // qu'il montre : une source, pas un schéma qui aurait échoué.
        const body = "```mermaid\nstateDiagram-v2\n  idle --> working\n```\n";

        // When
        const rendered = markdown(body);

        // Then
        expect(find(rendered, "is-mermaid")?.tag).toBe("pre");
        expect(plainText(rendered)).toContain("shown as source");
        expect(plainText(rendered)).toContain("idle --> working");
    });

    it("Given a link, when it is rendered, then it is text rather than something the window would follow", () => {
        // Given — la fiche vient d'un dépôt, donc d'ailleurs. Une webview d'application n'a
        // rien à ouvrir sur un clic qu'elle n'a pas décidé.
        const body = "voir [l'ADR](https://exemple.invalid/adr)\n";

        // When
        const rendered = markdown(body);

        // Then
        expect(find(rendered, "ash-md-link")?.tag).toBe("span");
        expect(JSON.stringify(rendered.build())).not.toContain('"href"');
        expect(plainText(rendered)).toContain("l'ADR");
    });
});
