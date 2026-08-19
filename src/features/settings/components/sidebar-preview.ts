/**
 * La miniature de la sidebar — ce que la section `appearance` fait choisir un thème **sur**.
 *
 * Elle vit dans son propre module et non dans la section, parce qu'elle n'est pas du même
 * ordre : la section arrange quatre lignes de réglage, celle-ci redessine une colonne. C'est
 * aussi la seule des deux qui porte une règle — **l'aperçu dit la vérité de la sidebar** —,
 * et une règle se lit mieux à côté de ce qu'elle contraint qu'au milieu d'une mise en page.
 * Elle est composée cinq fois par écran : une par tuile, plus la seconde palette de `system`.
 *
 * Toutes les formes, tous les mots et tous les traitements viennent de `shared/agent-state`,
 * la **même** source que la sidebar et la ligne de statut ; il n'y a pas une seconde table
 * ici, et le trait du glyphe lui-même est celui de la colonne ([`AGENT_GLYPH_STROKE`]).
 */

import { AGENT_GLYPH_STROKE, presentAgentState } from "@/shared/agent-state";
import type { AgentState } from "@/shared/ipc";
import { ElementBuilder, SVG_NAMESPACE, type UiComponent } from "@/shared/ui";

import { label, spacer, tag, type Tag } from "./atoms";

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

/** Les cinq états que l'aperçu montre — dérivé de la table, pour les tests. */
export const PREVIEWED_STATES: readonly AgentState[] = PREVIEW_ROWS.map((line) => line.state);

/**
 * La miniature de la sidebar dans une palette donnée.
 *
 * La palette est posée par une **classe**, et non par `data-theme` : les tokens de
 * `styles.css` sont déclarés sur `:root[data-theme]`, donc un `data-theme` posé sur un `div`
 * ne redéfinirait rien. `.ash-palette-light` et `.ash-palette-dark` sont les mêmes deux blocs
 * de tokens, atteignables ailleurs qu'à la racine — une seule définition, deux portées.
 */
export function sidebarPreview(palette: "light" | "dark"): Tag {
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
        this.inNamespace(SVG_NAMESPACE);
        // Le trait de la colonne, et non un trait recopié : une épaisseur qui divergerait
        // ferait mentir l'aperçu sur exactement ce qu'il est là pour montrer.
        for (const [name, value] of Object.entries(AGENT_GLYPH_STROKE)) this.attr(name, value);
        this.add(new PreviewPath(shape));
    }
}

class PreviewPath extends ElementBuilder {
    constructor(shape: string) {
        super("path");
        this.inNamespace(SVG_NAMESPACE).attr("d", shape);
    }
}

/** Deux miniatures superposées, la claire découpée en diagonale — la tuile `system`. */
export function bothPalettes(): UiComponent {
    return tag("div", "settings-preview-stack").add(
        sidebarPreview("dark"),
        sidebarPreview("light").class("is-clipped"),
    );
}
