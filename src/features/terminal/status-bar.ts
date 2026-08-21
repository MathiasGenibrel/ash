import { ElementBuilder, badge, column, row, type UiComponent } from "@/shared/ui";

/**
 * Le menu contextuel de la ligne de statut — « show in the status bar » (spec §4.2, vue 5c
 * de la maquette).
 *
 * Un clic droit n'importe où sur la ligne ouvre un panneau de 206 px, ancré au-dessus
 * d'elle, qui liste **tout** ce que la ligne sait montrer : la coche, le nom de l'élément,
 * et à droite un aperçu de sa **valeur courante**. Un élément décoché quitte la barre ; il
 * reste dans le menu, grisé et sans coche — c'est le seul endroit d'où on peut le rallumer.
 *
 * **Rien de ce qui se décide ici n'est détenu ici.** Les sept booléens vivent en Rust, dans
 * `features::theme` et `~/.ash/theme.json`, comme le thème et la densité de la sidebar
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : ce module lit ce que le
 * backend annonce, et lui demande une **bascule** — jamais un état qu'il aurait lu en
 * s'ouvrant.
 *
 * ## Deux règles qui se ressemblent et ne se touchent pas
 *
 * Ce menu dit ce que l'utilisateur **veut** lire ; les `@container` de `terminal.css` disent
 * ce qui **tient** dans la largeur restante. La spec §4.2 écrit que le `cwd`, la branche et
 * l'état de l'agent « ne se retirent jamais » : cette phrase parle du resserrement
 * automatique, pas d'un choix. La légende de la vue 5c, elle, est formelle — « chaque élément
 * de la barre se coupe ici, jauge comprise ». Les deux cohabitent, et c'est pour ça que les
 * sept lignes du menu sont coupables alors que trois d'entre elles survivent à toutes les
 * requêtes de conteneur.
 *
 * ## Ce que ce module ne porte pas
 *
 * Le **mode édition** de la vue 5e — glisser-déposer des segments, spacers — n'existe pas
 * (#165), donc la ligne « réorganiser la barre… » de la maquette n'est pas dans ce menu :
 * une entrée qui n'ouvre rien vaut moins que pas d'entrée du tout.
 *
 * Les **aperçus** non plus, et c'est une contrainte de dépendances plutôt qu'un choix : ils
 * se lisent sur `StatusLineModel`, donc [`visibilityRows`](./status-line.ts) vit là où ce
 * modèle est composé. Ce module reste en aval de tout — il n'importe que le socle de
 * composants —, ce qui laisse `ports.ts`, `usage.ts` et `status-line.ts` lire ses types sans
 * qu'aucun cycle ne se forme.
 */

/** L'identifiant d'un segment. Miroir de `StatusBarSegment` en Rust — voir `mirror.ts`. */
export type StatusBarSegmentId =
    | "session"
    | "weekly"
    | "context"
    | "model"
    | "agent"
    | "branch"
    | "cwd";

/** Ce que la ligne montre. Miroir de `StatusBarSegments` en Rust — voir `mirror.ts`. */
export interface StatusBarSegments {
    readonly session: boolean;
    readonly weekly: boolean;
    readonly context: boolean;
    readonly model: boolean;
    readonly agent: boolean;
    readonly branch: boolean;
    readonly cwd: boolean;
}

/**
 * Les défauts de la spec §4.2 : le weekly masqué, le reste visible.
 *
 * Ils sont écrits **deux fois** — ici et dans `features/theme/status_bar.rs` —, et c'est
 * délibéré : la ligne se dessine avant que le premier aller-retour Tauri ait répondu, comme
 * la palette se pose avant que le mode soit connu. Sans eux, la barre s'ouvrirait vide le
 * temps d'un battement. C'est aussi ce qui s'applique quand le backend ne répond rien du
 * tout, ce que le critère « un fichier de préférence absent ou illisible rend les défauts
 * actuels » demande.
 */
export const DEFAULT_STATUS_BAR_SEGMENTS: StatusBarSegments = {
    session: true,
    weekly: false,
    context: true,
    model: true,
    agent: true,
    branch: true,
    cwd: true,
};

/**
 * Les sept segments, dans l'ordre du menu de la vue 5c.
 *
 * Le même ordre qu'en Rust (`StatusBarSegment::ALL`), et il ne s'y vérifie pas à la
 * compilation : ce qui traverse est un identifiant, et le miroir de `mirror.ts` garantit
 * l'ensemble des noms, pas leur ordre. L'ordre est une décision de **présentation**, et il
 * appartient donc à ce côté-ci.
 */
export const MENU_ORDER: readonly StatusBarSegmentId[] = [
    "session",
    "weekly",
    "context",
    "model",
    "agent",
    "branch",
    "cwd",
];

/** Une ligne du menu : la coche, le nom, l'aperçu. */
export interface VisibilityRow {
    readonly id: StatusBarSegmentId;
    readonly name: string;
    /**
     * La valeur **courante** du segment, telle que la barre l'écrirait — jamais un exemple
     * figé, et **vide** quand la donnée n'existe pas.
     *
     * Vide, et non un tiret : c'est la règle d'ADR-0016 appliquée à l'aperçu. Un tiret
     * dirait qu'on attend une valeur, là où il n'y en a pas.
     */
    readonly preview: string;
    readonly shown: boolean;
    /** Un trait au-dessus de cette ligne. */
    readonly separated: boolean;
}

/**
 * Ce que le backend annonce, relu défensivement.
 *
 * Un objet dont il manque un booléen — un backend plus récent que la webview, une réponse
 * qui n'aboutit pas — ne doit pas vider la ligne de statut : chaque champ manquant ou d'un
 * autre type vaut son **défaut**, et non `false`. C'est la même conduite que la lecture
 * tolérante du fichier côté Rust, appliquée au fil.
 */
export function parseStatusBarSegments(value: unknown): StatusBarSegments {
    if (typeof value !== "object" || value === null) return DEFAULT_STATUS_BAR_SEGMENTS;

    const read = value as Partial<Record<StatusBarSegmentId, unknown>>;
    const segments: Record<StatusBarSegmentId, boolean> = { ...DEFAULT_STATUS_BAR_SEGMENTS };
    for (const id of MENU_ORDER) {
        const shown = read[id];
        if (typeof shown === "boolean") segments[id] = shown;
    }
    return segments;
}

/**
 * Une ligne cliquable du menu.
 *
 * Un vrai `<button>`, comme les pastilles de quota et l'ancre de branche : c'est ce qui la
 * met sur le chemin de `tab` et dans l'arbre d'accessibilité sans une ligne de code.
 * `menuitemcheckbox` et `aria-checked` disent ce que la coche dit à l'œil — sans eux, une
 * ligne décochée se lirait comme une ligne cochée dont on aurait effacé le signe.
 */
class MenuLine extends ElementBuilder {
    constructor(line: VisibilityRow) {
        super("button", "status-menu-line");
        this.attr("type", "button")
            .attr("role", "menuitemcheckbox")
            .attr("aria-checked", line.shown ? "true" : "false");
        if (!line.shown) this.class("is-hidden");
    }
}

/**
 * Le panneau, tel qu'il s'affiche : un titre, sept lignes, un trait.
 *
 * `onToggle` reçoit l'identifiant, jamais un booléen : ce qui part vers le backend est une
 * **bascule**, et c'est lui qui décide de ce qu'elle donne.
 */
export function composeVisibilityMenu(
    rows: readonly VisibilityRow[],
    onToggle: (id: StatusBarSegmentId) => void,
): UiComponent {
    const card = column(badge("show in the status bar").class("status-menu-title")).class(
        "status-menu-card",
    );

    for (const line of rows) {
        if (line.separated) card.add(row().class("status-menu-rule"));
        card.add(
            new MenuLine(line)
                .add(
                    // La coche occupe sa colonne même absente : sans elle, le nom d'un
                    // élément masqué glisserait de 14 px vers la gauche et la liste
                    // deviendrait illisible à mesure qu'on décoche.
                    badge(line.shown ? "✓" : "").class("status-menu-check"),
                    badge(line.name).class("status-menu-name"),
                    badge(line.preview).class("status-menu-preview"),
                )
                .on("click", () => {
                    onToggle(line.id);
                }),
        );
    }

    return card;
}
