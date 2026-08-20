import "./graph.css";

import { paint } from "@/shared/ui";

import type { CommitGraph } from "./contract";
import { commitGraphView, type CommitGraphState } from "./graph-view";

/**
 * Le graphe de commits, posé dans le corps du panneau bas (#24, spec §7.2).
 *
 * Ce fichier est la seule partie de la feature qui touche le DOM, et il ne décide rien : il
 * demande une fenêtre, garde ce qui est revenu, et repeint. Ce qui se dessine — les couloirs,
 * la colonne `by`, les branches repliées — est décidé en Rust
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * **Deux choses vivent ici, et ce sont deux faits d'affichage** : la ligne dont le détail est
 * ouvert, et la taille de fenêtre déjà demandée. Ni l'une ni l'autre n'est un état du produit
 * — les refermer et rouvrir le panneau les remet à zéro, ce qui est exactement ce qu'on
 * attend d'une position de lecture.
 */

/** Ce que le graphe sait demander, et qu'il ne sait pas faire lui-même. */
export interface CommitGraphPorts {
    /**
     * La fenêtre du graphe d'un worktree, ou `null` quand il n'y a rien à montrer — hors
     * dépôt, ou `git` qui n'a pas répondu. Les deux se rendent pareil.
     *
     * `window` vaut `null` pour un graphe qui s'ouvre : c'est le backend qui décide de quoi
     * est faite une première fenêtre, et il l'annonce dans sa réponse. Le panneau ne nomme un
     * nombre qu'en élargissant, à partir de ce qui lui a été annoncé.
     */
    read(worktreeRoot: string, window: number | null): Promise<CommitGraph | null>;
}

export interface CommitGraphPanel {
    readonly element: HTMLElement;
    /**
     * Le worktree à regarder — `null` quand aucun onglet n'est situé.
     *
     * Redemander le **même** ne remet pas la lecture à zéro : c'est l'annonce qui suit chaque
     * changement d'onglet, et elle ne doit pas refermer un détail qu'on est en train de lire.
     */
    show(worktreeRoot: string | null): void;
    /** Relire — le `HEAD` a bougé, ou la vue vient de s'afficher. */
    refresh(): void;
}

export function mountCommitGraph(ports: CommitGraphPorts): CommitGraphPanel {
    const element = document.createElement("div");
    element.className = "git-graph-host";

    let root: string | null = null;
    // `null` tant que rien n'a été élargi : voir `CommitGraphPorts.read`.
    let windowSize: number | null = null;
    let state: CommitGraphState = { graph: null, selected: null };
    /**
     * Le numéro de la lecture en cours.
     *
     * Deux lectures peuvent se croiser — on change d'onglet pendant qu'un `git log` répond —
     * et la plus ancienne écraserait alors la plus récente. Le compteur fait que seule la
     * dernière demandée a le droit de peindre.
     */
    let asked = 0;

    const draw = (): void => {
        element.replaceChildren(
            paint(
                commitGraphView(state, {
                    select: (sha) => {
                        // Reprendre la ligne ouverte referme son détail : c'est un fait
                        // d'affichage, et il se décide donc bien ici.
                        state = { ...state, selected: state.selected === sha ? null : sha };
                        draw();
                    },
                    widen: (next) => {
                        windowSize = next;
                        read();
                    },
                }).build(),
            ),
        );
    };

    const read = (): void => {
        const here = root;
        const mine = ++asked;
        if (here === null) {
            state = { graph: null, selected: null };
            draw();
            return;
        }
        ports
            .read(here, windowSize)
            .then((graph) => {
                if (mine !== asked) return;
                // La sélection survit à une relecture quand la ligne est toujours là : le
                // `HEAD` bouge pendant qu'on lit un détail, et le perdre pour ça serait
                // arbitraire.
                const stillThere =
                    graph !== null && graph.rows.some((commit) => commit.sha === state.selected);
                state = { graph, selected: stillThere ? state.selected : null };
                draw();
            })
            .catch(() => {
                if (mine !== asked) return;
                // Une lecture qui n'aboutit pas ne laisse **aucune moitié d'état** : rien n'a
                // été retenu côté backend, et le panneau garde ce qu'il montrait.
            });
    };

    draw();

    return {
        element,
        show(worktreeRoot) {
            if (worktreeRoot === root) return;
            root = worktreeRoot;
            windowSize = null;
            state = { graph: null, selected: null };
            read();
        },
        refresh: read,
    };
}
