/**
 * Le contrat du graphe de commits — miroir de `src-tauri/src/features/git/history.rs`.
 *
 * Il vit dans la feature et non dans `shared/ipc/`, comme celui de la fenêtre de réglages et
 * pour la même raison : `shared/` demande **deux** lecteurs, et ces formes n'en ont qu'un —
 * le panneau `graph`. Ce qui est réellement partagé — les onglets, l'état git d'un worktree —
 * est déjà là-bas, et le graphe n'y ajoute rien.
 *
 * **Rien n'est calculé ici.** Les couloirs, la colonne `by`, le repli des branches inactives
 * et la phrase qui remplace un prompt absent sont tous décidés en Rust
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md),
 * [ADR-0014](../../../docs/adr/0014-attribution-locale-des-commits.md)). Ce fichier écrit ce
 * qui traverse, et `mirror.ts` prouve qu'il l'écrit encore juste.
 */

/**
 * Un trait du dessin, d'une ligne vers la suivante.
 *
 * `from` est la colonne au niveau de la ligne qui le porte, `to` celle de la ligne d'en
 * dessous. `from === to` est un trait droit ; le reste dit quelque chose — une branche qui
 * naît, une fusion, deux enfants qui se rejoignent sur leur parent.
 */
export interface GraphLink {
    from: number;
    to: number;
}

/**
 * Une ligne du graphe.
 *
 * `by` est **la raison d'être de l'écran** (spec §7.2) : le nom de l'agent qu'Ash a vu écrire
 * ce commit, ou le nom d'auteur git à défaut. `attributed` dit lequel des deux, et il n'est
 * pas déductible du mot — un dépôt dont l'auteur git s'appelle `claude` rendrait les deux
 * indiscernables.
 *
 * `promptNote` porte ce que le panneau de détail dit **à la place** d'un prompt absent, et il
 * distingue les deux absences : un commit observé dont le prompt n'a pas été retenu, et un
 * commit qu'Ash n'a pas vu naître. Vide quand un prompt existe.
 */
export interface CommitRow {
    sha: string;
    /** L'identifiant abrégé, tel que git l'abrège. C'est celui qu'on affiche. */
    short: string;
    subject: string;
    by: string;
    attributed: boolean;
    author: string;
    /** La date d'auteur telle que git l'écrit (ISO 8601 strict). */
    authorDate: string;
    /** La même, en **secondes** Unix — pas en millisecondes, contrairement à `stateSince`. */
    authoredAt: number;
    /** Les refs qui pointent ici, `HEAD -> main` compris. Vide pour la plupart des lignes. */
    refs: string[];
    lane: number;
    links: GraphLink[];
    tabId: string | null;
    prompt: string | null;
    promptNote: string;
}

/** Une branche que la règle des 30 jours a écartée du dessin (spec §7.2). */
export interface FoldedBranch {
    name: string;
    /** La date de son dernier commit, en secondes Unix. */
    lastActivity: number;
}

/**
 * Une fenêtre de graphe.
 *
 * Une **fenêtre**, et non une page : elle repart toujours du sommet, et « voir plus loin » la
 * redemande plus grande. C'est une conséquence des couloirs — ceux d'une ligne dépendent de
 * tout ce qui la précède, donc une page qui commencerait au milieu ne saurait pas quels
 * traits y arrivent.
 */
export interface CommitGraph {
    rows: CommitRow[];
    /** Combien de couloirs réserver en largeur. */
    lanes: number;
    folded: FoldedBranch[];
    /** La fenêtre demandée — celle qu'il faut redemander plus grande pour voir plus loin. */
    window: number;
    hasMore: boolean;
}
