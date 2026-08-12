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
     * L'entrée a-t-elle passé les quatre tests de la spec §9.1 ?
     *
     * Toujours `false` à ce jalon : la vérification est l'issue #15. C'est **lui** qui
     * décide de l'écriture dans `~/.ash/config.toml` — tant qu'il est faux, l'entrée vit
     * en mémoire du backend, et la fenêtre le dit.
     */
    verified: boolean;
}

/** Ce que la fenêtre reçoit en s'affichant, et après chaque modification. */
export interface SettingsSnapshot {
    tools: readonly ToolDeclaration[];
    /** Les adaptateurs que cette version d'Ash embarque (ADR-0008). */
    adapters: readonly string[];
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
}
