/**
 * Le contrat de la feature `settings` avec `src-tauri/src/features/settings/`.
 *
 * Il vit **dans la feature** et non dans `shared/ipc/` — contrairement aux onglets et à
 * l'état git — parce que `shared/` obéit à une règle : au moins deux features, et aucune
 * règle propre à l'une d'elles. La fenêtre de réglages est aujourd'hui le seul lecteur de
 * ces formes. Le jour où la sidebar affichera le libellé (`Perso`) d'un agent, elle en
 * aura un second, et c'est ce jour-là que le type déménagera.
 *
 * Rien ici n'est calculé : le frontend **rend** ce que le backend détient
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */

import type { AgentState } from "@/shared/ipc";

/**
 * Une commande reconnue — le `[[command]]` de `~/.ash/config.toml` (spec §9).
 *
 * Miroir de `ToolDeclaration` en Rust. `command` est l'identité : c'est le `match` du
 * fichier, donc le nom de processus que la sonde compare.
 */
export interface ToolDeclaration {
    command: string;
    /** Le libellé d'affichage — `Pro`, `Perso`. Facultatif, et c'est le cas courant. */
    label: string | null;
    adapter: string;
    /** `null` veut dire « le dossier par défaut de l'adaptateur », pas « aucun dossier ». */
    config: string | null;
    /**
     * L'entrée a-t-elle prouvé assez pour qu'Ash écrive chez l'utilisateur ?
     *
     * Recopie de `verification.allowsHooks`, calculée en Rust. La fenêtre ne la recalcule
     * jamais : c'est la règle qui décide d'écrire dans un fichier, et elle n'a qu'un
     * propriétaire ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    verified: boolean;
    /** Ce que les quatre tests de la spec §9.1 ont dit de cette entrée. */
    verification: Verification;
    /**
     * Le dernier dossier qui a passé les quatre tests, ou `null` s'il n'y en a jamais eu.
     *
     * C'est ce que « réinitialiser » restaure (spec §9.1) — **pas** le défaut de
     * l'adaptateur. `null` veut dire qu'il n'y a rien à restaurer : le bouton reste alors
     * visible et éteint, avec sa raison.
     */
    lastValidConfig: string | null;
    /** Le dossier que la réinitialisation vient de remplacer, tant qu'on peut l'annuler. */
    resetFrom: string | null;
    /** Les autres entrées qui visent le même dossier. Signalé **sur les deux lignes**. */
    duplicates: readonly string[];
    /** Où en est le bloc de hooks de cette entrée. **Calculé en Rust.** */
    hooks: HooksReport;
}

/**
 * Les cinq états de la ligne `hooks`, et rien de plus.
 *
 * `missing` est **l'absence** — rien n'est posé et rien ne s'y oppose ; `conflict` est « il
 * y a là quelque chose que je n'ai pas mis » ; `blocked` est « ash ne peut pas écrire ici ».
 * Les trois se ressemblaient à l'écran, et l'absence passait pour un refus
 * ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md), amendement du 2026-08-12).
 */
export type HookState = "installed" | "missing" | "outdated" | "conflict" | "blocked";

/** Ce qu'un bouton de la ligne — ou du diff — déclenche. */
export type HookAction = "install" | "update" | "remove" | "seeTheDiff";

/**
 * Une issue offerte depuis le diff, avec son libellé et sa conséquence.
 *
 * Le mot vient du backend : « merge » et « install » sont le même geste pour lui, et deux
 * promesses différentes pour celui qui lit l'écran.
 */
export interface HookChoice {
    action: HookAction;
    label: string;
    note: string;
}

/**
 * Ce que la ligne `hooks` d'une entrée affiche, et ce qu'elle laisse faire.
 *
 * Miroir de `HooksReport` en Rust, et **entièrement calculé là-bas** : composer ce que la
 * vérification autorise, ce qu'une autre entrée a déjà pris et ce que le fichier de
 * l'utilisateur porte est la règle qui décide d'écrire chez lui, et elle n'a qu'un
 * propriétaire ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
 */
export interface HooksReport {
    state: HookState;
    /** La phrase de la ligne — `installed · v1`, `no ash hooks in this file`… */
    summary: string;
    /** La conséquence, en prose : ce que l'état coûte ou ce que l'action fera. */
    note: string;
    /** Le fichier concerné, quand il y en a un. */
    file: string | null;
    action: HookAction;
    /** Le bouton est-il allumé ? Il reste **visible** dans tous les cas. */
    enabled: boolean;
    /** Ce que le diff propose de trancher, dans l'ordre. Vide quand rien ne s'écrira. */
    choices: readonly HookChoice[];
    /** Le diff de ce qu'Ash écrirait, sur le fichier **tel qu'il est** — avant d'écrire. */
    diff: string | null;
    /** La copie prise **avant** l'action, annoncée avant et non après. */
    backup: string | null;
}

/** L'état d'une pastille de la rangée de tests. */
export type TestOutcome = "pending" | "running" | "passed" | "warned" | "failed" | "skipped";

/** Les cinq états de vérification, et rien de plus. */
export type VerificationState = "unverified" | "verifying" | "valid" | "caveat" | "invalid";

/** Ce qui était attendu, et ce qui a été trouvé. */
export interface Mismatch {
    expected: string;
    found: string;
}

/** Ce qu'`apply` changerait dans l'entrée — et rien d'autre ne change jamais. */
export type FixAction =
    | { kind: "useAdapter"; adapter: string }
    | { kind: "useFolder"; path: string };

/**
 * La correction proposée. `apply` est `null` quand rien de ce qu'Ash sait faire n'a de
 * chance — un dossier verrouillé ne se déverrouille pas depuis cette fenêtre — et la
 * question reste posée quand même.
 */
export interface SuggestedFix {
    question: string;
    apply: FixAction | null;
}

/** Ce qu'une entrée a prouvé, tel que le backend le calcule. Miroir de `Verification`. */
export interface Verification {
    state: VerificationState;
    /** Les quatre pastilles, dans l'ordre des tests. */
    tests: readonly TestOutcome[];
    /** La phrase de la ligne `test`. */
    summary: string;
    /** `stopped at test <n>`, quand la chaîne s'est arrêtée. */
    stoppedAt: number | null;
    detail: Mismatch | null;
    fix: SuggestedFix | null;
    /** La commande réellement lancée, montrée pendant l'attente du test 4. */
    launched: string | null;
    /** Les hooks peuvent-ils être écrits ? **Calculé en Rust, jamais ici.** */
    allowsHooks: boolean;
}

/**
 * Un des quatre tests, tel que le backend le nomme.
 *
 * Les libellés viennent du contrat et ne sont pas écrits une seconde fois dans la vue :
 * les tests existent en Rust, donc ils s'y nomment. Une liste recopiée finirait par
 * décrire un test que la séquence ne lance plus.
 */
export interface TestDescription {
    number: number;
    label: string;
    shortLabel: string;
    /** Son échec invalide-t-il l'entrée, ou la réserve-t-il seulement ? */
    decisive: boolean;
}

/** Ce que la fenêtre reçoit en s'affichant, et après chaque modification. */
export interface SettingsSnapshot {
    tools: readonly ToolDeclaration[];
    /** Les adaptateurs que cette version d'Ash embarque (ADR-0008). */
    adapters: readonly string[];
    /** Les quatre tests, dans l'ordre où ils se lancent. */
    tests: readonly TestDescription[];
}

/** Ce que le second temps rapporte, pour une entrée nommée. */
export interface Verified {
    command: string;
    verification: Verification;
    /**
     * Ce que `verified` vaut désormais pour cette entrée — **calculé en Rust**.
     *
     * Il voyage avec le résultat plutôt que d'être redéduit ici : c'est le oui/non qui
     * décide d'écrire chez l'utilisateur, et le rejouer côté fenêtre lui donnerait un
     * second propriétaire ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    verified: boolean;
    /**
     * La ligne `hooks` de l'entrée **après** ce résultat, telle que le registre la repose.
     *
     * Elle voyage avec lui parce que le test 4 peut la changer : une entrée qui attendait
     * sa réponse laissait déjà écrire, et un test 4 en échec la rend invalide. Sans elle,
     * la fenêtre garderait celle du premier temps — un bouton `install` allumé sur une
     * entrée que le backend refuse désormais.
     *
     * `null` pour une saisie du formulaire d'ajout : elle n'est pas au registre, et le
     * formulaire ne montre aucune ligne `hooks`.
     */
    hooks: HooksReport | null;
}

/**
 * Ce que macOS laisse savoir à Ash de son autorisation de notifier.
 *
 * `undisclosed` est **la seule valeur produite aujourd'hui**, et la raison est en Rust
 * (`features/settings/notifications.rs`) : le `permission_state()` de bureau du plugin rend
 * une constante `granted`, donc Ash ne peut affirmer ni l'un ni l'autre sans risquer de
 * mentir à celui qui a refusé.
 */
export type NotificationPermission = "granted" | "denied" | "undisclosed";

/**
 * Un interrupteur de la section `notifications` (spec §9, `[notifications]`).
 *
 * Sa position vient du backend, et y retourne : la fenêtre ne bascule rien elle-même — elle
 * demande, et redessine ce que le backend lui répond
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). Sans quoi un interrupteur
 * resterait allumé à l'écran alors que la bannière ne sortirait plus, ou l'inverse.
 */
export interface NotificationSwitch {
    state: AgentState;
    enabled: boolean;
    /** Ce que l'état veut dire, en quelques mots — écrit en Rust, comme le reste. */
    means: string;
}

/**
 * La section `notifications` de la fenêtre, telle que le backend la compose (spec §8).
 *
 * Rien n'est décidé ici, pas même quels états peuvent interrompre : ils viennent de
 * `features/agents`, seul propriétaire de ce que « notifier » veut dire
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface NotificationsReport {
    permission: NotificationPermission;
    /** La phrase de la ligne d'état. */
    summary: string;
    /** Sa conséquence, en prose. */
    note: string;
    /** Le chemin où l'autorisation se donne, mot pour mot. */
    path: string;
    /** Les trois interrupteurs, dans l'ordre de la spec §8. */
    switches: readonly NotificationSwitch[];
}

/**
 * Ce que le journal d'attribution pèse, tel que `features::journal` le dit
 * ([ADR-0014](../../../docs/adr/0014-attribution-locale-des-commits.md)).
 *
 * **Jamais son contenu.** Le fichier porte des prompts ; l'écran en montre le poids,
 * l'endroit et la promesse — il n'en montre pas une ligne. Les trois phrases viennent du
 * backend, comme celles des notifications et pour la même raison : celle qui compte est la
 * promesse de la spec §10, et elle ne doit pas pouvoir diverger d'un écran à l'autre.
 */
export interface JournalReport {
    entries: number;
    repos: number;
    /** Ce qu'il pèse, en toutes lettres — et ce qu'une purge emporterait. */
    summary: string;
    /** Ce qu'il ne fait pas : ni synchronisation, ni envoi. */
    note: string;
    /** Où il vit, mot pour mot. */
    path: string;
}

/**
 * Le thème choisi, tel que `features::theme` le détient (spec §9, `[appearance]`).
 *
 * Il est déclaré ici **en plus** de `src/app/theme.ts`, et les deux sont tenus au même type
 * Rust par une ligne de [`mirror`](./mirror.ts) : une feature n'importe pas le composition
 * root, et recopier trois chaînes sans filet est exactement la faute que ce dispositif
 * attrape ailleurs.
 */
export type ThemeMode = "light" | "dark" | "system";

/**
 * Le pas de taille de police que la fenêtre demande — **jamais un nombre**.
 *
 * Les bornes et la valeur courante appartiennent à `FontSize`, en Rust : envoyer une taille
 * ferait de cette fenêtre un second détenteur de l'état
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)), et rien n'y borne quoi que
 * ce soit. C'est aussi pourquoi les trois mots servent de libellés aux trois boutons : ils
 * viennent du contrat, pas d'une table écrite dans la vue.
 */
export type FontStep = "bigger" | "smaller" | "default";

/** Les trois pas, dans l'ordre où la section les propose. */
export const FONT_STEPS: readonly FontStep[] = ["smaller", "bigger", "default"];

/** Les trois thèmes, dans l'ordre du menu natif — du plus explicite au moins. */
export const THEME_MODES: readonly ThemeMode[] = ["light", "dark", "system"];

/**
 * La densité de la sidebar (spec §9). Miroir de `SidebarDensity` en Rust.
 *
 * Deux paliers et pas une hauteur : ce qui se règle est un confort de lecture, et les deux
 * jeux de mesures vivent dans `src/app/styles.css`, sous `[data-density]`.
 */
export type SidebarDensity = "comfortable" | "compact";

/** Les deux paliers, dans l'ordre du segmenté — du plus aéré au plus dense. */
export const SIDEBAR_DENSITIES: readonly SidebarDensity[] = ["comfortable", "compact"];

/**
 * L'apparence courante, telle que le backend la détient.
 *
 * Les quatre préférences voyagent ensemble parce qu'elles s'affichent ensemble, mais elles
 * n'ont ni la même surface ni le même chemin : le mode et la taille en ont deux (le menu
 * Vue et cet écran), la police et la densité n'en ont qu'une. Toutes les quatre sont à
 * `features::theme` ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface Appearance {
    mode: ThemeMode;
    /** La taille de police du terminal, en points. */
    fontSize: number;
    /** La **famille** du terminal — jamais une pile, jamais un fichier. */
    font: string;
    density: SidebarDensity;
}

/**
 * Une ligne de la section `shortcuts` (spec §4.4). Miroir de `ShortcutRow` en Rust.
 *
 * Les sept champs viennent de `features::shortcuts`, et c'est le point : les liaisons y sont
 * détenues, le menu natif s'en déduit, donc c'est là qu'elles se lisent. Une table écrite en
 * TypeScript aurait fini par annoncer un raccourci que le menu ne joue plus, et c'est l'écran
 * des réglages qu'on croit quand les deux ne disent pas la même chose (issue #110).
 */
export interface ShortcutRow {
    /** L'identifiant d'action — ce que la capture renvoie au backend. */
    action: string;
    /** Le sous-menu où l'action vit — `terminal`, `view`, `application`. */
    group: string;
    label: string;
    /** La combinaison, déjà écrite comme macOS l'écrit — `⇧⌘T`. Vide : aucun raccourci. */
    keys: string;
    /** Ce que `back to default` rendrait. */
    defaultKeys: string;
    /** La ligne porte l'icône de retour, et elle seule (`only appears on changed rows.`). */
    changed: boolean;
    /** La ligne s'ouvre en capture. Faux pour la famille `⌘1 … ⌘9`. */
    rebindable: boolean;
    /** Ce qui prend la combinaison avant Ash — un avertissement, jamais un refus. */
    reservation: Reservation | null;
}

/** Qui prend une combinaison avant Ash. Miroir de `ReservedBy` en Rust. */
export type ReservedBy = "macos" | "terminal";

/**
 * Ce qu'Ash annonce d'une combinaison réservée. Miroir de `Reservation` en Rust.
 *
 * La phrase vient du backend parce qu'elle est propre à **cette** combinaison : « force
 * quit » n'est pas « emoji picker ». La fenêtre ne fait que la poser.
 */
export interface Reservation {
    by: ReservedBy;
    note: string;
}

/**
 * Les deux lignes d'un conflit et la ou les issues qui le referment. Miroir de
 * `ShortcutConflict` en Rust.
 *
 * Il naît d'une capture qui viserait une combinaison déjà prise, et **rien n'est appliqué
 * tant qu'il vit** : ash ne réattribue jamais en silence.
 *
 * `give` est **absent** quand le détenteur ne peut pas céder — la famille `Tab 1 … Tab 9`
 * n'est pas réglable. Le bloc est alors un refus, et sa seule issue est `keep`. La fenêtre ne
 * décide pas laquelle des deux formes elle a sous les yeux : elle rend ce qui lui est donné.
 */
export interface ShortcutConflict {
    keys: string;
    holder: string;
    holderLabel: string;
    asked: string;
    askedLabel: string;
    diagnosis: string;
    give: string | null;
    keep: string;
}

/** Tout ce que la section `shortcuts` affiche. Miroir de `ShortcutsReport` en Rust. */
export interface ShortcutsReport {
    rows: ShortcutRow[];
    /** Le compteur d'en-tête — `n changed`. */
    changed: number;
    conflict: ShortcutConflict | null;
}

/**
 * Une frappe, telle que la webview la rapporte. Miroir de `KeyStroke` en Rust.
 *
 * Ce sont des **faits**, pas une décision : le caractère produit, la position physique de la
 * touche, et l'état des quatre modificateurs. C'est le backend qui dit lequel des deux fait
 * le raccourci ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) — sans ce
 * partage, la table des noms de touches existerait des deux côtés de la frontière.
 */
export interface KeyStroke {
    /** `KeyboardEvent.key` — le caractère produit, ou le nom d'une touche qui n'en produit pas. */
    key: string;
    /** `KeyboardEvent.code` — la position physique, nommée d'après un clavier US. */
    code: string;
    command: boolean;
    control: boolean;
    option: boolean;
    shift: boolean;
}

/** Ce que le bloc de capture montre pendant qu'on tape. Miroir de `CapturePreview`. */
export interface CapturePreview {
    keys: string;
    /** Elle peut être confirmée par `⏎`. */
    accepted: boolean;
    /** Pourquoi elle ne peut pas l'être, le cas échéant — écrit par le backend. */
    why: string | null;
    reservation: Reservation | null;
}

/** L'issue choisie devant un conflit. Miroir de `ConflictChoice` en Rust. */
export type ConflictChoice = "give" | "keep";

/**
 * Ce que la fenêtre demande aux **objets de fenêtre** : le thème, la taille de police, le
 * menu.
 *
 * Ils sont séparés de [`SettingsPorts`] parce qu'ils n'appartiennent pas à
 * `features::settings` : le thème et la taille sont à `features::theme`, la liste des
 * raccourcis à `src-tauri/src/menu.rs`. Le composition root de la fenêtre
 * (`src/app/settings.ts`) les branche sur les modules qui savent déjà leur parler — c'est
 * lui, et lui seul, qui connaît `theme_mode` et `ash://theme-mode`.
 *
 * **Rien n'est rendu par les deux verbes d'écriture.** La nouvelle valeur revient par
 * l'annonce du backend, celle que le menu natif fait déjà partir : c'est ce qui rend les deux
 * surfaces incapables de diverger ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface WindowPorts {
    /** L'apparence en vigueur, une fois le raccordement au backend fait. */
    appearance(): Promise<Appearance>;
    chooseThemeMode(mode: ThemeMode): Promise<void>;
    stepTerminalFontSize(step: FontStep): Promise<void>;
    /**
     * Les familles monospace que le système porte, telles que le backend les a lues.
     *
     * Demandées une fois, comme les raccourcis : installer une police passe par le Livre des
     * polices, pas par Ash, et la liste ne bouge donc pas pendant qu'une session dure.
     */
    monospaceFonts(): Promise<readonly string[]>;
    /**
     * Une **valeur**, là où la taille demande un pas : il n'existe pas de « police
     * suivante », et la liste est celle que le backend vient de rendre.
     */
    chooseTerminalFont(family: string): Promise<void>;
    chooseSidebarDensity(density: SidebarDensity): Promise<void>;
    /** Prévient à chaque changement, **d'où qu'il vienne** — le menu Vue compris. */
    onAppearanceChanged(listener: (appearance: Appearance) => void): void;
    /**
     * Les raccourcis en vigueur, et ce qu'il faut pour les montrer.
     *
     * Les six verbes qui suivent rendent le **même instantané**, comme les commandes de
     * `settings` : la fenêtre redessine à partir de ce que le backend renvoie, elle ne
     * modifie jamais une liste locale
     * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). C'est ce qui garantit
     * que l'écran et le menu natif ne peuvent pas diverger — les deux dérivent des mêmes
     * liaisons, refaites du même côté.
     */
    shortcuts(): Promise<ShortcutsReport>;
    /**
     * Éteint les entrées du menu le temps d'une capture, et les rallume après.
     *
     * Sur macOS, un accélérateur de menu est consommé **avant** la webview : sans ce geste,
     * `⌘W` frappé pendant une capture fermerait la fenêtre au lieu d'être lu, et échanger
     * deux raccourcis serait impossible.
     */
    listenForShortcut(active: boolean): Promise<void>;
    /** Ce que la frappe donnerait, **sans rien poser** : `⏎` seul pose. */
    previewShortcut(stroke: KeyStroke): Promise<CapturePreview>;
    /** `⏎` — pose la combinaison, ou ouvre le conflit qu'elle produirait. */
    bindShortcut(action: string, stroke: KeyStroke): Promise<ShortcutsReport>;
    /** `⌫` — la ligne n'a plus de raccourci, et garde son entrée de menu. */
    clearShortcut(action: string): Promise<ShortcutsReport>;
    /** L'icône de retour d'une ligne changée. */
    resetShortcut(action: string): Promise<ShortcutsReport>;
    /** `reset all` de l'en-tête. */
    resetAllShortcuts(): Promise<ShortcutsReport>;
    /** L'une des deux issues nommées du bloc de conflit. */
    resolveShortcutConflict(choice: ConflictChoice): Promise<ShortcutsReport>;
}

/**
 * Ce qu'un retrait emporterait dans un fichier — **avant** que rien ne soit écrit.
 *
 * Le pendant du diff d'installation, pour le geste inverse : l'écran ne décide pas ce qui
 * part, il rend ce que le backend a vu dans le fichier
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 */
export interface PlannedRemoval {
    file: string;
    /** Les entrées déclarées qui visent ce fichier — deux comptes peuvent le partager. */
    commands: string[];
    entries: number;
    /** Le fichier ne portait que ça : il s'en va avec elles (spec §10). */
    deletesTheFile: boolean;
    handEdited: boolean;
    diff: string;
}

/** Ce que « retirer ash de tous les fichiers » ferait, dit avant de le faire (spec §10). */
export interface RemovalPlan {
    files: PlannedRemoval[];
    summary: string;
    /** Une main est passée quelque part : l'écran montre le diff et demande. */
    handEdited: boolean;
    /** Ce que le geste ne touchera pas — les `.bak`, et `~/.ash`. */
    kept: string[];
}

/** Ce qu'un fichier est devenu. Les quatre issues viennent du backend, jamais d'un test. */
export type Outcome =
    | { kind: "removed" }
    | { kind: "removedTheFile" }
    | { kind: "nothingLeft" }
    | { kind: "refused"; why: string };

export interface RemovedFile {
    file: string;
    entries: number;
    outcome: Outcome;
}

/** Ce que le retrait a réellement fait — et ce qu'il a laissé derrière lui. */
export interface RemovalReport {
    files: RemovedFile[];
    summary: string;
    kept: string[];
}

/** Le compte rendu, et la liste telle qu'elle est après le retrait. */
export interface RemovalOutcome {
    report: RemovalReport;
    snapshot: SettingsSnapshot;
}

/** Ce que le formulaire d'ajout envoie : du texte, pas encore une déclaration. */
export interface ToolDraft {
    command: string;
    label: string;
    adapter: string;
    config: string;
}

/**
 * Ce que la fenêtre sait demander, et qu'elle ne sait pas faire elle-même.
 *
 * Un port, et non un `invoke` posé dans la vue : c'est ce qui permet d'écrire la fenêtre
 * sans Tauri sous la main, et c'est aussi la frontière que le jour du démon `ashd`
 * (ADR-0009) déplacerait sans toucher au rendu.
 */
export interface SettingsPorts {
    tools(): Promise<SettingsSnapshot>;
    /**
     * Ce que la section `notifications` affiche (spec §8).
     *
     * Demandée à chaque ouverture de la section, et pas une seule fois au montage :
     * l'autorisation macOS se change dans les Réglages Système pendant qu'Ash tourne.
     */
    notifications(): Promise<NotificationsReport>;
    /**
     * Bascule l'un des trois interrupteurs (spec §9, `[notifications]`).
     *
     * Elle rend la section **recomposée par le backend** : c'est lui qui détient la position
     * des trois, et l'écran ne fait que la rendre. Un refus laisse donc la section telle
     * qu'elle était — un interrupteur qui bougerait sans que le backend l'ait retenu
     * promettrait le silence à qui continuerait d'être dérangé.
     */
    setNotification(state: AgentState, enabled: boolean): Promise<NotificationsReport>;
    /** Ce que le journal d'attribution a retenu — **il ne rend aucune ligne du fichier**. */
    journal(): Promise<JournalReport>;
    /**
     * Efface le journal (spec §10, ADR-0014). Rend la fiche **relue après coup** : si un
     * fichier a résisté, l'écran doit le dire plutôt que d'afficher un zéro d'autorité.
     */
    purgeJournal(): Promise<JournalReport>;
    declareTool(draft: ToolDraft): Promise<SettingsSnapshot>;
    forgetTool(command: string): Promise<SettingsSnapshot>;
    /** Change le dossier ou l'adaptateur d'une entrée, et relance la séquence. */
    retargetTool(command: string, adapter: string, config: string): Promise<SettingsSnapshot>;
    /** Relance la séquence sur une entrée. */
    verifyTool(command: string): Promise<SettingsSnapshot>;
    /** Relance la séquence sur toute la liste, en parallèle. */
    verifyAll(): Promise<SettingsSnapshot>;
    /** Vérifie une saisie du formulaire d'ajout, sans rien ajouter. */
    verifyDraft(draft: ToolDraft): Promise<Verification>;
    /** Ramène une entrée à son dernier dossier valide (spec §9.1). */
    resetTool(command: string): Promise<SettingsSnapshot>;
    /** Défait la réinitialisation qui vient d'avoir lieu. */
    undoReset(command: string): Promise<SettingsSnapshot>;
    /**
     * Pose ou met à jour le bloc de hooks — **le seul geste de cette fenêtre qui écrive
     * dans un fichier de l'utilisateur**.
     *
     * Il ne porte aucune condition : celle qui décide est en Rust, et c'est la même qui a
     * allumé le bouton ([ADR-0007](../../../docs/adr/0007-etats-par-hooks.md)).
     */
    installHooks(command: string): Promise<SettingsSnapshot>;
    /** Retire le bloc et ses marqueurs. */
    removeHooks(command: string): Promise<SettingsSnapshot>;
    /**
     * Ce que « retirer ash de tous les fichiers » ferait — **elle n'écrit rien**.
     *
     * Deux appels et non un seul, et c'est la règle du produit : Ash dit ce qu'il va faire
     * avant de le faire, et rien ne s'écrit sans un second geste pris devant l'annonce
     * (spec §10).
     */
    removalPlan(): Promise<RemovalPlan>;
    /** Le retrait lui-même — le seul autre geste de cette fenêtre qui écrive. */
    removeAllHooks(): Promise<RemovalOutcome>;
    /**
     * S'abonne au **second temps** : le test 4 a répondu.
     *
     * Un abonnement et non une promesse : le premier temps a déjà répondu depuis longtemps
     * quand celui-ci arrive, et c'est exactement ce que « le résultat en deux temps » veut
     * dire. Rend de quoi se désabonner.
     */
    onVerified(listener: (verified: Verified) => void): Promise<() => void>;
    /**
     * L'outil sur lequel la fenêtre doit se poser, demandé **en s'affichant**.
     *
     * Le marqueur « non instrumenté » de la sidebar (ADR-0006) ouvre cette fenêtre : l'event
     * qui accompagne le geste part avant que la page n'existe, donc c'est la page qui vient
     * chercher la demande. La lire la consomme — rouvrir les réglages par le menu ne ramène
     * pas sur un outil désigné il y a une heure.
     */
    pendingFocus(): Promise<FocusedTool | null>;
    /** Le même geste, quand la fenêtre était **déjà** ouverte. */
    onFocusTool(listener: (focused: FocusedTool) => void): Promise<() => void>;
    /**
     * Le dossier conventionnel d'un adaptateur, `null` s'il n'y en a pas, s'il n'est pas là,
     * ou si Ash ne peut pas le lire (ADR-0006) — les trois se disent par un champ vide,
     * parce qu'un chemin proposé que le test 1 refuserait aussitôt aurait l'air d'une
     * réponse.
     *
     * Demandée **au moment où le formulaire s'ouvre**, et non transportée par
     * [`FocusedTool`] : la demande de la sidebar ne porte qu'un geste, et un résultat de
     * lecture disque glissé dedans daterait de l'instant du clic, pas de celui où la
     * fenêtre le montre. Le backend est seul à savoir ce que l'adaptateur nomme et si ce
     * dossier existe ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
     */
    proposedConfig(adapter: string): Promise<string | null>;
}

/**
 * L'outil que la sidebar désigne — une **demande d'affichage**, jamais une écriture.
 *
 * Miroir de `FocusedTool` en Rust. Ce que la fenêtre en fait est sa décision : montrer
 * l'entrée si elle existe, ou proposer de la déclarer si elle n'existe pas. Rien n'est écrit
 * chez l'utilisateur sans un geste fait dans cet écran (ADR-0007).
 */
export interface FocusedTool {
    command: string;
    adapter: string;
}
