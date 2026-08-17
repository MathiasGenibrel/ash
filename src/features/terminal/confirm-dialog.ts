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

import { button, FOCUS_KEY, paint, row, text, type UiComponent } from "@/shared/ui";

/** La réponse de l'utilisateur : `true` détruit le PTY, `false` ne touche à rien. */
export type CloseAnswer = (closeIt: boolean) => void;

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

/**
 * La boîte : la question, puis les deux réponses.
 *
 * Le clic de chaque bouton et la touche `Échap` mènent au **même** port : il n'y a qu'un
 * chemin de sortie, donc la souris et le clavier ne peuvent pas se répondre différemment.
 */
export function composeCloseBox(what: string, answer: CloseAnswer): UiComponent {
    // Le défaut est le choix qui ne détruit rien : la touche entrée sur un dialogue qui
    // vient d'apparaître ne doit pas tuer un processus.
    const cancel = button("Annuler")
        .class(CANCEL_CLASS)
        .focusKey(CANCEL_FOCUS_KEY)
        .onClick(() => {
            answer(false);
        });

    const destroy = button("Fermer l'onglet")
        .class(DANGER_CLASS)
        .onClick(() => {
            answer(true);
        });

    return row(
        row(text(`Quelque chose tourne dans « ${what} ». Fermer l'onglet ?`)).class(
            "ash-confirm-message",
        ),
        row(cancel, destroy).class("ash-confirm-actions"),
    ).class("ash-confirm-box");
}

/**
 * Pose la boîte dans `host`, et rend la réponse.
 *
 * C'est la moitié qui touche le DOM, et elle ne décide rien : le voile, l'écoute d'`Échap`,
 * le focus initial et le retrait. Elle se vérifie à la main — il n'y a pas de `document`
 * sous `bun test`.
 *
 * **Un clic à côté de la boîte ne répond pas.** Le voile n'a aucun gestionnaire, et ce n'est
 * pas un oubli : un geste imprécis ne peut pas valoir « ferme l'onglet », et le faire valoir
 * « annuler » ferait disparaître la question sous une souris qui a glissé. Il n'y a donc que
 * trois issues, toutes explicites : les deux boutons et `Échap`.
 */
export function askToClose(host: HTMLElement, what: string): Promise<boolean> {
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

        overlay.append(paint(composeCloseBox(what, answer).build()));
        document.addEventListener("keydown", onKey, true);
        host.append(overlay);

        overlay.querySelector<HTMLElement>(`[${FOCUS_KEY}="${CANCEL_FOCUS_KEY}"]`)?.focus();
    });
}
