import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { findAll, plainText } from "@/shared/ui";

import {
    DEFAULT_STATUS_BAR_LAYOUT,
    DEFAULT_STATUS_BAR_SEGMENTS,
    appendSpacer,
    composeDrawer,
    composeVisibilityMenu,
    drawerSegments,
    dropIndex,
    editorPills,
    moveItem,
    parseStatusBarLayout,
    placeStatusBar,
    removeAt,
    shownSegments,
    type StatusBarItemId,
    type StatusBarLayout,
    type StatusBarSegmentId,
    type VisibilityRow,
} from "./status-bar";

/**
 * La barre de statut : ce qu'elle montre (vue 5c) et **dans quel ordre** (vue 5e).
 *
 * Trois règles y valent d'être tenues. La première est que rien n'est détenu ici : un clic
 * part en bascule vers le backend, et la coche suit ce qui revient — un menu qui appliquerait
 * son propre clic serait le second détenteur d'une préférence (ADR-0009), et divergerait de
 * la barre au premier échec d'écriture. La deuxième est qu'un élément retiré **reste
 * atteignable** : dans la liste du menu, et dans le tiroir du mode édition. La troisième est
 * qu'une barre vidée reste récupérable.
 *
 * Ce qui n'est pas ici, et qui n'est nulle part : le geste lui-même. Le maintien de 430 ms, le
 * glissement, le trait qui file sur le bord haut de la barre touchent au DOM, et `bun test`
 * n'en monte pas. Ce qui est testable est ce que ces gestes **décident** — et c'est justement
 * ce que `status-bar.ts` porte en fonctions pures.
 */

/** La feuille de la feature, lue comme un texte : c'est tout ce que `bun test` peut en faire. */
const SHEET = readFileSync(new URL("./terminal.css", import.meta.url), "utf8");

function line(id: StatusBarSegmentId, shown: boolean, preview = ""): VisibilityRow {
    return { id, name: id, preview, shown, separated: false };
}

/** Tout occupe une place — le cas de la ligne quand chaque segment a une valeur. */
const everythingShows = (_item: StatusBarItemId): boolean => true;

describe("ce que le backend annonce", () => {
    it("Given a backend that answers nothing understandable, when the bar is read, then it keeps the default layout", () => {
        // Given — une réponse qui n'aboutit pas, un fichier de préférence absent ou illisible :
        // trois façons de ne rien dire, et une seule conduite acceptable — la barre d'avant
        const nonsense = [null, undefined, "dark", 3, { cwd: true }];

        // When
        const read = nonsense.map((value) => parseStatusBarLayout(value));

        // Then — surtout pas une suite vide : une ligne de statut effacée serait la façon la
        // plus spectaculaire de rater un fichier manquant
        expect(read).toEqual(nonsense.map(() => DEFAULT_STATUS_BAR_LAYOUT));
    });

    it("Given a bar the user emptied, when it is read, then it stays empty instead of coming back", () => {
        // Given — tout jeter est un choix, et le tiroir du mode édition est là pour en
        // revenir. Le confondre avec une réponse manquante rendrait la barre au prochain
        // démarrage sans que personne l'ait demandé
        const emptied: readonly unknown[] = [];

        // When / Then
        expect(parseStatusBarLayout(emptied)).toEqual([]);
    });

    it("Given a bar naming things this window does not know, when it is read, then only what it understands survives", () => {
        // Given — un backend plus récent que la webview, ou un `theme.json` édité à la main :
        // un mot inconnu, un segment répété, et deux élastiques qui ont le droit de coexister
        const suspicious = ["cwd", "tides", "spacer", "cwd", 7, "spacer", "agent"];

        // When
        const read = parseStatusBarLayout(suspicious);

        // Then — un segment a une identité, un spacer n'en a pas
        expect(read).toEqual(["cwd", "spacer", "spacer", "agent"]);
    });

    it("Given a bar, when its seven switches are derived, then a segment is shown exactly when it belongs to it", () => {
        // Given — la visibilité n'est plus un booléen reçu : c'est une appartenance. C'est ce
        // qui empêche « où est le cwd » et « le cwd est-il montré » de se contredire
        const bar: StatusBarLayout = ["agent", "spacer", "weekly"];

        // When
        const shown = shownSegments(bar);

        // Then
        expect(shown).toEqual({
            session: false,
            weekly: true,
            context: false,
            model: false,
            agent: true,
            branch: false,
            cwd: false,
        });
        expect(shownSegments(DEFAULT_STATUS_BAR_LAYOUT)).toEqual(DEFAULT_STATUS_BAR_SEGMENTS);
    });
});

describe("ce que le mode édition décide", () => {
    it("Given a bar being dragged, when a pill passes the middle of its neighbour, then it takes its place", () => {
        // Given — les milieux des quatre pastilles de la barre, telles qu'elles sont peintes
        const centers = [40, 120, 200, 280];

        // When — le pointeur balaie la ligne, d'avant la première à après la dernière
        const places = [0, 39, 41, 119, 121, 279, 281].map((x) => dropIndex(centers, x));

        // Then — on prend la place de la pastille dont on a dépassé le milieu, et jamais son
        // bord : deux voisines s'échangeraient sans fin dès que le pointeur les effleure
        expect(places).toEqual([0, 0, 1, 1, 2, 3, 4]);
    });

    it("Given an empty bar, when a pill is dropped anywhere on it, then it lands on its only place", () => {
        // Given — une barre vidée de tout reste une surface de dépôt : c'est ce qui permet
        // d'y ramener un élément du tiroir sans passer par le retour aux défauts
        const empty: number[] = [];

        // When / Then
        expect(dropIndex(empty, 320)).toBe(0);
    });

    it("Given the session pill dragged before the cwd, when the bar is recomposed, then it opens on the session", () => {
        // Given — le scénario de la tâche : « l'utilisateur glisse la pastille session avant
        // cwd », et la barre montre l'ordre nouveau **pendant** le glissement
        const bar = DEFAULT_STATUS_BAR_LAYOUT;

        // When
        const dragged = moveItem(bar, bar.indexOf("session"), 0);

        // Then
        expect(dragged).toEqual([
            "session",
            "cwd",
            "branch",
            "agent",
            "spacer",
            "context",
            "model",
        ]);
    });

    it("Given a pointer that has left the bar, when the drag is applied, then nothing moves", () => {
        // Given — un glissement qui sort de la ligne rend un indice hors bornes, et rien ne
        // doit se réordonner sur un geste qui a quitté la surface
        const bar = DEFAULT_STATUS_BAR_LAYOUT;

        // When
        const attempts = [moveItem(bar, 0, -1), moveItem(bar, 0, bar.length), moveItem(bar, 9, 0)];

        // Then
        expect(attempts).toEqual([bar, bar, bar]);
    });

    it("Given several spacers, when one of them is thrown away, then the others stay where they are", () => {
        // Given — le critère : plusieurs spacers coexistent, chacun déplaçable et supprimable
        // **indépendamment**. Deux élastiques ne se distinguent que par leur place
        const grouped: StatusBarLayout = ["cwd", "spacer", "agent", "spacer", "session"];

        // When
        const trimmed = removeAt(grouped, 3);

        // Then
        expect(trimmed).toEqual(["cwd", "spacer", "agent", "session"]);
        expect(appendSpacer(trimmed)).toEqual(["cwd", "spacer", "agent", "session", "spacer"]);
    });

    it("Given a removed segment, when the drawer is composed, then it is there and only there", () => {
        // Given — le tiroir est le **complément** de la barre : un élément est dans l'une ou
        // dans l'autre, jamais dans les deux ni dans aucune. C'est ce qui fait qu'un `×` et
        // un clic sur une pastille du tiroir sont deux moitiés du même geste
        const bar = DEFAULT_STATUS_BAR_LAYOUT;

        // When
        const drawer = drawerSegments(bar);

        // Then — le weekly, retiré par défaut, et lui seul
        expect(drawer).toEqual(["weekly"]);
        expect(drawerSegments([])).toEqual([
            "session",
            "weekly",
            "context",
            "model",
            "agent",
            "branch",
            "cwd",
        ]);
    });

    it("Given the bar in edit mode, when its pills are composed, then each carries its name and its own beat", () => {
        // Given / When — une pastille montre le **nom** de l'élément, jamais sa valeur : on
        // arrange des éléments, pas des chiffres, et un segment sans valeur se glisse comme
        // les autres
        const pills = editorPills(["cwd", "agent", "spacer", "context"]);

        // Then — et le frémissement est décalé de trois en trois, pour que les voisines ne
        // battent pas ensemble
        expect(pills.map((pill) => pill.label)).toEqual([
            "cwd",
            "agent state",
            "⟷ spacer",
            "context bar",
        ]);
        expect(pills.map((pill) => pill.beat)).toEqual([0, 1, 2, 0]);
    });
});

describe("où chaque élément se pose dans la ligne", () => {
    /** Les rangs, lus dans l'ordre où la ligne les attribue. */
    function orders(
        layout: StatusBarLayout,
        shows: (item: StatusBarItemId) => boolean = everythingShows,
    ): [StatusBarItemId, number][] {
        return placeStatusBar(layout, shows).slots.map((slot) => [slot.item, slot.order]);
    }

    it("Given the default bar, when it is placed, then it reads exactly as it did before it could be arranged", () => {
        // Given / When — la disposition de la vue 5e, et les rangs qu'elle donne
        const placement = placeStatusBar(DEFAULT_STATUS_BAR_LAYOUT, everythingShows);

        // Then — deux traits, entre les trois segments de gauche, et rien autour des
        // pastilles : c'est la ligne d'avant #165, au pixel
        expect(placement.slots.map((slot) => slot.item)).toEqual([...DEFAULT_STATUS_BAR_LAYOUT]);
        expect(placement.slots.map((slot) => slot.order)).toEqual([0, 2, 4, 6, 8, 10, 12]);
        expect(placement.rules.map((line) => line.order)).toEqual([1, 3]);
        expect(placement.hintOrder).toBe(14);
    });

    it("Given a removed cwd, when the bar is placed, then it opens on the branch instead of on a separator", () => {
        // Given — le trait tombe **entre** deux voisins montrés ; un `cwd` retiré qui
        // laisserait le sien ferait s'ouvrir la ligne sur un `│` orphelin
        const withoutCwd: StatusBarLayout = ["branch", "agent", "spacer", "session"];

        // When
        const placement = placeStatusBar(withoutCwd, everythingShows);

        // Then — un seul trait, entre la branche et l'état de l'agent
        expect(placement.slots[0]?.item).toBe("branch");
        expect(placement.rules.map((line) => line.order)).toEqual([1]);
    });

    it("Given a spacer between two text segments, when the bar is placed, then no rule is drawn across it", () => {
        // Given — un élastique est une respiration : un `│` posé de part et d'autre en ferait
        // une frontière, alors que c'est lui la frontière
        const split: StatusBarLayout = ["cwd", "spacer", "branch"];

        // When / Then
        expect(placeStatusBar(split, everythingShows).rules).toEqual([]);
    });

    it("Given a quota the backend cannot give, when the bar is placed, then its neighbours close ranks and the rule comes back", () => {
        // Given — la barre porte `cwd · session · branch`, mais le quota de session n'a
        // aucune valeur : il n'occupe donc aucune place, et les deux mots deviennent voisins
        const bar: StatusBarLayout = ["cwd", "session", "branch"];

        // When
        const placement = placeStatusBar(bar, (item) => item !== "session");

        // Then — un trait entre eux, et des rangs qui se suivent : un rang laissé libre
        // ferait un trou de 14 px au milieu de la ligne
        expect(orders(bar, (item) => item !== "session")).toEqual([
            ["cwd", 0],
            ["branch", 2],
        ]);
        expect(placement.rules.map((line) => line.order)).toEqual([1]);
    });

    it("Given a bar emptied of everything, when it is placed, then nothing is drawn rather than a row of separators", () => {
        // Given — la légende de la vue 5c est formelle : chaque élément de la barre se coupe
        const placement = placeStatusBar([], everythingShows);

        // When / Then — et le rappel de sidebar repliée se pose quand même, parce qu'aucun
        // réglage ne doit pouvoir cacher qu'un agent attend derrière une colonne repliée
        expect(placement.slots).toEqual([]);
        expect(placement.rules).toEqual([]);
        expect(placement.hintOrder).toBe(0);
    });
});

describe("le panneau du menu contextuel", () => {
    it("Given a hidden segment, when the menu is composed, then it is still listed, greyed and without its tick", () => {
        // Given — le weekly, retiré par défaut
        const rows = [line("session", true), line("weekly", false)];

        // When
        const menu = composeVisibilityMenu(rows, () => undefined);

        // Then — il reste une ligne du menu, et elle dit qu'elle est décochée à l'œil comme
        // au lecteur d'écran
        const lines = findAll(menu, "status-menu-line");
        expect(findAll(menu, "status-menu-name").map(plainText)).toEqual([
            "session",
            "weekly",
            "réorganiser la barre…",
        ]);
        expect(lines[1]?.classes).toContain("is-hidden");
        expect(lines[1]?.attrs["aria-checked"]).toBe("false");
        expect(findAll(menu, "status-menu-check").map(plainText)).toEqual(["✓", "", "⟷"]);
    });

    it("Given a menu line, when it is clicked, then what leaves is the segment and nothing else", () => {
        // Given — le clic ne dit pas ce que le segment devient : c'est le backend qui décide,
        // et c'est ce qui empêche le menu d'en devenir le second détenteur (ADR-0009)
        const toggled: StatusBarSegmentId[] = [];
        const menu = composeVisibilityMenu([line("cwd", true)], (id) => {
            toggled.push(id);
        });

        // When
        findAll(menu, "status-menu-line")[0]?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(toggled).toEqual(["cwd"]);
    });

    it("Given the last menu line, when it is composed, then it acts instead of toggling, behind a rule of its own", () => {
        // Given — le critère de la tâche : `réorganiser la barre…` est séparée des sept
        // interrupteurs par un trait, et cocher ou décocher n'a aucun sens sur elle. Un
        // lecteur d'écran qui l'annoncerait « non cochée » raconterait un état inexistant
        let opened = 0;
        const menu = composeVisibilityMenu(
            [{ ...line("agent", true), separated: true }],
            () => {
                throw new Error("la ligne d'action ne bascule rien");
            },
            () => {
                opened += 1;
            },
        );

        // When
        const action = findAll(menu, "status-menu-action")[0];
        action?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then — deux traits : celui du groupe `agent`, et celui qui isole l'action
        expect(findAll(menu, "status-menu-rule").length).toBe(2);
        expect(action?.attrs["role"]).toBe("menuitem");
        expect(action?.attrs["aria-checked"]).toBeUndefined();
        expect(plainText(action ?? { kind: "text", text: "" })).toContain("clic long");
        expect(opened).toBe(1);
    });

    it("Given a row that opens a group, when the menu is composed, then a rule is drawn above it", () => {
        // Given — le trait de la maquette sépare ce que la conversation consomme de ce qui dit
        // où l'on est ; posé partout, il ferait une liste illisible
        const rows: VisibilityRow[] = [
            line("context", true),
            { ...line("agent", true), separated: true },
            line("cwd", true),
        ];

        // When
        const menu = composeVisibilityMenu(rows, () => undefined);

        // Then — le trait précède `agent`, et n'est pas ailleurs dans les sept lignes
        const children = menu.build().children;
        const ruleAt = children.findIndex(
            (node) => node.kind === "element" && node.classes.includes("status-menu-rule"),
        );
        const agentAt = children.findIndex(
            (node) => node.kind === "element" && plainText(node).includes("agent"),
        );
        expect(ruleAt).toBe(agentAt - 1);
    });
});

describe("le tiroir du mode édition", () => {
    it("Given a bar missing two segments, when the drawer is composed, then clicking one asks for it back", () => {
        // Given — le scénario de la tâche : « weekly » retiré, et un clic sur sa pastille du
        // tiroir le ramène. Ce qui part est une **bascule**, comme depuis le menu : c'est le
        // backend qui sait où il revient (ADR-0009)
        const picked: StatusBarSegmentId[] = [];
        const drawer = composeDrawer(["cwd", "spacer", "session"], {
            onPick: (id) => {
                picked.push(id);
            },
            onSpacer: () => undefined,
            onReset: () => undefined,
        });

        // When
        const pills = findAll(drawer, "status-drawer-pill");
        pills[0]?.on["click"]?.({ value: "", key: "", shiftKey: false });

        // Then
        expect(pills.map(plainText)).toEqual([
            "weekly",
            "context bar",
            "model",
            "agent state",
            "branch",
        ]);
        expect(picked).toEqual(["weekly"]);
    });

    it("Given a bar emptied of everything, when the drawer is composed, then it still offers a way back", () => {
        // Given — le critère : une barre vidée de tout reste récupérable. Le tiroir est le
        // seul endroit qui existe encore, et il porte le retour aux défauts comme
        // `features/shortcuts` porte son `reset all` (spec §4.4)
        let reset = 0;
        let spacers = 0;
        const drawer = composeDrawer([], {
            onPick: () => undefined,
            onSpacer: () => {
                spacers += 1;
            },
            onReset: () => {
                reset += 1;
            },
        });

        // When
        findAll(drawer, "status-drawer-reset")[0]?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });
        findAll(drawer, "status-drawer-spacer")[0]?.on["click"]?.({
            value: "",
            key: "",
            shiftKey: false,
        });

        // Then
        expect(reset).toBe(1);
        expect(spacers).toBe(1);
        expect(findAll(drawer, "status-drawer-pill").length).toBe(7);
    });
});

describe("ce que la feuille de style peint", () => {
    it("Given the menu, the pills and the drawer, when their colours are read, then each names a token instead of a hexadecimal", () => {
        // Given — la même exigence que pour les segments d'usage : un hexadécimal écrit ici
        // sortirait des trois thèmes sans qu'aucun test de modèle ne bronche
        const rules = [
            ...SHEET.matchAll(
                /^\.[^\n{]*status-(?:bar-)?(?:menu|pill|drawer|editor|hold)[^\n{]*\{([^}]*)\}/gm,
            ),
        ];

        // When
        const painted = rules.map((rule) => rule[1] ?? "").join("\n");

        // Then
        expect(rules.length).toBeGreaterThan(0);
        expect(painted).not.toMatch(/#[0-9a-f]{3,8}\b/i);
    });
});
