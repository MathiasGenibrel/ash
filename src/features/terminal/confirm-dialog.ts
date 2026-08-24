/**
 * La confirmation de fermeture, en DOM plutôt qu'en `window.confirm`.
 *
 * `window.confirm` bloque la boucle d'événements de la webview : les morceaux de PTY
 * cessent d'être écrits, donc d'être acquittés, et un onglet actif se fige derrière la
 * boîte de dialogue. Rien ne garantit non plus qu'une WKWebView embarquée l'affiche.
 *
 * Le résultat est une promesse, jamais un booléen : c'est ce qui rend la règle
 * « `Cmd+W` ne détruit rien tant que l'utilisateur n'a pas répondu » (spec §4.4)
 * exprimable, et testable avec un port au lieu d'une fenêtre.
 *
 * Le fichier est en deux moitiés, comme la recherche du scrollback : `composeCloseBox` rend
 * une [description](../../shared/ui/node.ts) — les deux boutons, ce qu'ils répondent, celui
 * qui prend le focus — et `askToClose` la pose. C'est ce qui met le clic sous test :
 * `bun test` n'a pas de DOM, et la boîte n'était couverte que par sa touche `Échap`, la
 * seule chose qui marchait encore quand ses boutons avalaient la souris.
 */

import {
    button,
    FOCUS_KEY,
    paint,
    row,
    text,
    type UiChild,
    type UiComponent,
} from "@/shared/ui";

/**
 * La réponse de l'utilisateur : `true` va au bout du geste, `false` ne touche à rien.
 *
 * Le nom ne dit plus « fermer » depuis que la boîte sert aussi à quitter (issue #177) : ce
 * qu'un `true` détruit dépend de la question — un PTY pour `Cmd+W`, l'application entière
 * pour `Cmd+Q` —, et seul le composeur de la boîte le sait.
 */
export type ConfirmAnswer = (goAhead: boolean) => void;

/**
 * La clé du bouton qui reçoit le focus à l'ouverture.
 *
 * Elle est dans la description et pas dans le peintre pour la même raison que celle du champ
 * de recherche : c'est la description qui sait **lequel** des deux boutons ne détruit rien,
 * et un test le vérifie. Le peintre, lui, ne fait que suivre la clé — et c'est
 * [la même](../../shared/ui/node.ts) que celle de la recherche, pas un second protocole.
 */
export const CANCEL_FOCUS_KEY = "close-confirm-cancel";

/** La classe du bouton qui n'a d'effet que de refermer. Lue par `terminal.css`, et par le test. */
const CANCEL_CLASS = "ash-confirm-cancel";

/** La classe du geste destructeur — `is-danger`, comme la maquette la nomme. */
const DANGER_CLASS = "is-danger";

/** La classe d'une ligne d'énumération sous la question. Lue par `terminal.css`. */
const ITEM_CLASS = "ash-confirm-item";

/**
 * La forme commune des deux questions d'Ash : ce qu'elle dit, puis les deux réponses.
 *
 * Le clic de chaque bouton et la touche `Échap` mènent au **même** port : il n'y a qu'un
 * chemin de sortie, donc la souris et le clavier ne peuvent pas se répondre différemment.
 *
 * `message` est une **suite** de composants et non une chaîne : `Cmd+W` n'a qu'une phrase à
 * dire, mais quitter Ash doit nommer chaque agent sur sa ligne (issue #177), et une boîte
 * par question aurait été deux voiles, deux `Échap` et deux focus initiaux à tenir d'accord.
 */
export function composeConfirmBox(
    message: readonly UiChild[],
    dangerLabel: string,
    answer: ConfirmAnswer,
): UiComponent {
    // Le défaut est le choix qui ne détruit rien : la touche entrée sur un dialogue qui
    // vient d'apparaître ne doit pas tuer un processus — ni fermer un onglet, ni quitter
    // l'application.
    const cancel = button("Annuler")
        .class(CANCEL_CLASS)
        .focusKey(CANCEL_FOCUS_KEY)
        .onClick(() => {
            answer(false);
        });

    const destroy = button(dangerLabel).class(DANGER_CLASS).onClick(() => {
        answer(true);
    });

    return row(
        row(...message).class("ash-confirm-message"),
        row(cancel, destroy).class("ash-confirm-actions"),
    ).class("ash-confirm-box");
}

/**
 * Une ligne d'énumération sous la question — ce qu'on va perdre, un par ligne.
 *
 * Publiée avec la boîte, et pas seulement peinte par elle : `.ash-confirm-item` est dans
 * `terminal.css`, donc elle appartient à cette feature. La laisser écrire au composeur d'à
 * côté mettait le même nom de classe des deux côtés d'une frontière, qu'un renommage de la
 * feuille de style aurait cassé sans que rien ne le dise.
 */
export function confirmLine(line: string): UiComponent {
    return row(text(line)).class(ITEM_CLASS);
}

/**
 * La boîte de `Cmd+W` : la question, puis les deux réponses.
 *
 * Elle n'est plus qu'un habillage de [`composeConfirmBox`] — le voile, le focus, `Échap` et
 * la place du geste destructeur sont les mêmes pour les deux questions que pose Ash, et le
 * dépôt n'a qu'un dialogue. Ce qui lui reste en propre est ce qu'elle dit et ce que son
 * bouton rouge promet.
 */
export function composeCloseBox(what: string, answer: ConfirmAnswer): UiComponent {
    return composeConfirmBox(
        [text(`Quelque chose tourne dans « ${what} ». Fermer l'onglet ?`)],
        "Fermer l'onglet",
        answer,
    );
}

/** Pose la boîte de `Cmd+W` dans `host`, et rend la réponse. */
export function askToClose(host: HTMLElement, what: string): Promise<boolean> {
    return askForConfirmation(host, (answer) => composeCloseBox(what, answer));
}

/**
 * Pose une boîte — n'importe laquelle — dans `host`, et rend la réponse.
 *
 * C'est la moitié qui touche le DOM, et elle ne décide rien : le voile, l'écoute d'`Échap`,
 * le focus initial et le retrait. Elle se vérifie à la main — il n'y a pas de `document`
 * sous `bun test`.
 *
 * **Un clic à côté de la boîte ne répond pas.** Le voile n'a aucun gestionnaire, et ce n'est
 * pas un oubli : un geste imprécis ne peut pas valoir « ferme l'onglet » ni « quitte Ash »,
 * et le faire valoir « annuler » ferait disparaître la question sous une souris qui a
 * glissé. Il n'y a donc que trois issues, toutes explicites : les deux boutons et `Échap`.
 */
export function askForConfirmation(
    host: HTMLElement,
    box: (answer: ConfirmAnswer) => UiComponent,
): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
        const overlay = document.createElement("div");
        overlay.className = "ash-confirm";
        overlay.setAttribute("role", "dialog");
        overlay.setAttribute("aria-modal", "true");

        const answer = (closeIt: boolean): void => {
            document.removeEventListener("keydown", onKey, true);
            // `remove()` et non `removeChild` : deux réponses dans le même battement — un
            // clic et un `Échap` — ne doivent pas lever sur un nœud déjà détaché. La seconde
            // ne fait alors rien, et `resolve` est déjà sans effet.
            overlay.remove();
            resolve(closeIt);
        };

        // En capture : le terminal a le focus, et xterm.js consomme les touches. Sans
        // ça, `Échap` partirait dans le shell au lieu d'annuler.
        function onKey(event: KeyboardEvent): void {
            if (event.key === "Escape") {
                event.preventDefault();
                answer(false);
            }
        }

        overlay.append(paint(box(answer).build()));
        document.addEventListener("keydown", onKey, true);
        host.append(overlay);

        overlay.querySelector<HTMLElement>(`[${FOCUS_KEY}="${CANCEL_FOCUS_KEY}"]`)?.focus();
    });
}
