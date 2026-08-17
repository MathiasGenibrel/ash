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
 * Celle de la fenêtre principale porte le contexte de l'onglet actif — `ash — omelette-web
 * / feat/agent-sidebar` —, et c'est la barre d'onglets retirée qui le lui a laissé (spec
 * §4.2, amendée le 2026-08-17). Il se **met à jour** : le contexte suit les `cd` et les
 * changements d'onglet, d'où `setTitle` plutôt qu'un texte posé une fois. La règle qui décide
 * du texte, elle, n'est pas ici mais dans `window-title.ts` : cette bande pose ce qu'on lui
 * donne, comme la ligne de statut pose son modèle. Le rappel `◧ sidebar ⌘B` que la maquette
 * dessine à droite n'existe toujours pas.
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
/** La bande, et de quoi réécrire son titre — rien d'autre ne s'y change. */
export interface TitleBar {
    readonly element: HTMLElement;
    setTitle(title: string): void;
}

export function createTitleBar(title: string): TitleBar {
    const bar = document.createElement("div");
    bar.className = "ash-titlebar";
    bar.setAttribute("data-tauri-drag-region", "deep");

    // Trois cellules, dont deux réserves de 160 px : c'est la réserve de gauche qui
    // dégage les pastilles, et celle de droite — vide — qui rend le titre
    // **optiquement** centré au lieu de centré sur l'espace restant.
    bar.classList.add("is-titled");
    const label = document.createElement("span");
    label.className = "ash-titlebar-title";
    label.textContent = title;
    bar.append(document.createElement("span"), label, document.createElement("span"));

    return {
        element: bar,
        setTitle: (next) => {
            // Un titre identique n'est pas réécrit : la bande est repeinte au rythme de la
            // ligne de statut, une fois par seconde, et une écriture par seconde sur un
            // nœud de texte est du travail que personne n'a demandé.
            if (label.textContent !== next) label.textContent = next;
        },
    };
}
