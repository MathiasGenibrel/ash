import "./styles.css";
import { runBench } from "@/spike/bench";

/**
 * Composition root du frontend.
 *
 * C'est ici, et nulle part ailleurs, que les features sont instanciées et câblées
 * entre elles. Une feature ne va pas chercher sa voisine : elle reçoit ce dont elle a
 * besoin. Voir `.claude/docs/architecture.md`.
 *
 * Il n'y a encore aucune feature à câbler. Pendant la durée du spike, ce fichier monte
 * le banc de mesure ; il repart avec lui.
 */
function mount(root: HTMLElement): void {
    const output = document.createElement("pre");
    output.className = "spike-log";
    output.textContent = "spike xterm.js — mesure en cours…\n";

    const host = document.createElement("div");
    host.className = "spike-host";

    root.append(output, host);

    const log = (line: string): void => {
        output.textContent += `${line}\n`;
    };

    runBench(host, log).catch((error: unknown) => {
        log(`ÉCHEC : ${error instanceof Error ? error.message : String(error)}`);
    });
}

const root = document.querySelector<HTMLElement>("#root");

// Le point de montage vient d'`index.html`, pas de l'utilisateur : son absence est un
// bug de build, pas une erreur à rattraper.
if (root === null) {
    throw new Error("index.html n'expose pas #root");
}

mount(root);
