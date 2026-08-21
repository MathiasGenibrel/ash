import { ElementBuilder, badge, button, column, row, type UiComponent } from "@/shared/ui";

/**
 * Ce que la ligne de statut montre, **et dans quel ordre** — le menu contextuel de la vue 5c
 * et le mode édition de la vue 5e (spec §4.2).
 *
 * Un clic droit n'importe où sur la ligne ouvre un panneau de 206 px, ancré au-dessus
 * d'elle, qui liste **tout** ce que la ligne sait montrer : la coche, le nom de l'élément,
 * et à droite un aperçu de sa **valeur courante**. Un élément décoché quitte la barre ; il
 * reste dans le menu, grisé et sans coche. Sous un second trait, une dernière ligne —
 * `⟷ réorganiser la barre…` — ouvre le mode édition : un clic long ne s'invente pas, un menu
 * se lit.
 *
 * **Rien de ce qui se décide ici n'est détenu ici.** La disposition vit en Rust, dans
 * `features::theme` et `~/.ash/theme.json`, comme le thème et la densité de la sidebar
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : ce module lit ce que le
 * backend annonce, et lui demande une **bascule** ou une **disposition** — jamais un état
 * qu'il aurait lu en s'ouvrant.
 *
 * ## Une suite, et non sept booléens
 *
 * #164 lisait un enregistrement de sept booléens ; la vue 5e demande un **ordre**, et des
 * élastiques en nombre libre. Ce qui traverse la frontière est donc une [`StatusBarLayout`] —
 * une suite d'identifiants —, et la visibilité y est une **appartenance**. Les sept booléens
 * ne disparaissent pas pour autant : ils sont **dérivés** par [`shownSegments`], parce que
 * `usage.ts` et `status-line.ts` posent la seule question qu'ils posaient déjà — « ce
 * segment-là, je le peins ou pas ? ». La suite dit où ; l'enregistrement dit si ; le second
 * se calcule à partir du premier, donc ils ne peuvent pas se contredire.
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
 * Les **aperçus** du menu, et c'est une contrainte de dépendances plutôt qu'un choix : ils
 * se lisent sur `StatusLineModel`, donc [`visibilityRows`](./status-line.ts) vit là où ce
 * modèle est composé. Ce module reste en aval de tout — il n'importe que le socle de
 * composants —, ce qui laisse `ports.ts`, `usage.ts` et `status-line.ts` lire ses types sans
 * qu'aucun cycle ne se forme.
 *
 * Le **geste** non plus : le clic maintenu de 430 ms, le glissement et le tiroir touchent au
 * DOM, et vivent dans [`status-bar-editor.ts`](./status-bar-editor.ts). Ce qu'ils décident —
 * où tombe une pastille lâchée, ce que le tiroir contient — est ici, en fonctions pures.
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

/**
 * Ce que la barre porte à une place donnée : un segment, ou un **élastique**.
 *
 * Miroir de `StatusBarItem` en Rust. Le spacer n'est pas un segment : il n'a pas d'identité,
 * il y en a zéro, un ou cinq, et deux spacers ne se distinguent que par leur place. C'est
 * pour ça qu'on ne le **bascule** pas — il s'ajoute et se jette, comme un objet.
 */
export type StatusBarItemId = StatusBarSegmentId | "spacer";

/**
 * La barre, dans l'ordre. Un segment absent est un segment retiré.
 *
 * Un tableau nu plutôt qu'un objet, parce que c'est exactement ce que le backend envoie et
 * que l'y envelopper n'ajouterait rien : ce qui compte est la suite.
 */
export type StatusBarLayout = readonly StatusBarItemId[];

/** Ce que la ligne montre, segment par segment — **dérivé** de la suite, jamais reçu. */
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
 * La barre de la première ouverture — celle de la vue 5e : `cwd · branch · agent · ⟷ ·
 * session · context · model`, le weekly retiré.
 *
 * Elle est écrite **deux fois** — ici et dans `features/theme/status_bar.rs` —, et c'est
 * délibéré : la ligne se dessine avant que le premier aller-retour Tauri ait répondu, comme
 * la palette se pose avant que le mode soit connu. Sans elle, la barre s'ouvrirait vide le
 * temps d'un battement. C'est aussi ce qui s'applique quand le backend ne répond rien du
 * tout, ce que le critère « un fichier de préférence absent ou illisible rend les défauts
 * actuels » demande.
 */
export const DEFAULT_STATUS_BAR_LAYOUT: StatusBarLayout = [
    "cwd",
    "branch",
    "agent",
    "spacer",
    "session",
    "context",
    "model",
];

/**
 * Les sept segments, dans l'ordre du menu de la vue 5c.
 *
 * Le même ordre qu'en Rust (`StatusBarSegment::ALL`), et il ne s'y vérifie pas à la
 * compilation : ce qui traverse est un identifiant, et le miroir de `mirror.ts` garantit
 * l'ensemble des noms, pas leur ordre. L'ordre est une décision de **présentation**, et il
 * appartient donc à ce côté-ci — celui de la barre, lui, se règle et vit en Rust.
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

/**
 * Le nom lu dans le menu **et sur une pastille du mode édition** — celui de la maquette, pas
 * celui du champ.
 *
 * `context bar` et `agent state` disent ce que la ligne montre ; `context` et `agent` ne
 * diraient rien à quelqu'un qui n'a pas écrit le code. Une seule table pour les deux
 * surfaces : le nom d'un élément ne dépend pas de l'endroit où on le lit.
 */
export const SEGMENT_NAMES: Readonly<Record<StatusBarSegmentId, string>> = {
    session: "session",
    weekly: "weekly",
    context: "context bar",
    model: "model",
    agent: "agent state",
    branch: "branch",
    cwd: "cwd",
};

/**
 * Les segments que la ligne sépare d'un `│` quand ils se suivent.
 *
 * Le trait est la marque des segments qui sont des **mots** : le `cwd`, la branche et
 * l'état de l'agent. Les pastilles de quota, la jauge et le nom du modèle ont toujours été
 * séparés par du blanc, parce qu'une pastille se lit comme un objet et n'a pas besoin qu'on
 * lui dise où elle s'arrête. Réorganiser la barre ne change pas ce qu'un élément **est** :
 * c'est ce qui fait que la disposition par défaut se peint exactement comme avant #165.
 */
const RULED: readonly StatusBarItemId[] = ["cwd", "branch", "agent"];

/* ------------------------------------------------------------------------------------- *
 * L'algèbre de la barre — tout ce que le mode édition décide, en fonctions pures.
 * ------------------------------------------------------------------------------------- */

/** Cet identifiant est-il l'un des sept segments ? */
export function isSegment(item: StatusBarItemId): item is StatusBarSegmentId {
    return item !== "spacer";
}

/**
 * Ce que le backend annonce, relu défensivement.
 *
 * Une barre qui n'est pas un tableau — un backend plus récent que la webview, une réponse qui
 * n'aboutit pas — rend les **défauts**. Un tableau, lui, est pris tel quel une fois nettoyé :
 * les mots inconnus tombent, un segment répété ne compte qu'une fois, et les élastiques
 * restent tous.
 *
 * **Un tableau vide reste vide**, et c'est la seule subtilité : tout jeter est un choix de
 * l'utilisateur (le tiroir du mode édition est là pour en revenir), et le confondre avec une
 * réponse manquante lui rendrait sa barre au prochain démarrage sans qu'il l'ait demandé.
 */
export function parseStatusBarLayout(value: unknown): StatusBarLayout {
    if (!Array.isArray(value)) return DEFAULT_STATUS_BAR_LAYOUT;

    const known: readonly StatusBarItemId[] = [...MENU_ORDER, "spacer"];
    const kept: StatusBarItemId[] = [];
    for (const item of value as readonly unknown[]) {
        if (typeof item !== "string") continue;
        const id = known.find((name) => name === item);
        if (id === undefined) continue;
        if (id !== "spacer" && kept.includes(id)) continue;
        kept.push(id);
    }
    return kept;
}

/**
 * Les sept booléens que la barre implique — ce que `usage.ts` et le rendu de la ligne
 * consomment.
 *
 * Dérivés, jamais reçus : c'est ce qui empêche « où est le `cwd` » et « le `cwd` est-il
 * montré » de se contredire.
 */
export function shownSegments(layout: StatusBarLayout): StatusBarSegments {
    return {
        session: layout.includes("session"),
        weekly: layout.includes("weekly"),
        context: layout.includes("context"),
        model: layout.includes("model"),
        agent: layout.includes("agent"),
        branch: layout.includes("branch"),
        cwd: layout.includes("cwd"),
    };
}

/** Les défauts, sous leur forme dérivée — le premier battement de la ligne. */
export const DEFAULT_STATUS_BAR_SEGMENTS: StatusBarSegments =
    shownSegments(DEFAULT_STATUS_BAR_LAYOUT);

/**
 * Ce que le tiroir montre : les segments qui ne sont plus dans la barre, dans l'ordre du
 * menu.
 *
 * Le complément de la barre, et non une liste à part : un élément est **dans** la barre ou
 * **dans** le tiroir, jamais dans les deux ni dans aucun. C'est ce qui fait qu'un `×` et un
 * clic sur une pastille du tiroir sont deux moitiés du même geste.
 */
export function drawerSegments(layout: StatusBarLayout): readonly StatusBarSegmentId[] {
    return MENU_ORDER.filter((id) => !layout.includes(id));
}

/**
 * La barre, la pastille de `from` déposée à la place `to`.
 *
 * Les deux indices sont ceux de la barre **telle qu'elle est** : `to` est la place que la
 * pastille occupera une fois arrivée, pas une place lue avant son départ. C'est la forme
 * qu'un glissement produit naturellement — on regarde sous le pointeur, et on répond « c'est
 * la troisième » — et c'est ce qui rend la fonction sûre à appeler à chaque mouvement.
 *
 * Un indice hors de la barre rend la barre inchangée : le pointeur sort de la ligne pendant
 * un glissement, et rien ne doit se réordonner sur un geste qui a quitté la surface.
 */
export function moveItem(layout: StatusBarLayout, from: number, to: number): StatusBarLayout {
    const moved = layout[from];
    if (moved === undefined || to < 0 || to >= layout.length || from === to) return layout;

    const rest = [...layout.slice(0, from), ...layout.slice(from + 1)];
    return [...rest.slice(0, to), moved, ...rest.slice(to)];
}

/** La barre, sans son élément de rang `index` — le `×` d'une pastille. */
export function removeAt(layout: StatusBarLayout, index: number): StatusBarLayout {
    if (index < 0 || index >= layout.length) return layout;
    return [...layout.slice(0, index), ...layout.slice(index + 1)];
}

/**
 * La barre, un élastique de plus au bout — le bouton `⟷ spacer` du tiroir.
 *
 * **Au bout**, et non à une place devinée : un spacer n'a pas de voisin naturel, et le poser
 * ailleurs qu'à l'endroit le plus visible obligerait à le chercher. Il se déplace ensuite
 * comme les autres.
 */
export function appendSpacer(layout: StatusBarLayout): StatusBarLayout {
    return [...layout, "spacer"];
}

/**
 * Sur quelle place tombe un pointeur à l'abscisse `x`, connaissant le **milieu** de chaque
 * pastille.
 *
 * La règle est celle de tous les réordonnancements par glissement, et elle tient en une
 * phrase : on prend la place de la pastille dont on a dépassé le milieu. Comparer aux
 * milieux plutôt qu'aux bords est ce qui évite le battement — deux pastilles qui
 * s'échangeraient sans fin dès que le pointeur effleure leur frontière commune.
 *
 * Une barre vide rend `0` : il n'y a qu'une place, et c'est la première.
 */
export function dropIndex(centers: readonly number[], x: number): number {
    let index = 0;
    for (const center of centers) {
        if (x < center) break;
        index += 1;
    }
    return index;
}

/* ------------------------------------------------------------------------------------- *
 * Le menu contextuel (vue 5c) et ses lignes.
 * ------------------------------------------------------------------------------------- */

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
 * La dernière ligne du menu — `⟷ réorganiser la barre…`, avec `clic long` à droite.
 *
 * `menuitem` et non `menuitemcheckbox`, et c'est un critère de la tâche : elle **agit**, elle
 * ne bascule rien, donc elle n'a pas d'`aria-checked` à porter. Un lecteur d'écran qui
 * l'annoncerait « non cochée » raconterait un état qui n'existe pas.
 *
 * `clic long` occupe la place qu'occupe ailleurs l'aperçu d'une valeur : c'est un **rappel du
 * geste**, pas un second bouton. La ligne existe précisément parce qu'un clic long ne
 * s'invente pas — le menu est la porte découvrable de la vue 5e.
 */
class ReorderLine extends ElementBuilder {
    constructor() {
        super("button", "status-menu-line", "status-menu-action");
        this.attr("type", "button").attr("role", "menuitem");
    }
}

/**
 * Le panneau, tel qu'il s'affiche : un titre, sept lignes, un trait, et la porte du mode
 * édition.
 *
 * `onToggle` reçoit l'identifiant, jamais un booléen : ce qui part vers le backend est une
 * **bascule**, et c'est lui qui décide de ce qu'elle donne. `onReorder`, lui, ne dit rien au
 * backend — le mode édition est un moment de l'interface, pas une préférence : il ne survit
 * ni à `Échap`, ni à la fermeture de la fenêtre.
 */
export function composeVisibilityMenu(
    rows: readonly VisibilityRow[],
    onToggle: (id: StatusBarSegmentId) => void,
    onReorder: () => void = () => undefined,
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

    card.add(
        row().class("status-menu-rule"),
        new ReorderLine()
            .add(
                badge("⟷").class("status-menu-check"),
                badge("réorganiser la barre…").class("status-menu-name"),
                badge("clic long").class("status-menu-preview"),
            )
            .on("click", onReorder),
    );

    return card;
}

/* ------------------------------------------------------------------------------------- *
 * Le mode édition (vue 5e) — ce que la barre et le tiroir montrent.
 * ------------------------------------------------------------------------------------- */

/** Les gestes du tiroir. Aucun n'est appliqué ici : ils partent vers le backend. */
export interface DrawerActions {
    /** Le segment cliqué dans le tiroir revient dans la barre. */
    readonly onPick: (id: StatusBarSegmentId) => void;
    /** Un élastique de plus. */
    readonly onSpacer: () => void;
    /** La barre reprend sa disposition d'origine — le `reset all` de la spec §4.4. */
    readonly onReset: () => void;
}

/**
 * Les pastilles de la barre en édition, dans l'ordre.
 *
 * Le libellé est le **nom** de l'élément, jamais sa valeur, et c'est une décision : on
 * arrange des éléments, pas des chiffres. Une pastille qui montrerait `s 63% · 2h14`
 * demanderait de reconnaître un segment à ce qu'il affiche à cet instant — or `ctx` peut
 * être vide, un quota peut manquer, et un élément absent de l'écran ne se glisse pas. C'est
 * aussi ce qui donne au tiroir et à la barre le même vocabulaire, alors qu'une pastille du
 * tiroir n'a par construction aucune valeur à montrer.
 *
 * Le `frémissement` de la maquette est décalé de `(i % 3) × 60 ms`, et le rang est porté ici
 * plutôt que calculé en CSS : `nth-child(3n)` compterait les nœuds du DOM, et le tiroir en
 * ajoute juste à côté.
 */
export interface EditorPill {
    readonly index: number;
    readonly item: StatusBarItemId;
    readonly label: string;
    /** `0`, `1` ou `2` — le décalage du frémissement. */
    readonly beat: number;
}

/** Les pastilles que la barre montre en édition. */
export function editorPills(layout: StatusBarLayout): readonly EditorPill[] {
    return layout.map((item, index) => ({
        index,
        item,
        label: isSegment(item) ? SEGMENT_NAMES[item] : "⟷ spacer",
        beat: index % 3,
    }));
}

/**
 * Le tiroir, ancré contre la barre : son libellé, le bouton `⟷ spacer`, le retour aux
 * défauts, puis une pastille par élément retiré.
 *
 * **Le retour aux défauts n'est pas dans la maquette**, et il est là parce qu'un critère le
 * demande : une barre vidée de tout doit rester récupérable. Il est à côté du bouton
 * `⟷ spacer` parce que c'est le seul endroit qui existe encore quand la barre n'a plus rien
 * — c'est le `reset all` des raccourcis (spec §4.4), avec la même conduite : il rend la
 * disposition d'origine, il ne demande pas confirmation, et rien n'est perdu qui ne se
 * refasse.
 */
export function composeDrawer(layout: StatusBarLayout, actions: DrawerActions): UiComponent {
    const drawer = row(
        badge("glisser dans la barre · cliquer pour ajouter").class("status-drawer-hint"),
    ).class("status-drawer");

    drawer.add(
        button("⟷ spacer").class("status-drawer-spacer").onClick(actions.onSpacer),
        button("défauts").class("status-drawer-reset").onClick(actions.onReset),
    );

    for (const id of drawerSegments(layout)) {
        drawer.add(
            button(SEGMENT_NAMES[id])
                .class("status-drawer-pill")
                .onClick(() => {
                    actions.onPick(id);
                }),
        );
    }

    return drawer;
}

/* ------------------------------------------------------------------------------------- *
 * Le rendu de la barre au repos — quelle place occupe quoi, et où tombent les `│`.
 * ------------------------------------------------------------------------------------- */

/**
 * Une place dans la ligne : ce qui l'occupe, et le rang que le `flex` doit lui donner.
 *
 * Le rang est une **valeur CSS `order`**, et non une position dans le DOM. C'est ce qui
 * permet de réordonner la ligne sans jamais déplacer un nœud : les pastilles de quota et la
 * jauge de contexte ne sont jamais reconstruites ni détachées, donc la transition de 700 ms
 * de la jauge ne peut pas repartir parce qu'on a bougé le `cwd`. Un `append` sur un enfant
 * déjà présent le retire et le réinsère — pour le CSS, c'est un nouvel élément.
 *
 * Les rangs sont pairs pour les éléments et impairs pour les traits, ce qui laisse toujours
 * une place à un `│` entre deux voisins sans avoir à renuméroter.
 */
export interface StatusSlot {
    readonly item: StatusBarItemId;
    readonly order: number;
}

/** Un `│` posé entre deux places. */
export interface StatusRuleSlot {
    readonly order: number;
}

/** Ce que la ligne peint au repos : les places occupées, et les traits entre elles. */
export interface StatusPlacement {
    readonly slots: readonly StatusSlot[];
    readonly rules: readonly StatusRuleSlot[];
    /** Le rang du rappel de sidebar repliée — toujours après tout le reste. */
    readonly hintOrder: number;
}

/**
 * Où chaque élément de la barre se pose, et où tombent les `│`.
 *
 * `hasContent` dit lesquels ont réellement quelque chose à montrer : un segment peut être
 * dans la barre et n'avoir aucune valeur — un onglet sans quota, une jauge que le backend ne
 * sait pas mesurer. Un trait posé à côté d'un segment vide ferait un `│` orphelin, et c'est
 * la seule raison pour laquelle cette fonction a besoin de le savoir.
 *
 * Le **rappel** de sidebar repliée n'est pas un élément de la barre : il n'est pas dans le
 * menu de la maquette, et il ne dit rien de l'onglet — il dit qu'un agent attend derrière une
 * colonne repliée, ce qu'aucun réglage ne doit pouvoir cacher. Il se pose donc après tout,
 * quelle que soit la disposition.
 */
export function placeStatusBar(
    layout: StatusBarLayout,
    hasContent: (item: StatusBarItemId) => boolean,
): StatusPlacement {
    const slots: StatusSlot[] = [];
    const rules: StatusRuleSlot[] = [];

    let previous: StatusBarItemId | null = null;
    for (const item of layout) {
        if (!hasContent(item)) continue;

        const order = slots.length * 2;
        if (previous !== null && RULED.includes(previous) && RULED.includes(item)) {
            rules.push({ order: order - 1 });
        }
        slots.push({ item, order });
        previous = item;
    }

    return { slots, rules, hintOrder: slots.length * 2 };
}
