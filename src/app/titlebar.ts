/**
 * La bande de titre — et la seule chose par laquelle on peut saisir la fenêtre.
 *
 * `titleBarStyle: "Overlay"` + `hiddenTitle` masquent la barre de titre native et laissent
 * la webview occuper toute la fenêtre, pastilles comprises. macOS ne fournit donc plus
 * aucune zone de saisie, et il n'y en avait pas non plus côté web : la sidebar et la zone
 * terminal réservaient chacune 30 px en haut pour dégager les boutons de fenêtre, mais ce
 * n'était que du vide. La fenêtre se redimensionnait, elle ne se déplaçait pas.
 *
 * Elle vit dans `app/` et pas dans une feature : c'est du chrome de fenêtre, elle
 * surplombe **les deux** colonnes, et c'est précisément ce qui la rend indépendante de
 * `⌘B` — repliée ou non, la sidebar ne passe jamais dessous.
 *
 * `data-tauri-drag-region="deep"`, et pas l'attribut nu : nu, Tauri ne déclenche le
 * glissement que si l'élément visé est **exactement** la bande, donc le jour où on y pose
 * le titre centré de la maquette, ce titre deviendrait une zone morte au milieu de la
 * seule prise de la fenêtre. En `deep`, tout le sous-arbre glisse — sauf ce que Tauri
 * reconnaît comme cliquable (`button`, `a`, `input`, `role="tab"`…), qui continue de
 * recevoir ses clics. C'est la règle qu'on veut, et elle est déjà écrite dans
 * `tauri/src/window/scripts/drag.js` : la réécrire ici n'en ferait qu'une seconde version
 * à maintenir.
 *
 * Celle de la fenêtre principale est vide. La maquette y dessine `ash — omelette-web /
 * claude` au centre et un rappel `◧ sidebar ⌘B` à droite ; ni l'un ni l'autre n'est
 * nécessaire pour rendre la fenêtre déplaçable, et « l'agent actif » n'a pas encore de
 * source ([ADR-0007](../../docs/adr/0007-etats-par-hooks.md)).
 *
 * La fenêtre de réglages, elle, porte un titre — `settings — ash`, centré. Il est **posé
 * ici et pas dans la feature** parce que c'est du chrome de fenêtre : les deux fenêtres
 * ont la même bande, la même hauteur et la même réserve pour les pastilles de macOS, et
 * les écrire deux fois les ferait diverger au premier pixel.
 *
 * Sa hauteur reste **38 px**, celle de la fenêtre principale, alors que la maquette des
 * réglages en dessine 36. C'est un écart assumé : la valeur de 38 px porte une raison —
 * les pastilles de macOS sont posées à un décalage fixe, et une bande plus courte les
 * ferait retomber sur le contenu — et cette raison vaut pour les deux fenêtres. Deux
 * pixels ne justifient pas deux valeurs.
 */
export function createTitleBar(title?: string): HTMLElement {
    const bar = document.createElement("div");
    bar.className = "ash-titlebar";
    bar.setAttribute("data-tauri-drag-region", "deep");

    if (title !== undefined) {
        // Trois cellules, dont deux réserves de 160 px : c'est la réserve de gauche qui
        // dégage les pastilles, et celle de droite — vide — qui rend le titre
        // **optiquement** centré au lieu de centré sur l'espace restant.
        bar.classList.add("is-titled");
        const label = document.createElement("span");
        label.className = "ash-titlebar-title";
        label.textContent = title;
        bar.append(document.createElement("span"), label, document.createElement("span"));
    }

    return bar;
}
