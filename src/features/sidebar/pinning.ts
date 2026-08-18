import type { WorktreeNode } from "./tree";

/**
 * L'épingle d'une ligne de worktree, et ce que le clic sur cette ligne veut dire (spec §5.2).
 *
 * Un worktree existe dans la colonne tant qu'il a un onglet **ou** qu'il est épinglé. Une
 * ligne épinglée sans onglet n'a donc rien à replier — il n'y a rien dessous —, et c'est
 * précisément ce qu'elle offre à la place : ouvrir un onglet là.
 *
 * Ce module est pur pour la même raison que [`./instrumentation`] : la règle qui décide de ce
 * qu'une ligne signale et de ce que son clic déclenche ne se vérifierait pas dans un test de
 * rendu — `bun test` n'a pas de DOM. La vue peint le résultat ; elle ne le recalcule pas.
 *
 * **Rien n'est détenu ici.** L'épingle vit en Rust et survit à la fermeture
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) ; la colonne la rend, et le
 * geste repart au backend.
 */

/** Ce que la ligne montre de son épingle, et ce que le geste demanderait. */
export interface PinMark {
    /** Le glyphe posé à droite du nom. Plein quand la ligne est épinglée, creux sinon. */
    readonly glyph: string;
    /** La phrase entière — infobulle et nom accessible. */
    readonly title: string;
    /** Ce que le geste demande : l'inverse de l'état actuel. */
    readonly pin: boolean;
}

/**
 * Ce qu'un clic sur une ligne de worktree fait.
 *
 * `open-tab` **est** le second critère de la spec §5.2 : une ligne épinglée sans onglet en
 * ouvre un. Ce n'est pas une entorse à
 * [ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md) — ouvrir un onglet
 * depuis la colonne est le geste qui existe déjà, sur la ligne d'un agent et sur le `+` du
 * pied ; il vaut ici pour une ligne qui n'a pas encore d'onglet. Et il n'écrit rien : un
 * onglet porte au plus un PTY ([ADR-0003](../../../docs/adr/0003-zone-terminal-unique.md)),
 * c'est le backend qui l'ouvre.
 */
export type WorktreeGesture = "toggle-collapsed" | "open-tab";

export function worktreeGesture(worktree: WorktreeNode): WorktreeGesture {
    // Replier une ligne qui n'a rien dessous ne cacherait rien : le chevron n'aurait aucun
    // effet visible, et la ligne épinglée serait la seule de la colonne qu'un clic laisse
    // inerte.
    return worktree.tabs.length === 0 ? "open-tab" : "toggle-collapsed";
}

/**
 * L'épingle d'une ligne, dans les deux sens.
 *
 * La phrase dit la **conséquence**, pas le mécanisme : ce qu'on gagne en épinglant est une
 * ligne qui reste quand le dernier onglet se ferme, et ce qu'on perd en désépinglant est
 * exactement ça.
 */
export function pinMark(worktree: WorktreeNode): PinMark {
    return worktree.pinned
        ? {
              glyph: "◆",
              title: `unpin ${worktree.title} — its row goes when its last tab closes`,
              pin: false,
          }
        : {
              glyph: "◇",
              title: `pin ${worktree.title} — its row stays, with or without a tab`,
              pin: true,
          };
}
