import { describe, expect, it } from "bun:test";

import { CANCEL_FOCUS_KEY } from "@/features/terminal";
import { TabBuilder } from "@/shared/ipc/builders";
import { find, findAll, FOCUS_KEY, plainText, type UiElementNode } from "@/shared/ui";

import { composeQuitBox } from "./confirm-quit";

/**
 * La confirmation de sortie, lue comme une description.
 *
 * Ce qu'elle protège : `⌘Q` est voisin de `⌘W` sur QWERTY et occupe la position du `A` sur
 * AZERTY, et il coupait un agent en plein travail sans rien demander (issue #177). Ce que
 * la boîte doit dire, c'est **ce qu'on va perdre** — combien d'agents, et lesquels : une
 * question qui dirait seulement « quitter ? » se répondrait sans être lue.
 *
 * Le critère « faut-il demander ? », lui, n'est pas ici : il est en Rust, dans
 * `features/quit/question.rs`, parce que c'est le backend qui détient les onglets
 * (ADR-0009). Cette boîte-ci ne compte que ce qu'on lui donne.
 *
 * La couche qui empile — le voile, `Échap`, le focus initial — est celle de `⌘W`, et elle a
 * ses propres tests dans `features/terminal/confirm-dialog.test.ts`.
 */

/** Test Data Builder : la boîte de sortie, et ce qu'elle a répondu jusqu'ici. */
class QuitDialog {
    private readonly answers: boolean[] = [];
    readonly box: UiElementNode;

    private constructor(appName: string, running: readonly ReturnType<TabBuilder["build"]>[]) {
        this.box = composeQuitBox(appName, running, (quit) => {
            this.answers.push(quit);
        }).build();
    }

    /** Un seul agent, au travail — le cas le plus courant d'un `⌘Q` de travers. */
    static overOneWorkingAgent(): QuitDialog {
        return new QuitDialog("Ash", [
            TabBuilder.create()
                .runningAgent("claude")
                .inState("working")
                .workingIn("/wt/ash-177")
                .build(),
        ]);
    }

    static over(appName: string, running: readonly ReturnType<TabBuilder["build"]>[]): QuitDialog {
        return new QuitDialog(appName, running);
    }

    /** Clique le bouton qui porte cette classe, et lève s'il est absent ou muet. */
    click(className: string): this {
        const node = find(this.box, className);
        if (node === null) throw new Error(`aucun bouton « ${className} » dans la boîte`);
        const onClick = node.on["click"];
        if (onClick === undefined) throw new Error(`« ${className} » ne répond pas au clic`);
        onClick({ value: "", key: "", shiftKey: false });
        return this;
    }

    /** Les lignes d'agent, dans leur ordre. */
    get lines(): string[] {
        return findAll(this.box, "ash-confirm-item").map((node) => plainText(node));
    }

    get answered(): readonly boolean[] {
        return this.answers;
    }
}

describe("la confirmation de sortie quand un agent est reconnu", () => {
    it("Given a single recognized agent at work, when the box is composed, then it counts one agent and names the application", () => {
        // Given / When
        const dialog = QuitDialog.overOneWorkingAgent();

        // Then
        expect(plainText(dialog.box)).toContain("1 agent tourne. Quitter Ash ?");
    });

    it("Given two recognized agents, when the box is composed, then it announces two and lists each with its path and its state", () => {
        // Given — c'est la liste qui rend la question répondable : sans elle, « 2 agents »
        // ne dit pas lesquels, donc ne dit pas ce qu'on perd
        const dialog = QuitDialog.over("Ash", [
            TabBuilder.create()
                .runningAgent("claude")
                .inState("working")
                .workingIn("/wt/ash-177")
                .build(),
            TabBuilder.create()
                .runningAgent("claude")
                .inState("waiting")
                .workingIn("/dev/ash")
                .build(),
        ]);

        // When
        const lines = dialog.lines;

        // Then
        expect(plainText(dialog.box)).toContain("2 agents tournent.");
        expect(lines).toEqual(["/wt/ash-177 — working", "/dev/ash — waiting"]);
    });

    it("Given an agent that is merely idle at its prompt, when the box is composed, then it is listed all the same", () => {
        // Given — le critère est la **présence** d'un agent reconnu, pas son état (ADR-0006) :
        // un Claude Code `idle` compte, parce que le quitter perd sa session
        const dialog = QuitDialog.over("Ash", [
            TabBuilder.create()
                .runningAgent("claude")
                .inState("idle")
                .workingIn("/dev/ash")
                .build(),
        ]);

        // When
        const lines = dialog.lines;

        // Then
        expect(lines).toEqual(["/dev/ash — idle"]);
    });

    it("Given a development build, when the box is composed, then it names Ash-dev and not the installed application", () => {
        // Given — le nom affiché a une seule source, `APP_NAME` en Rust : une boîte qui
        // dirait « Quitter Ash » depuis Ash-dev ferait douter du binaire qu'on regarde
        const dialog = QuitDialog.over("Ash-dev", [
            TabBuilder.create().runningAgent("claude").build(),
        ]);

        // Then
        expect(plainText(dialog.box)).toContain("Quitter Ash-dev ?");
    });

    it("Given the box just opened, when the painter looks for what to focus, then it is Cancel and not the button that quits", () => {
        // Given — `⏎` sur une modale qui vient d'apparaître ne doit pas couper un agent
        const dialog = QuitDialog.overOneWorkingAgent();

        // When
        const focused = find(dialog.box, "ash-confirm-cancel");

        // Then
        expect(focused?.attrs[FOCUS_KEY]).toBe(CANCEL_FOCUS_KEY);
        expect(plainText(find(dialog.box, "is-danger") ?? dialog.box)).toBe("Quitter");
        expect(find(dialog.box, "is-danger")?.attrs[FOCUS_KEY]).toBeUndefined();
    });

    it("Given the box, when the two buttons are clicked with the mouse, then only the danger one answers yes", () => {
        // Given
        const dialog = QuitDialog.overOneWorkingAgent();

        // When
        dialog.click("ash-confirm-cancel").click("is-danger");

        // Then — `false` est ce que l'application lit pour n'appeler `quit_now` sur rien
        expect(dialog.answered).toEqual([false, true]);
    });
});
