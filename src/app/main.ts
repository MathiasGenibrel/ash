import "./styles.css";
import { mountTerminals, type Terminals } from "@/features/terminal";
import { onMenuAction, type MenuAction } from "./menu";

/**
 * Composition root du frontend.
 *
 * C'est ici, et nulle part ailleurs, que les features sont instanciées et câblées
 * entre elles. Une feature ne va pas chercher sa voisine : elle reçoit ce dont elle a
 * besoin. Voir `.claude/docs/architecture.md`.
 *
 * Le menu applicatif est déclaré en Rust ; c'est ici qu'on relie ses actions à la
 * feature terminal. La feature ne connaît pas le menu, et le menu ne connaît pas la
 * feature.
 */
function mount(root: HTMLElement): void {
    const host = document.createElement("div");
    host.className = "terminal-host";
    root.append(host);

    const terminals = mountTerminals(host);

    const fail = (error: unknown): void => {
        // Un shell qui ne démarre pas laisse l'application sans rien à montrer : le dire
        // vaut mieux qu'une fenêtre noire dont l'utilisateur ne peut rien conclure.
        const banner = document.createElement("p");
        banner.className = "ash-banner";
        banner.textContent = `ash : le shell n'a pas démarré — ${
            error instanceof Error ? error.message : String(error)
        }`;
        host.append(banner);
    };

    // Le premier onglet part de `~`, faute d'onglet actif dont reprendre le répertoire.
    terminals.openTab("home").catch(fail);

    onMenuAction((action) => {
        dispatch(terminals, action).catch(fail);
    }).catch(fail);
}

function dispatch(terminals: Terminals, action: MenuAction): Promise<void> {
    switch (action.kind) {
        case "new-tab":
            return terminals.openTab("current-worktree");
        case "new-home-tab":
            return terminals.openTab("home");
        case "close-tab":
            return terminals.closeActiveTab();
        case "clear-scrollback":
            return terminals.clearActiveScrollback();
        case "select-tab":
            return terminals.selectTabAt(action.position);
    }
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
