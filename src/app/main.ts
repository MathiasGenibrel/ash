import "./styles.css";
import { mountSidebar, type Sidebar } from "@/features/sidebar";
import { mountTerminals, type Terminals } from "@/features/terminal";
import { onMenuAction, type MenuAction } from "./menu";
import { followThemeMode, type ThemeChanges } from "./theme";
import { createTitleBar } from "./titlebar";

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
function mount(root: HTMLElement, theme: ThemeChanges): void {
    // Deux rangées : la bande de titre, puis les deux colonnes. La bande traverse toute la
    // largeur — c'est ce qui la laisse saisissable à droite des pastilles, et ce qui la
    // rend indifférente à `⌘B`.
    root.classList.add("ash-shell");

    const layout = document.createElement("div");
    layout.className = "ash-layout";

    const host = document.createElement("div");
    host.className = "terminal-host";

    // Le thème est passé, pas cherché : la table de tokens et sa bascule appartiennent à
    // l'application, et un terminal ne peint pas en CSS — il lui faut l'avis pour relire la
    // palette et se repeindre, onglets déjà ouverts compris.
    const terminals = mountTerminals(host, theme);

    // La sidebar ne connaît pas la feature terminal, et la feature terminal ne connaît pas
    // la sidebar : elles se rencontrent ici, et nulle part ailleurs. La sidebar ne
    // s'abonne à rien côté Tauri — elle reçoit les onglets **déjà situés** par le backend
    // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    const sidebar = mountSidebar({
        selectTab: (tabId) => void terminals.selectTab(tabId),
        newTab: () => void terminals.openTab("current-worktree"),
    });
    terminals.onTabs((tabs, activeTabId) => {
        sidebar.render(tabs, activeTabId);
    });

    layout.append(sidebar.element, host);
    root.append(createTitleBar(), layout);

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
        dispatch(terminals, sidebar, action).catch(fail);
    }).catch(fail);
}

function dispatch(terminals: Terminals, sidebar: Sidebar, action: MenuAction): Promise<void> {
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
        case "toggle-sidebar":
            // Repliée, la sidebar ne porte plus le contexte : la zone terminal le reprend
            // — un onglet s'intitule `omelette-web/claude`, et la ligne de statut nomme
            // l'agent qui attend.
            terminals.setSidebarCollapsed(sidebar.toggleCollapsed());
            return Promise.resolve();
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

// Avant tout montage : la palette d'abord, pour ne pas peindre une fenêtre en clair sur
// un macOS sombre le temps du premier aller-retour. Un échec du raccordement au backend
// laisse le thème du système, ce qui est exactement le défaut — il n'y a rien à rattraper.
const theme = followThemeMode(document.documentElement);
theme.ready.catch(() => undefined);

if (import.meta.env.VITE_SPIKE === "1") {
    mountSpike(root).catch((error: unknown) => {
        root.textContent = `spike — ÉCHEC : ${error instanceof Error ? error.message : String(error)}`;
    });
} else {
    mount(root, theme.changes);
}
