import { describe, expect, it } from "bun:test";

import {
    HOOK_STATES,
    hookShapes,
    presentHooks,
    presentVerification,
    testTileClass,
    testTileLabel,
    VERIFICATION_STATES,
} from "./verification-state";

describe("la présentation des cinq états de vérification", () => {
    it("Given the five verification states, when they are presented, then each one has a shape and a word of its own", () => {
        // Given — la discipline de `shared/agent-state` : la forme porte l'état à elle
        // seule, et la couleur ne fait que la doubler. Deux états qui partagent une classe
        // partagent une couleur, donc se confondent dès qu'on ne lit plus la phrase
        const shown = VERIFICATION_STATES.map((state) => presentVerification(state));

        // When
        const words = new Set(shown.map((one) => one.label));
        const classes = new Set(shown.map((one) => one.className));

        // Then
        expect(VERIFICATION_STATES.length).toBe(5);
        expect(words.size).toBe(5);
        expect(classes.size).toBe(5);
    });

    it("Given the five states, when the one that is happening now is looked for, then only verifying moves", () => {
        // Given — le mouvement est ce qui distingue `verifying` d'`unverified` sans lire un
        // mot. Un second état animé lui prendrait cette distinction
        // When
        const moving = VERIFICATION_STATES.filter((state) => presentVerification(state).spinning);

        // Then
        expect(moving).toEqual(["verifying"]);
    });

    it("Given the two states that leave the folder undecided, when a card is drawn, then its border stays neutral", () => {
        // Given — la bordure teintée est la seconde façon de dire l'état, sans texte. La
        // teindre pour `unverified` dirait qu'il s'est passé quelque chose
        // When
        const neutral = VERIFICATION_STATES.filter(
            (state) => presentVerification(state).cardClassName === "",
        );

        // Then
        expect(neutral).toEqual(["unverified", "verifying"]);
    });
});

describe("la présentation des cinq états de la ligne hooks", () => {
    it("Given the five hook states, when they are presented, then each one has a word of its own", () => {
        // Given — mêmes exigences que les états de vérification : un état qui partage son
        // mot avec un autre est un état qu'un lecteur d'écran ne distingue plus
        const shown = HOOK_STATES.map((state) => presentHooks(state));

        // When
        const words = new Set(shown.map((one) => one.label));

        // Then
        expect(HOOK_STATES.length).toBe(5);
        expect(words.size).toBe(5);
    });

    it("Given missing and blocked, the two grey states, when their shapes are compared, then they share no stroke at all", () => {
        // Given — c'est le point délicat de cet écran. La maquette les dessine en cercle
        // vide contre cercle barré : à 13 px, une diagonale d'un pixel est tout ce qui les
        // sépare, et le dépôt exige qu'un état soit distinguable **sans la couleur** — les
        // deux sont gris. `blocked` est donc un cadenas : aucune forme partagée avec un
        // cercle, et « pas possible » ne se lit plus comme « pas encore fait »
        const missing = hookShapes("missing");
        const blocked = hookShapes("blocked");

        // When
        const shared = missing.filter((shape) => blocked.includes(shape));

        // Then
        expect(shared).toEqual([]);
    });

    it("Given the five hook states, when their shapes are compared, then no two of them draw the same thing", () => {
        // Given — la forme porte l'état à elle seule. Deux états au même tracé ne se
        // distinguent plus qu'à la couleur, ce que le dépôt refuse depuis `agent-state`
        // When
        const drawn = new Set(HOOK_STATES.map((state) => hookShapes(state).join("|")));

        // Then
        expect(drawn.size).toBe(5);
    });

    it("Given a blocked hooks line whose refusal already names its file, when it is presented, then the file is not written twice on the same row", () => {
        // Given — les trois blocages qui portent un fichier sont des refus qui le nomment
        // dans leur phrase (« ash can't read /home/… ») ; les trois autres n'ont pas de
        // fichier du tout. Les quatre autres états, eux, ont une phrase courte et le fichier
        // en pastille à côté. La règle vivait dans la vue, que `bun test` ne monte pas
        // When
        const shown = HOOK_STATES.filter((state) => presentHooks(state).showsFile);

        // Then
        expect(shown).toEqual(["installed", "missing", "outdated", "conflict"]);
    });
});

describe("les pastilles de la rangée de tests", () => {
    it("Given a test that will never run because the chain stopped, when its tile is drawn, then it is not the same as one still waiting", () => {
        // Given — les deux disent « pas lancé », et ne se peignent pas pareil : le premier
        // attend, le second ne viendra jamais. Les confondre ferait attendre une réponse
        // When
        const waiting = testTileClass("pending");
        const abandoned = testTileClass("skipped");

        // Then
        expect(waiting).not.toBe(abandoned);
    });

    it("Given a tile, when a screen reader reads it, then it hears the test and its result rather than a number", () => {
        // Given — un `3` seul ne dit rien : ni de quel test il s'agit, ni ce qu'il a donné
        const test = { number: 3, label: "the command exists in PATH" };

        // When
        const said = testTileLabel("warned", test);

        // Then
        expect(said).toBe("test 3, the command exists in PATH: passed with a caveat");
    });
});
