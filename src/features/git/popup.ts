/**
 * La popup de branches — sa **description**, pas son DOM (spec §7.1).
 *
 * Elle est ancrée sur la branche du pied de fenêtre, filtrable en tapant, groupée
 * `current` / `recent` / `local` / `remote`, et elle porte les deux choses qu'aucun client
 * git n'a : la colonne de droite qui nomme le worktree quand la branche vit ailleurs, et
 * l'avertissement qui **nomme** l'agent qu'un checkout dérangerait.
 *
 * Comme la boîte de recherche et la confirmation de fermeture, ce module rend une
 * [`UiNode`](../../shared/ui/node.ts) et ne connaît pas le DOM : `controller.ts` la pose.
 * C'est ce qui met sous test ce qui décide — les quatre étapes, les actions éteintes avec
 * leur raison, la phrase de l'avertissement — sans monter de navigateur.
 *
 * **Rien ici n'écrit dans un dépôt, et rien n'y met un agent en pause.** Une description ne
 * s'exécute pas : elle porte des rappels que `controller.ts` branche sur les commandes
 * Tauri, et aucun d'eux ne part sans un geste
 * ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
 */

import type { ActionOffer, ActionOutcome, Branch, BranchGroup, BranchOverview } from "@/shared/ipc";
import { button, column, field, row, text, type UiComponent } from "@/shared/ui";

import type { BranchRow } from "./branch-list";
import { pauseOffers, warnAbout, type PauseOffer } from "./warning";

/** Les quatre moments de la popup. Une seule est vraie à la fois. */
export type PopupStage =
    /** La liste, filtrable. */
    | { readonly kind: "list" }
    /** `⌘⏎` : les actions d'une branche, sans quitter le clavier. */
    | { readonly kind: "actions"; readonly branch: Branch; readonly offers: readonly ActionOffer[] }
    /**
     * L'action touche l'arbre pendant qu'un agent écrit : on demande, et on propose la pause.
     *
     * Cette étape n'existe **que** dans ce cas. Une confirmation qui apparaîtrait aussi
     * quand personne ne travaille se cliquerait sans être lue, et l'avertissement qui compte
     * passerait avec elle.
     */
    | { readonly kind: "confirm"; readonly branch: Branch; readonly offer: ActionOffer }
    /** Ce que git a répondu — succès comme échec, avec les deux côtés nommés. */
    | { readonly kind: "outcome"; readonly outcome: ActionOutcome };

export interface PopupModel {
    /** `null` tant que le backend n'a pas répondu, ou quand il n'a pas su lire le dépôt. */
    readonly overview: BranchOverview | null;
    readonly query: string;
    readonly rows: readonly BranchRow[];
    /** L'index dans `rows`, ou `-1` quand il n'y a rien à sélectionner. */
    readonly selected: number;
    readonly stage: PopupStage;
    /** Une action est en vol : on n'en lance pas une seconde par-dessus. */
    readonly running: boolean;
}

/** Ce que la popup sait demander. Le contrôleur les branche sur le backend. */
export interface PopupActions {
    readonly filter: (query: string) => void;
    readonly move: (step: number) => void;
    /** Le geste principal sur une ligne : `⏎`, ou un clic dessus. */
    readonly choose: (branch: Branch) => void;
    /** `⌘⏎`, ou le bouton `⋯` de la ligne — la même porte pour le clavier et la souris. */
    readonly openActions: (branch: Branch) => void;
    readonly pick: (offer: ActionOffer) => void;
    /** « Je sais, fais-le quand même » — jamais le défaut. */
    readonly proceed: () => void;
    readonly pause: (offer: PauseOffer) => void;
    /** `⎋` depuis une étape : on revient d'un cran, on ne referme pas tout. */
    readonly back: () => void;
    readonly close: () => void;
}

/** La clé du champ, que le contrôleur relève pour lui rendre le focus après un rendu. */
export const FILTER_FOCUS_KEY = "branch-popup-filter";

/** La clé du bouton qui ne touche à rien — celui qui reçoit les doigts sur la confirmation. */
export const CANCEL_FOCUS_KEY = "branch-popup-cancel";

/** Les titres des quatre groupes, tels que la spec les nomme. */
const GROUP_LABELS: Readonly<Record<BranchGroup, string>> = {
    current: "current",
    recent: "recent",
    local: "local",
    remote: "remote",
};

export function composeBranchPopup(model: PopupModel, actions: PopupActions): UiComponent {
    switch (model.stage.kind) {
        case "actions":
            return composeActions(model.stage.branch, model.stage.offers, actions);
        case "confirm":
            return composeConfirmation(model, model.stage.offer, actions);
        case "outcome":
            return composeOutcome(model.stage.outcome, actions);
        case "list":
            return composeList(model, actions);
    }
}

/**
 * La liste : un champ, puis une ligne par branche, groupée.
 *
 * Le champ prend les doigts à l'ouverture — c'est le sens de « filtrable en tapant » : on
 * ne clique pas d'abord dans une boîte. Il est aussi le seul élément focalisé de la popup,
 * ce qui garde le clavier en un seul endroit.
 */
function composeList(model: PopupModel, actions: PopupActions): UiComponent {
    const filter = field("filter branches")
        .class("branch-popup-filter")
        .focusKey(FILTER_FOCUS_KEY)
        .placeholder("filter")
        .value(model.query)
        .onInput(actions.filter);

    const body =
        model.overview === null
            ? [
                  row(text("this directory is not in a repository Ash could read")).class(
                      "branch-popup-empty",
                  ),
              ]
            : model.rows.length === 0
              ? [row(text(`no branch matches “${model.query}”`)).class("branch-popup-empty")]
              : model.rows.map((shown, index) =>
                    composeRow(shown, index === model.selected, actions),
                );

    const warning = warnAbout(model.overview?.agentsAtRisk ?? [], worktreeNameOf(model.overview));

    return column(
        row(filter).class("branch-popup-head"),
        column(...body).class("branch-popup-list"),
        ...(warning === null
            ? []
            : [row(text(warning)).class("branch-popup-warning").attr("role", "status")]),
    ).class("branch-popup");
}

/**
 * Une ligne : le nom, puis le worktree qui la détient, poussé à droite.
 *
 * La colonne de droite n'est écrite que quand la branche vit **ailleurs** — c'est la seule
 * information qu'elle porte, et l'écrire toujours (avec le worktree courant pour la
 * courante) la rendrait illisible en la rendant constante.
 *
 * **Deux cibles de souris, comme au clavier** (spec §4.4) : la ligne fait le geste
 * principal, le `⋯` ouvre les actions. Le clavier a `⏎` et `⌘⏎` pour les mêmes deux choses,
 * et les deux paires passent par les mêmes rappels — la souris et le clavier ne peuvent donc
 * pas se répondre différemment.
 */
function composeRow(shown: BranchRow, selected: boolean, actions: PopupActions): UiComponent {
    const line = row(
        text(shown.branch.name),
        ...(shown.branch.worktree === null
            ? []
            : [
                  row(text(shown.branch.worktree.name))
                      .class("branch-popup-elsewhere")
                      .title(`checked out in ${shown.branch.worktree.root}`),
              ]),
        button("⋯")
            .class("branch-popup-more")
            .title(`actions for ${shown.branch.name}`)
            .onClick(() => {
                actions.openActions(shown.branch);
            }),
    )
        .class("branch-popup-row")
        .class(selected ? "is-selected" : "")
        .attr("data-branch", shown.branch.name)
        .on("click", () => {
            actions.choose(shown.branch);
        });

    return shown.opensGroup
        ? column(row(text(GROUP_LABELS[shown.group])).class("branch-popup-group"), line).class(
              "branch-popup-section",
          )
        : line;
}

/**
 * Le sous-menu de `⌘⏎` : un bouton par action, **refus compris**.
 *
 * Une action refusée reste visible avec sa raison, elle n'est pas masquée : c'est la règle
 * que [`disabled`](../../shared/ui/button.ts) rend non contournable, et elle vaut ici comme
 * ailleurs — un « Rebase » qui disparaît fait croire qu'il n'existe pas, alors que ce qu'il
 * faut lire est *pourquoi* il ne peut pas se faire maintenant.
 *
 * Les libellés viennent du backend et ne sont pas recomposés ici : ce sont eux qui nomment
 * les deux côtés, et le message d'erreur les reprendra tels quels.
 */
function composeActions(
    branch: Branch,
    offers: readonly ActionOffer[],
    actions: PopupActions,
): UiComponent {
    const buttons = offers.map((offer) => {
        const item = button(offer.label).class("branch-popup-action");
        return offer.refused === null
            ? item.onClick(() => {
                  actions.pick(offer);
              })
            : item.disabled(offer.refused);
    });

    return column(
        row(text(branch.name)).class("branch-popup-title"),
        ...(offers.length === 0
            ? [
                  row(text(`${branch.name} is no longer a branch of this repository`)).class(
                      "branch-popup-empty",
                  ),
              ]
            : buttons.map((item) => row(item))),
        row(
            button("Back")
                .class("branch-popup-back")
                .focusKey(CANCEL_FOCUS_KEY)
                .onClick(actions.back),
        ).class("branch-popup-actions"),
    ).class("branch-popup");
}

/**
 * La confirmation : ce qu'on s'apprête à faire, qui ça dérange, et la pause.
 *
 * L'ordre des boutons n'est pas décoratif. **Le focus va à `Cancel`**, comme sur la
 * confirmation de fermeture d'onglet : une touche entrée sur une boîte qui vient
 * d'apparaître ne doit pas déplacer les fichiers d'un agent en train d'écrire. La pause
 * vient avant « fais-le quand même », parce que c'est la réponse que la spec propose, et
 * parce qu'elle est la seule des trois qui rende le geste sûr.
 */
function composeConfirmation(
    model: PopupModel,
    offer: ActionOffer,
    actions: PopupActions,
): UiComponent {
    const agents = model.overview?.agentsAtRisk ?? [];
    const warning = warnAbout(agents, worktreeNameOf(model.overview)) ?? "";

    const pauses = pauseOffers(agents).map((pause) =>
        row(
            button(pause.label)
                .class("branch-popup-pause")
                .onClick(() => {
                    actions.pause(pause);
                }),
        ),
    );

    return column(
        row(text(offer.label)).class("branch-popup-title"),
        row(text(warning)).class("branch-popup-warning").attr("role", "alert"),
        column(...pauses).class("branch-popup-pauses"),
        row(
            button("Cancel")
                .class("branch-popup-cancel")
                .focusKey(CANCEL_FOCUS_KEY)
                .onClick(actions.back),
            model.running
                ? button(offer.label).class("is-danger").disabled("this action is already running")
                : button(offer.label).class("is-danger").onClick(actions.proceed),
        ).class("branch-popup-actions"),
    ).class("branch-popup");
}

/**
 * Ce que git a répondu.
 *
 * Le libellé est celui de l'action, et il est là **même en cas d'échec** : « y compris dans
 * les messages d'erreur » (spec §7.1). Sans lui, un « cannot rebase: You have unstaged
 * changes » ne dirait pas de quelle branche vers quelle branche.
 */
function composeOutcome(outcome: ActionOutcome, actions: PopupActions): UiComponent {
    return column(
        row(text(outcome.label)).class("branch-popup-title"),
        row(text(outcome.success ? "done" : "failed")).class(
            outcome.success ? "branch-popup-ok" : "branch-popup-failed",
        ),
        ...(outcome.output === "" ? [] : [row(text(outcome.output)).class("branch-popup-output")]),
        row(
            button("Close")
                .class("branch-popup-cancel")
                .focusKey(CANCEL_FOCUS_KEY)
                .onClick(actions.close),
        ).class("branch-popup-actions"),
    ).class("branch-popup");
}

/** Le nom du worktree d'où l'on regarde — la matière de l'avertissement. */
function worktreeNameOf(overview: BranchOverview | null): string {
    if (overview === null) return "this worktree";
    const segments = overview.worktreeRoot.split("/").filter((part) => part !== "");
    return segments[segments.length - 1] ?? overview.worktreeRoot;
}
