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

/** Les cinq états de la ligne `hooks`, et rien de plus. */
export type HookState = "installed" | "missing" | "outdated" | "conflict" | "blocked";

/** Ce que le bouton de la ligne propose. Un seul, jamais deux. */
export type HookAction = "install" | "update" | "remove" | "seeTheDiff";

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
    /** La phrase de la ligne — `installed · v1`, `missing`, `v1 · v2 available`… */
    summary: string;
    /** La conséquence, en prose : ce que l'état coûte ou ce que l'action fera. */
    note: string;
    /** Le fichier concerné, quand il y en a un. */
    file: string | null;
    action: HookAction;
    /** Le bouton est-il allumé ? Il reste **visible** dans tous les cas. */
    enabled: boolean;
    /** Les lignes qui divergent — seulement en conflit, et c'est le refus lui-même. */
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
