import { isShell } from "@/shared/ipc";
import type { GitHead, Tab, WorktreeMetadata } from "@/shared/ipc";

/**
 * Comment on **nomme** le contexte d'un onglet — son lieu, et sa branche.
 *
 * « Contexte » est le mot du produit : c'est celui que la spec §4.2 emploie pour ce que la
 * barre d'onglets ne portait qu'à demi et que la bande de titre porte depuis
 * (`<application> — <dépôt> / <branche>`). Les deux moitiés sont ici parce qu'elles sont **la même
 * phrase** : les écrire à deux endroits, c'est accepter que la bande de titre et la ligne de
 * statut finissent par désigner deux endroits différents — exactement ce que le canal unique
 * d'`ActiveTab` cherche à empêcher.
 *
 * Dans `shared/` pour la raison qui y met déjà `agent-state` : ce sont des **dérivations
 * pures des types du contrat** (`TabInfo`, `WorktreeMetadata`), sans une règle propre à qui
 * les affiche. Deux consommateurs les lisent, et chacun n'en tire que du texte — la bande de
 * titre de la fenêtre (`app/window-title.ts`) et la ligne de statut
 * (`features/terminal/status-line.ts`), qui garde pour elle ses couleurs et ses infobulles.
 *
 * La sidebar n'est pas cliente, et ne le sera pas : elle ne nomme pas un contexte à plat,
 * elle range une hiérarchie — un dépôt, ses worktrees, leurs onglets (ADR-0012) —, et ses
 * propres règles de raccourcissement à 240 px vivent dans `sidebar/labels.ts`.
 *
 * Rien n'est deviné ici : la localisation vient du backend, qui seul la résout, et la branche
 * de sa surveillance des fichiers de contrôle
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). On ne choisit que le mot à
 * écrire.
 */

/**
 * Le **lieu** d'un onglet : le dépôt quand il y en a un, sinon le worktree — la forme à plat
 * d'[ADR-0012](../../../docs/adr/0012-worktree-unite-de-travail.md) —, sinon le dernier
 * segment du répertoire, pour un onglet que le backend n'a pas su situer.
 *
 * Toujours un mot : un onglet a un répertoire avant d'avoir un dépôt, et un lieu vide ferait
 * clignoter ce qui l'affiche à chaque `cd`.
 */
export function locationLabel(tab: Tab): string {
    // Le dernier repli diffère selon le genre, et il ne peut pas ne pas différer : un shell
    // a un répertoire courant, une surface d'outil a la racine du worktree qu'elle traite.
    // Les deux sont un chemin, et c'est tout ce dont cette règle a besoin.
    const path = isShell(tab) ? tab.cwd : tab.worktreeRoot;
    return tab.location?.repo?.name ?? tab.location?.worktreeName ?? basename(path);
}

/**
 * La **branche** d'un worktree : le mot à écrire, et de quoi savoir si c'en est vraiment une.
 *
 * `detachedAt` n'est pas un doublon du label : `@a1b2c3d` est déjà lisible, mais la ligne de
 * statut en fait une infobulle en phrase entière, et il vaut mieux qu'elle reçoive le commit
 * que de relire le `@` d'un texte. C'est aussi ce qui garde ici — et seulement ici — la
 * décision de **quelle** source l'emporte : un consommateur qui devrait la refaire pour
 * savoir ce qu'il affiche finirait par ne plus être d'accord.
 */
export interface TabBranch {
    /** Un nom de branche, ou `@a1b2c3d`. Jamais vide. */
    readonly label: string;
    /** Le commit, quand le label est un détachement. `null` sinon. */
    readonly detachedAt: string | null;
}

/**
 * La branche d'un worktree, telle qu'on l'écrit.
 *
 * `operation.branch` d'abord : pendant un rebase `HEAD` est détaché, et c'est le `head-name`
 * que git garde qui dit encore où l'on travaille. Un détachement sans opération s'écrit
 * `@a1b2c3d` — court, et impossible à confondre avec un nom de branche.
 *
 * Toujours un mot, là aussi : un worktree a un `HEAD`, même détaché. C'est l'**absence de
 * métadonnées** qui veut dire « hors dépôt, ou pas encore lu », et cette absence-là se rend
 * différemment selon l'endroit — `no repo` dans la ligne de statut, un titre plus court dans
 * la bande. Chaque appelant la tranche donc lui-même, avant d'appeler.
 */
export function branchOf(metadata: WorktreeMetadata): TabBranch {
    const moving = metadata.operation?.branch ?? null;
    if (moving !== null) return { label: moving, detachedAt: null };
    return headBranch(metadata.head);
}

function headBranch(head: GitHead): TabBranch {
    return head.kind === "branch"
        ? { label: head.name, detachedAt: null }
        : { label: `@${head.commit}`, detachedAt: head.commit };
}

/** Dernier segment d'un chemin — `~` reste `~`, `/` reste `/`. */
function basename(path: string): string {
    const segments = path.split("/").filter((segment) => segment.length > 0);
    return segments[segments.length - 1] ?? "/";
}
