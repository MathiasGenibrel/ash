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
 */
export function askToClose(host: HTMLElement, what: string): Promise<boolean> {
    return new Promise<boolean>((resolve) => {
        const overlay = document.createElement("div");
        overlay.className = "ash-confirm";
        overlay.setAttribute("role", "dialog");
        overlay.setAttribute("aria-modal", "true");

        const box = document.createElement("div");
        box.className = "ash-confirm-box";

        const message = document.createElement("p");
        message.textContent = `Quelque chose tourne dans « ${what} ». Fermer l'onglet ?`;

        const cancel = document.createElement("button");
        cancel.type = "button";
        cancel.textContent = "Annuler";

        const confirm = document.createElement("button");
        confirm.type = "button";
        confirm.className = "is-danger";
        confirm.textContent = "Fermer l'onglet";

        const answer = (closeIt: boolean): void => {
            overlay.remove();
            document.removeEventListener("keydown", onKey, true);
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

        cancel.addEventListener("click", () => {
            answer(false);
        });
        confirm.addEventListener("click", () => {
            answer(true);
        });
        document.addEventListener("keydown", onKey, true);

        const actions = document.createElement("div");
        actions.className = "ash-confirm-actions";
        actions.append(cancel, confirm);
        box.append(message, actions);
        overlay.append(box);
        host.append(overlay);

        // Le défaut est le choix qui ne détruit rien : la touche entrée sur un dialogue
        // qui vient d'apparaître ne doit pas tuer un processus.
        cancel.focus();
    });
}
