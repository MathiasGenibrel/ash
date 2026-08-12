import type { ToolDeclaration, ToolDraft } from "./contract";

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
}

/** Ce qu'on affiche d'une entrée, sans que la vue ait à connaître les `null`. */
export function describeTool(tool: ToolDeclaration): ToolHeading {
    return {
        name: tool.command,
        badge: tool.label,
        // Le dossier absent n'est pas un dossier vide : c'est celui de l'adaptateur, que
        // l'adaptateur est seul à connaître. Le dire est plus honnête qu'un champ vide.
        config: tool.config ?? "adapter default",
    };
}

/**
 * Le compteur de l'en-tête de section — `3 declared · 0 verified`, ou `none`.
 *
 * Les deux formes sont normatives (maquette §3.9 pour `none`). `none` n'est pas
 * `0 declared` : l'état vide se dit d'un mot, parce qu'il n'y a rien à compter.
 */
export function describeToolCount(tools: readonly ToolDeclaration[]): string {
    if (tools.length === 0) return "none";
    const verified = tools.filter((tool) => tool.verified).length;
    return `${tools.length} declared · ${verified} verified`;
}

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
 * Trois conditions bloquantes aujourd'hui, et une quatrième viendra : la maquette veut
 * `add` éteint **tant que les quatre tests n'ont pas répondu** (§3.8), et ces tests sont
 * l'issue #15. C'est ici qu'elle se branchera, à côté des autres — pas dans la vue, qui
 * n'est pas sous test.
 */
export function describeAddAction(
    draft: ToolDraft,
    declared: readonly ToolDeclaration[],
    failure: string | null,
): AddAction {
    const blocked = blockedReason(draft, declared);
    return {
        reason: blocked ?? failure ?? "hooks install after adding, once the four tests pass",
        enabled: blocked === null,
    };
}

/** Pourquoi l'ajout est refusé sans même appeler le backend, ou `null` s'il ne l'est pas. */
function blockedReason(draft: ToolDraft, declared: readonly ToolDeclaration[]): string | null {
    const command = draft.command.trim();
    if (command === "") return "name the command first";
    // Les mêmes deux refus que le backend, et pour la même raison : un `match` est comparé
    // à un nom de processus (ADR-0005/0006), et deux entrées homonymes désigneraient le
    // même processus.
    if (command.includes("/") || /\s/.test(command)) return `${command} is not a command name`;
    if (declared.some((tool) => tool.command === command)) return `${command} is already declared`;
    return null;
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
