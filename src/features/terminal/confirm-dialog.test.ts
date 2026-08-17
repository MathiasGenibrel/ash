import { readFileSync } from "node:fs";

import { describe, expect, it } from "bun:test";

import { find, FOCUS_KEY, plainText, type UiElementNode } from "@/shared/ui";

import { CANCEL_FOCUS_KEY, composeCloseBox } from "./confirm-dialog";

/**
 * La confirmation de fermeture, lue comme une description.
 *
 * Le bug qu'elle protège : la boîte s'affichait, `Échap` en sortait, et ses **deux boutons
 * ne répondaient pas à la souris** — un canevas de xterm.js se peignait par-dessus et
 * avalait le clic. C'est le seul geste d'Ash qui détruit un processus : une confirmation qui
 * ne se répond qu'au clavier est un piège, et la spec §4.4 veut tout atteignable à la souris.
 *
 * Ce qui se vérifie ici est ce qui décide : quel bouton répond quoi, et lequel prend le focus.
 * La couche qui empile — le voile, le `z-index`, le canevas — est du CSS, et les deux derniers
 * tests du fichier sont ce qui reste vérifiable d'elle sans navigateur.
 */

/** La feuille de la feature, lue comme un texte : c'est tout ce que `bun test` peut en faire. */
const SHEET = readFileSync(new URL("./terminal.css", import.meta.url), "utf8");

/** Le corps d'une règle dont le sélecteur est exactement celui-là. */
function ruleBody(selector: string): string {
    const rule = new RegExp(`^\\${selector}\\s*\\{([^}]*)\\}`, "m").exec(SHEET);
    if (rule === null) throw new Error(`aucune règle « ${selector} » dans terminal.css`);
    return rule[1] ?? "";
}

/** Test Data Builder : la boîte, et ce qu'elle a répondu jusqu'ici. */
class Dialog {
    private readonly answers: boolean[] = [];
    readonly box: UiElementNode;

    private constructor(what: string) {
        this.box = composeCloseBox(what, (closeIt) => {
            this.answers.push(closeIt);
        }).build();
    }

    /** Un onglet où un agent tourne — le seul cas où `⌘W` demande quelque chose. */
    static overAWorkingAgent(): Dialog {
        return new Dialog("/wt/ash-sidebar");
    }

    /**
     * Clique le bouton qui porte cette classe.
     *
     * Elle lève plutôt que de ne rien faire : `find(…)?.on["click"]?.(…)` passerait en
     * silence sur un bouton absent ou muet, c'est-à-dire exactement sur la panne.
     */
    click(className: string): this {
        const node = find(this.box, className);
        if (node === null) throw new Error(`aucun bouton « ${className} » dans la boîte`);
        const onClick = node.on["click"];
        if (onClick === undefined) throw new Error(`« ${className} » ne répond pas au clic`);
        onClick({ value: "", key: "", shiftKey: false });
        return this;
    }

    /** Ce que la boîte a répondu — `[]` tant qu'elle n'a rien répondu. */
    get answered(): readonly boolean[] {
        return this.answers;
    }
}

describe("la confirmation de fermeture d'un onglet", () => {
    it("Given a tab where an agent is running, when Cancel is clicked with the mouse, then the box answers no and nothing is closed", () => {
        // Given
        const dialog = Dialog.overAWorkingAgent();

        // When
        dialog.click("ash-confirm-cancel");

        // Then — `false` est ce que l'atelier lit pour ne pas appeler `closeTab`
        expect(dialog.answered).toEqual([false]);
    });

    it("Given a tab where an agent is running, when the danger button is clicked with the mouse, then the box answers yes", () => {
        // Given
        const dialog = Dialog.overAWorkingAgent();

        // When
        dialog.click("is-danger");

        // Then
        expect(dialog.answered).toEqual([true]);
    });

    it("Given a box just opened, when the painter looks for what to focus, then it is the button that destroys nothing", () => {
        // Given — la touche entrée sur un dialogue qui vient d'apparaître ne doit pas tuer un
        // processus : le marqueur est dans la description, donc l'ordre des boutons peut
        // changer sans que le défaut change avec lui
        const dialog = Dialog.overAWorkingAgent();

        // When
        const focused = find(dialog.box, "ash-confirm-cancel");

        // Then
        expect(focused?.attrs[FOCUS_KEY]).toBe(CANCEL_FOCUS_KEY);
        expect(plainText(focused ?? dialog.box)).toBe("Annuler");
        expect(find(dialog.box, "is-danger")?.attrs[FOCUS_KEY]).toBeUndefined();
    });

    it("Given the question, when the box is composed, then it names what is still running", () => {
        // Given / When — la boîte est la seule chose qui dise **quel** onglet va disparaître ;
        // sans le `cwd`, `⌘W` sur la mauvaise fenêtre se répond à l'aveugle
        const dialog = Dialog.overAWorkingAgent();

        // Then
        expect(plainText(dialog.box)).toContain("/wt/ash-sidebar");
    });

    it("Given the stylesheet, when the rules of the dialog are read, then none of them writes a colour of its own", () => {
        // Given — c'était la dernière surface de l'application à peindre en dur (`#e5484d`,
        // `#fff`), donc la seule à ne pas suivre le thème. Les couleurs viennent maintenant
        // de la table de `app/styles.css`, et ce test est ce qui empêche d'en réécrire une à
        // la main. L'ombre est hors du compte : un noir translucide est un repère de
        // profondeur, pas une couleur de la palette — `.terminal-search-box` a le même.
        const rules = [...SHEET.matchAll(/^\.ash-confirm[^{]*\{([^}]*)\}/gm)].map(
            (match) => match[1] ?? "",
        );

        // When
        const literals = rules.flatMap((body) =>
            body
                .split("\n")
                .filter((line) => !line.includes("box-shadow"))
                .flatMap((line) => [...line.matchAll(/#[0-9a-f]{3,8}\b|\brgba?\(/gi)])
                .map((match) => match[0]),
        );

        // Then
        expect(rules.length).toBeGreaterThan(0);
        expect(literals).toEqual([]);
    });

    it("Given the stylesheet, when the terminal stack is read, then it still contains the layers xterm.js paints inside it", () => {
        // Given — c'est **la** ligne qui rend la boîte cliquable, et la seule chose du
        // correctif qu'aucun comportement ne peut attraper : `position: relative` seul ne
        // crée pas de contexte d'empilement, donc sans `isolation: isolate` le canevas de
        // liens de xterm.js (`z-index: 2`, sans `pointer-events: none`) remonte au contexte
        // racine et se repose par-dessus la confirmation. Elle resterait parfaitement
        // visible, ses deux boutons redeviendraient muets, et `bun test` resterait vert.
        // Une assertion de texte est tout ce qu'on peut en dire sans navigateur — la même
        // mécanique que le test des couleurs ci-dessus.
        const stack = ruleBody(".terminal-stack");

        // When
        const isolated = /isolation:\s*isolate/.test(stack);

        // Then
        expect(isolated).toBe(true);
    });
});
