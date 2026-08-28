/**
 * Le rendu de la fiche : du markdown standard, et **rien d'autre**.
 *
 * [ADR-0013](../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md) borne le périmètre
 * en une phrase — « le rendu n'invente aucune syntaxe, il met en forme du markdown que
 * n'importe quel éditeur affiche déjà » — et le nomme : front matter YAML, GFM pour le
 * corps (les cases `- [ ]` deviennent la barre de progression, un tableau reste un
 * tableau), clôtures `mermaid` pour les schémas. Pas de MDX, pas de HTML.
 *
 * # Pourquoi c'est écrit ici plutôt que pris dans une bibliothèque
 *
 * Trois raisons, dans cet ordre :
 *
 * 1. **la surface est fermée**, et par une ADR. Ce n'est pas « du markdown » au sens large,
 *    c'est cette liste-là. Une bibliothèque complète apporterait le HTML brut, les entités,
 *    les notes de bas de page et les identifiants automatiques — tout ce que l'ADR écarte ;
 * 2. **rien ne passe par `innerHTML`.** Le rendu produit des `UiNode`, que `paint` pose
 *    comme des éléments et des nœuds texte. Une fiche est écrite par des **agents**, dans un
 *    dépôt qu'on vient peut-être de cloner : la question « que se passe-t-il si elle
 *    contient une balise ? » doit avoir une réponse ennuyeuse, et c'est celle-ci — le
 *    texte s'affiche comme du texte, parce qu'il n'y a pas de chemin par lequel il
 *    deviendrait autre chose ;
 * 3. la dépendance se paierait dans une webview dont le risque de performance est déjà
 *    identifié ([ADR-0002](../../../docs/adr/0002-tauri-rust-portable-pty.md)).
 *
 * **Le `mermaid` n'est pas dessiné**, et c'est dit à l'écran : la clôture est rendue comme
 * un bloc de code étiqueté. Le dessiner demande la dépendance de rendu que l'ADR nomme
 * elle-même comme une conséquence à peser ; elle mérite sa propre tranche, et son propre
 * arbitrage. Un schéma illisible vaut mieux qu'un schéma inventé.
 */

import { column, text, type UiChild, type UiComponent, type UiNode } from "@/shared/ui";

import { tag } from "./tag";

/** Une entrée du front matter — `type: feat`. */
export interface Meta {
    readonly key: string;
    readonly value: string;
}

/** Ce que les cases à cocher du corps disent de l'avancement. */
export interface TaskProgress {
    readonly done: number;
    readonly total: number;
}

/** La fiche, lue. */
export interface CardContent {
    readonly meta: readonly Meta[];
    readonly progress: TaskProgress;
    readonly body: string;
}

/**
 * Sépare le front matter du corps.
 *
 * Il n'est reconnu qu'**en tête de fichier**, entre deux lignes `---` : c'est la règle de
 * tous les outils qui en lisent, et un `---` au milieu d'un document reste une ligne
 * horizontale.
 *
 * Un front matter que YAML refuserait n'est pas une erreur ici : la fiche appartient à
 * l'utilisateur et aux agents, et une clé sans deux-points est rendue comme une ligne de
 * texte plutôt que jetée. Ash ne le lit pas pour en tirer une décision — il l'affiche.
 */
export function readCard(source: string): CardContent {
    const lines = source.split("\n");
    let body = source;
    const meta: Meta[] = [];

    if (lines[0]?.trim() === "---") {
        const end = lines.findIndex((line, at) => at > 0 && line.trim() === "---");
        if (end > 0) {
            for (const line of lines.slice(1, end)) {
                const at = line.indexOf(":");
                if (at <= 0) continue;
                meta.push({
                    key: line.slice(0, at).trim(),
                    value: line.slice(at + 1).trim(),
                });
            }
            body = lines.slice(end + 1).join("\n");
        }
    }

    return { meta, progress: progressOf(body), body };
}

/**
 * Ce que les cases à cocher disent — la barre de progression d'ADR-0013.
 *
 * Les cases d'un **bloc de code** ne comptent pas : une fiche qui documente sa propre
 * syntaxe ferait avancer sa barre en parlant d'elle-même.
 */
export function progressOf(body: string): TaskProgress {
    let done = 0;
    let total = 0;
    let fenced = false;

    for (const line of body.split("\n")) {
        const trimmed = line.trim();
        if (trimmed.startsWith("```") || trimmed.startsWith("~~~")) {
            fenced = !fenced;
            continue;
        }
        if (fenced) continue;
        const box = /^[-*+]\s+\[([ xX])\]\s/.exec(trimmed);
        if (box === null) continue;
        total += 1;
        if (box[1] !== " ") done += 1;
    }
    return { done, total };
}

/** Le corps de la fiche, mis en forme. */
export function markdown(body: string): UiComponent {
    return column(...blocks(body.split("\n"))).class("ash-md");
}

function blocks(lines: readonly string[]): readonly UiChild[] {
    const out: UiChild[] = [];
    let at = 0;

    while (at < lines.length) {
        const line = lines[at] ?? "";
        const trimmed = line.trim();

        if (trimmed === "") {
            at += 1;
            continue;
        }

        const fence = /^(```|~~~)(.*)$/.exec(trimmed);
        if (fence !== null) {
            const closing = fence[1] ?? "```";
            const language = (fence[2] ?? "").trim();
            const content: string[] = [];
            at += 1;
            while (at < lines.length && (lines[at] ?? "").trim() !== closing) {
                content.push(lines[at] ?? "");
                at += 1;
            }
            at += 1;
            out.push(fenced(language, content.join("\n")));
            continue;
        }

        const heading = /^(#{1,6})\s+(.*)$/.exec(trimmed);
        if (heading !== null) {
            const level = (heading[1] ?? "#").length;
            out.push(tag(`h${level}`, "ash-md-heading").add(...inline(heading[2] ?? "")));
            at += 1;
            continue;
        }

        if (trimmed.startsWith("|") && isDelimiter(lines[at + 1])) {
            const rows: string[] = [];
            while (at < lines.length && (lines[at] ?? "").trim().startsWith("|")) {
                rows.push(lines[at] ?? "");
                at += 1;
            }
            out.push(table(rows));
            continue;
        }

        if (isListItem(trimmed)) {
            const items: string[] = [];
            while (at < lines.length && isListItem((lines[at] ?? "").trim())) {
                items.push((lines[at] ?? "").trim());
                at += 1;
            }
            out.push(list(items));
            continue;
        }

        if (trimmed.startsWith(">")) {
            const quoted: string[] = [];
            while (at < lines.length && (lines[at] ?? "").trim().startsWith(">")) {
                quoted.push((lines[at] ?? "").trim().replace(/^>\s?/, ""));
                at += 1;
            }
            out.push(tag("blockquote", "ash-md-quote").add(...inline(quoted.join(" "))));
            continue;
        }

        if (/^(-{3,}|\*{3,}|_{3,})$/.test(trimmed)) {
            out.push(tag("hr", "ash-md-rule"));
            at += 1;
            continue;
        }

        const paragraph: string[] = [];
        while (
            at < lines.length &&
            (lines[at] ?? "").trim() !== "" &&
            !breaksAParagraph(lines[at] ?? "")
        ) {
            paragraph.push((lines[at] ?? "").trim());
            at += 1;
        }
        out.push(tag("p", "ash-md-paragraph").add(...inline(paragraph.join(" "))));
    }

    return out;
}

function breaksAParagraph(line: string): boolean {
    const trimmed = line.trim();
    return (
        trimmed.startsWith("#") ||
        trimmed.startsWith("|") ||
        trimmed.startsWith(">") ||
        trimmed.startsWith("```") ||
        trimmed.startsWith("~~~") ||
        isListItem(trimmed)
    );
}

function isListItem(trimmed: string): boolean {
    return /^([-*+]\s+|\d+\.\s+)/.test(trimmed);
}

function isDelimiter(line: string | undefined): boolean {
    const trimmed = (line ?? "").trim();
    return trimmed.startsWith("|") && /^[|\s:-]+$/.test(trimmed) && trimmed.includes("-");
}

/**
 * Une clôture de code. `mermaid` porte sa propre classe **et** le dit en toutes lettres :
 * l'écran ne doit pas laisser croire qu'un schéma a échoué à s'afficher.
 */
function fenced(language: string, content: string): UiChild {
    const block = tag("pre", "ash-md-code").add(tag("code").add(text(content)));
    if (language !== "") block.attr("data-language", language);
    if (language !== "mermaid") return block;
    return column(
        tag("p", "ash-md-note").add(text("mermaid — shown as source")),
        block.class("is-mermaid"),
    ).class("ash-md-mermaid");
}

function list(items: readonly string[]): UiChild {
    const ordered = /^\d+\.\s/.test(items[0] ?? "");
    const listing = tag(ordered ? "ol" : "ul", "ash-md-list");

    for (const item of items) {
        const content = item.replace(/^([-*+]\s+|\d+\.\s+)/, "");
        const box = /^\[([ xX])\]\s+(.*)$/.exec(content);
        if (box === null) {
            listing.add(tag("li", "ash-md-item").add(...inline(content)));
            continue;
        }
        // La case est rendue comme un **glyphe**, pas comme une case à cocher : cliquer une
        // case cocherait une tâche dans un fichier que l'utilisateur et les agents tiennent,
        // et Ash n'écrit dans la fiche que sa zone (ADR-0013).
        const checked = box[1] !== " ";
        listing.add(
            tag("li", "ash-md-item", "ash-md-task")
                .class(checked ? "is-done" : "is-todo")
                .add(tag("span", "ash-md-box").add(text(checked ? "☑" : "☐")))
                .add(...inline(box[2] ?? "")),
        );
    }
    return listing;
}

function table(rows: readonly string[]): UiChild {
    const cells = (row: string): readonly string[] =>
        row
            .trim()
            .replace(/^\|/, "")
            .replace(/\|$/, "")
            .split("|")
            .map((cell) => cell.trim());

    const head = tag("tr", "ash-md-row");
    for (const cell of cells(rows[0] ?? ""))
        head.add(tag("th", "ash-md-cell").add(...inline(cell)));

    const body = tag("tbody");
    for (const row of rows.slice(2)) {
        const line = tag("tr", "ash-md-row");
        for (const cell of cells(row)) line.add(tag("td", "ash-md-cell").add(...inline(cell)));
        body.add(line);
    }

    return tag("table", "ash-md-table").add(tag("thead").add(head)).add(body);
}

/**
 * Le peu d'inline que la fiche emploie : `code`, **gras**, *italique*, et les liens.
 *
 * Un lien est rendu comme du **texte souligné**, jamais comme un `<a href>` : la fiche vient
 * d'un dépôt, donc d'ailleurs, et une webview d'application n'a rien à ouvrir sur un clic
 * qu'elle n'a pas décidé. Le texte de la cible reste lisible, ce qui suffit à la copier.
 */
export function inline(source: string): readonly UiNode[] {
    const out: UiNode[] = [];
    let plain = "";

    const flush = (): void => {
        if (plain !== "") out.push(text(plain));
        plain = "";
    };

    let at = 0;
    while (at < source.length) {
        const rest = source.slice(at);

        const code = /^`([^`]+)`/.exec(rest);
        if (code !== null) {
            flush();
            out.push(
                tag("code", "ash-md-inline-code")
                    .add(text(code[1] ?? ""))
                    .build(),
            );
            at += code[0].length;
            continue;
        }

        const strong = /^\*\*([^*]+)\*\*/.exec(rest);
        if (strong !== null) {
            flush();
            out.push(
                tag("strong", "ash-md-strong")
                    .add(...inline(strong[1] ?? ""))
                    .build(),
            );
            at += strong[0].length;
            continue;
        }

        const emphasis = /^[*_]([^*_]+)[*_]/.exec(rest);
        if (emphasis !== null) {
            flush();
            out.push(
                tag("em", "ash-md-emphasis")
                    .add(...inline(emphasis[1] ?? ""))
                    .build(),
            );
            at += emphasis[0].length;
            continue;
        }

        const link = /^\[([^\]]*)\]\(([^)]*)\)/.exec(rest);
        if (link !== null) {
            flush();
            const label = link[1] ?? "";
            out.push(
                tag("span", "ash-md-link")
                    .title(link[2] ?? "")
                    .add(text(label === "" ? (link[2] ?? "") : label))
                    .build(),
            );
            at += link[0].length;
            continue;
        }

        plain += source[at] ?? "";
        at += 1;
    }

    flush();
    return out;
}
