import type {
    Appearance,
    HooksReport,
    JournalReport,
    NotificationPermission,
    NotificationsReport,
    SettingsSnapshot,
    ShortcutRow,
    ShortcutsReport,
    TestDescription,
    ToolDeclaration,
    ToolDraft,
    UsageReadability,
    UsageReport,
    Verification,
    VerificationState,
} from "./contract";

/**
 * Test Data Builders de la fenêtre de réglages : une vérification, une entrée déclarée, une
 * ligne `hooks`, une saisie, et l'instantané qui les porte.
 *
 * Les défauts sont valides et **déterministes** — une entrée `claude` sur l'adaptateur de
 * repli, dont les quatre tests sont passés. Un scénario ne surcharge que ce qu'il regarde.
 *
 * Ils vivent dans un fichier de la feature, comme [`shared/ipc/builders.ts`](../../shared/ipc/builders.ts)
 * et pour la même raison : `model`, les composites et l'assemblage décrivent tous les trois
 * une entrée déclarée. Chacun avec sa propre fabrique, un champ ajouté à `ToolDeclaration`
 * se rattrape à trois endroits — et les trois finissent par ne plus décrire la même entrée.
 * Rien du bundle applicatif ne l'importe : seuls les tests.
 *
 * `aVerification` **dérive** `allowsHooks` de l'état plutôt que de le laisser surcharger :
 * la règle est celle du backend, et un test qui la contredirait dans son `Given` prouverait
 * quelque chose qui ne peut pas arriver.
 */
export function aVerification(
    state: VerificationState = "valid",
    overrides: Partial<Verification> = {},
): Verification {
    return {
        state,
        tests: ["passed", "passed", "passed", "passed"],
        summary: "folder recognised · claude answers with this folder",
        stoppedAt: null,
        detail: null,
        fix: null,
        launched: null,
        allowsHooks: state === "valid" || state === "caveat" || state === "verifying",
        ...overrides,
    };
}

export function aTool(overrides: Partial<ToolDeclaration> = {}): ToolDeclaration {
    const verification = overrides.verification ?? aVerification();
    return {
        command: "claude",
        label: null,
        adapter: "generic",
        config: null,
        lastValidConfig: null,
        resetFrom: null,
        duplicates: [],
        hooks: aHooksReport(),
        ...overrides,
        verification,
        verified: overrides.verified ?? verification.allowsHooks,
    };
}

/** Une ligne `hooks` posée et à jour — l'état nominal, dont on ne surcharge que le reste. */
export function aHooksReport(overrides: Partial<HooksReport> = {}): HooksReport {
    return {
        state: "installed",
        summary: "installed · v1",
        note: "remove takes out the entries carrying ash's marker.",
        file: "/home/someone/.claude/settings.json",
        action: "remove",
        enabled: true,
        choices: [
            {
                action: "remove",
                label: "remove ash's hooks",
                note: "the entries carrying ash's marker are taken out; yours stay.",
            },
        ],
        diff: null,
        backup: "/home/someone/.claude/settings.json.bak",
        ...overrides,
    };
}

export function aDraft(overrides: Partial<ToolDraft> = {}): ToolDraft {
    return { command: "claude", label: "", adapter: "generic", config: "", ...overrides };
}

/** Les quatre tests, tels que le backend les nomme — leurs libellés viennent de Rust. */
export const FOUR_TESTS: readonly TestDescription[] = [
    { number: 1, label: "the folder exists", shortLabel: "folder", decisive: true },
    { number: 2, label: "the folder is readable", shortLabel: "readable", decisive: true },
    { number: 3, label: "the command exists in PATH", shortLabel: "in PATH", decisive: true },
    { number: 4, label: "the command answers", shortLabel: "answers", decisive: false },
];

/**
 * La section `notifications` telle que le backend la compose aujourd'hui.
 *
 * Le défaut est le cas **réel** : macOS ne dit pas à Ash si l'autorisation est accordée. Les
 * phrases sont celles de `features/settings/notifications.rs` — un scénario qui les
 * réécrirait prouverait quelque chose que le backend n'envoie pas.
 */
export function aNotificationsReport(
    permission: NotificationPermission = "undisclosed",
    overrides: Partial<NotificationsReport> = {},
): NotificationsReport {
    return {
        permission,
        summary: "macOS doesn't tell ash whether notifications are allowed",
        note: "if nothing appears while ash is in the background and an agent is waiting, the permission is the first thing to check:",
        path: "System Settings ▸ Notifications ▸ ash",
        switches: [
            { state: "waiting", enabled: true, means: "an agent is waiting for an answer" },
            { state: "error", enabled: true, means: "an agent failed" },
            { state: "done", enabled: false, means: "an agent finished" },
        ],
        ...overrides,
    };
}

/**
 * La section `usage` telle que le backend la compose (ADR-0016, ADR-0017).
 *
 * Le défaut est le cas **nominal** : les appels sont autorisés, et le trousseau a rendu un
 * jeton. Les phrases et l'adresse sont celles de `features/settings/usage.rs` — un scénario
 * qui les réécrirait prouverait quelque chose que le backend n'envoie pas.
 *
 * Deux surcharges seulement, parce que ce sont les deux seules variations que la section
 * porte : l'interrupteur, et l'issue de la lecture du trousseau.
 */
export class UsageReportBuilder {
    private polling = true;
    private token: UsageReadability = "readable";
    private summary = "ash can read claude code's token";

    /** L'utilisateur a coupé les appels sortants (ADR-0016, condition 3). */
    withCallsCut(): this {
        this.polling = false;
        return this;
    }

    /** L'une des cinq issues possibles d'une lecture de trousseau, avec sa phrase. */
    withToken(token: UsageReadability, summary: string): this {
        this.token = token;
        this.summary = summary;
        return this;
    }

    build(): UsageReport {
        return {
            polling: this.polling,
            token: this.token,
            summary: this.summary,
            note: "it is read when ash calls, kept in memory, and never written anywhere. revoke it whenever you like, here:",
            path: "Keychain Access ▸ login ▸ Claude Code-credentials",
            endpoint: "https://api.anthropic.com/api/oauth/usage",
            accounts:
                "the keychain holds one token. if you sign in with more than one account, the quotas are those of whichever one wrote it last — ash has no way to tell which, and would rather say nothing than name the wrong one.",
        };
    }
}

/** La section `usage` dans son cas nominal. Voir [`UsageReportBuilder`]. */
export function usageReport(): UsageReportBuilder {
    return new UsageReportBuilder();
}

/**
 * Un journal d'attribution vide — l'état de toute session qui n'a encore rien vu naître.
 *
 * Les phrases sont celles que `features::journal` compose : un scénario qui les réécrirait
 * décrirait un backend qui n'existe pas.
 */
export function aJournalReport(overrides: Partial<JournalReport> = {}): JournalReport {
    return {
        entries: 0,
        repos: 0,
        summary: "nothing recorded yet",
        note: "the journal never leaves this machine. it is not synced, and nothing is sent anywhere.",
        path: "~/.ash/journal",
        ...overrides,
    };
}

/**
 * L'apparence par défaut, celle d'une session qui n'a rien choisi : macOS décide, 13 points.
 *
 * Les quatre valeurs sont celles de `features::theme` (`ThemeMode::System`,
 * `FontSize::DEFAULT`, `TerminalFont::DEFAULT_FAMILY`, `SidebarDensity::Comfortable`)
 * — un scénario qui les réécrirait décrirait un backend qui n'existe pas.
 */
export function anAppearance(overrides: Partial<Appearance> = {}): Appearance {
    return { mode: "system", fontSize: 13, font: "JetBrains Mono", density: "comfortable", ...overrides };
}

/** Une ligne de raccourci telle que `menu_shortcuts` la rend — glyphes déjà écrits. */
export function aShortcut(overrides: Partial<ShortcutRow> = {}): ShortcutRow {
    return {
        action: "tab:new",
        group: "terminal",
        label: "New Tab",
        keys: "⌘T",
        defaultKeys: "⌘T",
        changed: false,
        rebindable: true,
        reservation: null,
        ...overrides,
    };
}

/**
 * L'instantané des raccourcis : les lignes qu'on lui donne, sans conflit et rien de changé.
 *
 * Le compteur `changed` est **calculé** depuis les lignes plutôt que réglable : il vient du
 * backend, où il compte les lignes changées, et un scénario qui les ferait mentir l'un à
 * l'autre décrirait un backend qui n'existe pas.
 */
export function aShortcutsReport(rows: readonly ShortcutRow[] = [aShortcut()]): ShortcutsReport {
    return {
        rows: [...rows],
        changed: rows.filter((row) => row.changed).length,
        conflict: null,
    };
}

export function aSnapshot(overrides: Partial<SettingsSnapshot> = {}): SettingsSnapshot {
    return {
        tools: [],
        adapters: ["claude-code", "codex", "generic"],
        tests: FOUR_TESTS,
        ...overrides,
    };
}
