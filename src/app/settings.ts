import "./styles.css";
import { mountSettings, tauriSettings, type WindowPorts } from "@/features/settings";
import { loadAppName } from "./app-name";
import { followTerminalFontSize } from "./font-size";
import { menuShortcuts } from "./menu";
import { followThemeMode } from "./theme";
import { createTitleBar } from "./titlebar";

/**
 * Composition root de la **fenêtre de réglages**.
 *
 * Une seconde fenêtre est une seconde page (`settings.html`), pas un second état de la
 * première : elle a son propre document, donc son propre point de montage. Ce qu'elles
 * partagent, elles le partagent par le code — la table de tokens, la bande de titre, et la
 * façon de suivre le thème.
 *
 * **Le thème se suit ici comme dans la fenêtre principale, et par le même module.**
 * `followThemeMode` s'abonne à `ash://theme-mode`, que Tauri diffuse à **toutes** les
 * fenêtres, et relit le mode au démarrage par la commande `theme_mode`. Une bascule faite
 * dans le menu pendant que les réglages sont ouverts les repeint donc au même instant que
 * la fenêtre principale, et le mode *système* y suit macOS par le même `matchMedia`. Rien
 * n'est recopié : le mode reste détenu par le backend
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * **C'est aussi ce qui sert la section `appearance`** (spec §9). Elle n'ouvre pas un second
 * chemin vers l'apparence : elle branche la fenêtre sur celui-ci. Un thème choisi dans
 * l'écran passe par le même `theme_set_mode` que l'entrée de menu — qui corrige au passage
 * ses trois coches —, une taille par le même pas que `⌘+`, et les deux reviennent par les
 * mêmes annonces. Il n'y a donc, d'un bout à l'autre, qu'un seul détenteur de l'apparence :
 * `features::theme`.
 */
function mount(root: HTMLElement, appName: string, ports: WindowPorts): void {
    root.classList.add("ash-shell");
    // Son titre ne bouge jamais : la fenêtre de réglages n'a pas d'onglet actif dont suivre
    // le contexte. Seul le nom en vient d'ailleurs — `APP_NAME` côté Rust, comme la bande de
    // la fenêtre principale —, et le même mot est posé sur la fenêtre par
    // `features::settings::commands::open` : c'est ce dernier que macOS met dans le menu
    // Fenêtre, celui-ci est ce qu'on lit dans la page.
    root.append(createTitleBar(`settings — ${appName}`).element);
    mountSettings(root, tauriSettings, ports);
}

const root = document.querySelector<HTMLElement>("#root");

// Le point de montage vient de `settings.html`, pas de l'utilisateur : son absence est un
// bug de build, pas une erreur à rattraper.
if (root === null) {
    throw new Error("settings.html n'expose pas #root");
}

// La palette d'abord, comme dans la fenêtre principale : sans ça, une fenêtre ouverte sur
// un macOS sombre serait peinte en clair le temps d'un aller-retour.
const theme = followThemeMode(document.documentElement);
theme.ready.catch(() => undefined);

// La taille de police, elle, ne peint rien dans cette fenêtre : il n'y a pas de terminal ici.
// Elle n'est suivie que pour être **montrée** et réglée par la section `appearance`.
const fontSize = followTerminalFontSize();
fontSize.ready.catch(() => undefined);

/**
 * Ce que la fenêtre de réglages demande aux objets de fenêtre — assemblé ici, parce que c'est
 * ici qu'on connaît les deux modules qui savent déjà leur parler.
 *
 * `appearance()` attend les deux raccordements : rendre la valeur par défaut avant le retour
 * du backend ferait cocher `system` et afficher `13 pt` sur une session qui a choisi autre
 * chose — un écran qui affirme un réglage qui n'est pas le sien est pire qu'un écran qui
 * attend.
 */
const windowPorts: WindowPorts = {
    appearance: async () => {
        await Promise.all([theme.ready, fontSize.ready]);
        return { mode: theme.modes.current, fontSize: fontSize.changes.current };
    },
    chooseThemeMode: (mode) => theme.modes.choose(mode),
    stepTerminalFontSize: (step) => fontSize.step(step),
    onAppearanceChanged: (listener) => {
        // Deux abonnements pour un seul état affiché : le mode et la taille sont annoncés
        // séparément par le backend — ce sont deux préférences, et un `⌘+` n'a pas à faire
        // repasser le thème. La scène, elle, les porte ensemble.
        theme.modes.subscribe((mode) => {
            listener({ mode, fontSize: fontSize.changes.current });
        });
        fontSize.changes.subscribe((points) => {
            listener({ mode: theme.modes.current, fontSize: points });
        });
    },
    shortcuts: menuShortcuts,
};

// Le nom vient du backend, et la page attend sa réponse : comme dans la fenêtre principale,
// il n'y a pas de nom d'attente honnête à écrire, et `loadAppName` ne rejette jamais.
void loadAppName().then((appName) => {
    mount(root, appName, windowPorts);
});
