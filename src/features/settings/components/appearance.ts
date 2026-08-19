import { presentAgentState } from "@/shared/agent-state";
import type { AgentState } from "@/shared/ipc";
import {
    button,
    choice,
    ElementBuilder,
    row,
    SVG_NAMESPACE,
    text,
    type UiChild,
    type UiComponent,
} from "@/shared/ui";

import {
    FONT_STEPS,
    SIDEBAR_DENSITIES,
    THEME_MODES,
    type Appearance,
    type FontStep,
    type SidebarDensity,
    type ThemeMode,
} from "../contract";
import { label, para, spacer, tag, type Tag } from "./atoms";
import { sectionHeader } from "./chrome";

/** Les quatre gestes de la section — tous partent au backend, et n'y reviennent pas. */
export interface AppearanceActions {
    chooseTheme(mode: ThemeMode): void;
    stepFontSize(step: FontStep): void;
    chooseFont(family: string): void;
    chooseDensity(density: SidebarDensity): void;
}

/**
 * Ce que la sidebar de l'aperçu montre : une ligne par état, dans l'ordre de la planche.
 *
 * Les noms sont **des exemples**, pas des données : l'aperçu ne montre pas la session en
 * cours, il montre ce que le thème fait des cinq états. Les prendre aux vrais onglets
 * donnerait une miniature qui change de contenu d'une ouverture à l'autre, et qui ne
 * montrerait pas `error` tant que rien n'a échoué — c'est-à-dire jamais au moment où on
 * choisit un thème.
 *
 * Ce qui **n'est pas** un exemple : l'état de chaque ligne, et tout ce que
 * `shared/agent-state` en dit. La table ci-dessous ne porte que le nom et la durée.
 */
const PREVIEW_ROWS: readonly { state: AgentState; name: string; trailing: string }[] = [
    { state: "working", name: "claude", trailing: "15m" },
    { state: "waiting", name: "codex", trailing: "2m" },
    { state: "done", name: "claude-perso", trailing: "8m" },
    { state: "idle", name: "bash", trailing: "3h" },
    { state: "error", name: "kimi", trailing: "exit 1" },
];

/** Le dépôt qui coiffe l'aperçu — un nom, sans glyphe, comme dans la vraie colonne. */
const PREVIEW_REPO = "omelette-web";

/**
 * Les deux hauteurs de ligne, telles que `src/app/styles.css` les pose sous `[data-density]`.
 *
 * Elles sont **recopiées** ici, et c'est le seul endroit du dépôt où une mesure l'est : la
 * note chiffre ce que le réglage change, et une note qui annonce 24 px pendant que la
 * feuille de style en pose 22 est pire que pas de note du tout. La recopie est tenue par un
 * test qui lit `styles.css` — c'est le même dispositif que `app/styles.test.ts` pour les
 * palettes, et pour la même raison : rien d'autre ne peut rattacher un nombre affiché à un
 * pixel peint.
 */
export const SIDEBAR_ROW_HEIGHTS: Readonly<Record<SidebarDensity, number>> = {
    comfortable: 24,
    compact: 18,
};

/**
 * La section `appearance` de la fenêtre (spec §9, `[appearance]`).
 *
 * **Elle ne détient rien.** Les quatre préférences — thème, taille, police, densité — sont à
 * `features::theme`, en Rust ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 * La section montre ce que le backend dit et demande un changement sans jamais l'appliquer
 * elle-même : un thème bascule quand `ash://theme-mode` revient, pas quand on clique. C'est
 * ce qui fait que la coche du menu Vue et cet écran ne peuvent pas se contredire.
 *
 * **Le thème se choisit sur la sidebar, pas sur trois boutons.** Chaque tuile est une
 * miniature redessinée de la colonne, avec ses cinq états : c'est ce qu'un thème change
 * réellement, et un mot (`dark`) ne le dit pas. La tuile `system` superpose les deux rendus
 * et découpe le clair en triangle — la diagonale est le message, et aucun texte ne
 * l'explique.
 *
 * **L'aperçu dit la vérité de la sidebar, y compris là où la planche s'en écarte** : la
 * planche laisse le nom de la ligne `error` non barré, `presentAgentState` pose `struck` sur
 * cet état, et la vraie colonne le barre. C'est la colonne qui gagne — un aperçu qui ment
 * sur ce qu'il montre est pire qu'une absence d'aperçu. Toutes les formes, tous les mots et
 * tous les traitements viennent de `shared/agent-state`, la **même** source que la sidebar
 * et la ligne de statut ; il n'y a pas une seconde table ici.
 */
export function appearanceSection(
    appearance: Appearance | null,
    fonts: readonly string[] | null,
    actions: AppearanceActions,
): readonly UiChild[] {
    const head = sectionHeader("appearance", null, []);
    if (appearance === null) {
        // L'aller-retour est immédiat en pratique ; un panneau muet ferait quand même croire
        // à une panne le temps qu'il revienne — c'est la conduite de la section
        // `notifications`, pour la même raison.
        return [
            head,
            tag("div", "settings-body").add(
                para("settings-empty-prose", text("asking ash what it is set to…")),
            ),
        ];
    }

    return [
        head,
        tag("div", "settings-body").add(
            themeBlock(appearance.mode, actions),
            tag("div", "settings-appearance-grid").add(
                ...fontRow(appearance, fonts, actions),
                ...sizeRow(appearance, actions),
                ...densityRow(appearance.density, actions),
            ),
        ),
    ];
}

/** Le bloc du thème : son intitulé, ce que l'aperçu montre, et les trois tuiles. */
function themeBlock(current: ThemeMode, actions: AppearanceActions): UiComponent {
    return tag("div", "settings-theme-block").add(
        row(
            label("settings-block-title", "theme"),
            label("settings-block-hint", "preview: the sidebar and its five states"),
        ).class("settings-block-head"),
        tag("div", "settings-theme-grid").add(
            ...THEME_MODES.map((mode) => themeTile(mode, current, actions)),
        ),
    );
}

/**
 * Une tuile : l'aperçu, puis la ligne de choix.
 *
 * C'est un vrai `<button>`, et il porte `aria-pressed` : trois images cliquables sans état
 * exposé laisseraient un lecteur d'écran annoncer trois dessins muets, là où le menu natif
 * annonce trois coches exclusives.
 */
function themeTile(mode: ThemeMode, current: ThemeMode, actions: AppearanceActions): UiComponent {
    const chosen = mode === current;
    return button("")
        .class("settings-theme-tile", chosen ? "is-chosen" : "")
        .attr("aria-pressed", String(chosen))
        .attr("aria-label", `theme: ${mode}`)
        .add(preview(mode), choiceLine(mode, chosen))
        .onClick(() => {
            actions.chooseTheme(mode);
        });
}

/**
 * L'aperçu d'un mode.
 *
 * `system` n'est pas une troisième palette : c'est l'**absence** de choix, donc les deux à la
 * fois. Les deux miniatures sont superposées et celle du dessus est découpée en triangle par
 * `clip-path` — le coin haut-gauche est clair, le coin bas-droit sombre. Rien d'autre ne
 * change entre les trois tuiles : mêmes formes, mêmes retraits, même rail, même teinte sur
 * `waiting`. C'est la démonstration que la section veut faire — le thème clair ne perd ni la
 * hiérarchie ni l'urgence, parce que l'urgence tient au rail et au fond teinté, pas à la
 * luminosité.
 */
function preview(mode: ThemeMode): UiComponent {
    if (mode !== "system") return sidebarPreview(mode);
    return tag("div", "settings-preview-stack").add(
        sidebarPreview("dark"),
        sidebarPreview("light").class("is-clipped"),
    );
}

/**
 * La miniature de la sidebar dans une palette donnée.
 *
 * La palette est posée par une **classe**, et non par `data-theme` : les tokens de
 * `styles.css` sont déclarés sur `:root[data-theme]`, donc un `data-theme` posé sur un `div`
 * ne redéfinirait rien. `.ash-palette-light` et `.ash-palette-dark` sont les mêmes deux blocs
 * de tokens, atteignables ailleurs qu'à la racine — une seule définition, deux portées.
 */
function sidebarPreview(palette: "light" | "dark"): Tag {
    return tag("div", "settings-preview", `ash-palette-${palette}`)
        .attr("aria-hidden", "true")
        .add(
            label("settings-preview-repo", PREVIEW_REPO),
            ...PREVIEW_ROWS.map((line) => previewRow(line.state, line.name, line.trailing)),
        );
}

/**
 * Une ligne d'agent de la miniature — **exactement** ce que `sidebar/view.ts` compose.
 *
 * Les classes sont les mêmes (`is-tinted`, `has-accent-rail`, `is-struck`) parce que ce sont
 * les mêmes décisions : la seule chose que cette fonction ajoute est la petite taille.
 */
function previewRow(state: AgentState, name: string, trailing: string): UiComponent {
    const shown = presentAgentState(state);
    const line = tag("div", "settings-preview-row", shown.className);
    if (shown.tinted) line.class("is-tinted");
    if (shown.rail !== "none") line.class(`has-${shown.rail}-rail`);

    const agent = label("settings-preview-name", name);
    if (shown.struck) agent.class("is-struck");

    return line.add(
        previewGlyph(state),
        agent,
        spacer(),
        label("settings-preview-time", trailing),
    );
}

/**
 * Le glyphe d'un état, à la taille de la miniature.
 *
 * Il lit `shared/agent-state` — la forme, la classe qui la peint, le mot que lit un lecteur
 * d'écran — plutôt que de redessiner cinq signes : une seconde table finirait par montrer un
 * `working` que la sidebar n'a plus. Ce que ce module ajoute est la seule chose que la table
 * ne décide pas : `working` n'a pas de caractère, il a un tracé, et un tracé se pose dans
 * l'espace de noms SVG (même geste que `verification-state.ts`).
 */
function previewGlyph(state: AgentState): UiComponent {
    const shown = presentAgentState(state);
    if (shown.shape === null) {
        return label("settings-preview-glyph", shown.glyph).class(shown.className);
    }
    return new PreviewShape(shown.shape).class(shown.className);
}

class PreviewShape extends ElementBuilder {
    constructor(shape: string) {
        super("svg", "settings-preview-glyph");
        this.inNamespace(SVG_NAMESPACE)
            .attr("viewBox", "0 0 24 24")
            .attr("fill", "none")
            .attr("stroke", "currentColor")
            .attr("stroke-width", "2.75")
            .attr("stroke-linecap", "round")
            .add(new PreviewPath(shape));
    }
}

class PreviewPath extends ElementBuilder {
    constructor(shape: string) {
        super("path");
        this.inNamespace(SVG_NAMESPACE).attr("d", shape);
    }
}

/** La ligne sous une tuile : la pastille, le nom du mode, et ce qu'il engage. */
function choiceLine(mode: ThemeMode, chosen: boolean): UiComponent {
    return row(
        tag("span", "settings-theme-dot", chosen ? "is-chosen" : ""),
        label("settings-theme-name", mode),
        spacer(),
        // `system` suit les bascules de macOS sans redémarrage : c'est ce que la diagonale
        // montre, et le seul mot que la section ajoute.
        label("settings-theme-mention", mention(mode, chosen)),
    ).class("settings-theme-choice");
}

function mention(mode: ThemeMode, chosen: boolean): string {
    if (mode === "system") return chosen ? "active · follows macOS" : "follows macOS";
    return chosen ? "active" : "";
}

/**
 * La police du terminal : une famille choisie dans ce que le système porte réellement.
 *
 * La liste vient du backend, qui a lu les tables `post` et `name` des fichiers de polices de
 * macOS (`features::theme::FontCatalog`). Tant qu'il n'a pas répondu, la section le dit au
 * lieu de proposer un menu à un seul choix — c'est la même conduite que le panneau en
 * attente.
 */
function fontRow(
    appearance: Appearance,
    fonts: readonly string[] | null,
    actions: AppearanceActions,
): readonly UiChild[] {
    if (fonts === null) {
        return settingRow(
            "font",
            [label("settings-appearance-value", appearance.font)],
            "asking macOS which monospace fonts are installed…",
        );
    }

    return settingRow(
        "font",
        [
            choice("terminal font")
                .class("settings-choice")
                .options(fonts, appearance.font)
                .onSelect((family) => {
                    actions.chooseFont(family);
                }),
            label("settings-appearance-count", countFonts(fonts.length)),
        ],
        // Le mot compte : elles sont **installées**, pas embarquées. La seule exception est
        // JetBrains Mono, qu'Ash livre avec lui — c'est pour ça qu'elle est toujours dans la
        // liste, même sur une machine où personne ne l'a installée.
        "monospace only — a terminal whose cells differ in width no longer aligns anything.",
    );
}

function countFonts(count: number): string {
    return `${String(count)} monospace font${count === 1 ? "" : "s"} installed`;
}

/**
 * La taille : trois pas, la valeur, et un échantillon rendu **à cette taille**.
 *
 * Le pas et non un nombre saisi, ni un curseur qui enverrait une valeur : les bornes sont à
 * `FontSize`, en Rust, et une fenêtre qui enverrait une taille en deviendrait le second
 * détenteur ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)). L'échantillon,
 * lui, est ce que la planche apporte de neuf : il rend la même ligne à la taille en cours,
 * donc il montre la conséquence au lieu de l'annoncer.
 */
function sizeRow(appearance: Appearance, actions: AppearanceActions): readonly UiChild[] {
    return settingRow(
        "size",
        [
            label("settings-appearance-value", `${String(appearance.fontSize)} px`),
            ...FONT_STEPS.map((step) =>
                button(step)
                    .class("settings-button")
                    .onClick(() => {
                        actions.stepFontSize(step);
                    }),
            ),
            label("settings-appearance-sample", "❯ bun test src/sidebar").attr(
                // La seule mesure que ce module écrit dans le DOM, et elle n'est pas un
                // choix de dessin : c'est la valeur que le backend détient, montrée telle
                // quelle. Une classe ne pourrait pas la porter — elle change à chaque pas.
                "style",
                `font-size: ${String(appearance.fontSize)}px`,
            ),
        ],
        "one point at a time, and for every open tab at once — the same setting the View menu steps.",
    );
}

/** La densité de la sidebar : deux paliers, deux miniatures, et ce qu'ils mesurent. */
function densityRow(current: SidebarDensity, actions: AppearanceActions): readonly UiChild[] {
    return settingRow(
        "density",
        [
            tag("div", "settings-segmented").add(
                ...SIDEBAR_DENSITIES.map((density) =>
                    button(density)
                        .class("settings-segment", density === current ? "is-chosen" : "")
                        .attr("aria-pressed", String(density === current))
                        .onClick(() => {
                            actions.chooseDensity(density);
                        }),
                ),
            ),
            ...SIDEBAR_DENSITIES.map((density) => densitySketch(density)),
            label("settings-appearance-count", describeDensity()),
        ],
        "it changes the sidebar of the main window, and nothing else — the terminal keeps its size.",
    );
}

/**
 * Les deux croquis : trois barres espacées, puis quatre barres serrées.
 *
 * Abstraits, et pas une seconde miniature de sidebar : ce qui se compare ici est un
 * **rythme**, et redessiner les cinq états une quatrième fois ferait croire que la densité
 * change aussi ce qu'une ligne contient.
 */
function densitySketch(density: SidebarDensity): UiComponent {
    const bars = density === "comfortable" ? 3 : 4;
    const sketch = tag("div", "settings-density-sketch", `is-${density}`).attr("aria-hidden", "true");
    for (let index = 0; index < bars; index += 1) sketch.add(tag("div", "settings-density-bar"));
    return sketch;
}

function describeDensity(): string {
    const [comfortable, compact] = [
        SIDEBAR_ROW_HEIGHTS.comfortable,
        SIDEBAR_ROW_HEIGHTS.compact,
    ];
    return `${String(comfortable)} px / row · ${String(compact)} px when compact`;
}

/** Une ligne de réglage : son nom, ses contrôles, et ce que le réglage engage. */
function settingRow(name: string, controls: readonly UiChild[], note: string): readonly UiChild[] {
    return [
        label("settings-appearance-key", name),
        tag("div", "settings-appearance-cell").add(
            row(...controls).class("settings-appearance-line"),
            para("settings-note", text(note)),
        ),
    ];
}

/** Les cinq états que l'aperçu montre — dérivé de la table, pour les tests. */
export const PREVIEWED_STATES: readonly AgentState[] = PREVIEW_ROWS.map((line) => line.state);
