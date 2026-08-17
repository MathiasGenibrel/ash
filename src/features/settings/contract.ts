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
 * La section `notifications` de la fenêtre, telle que le backend la compose (spec §8).
 *
 * Rien n'est décidé ici, pas même les deux états qui interrompent : ils viennent de
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
    /** Les états qui interrompent l'utilisateur, dans l'ordre de la spec §8. */
    notified: readonly AgentState[];
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

/** L'apparence courante, telle que le backend la détient. */
export interface Appearance {
    mode: ThemeMode;
    /** La taille de police du terminal, en points. */
    fontSize: number;
}

/**
 * Un raccourci, tel que le menu natif le déclare (spec §4.4). Miroir de `Shortcut` en Rust.
 *
 * Les trois champs viennent de `src-tauri/src/menu.rs`, et c'est le point : les accélérateurs
 * y sont déclarés, donc c'est là qu'ils se lisent. Une table écrite en TypeScript aurait fini
 * par annoncer un raccourci que le menu ne déclare plus, et c'est l'écran des réglages qu'on
 * croit quand les deux ne disent pas la même chose.
 */
export interface Shortcut {
    /** Le sous-menu où l'action vit — `terminal`, `view`, `application`. */
    group: string;
    label: string;
    /** La combinaison, déjà écrite comme macOS l'écrit — `⇧⌘T`. */
    keys: string;
}

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
    /** Prévient à chaque changement, **d'où qu'il vienne** — le menu Vue compris. */
    onAppearanceChanged(listener: (appearance: Appearance) => void): void;
    /** Les raccourcis que le menu déclare. Demandés une fois : le menu ne change pas. */
    shortcuts(): Promise<readonly Shortcut[]>;
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
     * S'abonne au **second temps** : le test 4 a répondu.
     *
     * Un abonnement et non une promesse : le premier temps a déjà répondu depuis longtemps
     * quand celui-ci arrive, et c'est exactement ce que « le résultat en deux temps » veut
     * dire. Rend de quoi se désabonner.
     */
    onVerified(listener: (verified: Verified) => void): Promise<() => void>;
}
