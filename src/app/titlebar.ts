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
 * Elle est vide pour l'instant. La maquette y dessine `ash — omelette-web / claude` au
 * centre et un rappel `◧ sidebar ⌘B` à droite ; ni l'un ni l'autre n'est nécessaire pour
 * rendre la fenêtre déplaçable, et « l'agent actif » n'a pas encore de source
 * ([ADR-0007](../../docs/adr/0007-etats-par-hooks.md)).
 */
export function createTitleBar(): HTMLElement {
    const bar = document.createElement("div");
    bar.className = "ash-titlebar";
    bar.setAttribute("data-tauri-drag-region", "deep");
    return bar;
}
