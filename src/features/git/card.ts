/**
 * La fiche de branche, telle que le panneau bas la montre : **rendu à gauche, source à
 * droite** (spec §7.5).
 *
 * Deux volets, et un partage net entre eux : à gauche, le markdown mis en forme — front
 * matter, barre de progression, tableaux, clôtures — ; à droite, le fichier tel qu'il est
 * sur le disque, en texte brut. C'est ce qui permet de vérifier d'un coup d'œil que ce
 * qu'Ash affiche est bien ce que le fichier dit, ce qui compte double pour un fichier
 * qu'Ash lui-même écrit en partie.
 *
 * **Cette vue ne décide rien.** L'état de la zone `ash:log`, le diff, la phrase qui
 * l'accompagne et le droit d'écrire viennent tous du backend, calculés une seule fois là où
 * le fichier est lu ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Elle
 * n'ouvre aucun fichier, ne cherche aucun marqueur, et ne compose aucune table.
 */

import "./card.css";

import type { BranchCard, CardLog } from "@/shared/ipc";
import { button, column, paint, row, text, toNode, type UiChild, type UiComponent } from "@/shared/ui";

import { markdown, readCard, type TaskProgress } from "./markdown";
import { tag } from "./tag";

/** Ce que la vue sait demander, et qu'elle ne sait pas faire elle-même. */
export interface BranchCardPorts {
    /** Poser le journal dans le bloc `ash:log` — le seul geste qui écrit. */
    writeLog(): void;
    /**
     * Choisir où la fiche vit. `null` rend la main à la détection.
     *
     * **Rien n'est déplacé et aucun `.gitignore` n'est écrit** : le backend change où il
     * regarde ([ADR-0013](../../../docs/adr/0013-fiche-de-branche-dans-le-depot.md)).
     */
    place(local: boolean | null): void;
}

export interface BranchCardView {
    readonly element: HTMLElement;
    /** La fiche que le backend vient de rendre — `null` quand aucun worktree n'est visé. */
    render(card: BranchCard | null): void;
}

/** La vue, prête à être posée dans le corps du panneau bas (#24). */
export function mountBranchCard(ports: BranchCardPorts): BranchCardView {
    const element = document.createElement("div");
    element.className = "ash-card";

    return {
        element,
        render(card) {
            // Tout le DOM est refait à chaque rendu, comme les trois autres vues du dépôt :
            // il n'y a ici ni état, ni champ en cours de frappe à préserver.
            element.replaceChildren(paint(toNode(view(card, ports))));
        },
    };
}

/** La description complète de l'écran — pure, donc lisible par un test. */
export function view(card: BranchCard | null, ports: BranchCardPorts): UiComponent {
    if (card === null) {
        return column(
            tag("p", "ash-card-empty").add(
                text("no worktree in sight — open a tab inside one to see its card."),
            ),
        ).class("ash-card-view");
    }

    const content = readCard(card.source);

    return column(
        header(card, ports),
        row(
            column(
                ...(content.meta.length === 0 ? [] : [frontMatter(content.meta)]),
                ...(content.progress.total === 0 ? [] : [progressBar(content.progress)]),
                card.exists
                    ? markdown(content.body)
                    : tag("p", "ash-card-empty").add(
                          text(`no card yet at ${card.path} — writing the log creates one.`),
                      ),
            ).class("ash-card-rendered"),
            tag("pre", "ash-card-source").add(tag("code").add(text(card.source))),
        ).class("ash-card-panes"),
    ).class("ash-card-view");
}

/**
 * L'en-tête : où la fiche vit, ce que le bloc porte, et le seul bouton qui écrit.
 */
function header(card: BranchCard, ports: BranchCardPorts): UiChild {
    const local = card.mode === "local";
    return column(
        row(
            tag("span", "ash-card-path").title(card.path).add(text(card.path)),
            tag("span", "ash-card-mode")
                .class(local ? "is-local" : "is-repo")
                .add(text(local ? "local" : "in the repo")),
            button(local ? "keep it in the repo" : "keep it out of the repo")
                .class("ash-card-place")
                .title(
                    local
                        ? `it would live at ${card.otherPath}, and travel with the branch.`
                        : `it would live at ${card.otherPath}, and stop travelling with the branch.`,
                )
                .onClick(() => {
                    ports.place(!local);
                }),
        ).class("ash-card-head"),
        ...(card.ignoredByTheRepo && !local
            ? [
                  tag("p", "ash-card-warning").add(
                      text(".ash is gitignored here — this card will not be committed."),
                  ),
              ]
            : []),
        logBar(card.log, ports),
        ...(card.log.diff === ""
            ? []
            : [tag("pre", "ash-card-diff").add(tag("code").add(text(card.log.diff)))]),
    ).class("ash-card-header");
}

/**
 * L'état du bloc `ash:log`, sa phrase, et le bouton.
 *
 * Le bouton **reste visible et éteint** quand Ash ne peut pas écrire, avec sa raison : c'est
 * la règle que `shared/ui` rend impossible à contourner, et elle vaut ici plus qu'ailleurs —
 * un bouton disparu ferait croire que la fiche n'a pas de journal, alors que c'est le bloc
 * qui est en conflit ou édité à la main.
 */
function logBar(log: CardLog, ports: BranchCardPorts): UiChild {
    const write = button("write the log")
        .class("ash-card-write")
        .onClick(() => {
            ports.writeLog();
        });
    return row(
        tag("span", "ash-card-state").class(`is-${log.state}`).add(text(log.state)),
        tag("p", "ash-card-note").add(text(log.note)),
        log.writable ? write : write.disabled(log.note),
    ).class("ash-card-log");
}

/** Le front matter, rendu comme ce qu'il est : des métadonnées, pas du texte. */
function frontMatter(meta: readonly { readonly key: string; readonly value: string }[]): UiChild {
    const list = tag("dl", "ash-card-meta");
    for (const entry of meta) {
        list.add(tag("dt", "ash-card-meta-key").add(text(entry.key)));
        list.add(tag("dd", "ash-card-meta-value").add(text(entry.value)));
    }
    return list;
}

/**
 * La barre de progression — « les cases `- [ ]` deviennent la barre de progression »
 * (ADR-0013).
 *
 * La largeur est posée en style en ligne parce que c'est une **donnée**, pas une décision de
 * présentation : le CSS ne peut pas connaître trois tâches sur sept.
 */
function progressBar(progress: TaskProgress): UiChild {
    const percent = Math.round((progress.done / progress.total) * 100);
    return row(
        tag("span", "ash-card-progress-count").add(text(`${progress.done}/${progress.total}`)),
        tag("span", "ash-card-progress-track").add(
            tag("span", "ash-card-progress-fill").attr("style", `width: ${percent}%`),
        ),
    )
        .class("ash-card-progress")
        .title(`${percent}% of the tasks are ticked`);
}
