import "./styles.css";
import { openTerminal } from "@/features/terminal";

/**
 * Composition root du frontend.
 *
 * C'est ici, et nulle part ailleurs, que les features sont instanciées et câblées
 * entre elles. Une feature ne va pas chercher sa voisine : elle reçoit ce dont elle a
 * besoin. Voir `.claude/docs/architecture.md`.
 *
 * Un seul onglet pour l'instant, et un seul terminal visible à la fois
 * ([ADR-0003](../../docs/adr/0003-zone-terminal-unique.md)). La barre d'onglets et les
 * raccourcis viennent avec la tâche suivante.
 */
function mount(root: HTMLElement): void {
    const host = document.createElement("div");
    host.className = "terminal-host";
    root.append(host);

    openTerminal(host).catch((error: unknown) => {
        // Un shell qui ne démarre pas laisse l'application sans rien à montrer : le dire
        // vaut mieux qu'une fenêtre noire dont l'utilisateur ne peut rien conclure.
        host.textContent = `ash : le shell n'a pas démarré — ${
            error instanceof Error ? error.message : String(error)
        }`;
    });
}

/**
 * Monte le banc de mesure du spike xterm.js au lieu de l'application.
 *
 * Derrière un drapeau, et éteint par défaut : l'application ne doit pas démarrer sur un
 * banc de mesure. Se relance avec `VITE_SPIKE=1 bun run tauri dev` — voir
 * `docs/spike-xterm.md`. L'import est dynamique pour que le banc ne pèse pas dans le
 * bundle quand le drapeau est absent.
 */
async function mountSpike(root: HTMLElement): Promise<void> {
    const output = document.createElement("pre");
    output.className = "spike-log";
    output.textContent = "spike xterm.js — mesure en cours…\n";

    const host = document.createElement("div");
    host.className = "spike-host";
    root.classList.add("spike");
    root.append(output, host);

    const log = (line: string): void => {
        output.textContent += `${line}\n`;
    };

    const { runBench } = await import("@/spike/bench");
    await runBench(host, log);
}

const root = document.querySelector<HTMLElement>("#root");

// Le point de montage vient d'`index.html`, pas de l'utilisateur : son absence est un
// bug de build, pas une erreur à rattraper.
if (root === null) {
    throw new Error("index.html n'expose pas #root");
}

if (import.meta.env.VITE_SPIKE === "1") {
    mountSpike(root).catch((error: unknown) => {
        root.textContent = `spike — ÉCHEC : ${error instanceof Error ? error.message : String(error)}`;
    });
} else {
    mount(root);
}
