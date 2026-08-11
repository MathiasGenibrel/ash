import type { ToolDeclaration, ToolDraft, Verification } from "./contract";

/**
 * Les règles de la liste d'outils : ce qu'une ligne dit, ce que l'en-tête compte, et ce
 * qui autorise un ajout.
 *
 * Des fonctions pures, et pas des méthodes de la vue : ce sont les seules décisions de la
 * fenêtre, et ce sont donc les seules choses qui méritent d'être vérifiées. Le reste est
 * du DOM.
 *
 * Aucune de ces règles n'est la source de vérité : le backend juge à nouveau, et c'est lui
 * qui tranche ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Ce qui est
 * décidé ici est ce que l'interface **montre** avant d'appeler — un bouton éteint et sa
 * raison, jamais un aller-retour dont le seul résultat serait un message d'erreur.
 */

/** L'adaptateur de repli d'ADR-0008 — le seul dont le mode dégradé se dit à l'écran. */
export const GENERIC_ADAPTER = "generic";

/** Ce qu'une carte affiche en tête. */
export interface ToolHeading {
    /** Le nom de la commande — c'est l'identité de l'entrée, elle reste visible. */
    name: string;
    /**
     * Le libellé d'affichage, en badge à côté du nom, ou `null`.
     *
     * La maquette et la spec §9 le décrivent des deux façons — « badge » sur la carte,
     * « shown instead of the command » dans la glose du formulaire — et les deux sont
     * vraies : **ici** la commande reste visible, parce que c'est la clé du fichier et ce
     * qu'on tape dans le shell ; **ailleurs** dans Ash (sidebar, ligne de statut), c'est
     * le libellé qui nommera l'agent. Masquer la commande dans l'écran qui sert justement
     * à la déclarer serait cacher ce qu'on est en train de régler.
     */
    badge: string | null;
    /** Le dossier de configuration, ou ce que veut dire son absence. */
    config: string;
    /**
     * Ce que le champ de chemin contient réellement — vide quand l'entrée s'en remet à
     * l'adaptateur.
     *
     * Distinct de [`ToolHeading.config`] depuis que le champ est modifiable : ce qu'on
     * **lit** (`adapter default`) et ce qu'on **modifie** (rien) ne sont pas la même
     * chaîne, et écrire la première dans le champ ferait d'une explication un chemin.
     */
    path: string;
}

/** Ce qu'on affiche d'une entrée, sans que la vue ait à connaître les `null`. */
export function describeTool(tool: ToolDeclaration): ToolHeading {
    return {
        name: tool.command,
        badge: tool.label,
        // Le dossier absent n'est pas un dossier vide : c'est celui de l'adaptateur, que
        // l'adaptateur est seul à connaître. Le dire est plus honnête qu'un champ vide.
        config: tool.config ?? "adapter default",
        path: tool.config ?? "",
    };
}

/**
 * Le compteur de l'en-tête de section — `3 declared · 0 verified`, `3 declared · 1 invalid`,
 * ou `none`.
 *
 * Les trois formes sont normatives (maquette §3.9 pour `none`, §3.6 pour le décompte des
 * invalides). `none` n'est pas `0 declared` : l'état vide se dit d'un mot, parce qu'il n'y
 * a rien à compter.
 *
 * **Un problème l'emporte sur un décompte** : tant qu'une entrée est invalide, c'est elle
 * que l'en-tête annonce. Compter les vérifiées à côté ferait chercher lesquelles manquent.
 */
export function describeToolCount(tools: readonly ToolDeclaration[]): string {
    if (tools.length === 0) return "none";
    const invalid = countProblems(tools);
    if (invalid > 0) return `${tools.length} declared · ${invalid} invalid`;
    const verified = tools.filter((tool) => tool.verified).length;
    return `${tools.length} declared · ${verified} verified`;
}

/**
 * Combien d'entrées posent un problème — le chiffre de l'en-tête, et celui de la colonne.
 *
 * Les deux le montrent au même instant et doivent donc le compter au même endroit : la
 * maquette `3e` met `3 declared · 1 invalid` en tête de section **et** `1` sur la ligne
 * `tools` de la navigation. Écrit deux fois, ce filtre finirait par ne plus dire la même
 * chose des deux côtés le jour où `caveat` compterait aussi — et l'un des deux n'est pas
 * sous test.
 */
export function countProblems(tools: readonly ToolDeclaration[]): number {
    return tools.filter((tool) => tool.verification.state === "invalid").length;
}

/**
 * Où la chaîne s'est arrêtée, quand c'est une information et non un détail.
 *
 * La séquence pose `stoppedAt` dès qu'elle s'arrête, **y compris sur une réserve** — et une
 * réserve n'a pas besoin de l'annoncer : son résumé dit déjà ce qui manque, et un
 * `stopped at test 3` à côté ferait lire un échec là où le dossier a été reconnu. Seul un
 * état invalide le dit, parce que là le numéro est ce qui désigne la chose à corriger.
 */
export function describeStop(verification: Verification): string | null {
    if (verification.state !== "invalid" || verification.stoppedAt === null) return null;
    return `stopped at test ${verification.stoppedAt}`;
}

/**
 * Ce qu'un formulaire d'ajout montre tant que les tests n'ont pas parlé.
 *
 * Une vérification vide, et non un cas particulier de la vue : `allowsHooks` y est faux
 * comme partout ailleurs, et c'est ce qui garantit qu'une saisie que rien n'a jugée
 * n'autorise jamais une écriture ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 * La vue la dessinait elle-même, hors de portée de tout test.
 */
export const NOTHING_VERIFIED_YET: Verification = {
    state: "unverified",
    tests: ["pending", "pending", "pending", "pending"],
    summary: "nothing verified yet",
    stoppedAt: null,
    detail: null,
    fix: null,
    launched: null,
    allowsHooks: false,
};

/** Ce que la barre d'action du formulaire montre : une phrase à gauche, un bouton à droite. */
export interface AddAction {
    /**
     * Ce qui est écrit à gauche du bouton — jamais rien.
     *
     * C'est soit le refus local, soit celui que le backend a opposé, soit ce que l'ajout
     * fera. La barre garde sa phrase parce que la maquette garde son bouton : « le masquer
     * ferait croire que ça n'existe pas ».
     */
    reason: string;
    /** Le bouton `add` est-il allumé ? */
    enabled: boolean;
}

/**
 * Ce que la barre d'action du formulaire d'ajout dit et permet.
 *
 * **La précédence est une règle, pas une mise en forme**, et c'est pourquoi elle est ici :
 * un refus local décrit la saisie qu'on a sous les yeux, tandis qu'un refus du backend
 * décrit celle qu'on lui a envoyée. Le premier gagne — sinon on lirait le reproche fait à
 * une saisie qu'on vient de corriger. Un refus du backend, lui, n'éteint pas le bouton :
 * réessayer est exactement ce qu'on veut pouvoir faire.
 *
 * **La quatrième condition est la patience**, et pas un jugement : la maquette veut `add`
 * éteint tant que les quatre tests n'ont pas **répondu** (§3.8) — pas tant qu'ils n'ont pas
 * réussi. Une entrée invalide se déclare : la planche `3e` en montre justement une dans la
 * liste, avec sa correction à portée. Ash n'empêche pas de déclarer, il refuse d'écrire —
 * et ce refus-là est calculé en Rust, transporté par `verification.allowsHooks`, et jamais
 * rejoué ici.
 *
 * C'est aussi pourquoi cette condition ne se double pas d'une règle dans le backend :
 * savoir si l'écran a vu la réponse des tests est une affaire d'écran. Ce que le backend
 * garantit, lui, est qu'une entrée déclarée porte **toujours** une vérification, et que
 * `verified` n'est jamais vrai pour une entrée invalide.
 */
export function describeAddAction(
    draft: ToolDraft,
    declared: readonly ToolDeclaration[],
    failure: string | null,
    verification: Verification | null,
): AddAction {
    const blocked = blockedReason(draft, declared, verification);
    return {
        reason: blocked ?? failure ?? "hooks install after adding, once the four tests pass",
        enabled: blocked === null,
    };
}

/** Pourquoi l'ajout est refusé sans même appeler le backend, ou `null` s'il ne l'est pas. */
function blockedReason(
    draft: ToolDraft,
    declared: readonly ToolDeclaration[],
    verification: Verification | null,
): string | null {
    const command = draft.command.trim();
    if (command === "") return "name the command first";
    // Les mêmes deux refus que le backend, et pour la même raison : un `match` est comparé
    // à un nom de processus (ADR-0005/0006), et deux entrées homonymes désigneraient le
    // même processus.
    if (command.includes("/") || /\s/.test(command)) return `${command} is not a command name`;
    if (declared.some((tool) => tool.command === command)) return `${command} is already declared`;
    // La patience : les tests n'ont pas fini de parler. Le bouton reste à sa place, éteint,
    // et la phrase à gauche dit ce qu'on attend.
    if (verification === null || verification.state === "unverified") {
        return "waiting on the four tests";
    }
    if (verification.state === "verifying") return "waiting on test 4 of 4";
    return null;
}

/**
 * Si les hooks peuvent être écrits sur cette entrée, et sinon pourquoi.
 *
 * **La règle n'est pas ici** : `allowsHooks` est calculé par la séquence en Rust, qui seule
 * a lu le dossier. Ce que cette fonction fait est de lui donner sa **phrase**, celle qui
 * reste à gauche du bouton éteint — « le masquer ferait croire que les hooks n'existent pas
 * pour cet outil ».
 *
 * Elle est ici, et pas dans la vue, parce que la précédence des raisons en est une : une
 * entrée invalide et une entrée jamais vérifiée sont éteintes toutes les deux, et ne disent
 * pas la même chose.
 */
export function describeHooksAvailability(verification: Verification): AddAction {
    if (verification.allowsHooks) {
        return { reason: "", enabled: true };
    }
    return {
        reason:
            verification.state === "invalid"
                ? "unavailable until the path is verified"
                : "install unavailable",
        enabled: false,
    };
}

/**
 * Qui l'avertissement de mode dégradé concerne, ou `null` s'il n'y a pas lieu d'avertir.
 *
 * Il n'apparaît que pour l'adaptateur `generic` (§3.8) : un adaptateur dédié n'a rien à
 * annoncer. Rendre le **sujet** plutôt que la phrase laisse à la vue le soin de teindre
 * `idle`, `done`, `error` et `waiting` de leurs vraies couleurs d'état — c'est le seul
 * endroit de l'interface où du texte courant est teint, et ça se fait avec des nœuds, pas
 * avec des chaînes.
 */
export function degradedModeSubject(draft: ToolDraft): string | null {
    if (draft.adapter !== GENERIC_ADAPTER) return null;
    const command = draft.command.trim();
    return command === "" ? "this tool" : command;
}
