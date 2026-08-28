import { describe, expect, it } from "bun:test";

import type { KeyStroke } from "@/features/settings";

import type { MenuAction } from "./menu";
import { installShortcuts, withheldFromTheMenu, type KeyPress } from "./shortcuts";

/**
 * Test Data Builder : une frappe part sans aucun modificateur, et n'en porte que ceux
 * qu'on lui ajoute. C'est la seule façon de lire d'un coup d'œil ce qui distingue `⇥` de
 * `⌃⇥` — la différence tient à un booléen, et c'est tout le sujet.
 */
class KeyPressBuilder {
    private press: KeyPress = {
        key: "Tab",
        code: "Tab",
        ctrlKey: false,
        shiftKey: false,
        metaKey: false,
        altKey: false,
    };

    static press(key: string, code = key): KeyPressBuilder {
        const builder = new KeyPressBuilder();
        builder.press = { ...builder.press, key, code };
        return builder;
    }

    withCtrl(): this {
        this.press = { ...this.press, ctrlKey: true };
        return this;
    }

    withShift(): this {
        this.press = { ...this.press, shiftKey: true };
        return this;
    }

    withCmd(): this {
        this.press = { ...this.press, metaKey: true };
        return this;
    }

    withOption(): this {
        this.press = { ...this.press, altKey: true };
        return this;
    }

    build(): KeyPress {
        return this.press;
    }
}

/**
 * Un document réduit à ce que l'écoute lui demande, et la frappe qu'on lui donne.
 *
 * Pas de DOM à installer pour autant : ce module ne touche au document que pour poser et
 * retirer un écouteur, et à l'event que pour l'arrêter. Le reste — ce que la frappe **joue**
 * — est une réponse du backend, donc un port qu'on fournit.
 */
function aDocument(): { document: Document; press: (press: KeyPress) => void } {
    let listener: ((event: KeyboardEvent) => void) | null = null;
    const document = {
        addEventListener: (_name: string, handler: (event: KeyboardEvent) => void) => {
            listener = handler;
        },
        removeEventListener: () => {
            listener = null;
        },
    } as unknown as Document;

    return {
        document,
        press: (press) => {
            const event = {
                ...press,
                preventDefault: () => undefined,
                stopPropagation: () => undefined,
            } as unknown as KeyboardEvent;
            listener?.(event);
        },
    };
}

describe("la porte que le menu natif ne peut pas fermer", () => {
    it("Given Ctrl and Tab are pressed together, when the press is judged, then it is one the menu cannot catch", () => {
        // Given — `muda` donne à `Key::Tab` un équivalent clavier qu'AppKit ne reconnaît
        // jamais : ces deux accords-là n'arrivent que par la webview
        const press = KeyPressBuilder.press("Tab").withCtrl().build();

        // When / Then
        expect(withheldFromTheMenu(press)).toBe(true);
        expect(
            withheldFromTheMenu(KeyPressBuilder.press("Tab").withCtrl().withShift().build()),
        ).toBe(true);
    });
});

describe("ce que le terminal doit continuer de recevoir", () => {
    it("Given Tab is pressed on its own, when the press is judged, then it is not for ash and reaches the shell", () => {
        // Given — la complétion de `zsh`. Elle coûterait bien plus cher que le raccourci
        // ne rapporte, et c'est elle qui a dicté la forme de la règle.
        const press = KeyPressBuilder.press("Tab").build();

        // When / Then — « faux » veut dire « laisse passer » : rien n'est ni arrêté ni annulé
        expect(withheldFromTheMenu(press)).toBe(false);
    });

    it("Given Tab is pressed with Cmd or with Option, when the press is judged, then it is not for ash either", () => {
        // Given — `⌘⇥` appartient au commutateur d'applications de macOS, et `⌥⇥` à la
        // saisie ; les revendiquer volerait une touche à quelqu'un d'autre
        const presses = [
            KeyPressBuilder.press("Tab").withCmd().build(),
            KeyPressBuilder.press("Tab").withCtrl().withCmd().build(),
            KeyPressBuilder.press("Tab").withCtrl().withOption().build(),
        ];

        // When
        const judged = presses.map(withheldFromTheMenu);

        // Then
        expect(judged).toEqual([false, false, false]);
    });

    it("Given the chords the menu does catch, when they are judged, then this door stays shut on all of them", () => {
        // Given — `⌃C` et les touches d'édition de ligne, que le shell utilise tous les
        // jours ; `⌘F`, `⌘C`, `⌘V`, que la recherche du terminal et macOS possèdent ; et
        // toutes celles que le menu natif porte déjà, qu'AppKit consomme de toute façon.
        //
        // Cette écoute est posée **en capture sur le document**, donc elle voit le clavier
        // avant tout champ de saisie : ce qu'elle laisse passer est ce qui continue de
        // fonctionner partout ailleurs.
        const owned = [
            KeyPressBuilder.press("c", "KeyC").withCtrl().build(),
            KeyPressBuilder.press("f", "KeyF").withCmd().build(),
            KeyPressBuilder.press("v", "KeyV").withCmd().build(),
            KeyPressBuilder.press("z", "KeyZ").withCmd().withShift().build(),
            KeyPressBuilder.press("Escape").build(),
        ];

        // When
        const judged = owned.map(withheldFromTheMenu);

        // Then
        expect(judged).toEqual(owned.map(() => false));
    });
});

describe("ce qu'une frappe retenue joue", () => {
    it("Given the chord still belongs to an action, when it is pressed, then the action the backend names is played", () => {
        // Given — la webview ne sait pas ce que `⌃⇥` fait : elle envoie la frappe et obéit à
        // la réponse. La valeur envoyée est celle de la capture — le caractère produit, la
        // position physique et les quatre modificateurs —, pas une combinaison
        const asked: KeyStroke[] = [];
        const played: MenuAction[] = [];
        const { document, press } = aDocument();
        installShortcuts(
            document,
            (stroke) => {
                asked.push(stroke);
                return Promise.resolve<MenuAction | null>({ kind: "next-tab" });
            },
            (action) => played.push(action),
        );

        // When
        press(KeyPressBuilder.press("Tab").withCtrl().build());

        // Then
        return Promise.resolve().then(() => {
            expect(asked).toEqual([
                {
                    key: "Tab",
                    code: "Tab",
                    command: false,
                    control: true,
                    option: false,
                    shift: false,
                },
            ]);
            expect(played).toEqual([{ kind: "next-tab" }]);
        });
    });

    it("Given select next tab has been rebound elsewhere, when the old chord is pressed, then nothing changes tab", () => {
        // Given — c'est la moitié manquante du rebinding : l'écran disait le raccourci
        // déplacé, et `⌃⇥` continuait pourtant de changer d'onglet. Le backend ne nomme plus
        // personne, donc il n'y a plus rien à jouer — et aucune ligne de TypeScript n'a eu à
        // l'apprendre
        const asked: KeyStroke[] = [];
        const played: MenuAction[] = [];
        const { document, press } = aDocument();
        installShortcuts(
            document,
            (stroke) => {
                asked.push(stroke);
                return Promise.resolve<MenuAction | null>(null);
            },
            (action) => played.push(action),
        );

        // When
        press(KeyPressBuilder.press("Tab").withCtrl().build());

        // Then — la question a bien été posée, et c'est la **réponse** qui décide : sans le
        // premier `expect`, ce test passerait aussi le jour où la porte cesserait de s'ouvrir
        return Promise.resolve().then(() => {
            expect(asked).toHaveLength(1);
            expect(played).toEqual([]);
        });
    });
});
