import type {
    FocusedTool,
    HookAction,
    SettingsSnapshot,
    KeyStroke,
    ShortcutRow,
    ToolDeclaration,
    ToolDraft,
    ToolSuggestion,
    Verification,
} from "./contract";

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

/**
 * Ce que veut dire un dossier absent — **le mot, écrit une fois**.
 *
 * Trois endroits le montrent au même instant : le libellé d'une carte, l'invite de son champ
 * de chemin, et la glose du formulaire d'ajout. Ce n'est pas une décoration : c'est ce que
 * l'absence *signifie*, et le jour où la phrase change, deux des trois endroits ne sont sous
 * aucun test. Le mot appartient donc au modèle, comme les autres phrases de cet écran.
 */
export const ADAPTER_DEFAULT = "adapter default";

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
        config: tool.config ?? ADAPTER_DEFAULT,
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
 * La liste porte-t-elle une entrée que rien n'a jugée ?
 *
 * C'est le cas d'un **redémarrage** : `~/.ash/tools.json` garde la déclaration et le dernier
 * dossier valide, jamais le résultat des quatre tests — une vérification est un fait daté sur
 * la machine, et un dossier peut avoir disparu entre deux lancements
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)). Les entrées relues arrivent donc
 * *non vérifiées*, et la fenêtre relance la séquence en s'ouvrant : sans ça, la ligne `hooks`
 * d'un outil instrumenté depuis des mois resterait éteinte jusqu'à ce que quelqu'un pense à
 * cliquer `re-verify all`.
 *
 * Une question, pas une liste de commandes : la séquence se relance **d'un bloc**
 * (`verifyAll`), parce qu'un aller-retour par entrée ferait autant de réponses concurrentes
 * qui se remplaceraient les unes les autres à l'écran. La règle reste en Rust — l'écran
 * demande, il ne juge pas (ADR-0009).
 */
export function needsVerifying(tools: readonly ToolDeclaration[]): boolean {
    return tools.some((tool) => tool.verification.state === "unverified");
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
    // même processus. Les deux phrases sont **mot pour mot** celles de `NotACommandName` et
    // `DuplicateCommand` dans `src-tauri/src/features/settings/error.rs`, parce que
    // l'utilisateur ne sait pas lequel des deux a parlé. Aucun test ne relie les deux
    // côtés : en changer une ici sans changer l'autre là-bas laisse la suite verte.
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
 * Le mot du bouton de la ligne `hooks`.
 *
 * La seule chose que le frontend ajoute à [`HooksReport`] : le backend dit **quelle** action
 * la ligne propose, l'écran dit comment elle s'écrit. Le libellé de `seeTheDiff` est le seul
 * qui ne se déduise pas de son nom, et c'est aussi le seul qui n'écrive rien.
 */
export function hookActionLabel(action: HookAction): string {
    const said: Record<HookAction, string> = {
        install: "install",
        update: "update",
        remove: "remove",
        seeTheDiff: "see the diff",
    };
    return said[action];
}

/**
 * La bannière de doublon, ou `null` quand deux entrées ne se marchent pas dessus.
 *
 * Elle est **de section** et non de carte, parce que le doublon n'appartient à aucune des
 * deux entrées : « claude et claude-perso pointent le même dossier — l'une des deux ne fera
 * rien ». Les cartes, elles, portent chacune leur étiquette (spec §9.1 : signalé sur les
 * deux lignes).
 *
 * `undo` nomme l'entrée qu'on peut ramener en arrière, et il n'existe que si une
 * réinitialisation a **créé** la collision : proposer d'annuler un geste qui n'a pas eu lieu
 * ferait chercher lequel.
 */
export interface DuplicateBanner {
    /** Les entrées en cause, dans l'ordre de la liste. */
    readonly commands: readonly string[];
    readonly sentence: string;
    /** L'entrée dont la réinitialisation a produit le doublon, s'il y en a une. */
    readonly undo: string | null;
}

export function describeDuplicates(tools: readonly ToolDeclaration[]): DuplicateBanner | null {
    const colliding = tools.filter((tool) => tool.duplicates.length > 0);
    if (colliding.length < 2) return null;

    const commands = colliding.map((tool) => tool.command);
    const named = commands.slice(0, -1).join(", ");
    const last = commands[commands.length - 1] ?? "";
    return {
        commands,
        sentence: `${named} and ${last} point at the same folder — one of them will do nothing`,
        undo: colliding.find((tool) => tool.resetFrom !== null)?.command ?? null,
    };
}

/**
 * Ce que le `↺` d'une carte fait, et ce qu'il dit quand il ne peut rien faire.
 *
 * « Réinitialiser une entrée la ramène à sa dernière valeur valide, pas au défaut de son
 * adaptateur » (spec §9.1). Une entrée qui n'a jamais rien prouvé n'a donc **rien** à
 * restaurer, et son bouton suit la règle que la maquette répète trois fois : visible,
 * éteint, avec sa raison.
 */
export function describeReset(tool: ToolDeclaration): AddAction {
    if (tool.lastValidConfig === null) {
        return { reason: "no verified folder to go back to yet", enabled: false };
    }
    if (tool.lastValidConfig === (tool.config ?? "")) {
        return { reason: "already on the last folder that worked", enabled: false };
    }
    return { reason: `back to ${tool.lastValidConfig}`, enabled: true };
}

/** Une ligne du diff, telle que l'écran la peint. */
export interface DiffLine {
    readonly kind: "removed" | "added" | "context";
    readonly text: string;
}

/**
 * Le diff, découpé en lignes qualifiées.
 *
 * C'est une **décision**, donc elle est ici : reconnaître un préfixe et jeter l'en-tête
 * `---`/`+++` du backend est exactement le genre de chose qu'une vue non testée finit par
 * faire de travers — et le dépôt ne monte pas de DOM dans `bun test`.
 *
 * **Le sens est celui du backend, et l'écran l'annonce tel quel** : `-` est ce qu'Ash
 * écrirait, `+` ce que le fichier porte. La maquette légende l'inverse ; suivre sa légende
 * sur ces lignes-ci ferait lire chaque ligne à l'envers, ce qui est la seule faute qu'un
 * diff ne pardonne pas.
 */
export function parseDiff(diff: string): DiffLine[] {
    return diff
        .split("\n")
        .filter((line) => !line.startsWith("---") && !line.startsWith("+++"))
        .map((line) => {
            if (line.startsWith("-")) return { kind: "removed", text: line.slice(1).trimEnd() };
            if (line.startsWith("+")) return { kind: "added", text: line.slice(1).trimEnd() };
            return { kind: "context", text: line.slice(2).trimEnd() };
        });
}

/**
 * L'entrée que la correction proposée ferait basculer en mode dégradé, ou `null`.
 *
 * `generic` est un mode dégradé, et l'écran doit le dire **avant** qu'on l'applique
 * (maquette §3.6) : le bouton `apply` d'une carte invalide propose justement `generic`, et
 * l'appuyer sans savoir ce qu'il coûte ferait perdre `waiting` sans que rien ne l'ait
 * annoncé.
 */
export function degradedFixSubject(tool: ToolDeclaration): string | null {
    const apply = tool.verification.fix?.apply ?? null;
    if (apply === null || apply.kind !== "useAdapter") return null;
    return apply.adapter === GENERIC_ADAPTER ? tool.command : null;
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

/** Un groupe de la section `shortcuts` : le nom du sous-menu, et ses lignes. */
export interface ShortcutGroup {
    readonly group: string;
    readonly shortcuts: readonly ShortcutRow[];
}

/**
 * Les raccourcis, groupés — **dans l'ordre où le backend les envoie**, jamais trié ici.
 *
 * L'ordre est celui du menu natif, et c'est tout l'intérêt : on retrouve un raccourci dans
 * l'écran là où on l'a vu dans le menu. Trier alphabétiquement, ou ranger les groupes selon
 * une liste écrite ici, donnerait à cette fenêtre un second avis sur une question dont le
 * menu a déjà décidé — et un groupe ajouté en Rust n'apparaîtrait pas du tout.
 *
 * Un même groupe qui reviendrait après un autre est **replié dans le premier** plutôt que
 * dupliqué : deux titres identiques dans une liste se lisent comme un bug d'affichage.
 */
export function groupShortcuts(shortcuts: readonly ShortcutRow[]): readonly ShortcutGroup[] {
    const grouped: { group: string; shortcuts: ShortcutRow[] }[] = [];
    for (const shortcut of shortcuts) {
        const opened = grouped.find((candidate) => candidate.group === shortcut.group);
        if (opened === undefined) grouped.push({ group: shortcut.group, shortcuts: [shortcut] });
        else opened.shortcuts.push(shortcut);
    }
    return grouped;
}

/**
 * Les suggestions qu'il reste à montrer, une fois la liste déclarée sous les yeux.
 *
 * **Le backend applique déjà cette règle**, et c'est lui qui la détient : une suggestion est
 * par définition un outil que personne n'a déclaré. Ce filtre-là est un fait d'affichage, et
 * il existe parce que les deux valeurs n'arrivent pas ensemble — la liste revient de la
 * commande qu'on vient d'appeler, les suggestions d'un second aller-retour. Déclarer un outil
 * laisserait donc, le temps d'une image, sa carte **et** sa suggestion à l'écran : le même
 * outil deux fois, dont une sous un geste qui serait refusé.
 *
 * C'est exactement la garde de [`focusedDraft`], sur la même règle et pour la même raison :
 * l'écran ne juge pas, il évite de montrer ce que le backend vient de rendre faux
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export function pendingSuggestions(
    suggestions: readonly ToolSuggestion[],
    tools: readonly ToolDeclaration[],
): readonly ToolSuggestion[] {
    return suggestions.filter(
        (suggestion) => !tools.some((tool) => tool.command === suggestion.command),
    );
}

/**
 * Ce que l'état vide dit quand Ash a vu tourner quelque chose.
 *
 * « no tools declared » reste vrai, et devient trompeur : Ash sait très bien que `claude`
 * tourne dans trois onglets, et l'écran laissait deviner qu'il fallait passer par la sidebar
 * pour le lui dire (ADR-0006). Quand il y a des suggestions, l'état vide n'est donc plus un
 * constat mais **ce qu'un clic ferait**.
 */
export function emptyToolsProse(suggestions: readonly ToolSuggestion[]): string | null {
    if (suggestions.length === 0) return null;
    const names = suggestions.map((suggestion) => suggestion.command).join(", ");
    return `ash has seen ${names} running in your tabs. declaring one writes nothing — it starts the checks, and the hooks stay yours to install.`;
}

/**
 * Ce que la fenêtre fait d'un outil désigné par la sidebar (ADR-0006).
 *
 * `null` veut dire « il n'y a rien à saisir » : l'outil est **déjà déclaré**, et sa carte est
 * là avec sa ligne `hooks` et son bouton. Proposer une saisie de plus ferait apparaître deux
 * fois le même outil dans l'écran, dont une qui serait refusée à l'ajout.
 *
 * Sinon, on rend une saisie **pré-remplie** — et rien n'est écrit : c'est le formulaire
 * d'ajout ordinaire, avec sa vérification et son bouton, donc le flux qui existe déjà
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
 *
 * L'adaptateur reconnu n'est pas forcément embarqué par cette compilation — `claude-code`
 * disparaît quand `ash-event` est introuvable : on retombe alors sur le premier proposé,
 * plutôt que d'afficher un menu sur une valeur qu'il ne contient pas.
 *
 * **Le dossier de configuration reste vide ici, et c'est l'appelant qui le propose**
 * (ADR-0006) : il se demande au backend, pour l'adaptateur que cette saisie porte — donc
 * une fois qu'elle existe, et jamais quand il n'y en a pas. Un champ vide est le bon état
 * par défaut : un adaptateur sans dossier conventionnel (`generic`) et un dossier qui n'est
 * pas sur le disque se disent tous deux par le silence du champ, puis par le test 1 que la
 * séquence lance aussitôt sur ce brouillon — « no configuration folder — the generic
 * adapter has no default ». Rempli, le champ reste un champ : on l'édite, et les quatre
 * tests le jugent comme un chemin tapé à la main.
 */
export function focusedDraft(focused: FocusedTool, snapshot: SettingsSnapshot): ToolDraft | null {
    if (snapshot.tools.some((tool) => tool.command === focused.command)) return null;
    const adapter = snapshot.adapters.includes(focused.adapter)
        ? focused.adapter
        : (snapshot.adapters[0] ?? GENERIC_ADAPTER);
    return { command: focused.command, label: "", adapter, config: "" };
}

/**
 * Ce qu'une frappe demande au bloc de capture — les trois issues de la planche, et le reste.
 *
 * Elle est pure et testée pour la même raison que [`sectionStep`](./sections.ts) : le bloc de
 * capture consomme **toutes** les frappes tant qu'il est ouvert, donc se tromper d'issue
 * signifie ne plus pouvoir en sortir. Un test vaut mieux qu'un essai.
 *
 * `ignore` est le cas des modificateurs pressés seuls : on tient `⌘` avant de frapper la
 * lettre, et chacun de ces `keydown` arrive ici. Les traiter comme une frappe ferait clignoter
 * un refus (« add ⌘, ⌃ or ⌥ ») entre le moment où l'on presse le modificateur et celui où
 * l'on presse la touche.
 */
export type CaptureIntent = "cancel" | "clear" | "confirm" | "ignore" | "stroke";

/** Les modificateurs, dont un `keydown` arrive seul avant la vraie touche. */
const MODIFIER_KEYS: readonly string[] = ["Shift", "Meta", "Alt", "Control", "CapsLock"];

export function captureIntent(event: { key: string }): CaptureIntent {
    if (event.key === "Escape") return "cancel";
    if (event.key === "Backspace") return "clear";
    if (event.key === "Enter") return "confirm";
    return MODIFIER_KEYS.includes(event.key) ? "ignore" : "stroke";
}

/**
 * La frappe telle que le backend l'attend — le **caractère produit**, la position physique,
 * et les quatre modificateurs.
 *
 * `key` d'abord, et c'est tout le sens de l'issue #133 : macOS apparie un équivalent clavier
 * par **caractère**, pas par position. Sur un AZERTY, la touche marquée `W` est à la position
 * `KeyZ` ; retenir la position posait `⌘Z` sur une touche qui joue `⌘W`, et l'action devenait
 * injoignable. `code` part avec, en repli — le backend ne s'en sert que pour les caractères
 * qu'aucun accélérateur ne sait écrire.
 *
 * Rien n'est décidé ici, pas même « est-ce une combinaison valable » : ce sont deux faits, et
 * la règle est en Rust ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export function readStroke(event: {
    key: string;
    code: string;
    metaKey: boolean;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
}): KeyStroke {
    return {
        key: event.key,
        code: event.code,
        command: event.metaKey,
        control: event.ctrlKey,
        option: event.altKey,
        shift: event.shiftKey,
    };
}
