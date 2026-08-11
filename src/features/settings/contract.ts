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
    /**
     * S'abonne au **second temps** : le test 4 a répondu.
     *
     * Un abonnement et non une promesse : le premier temps a déjà répondu depuis longtemps
     * quand celui-ci arrive, et c'est exactement ce que « le résultat en deux temps » veut
     * dire. Rend de quoi se désabonner.
     */
    onVerified(listener: (verified: Verified) => void): Promise<() => void>;
}
