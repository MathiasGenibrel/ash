import "./styles.css";

/**
 * Composition root du frontend.
 *
 * C'est ici, et nulle part ailleurs, que les features sont instanciées et câblées
 * entre elles. Une feature ne va pas chercher sa voisine : elle reçoit ce dont elle a
 * besoin. Voir `.claude/docs/architecture.md`.
 *
 * Il n'y a encore aucune feature à câbler. Ce fichier existe pour que la première
 * n'ait pas à inventer où se brancher.
 */
function mount(root: HTMLElement): void {
    const placeholder = document.createElement("p");
    placeholder.textContent = "ash";
    root.append(placeholder);
}

const root = document.querySelector<HTMLElement>("#root");

// Le point de montage vient d'`index.html`, pas de l'utilisateur : son absence est un
// bug de build, pas une erreur à rattraper.
if (root === null) {
    throw new Error("index.html n'expose pas #root");
}

mount(root);
