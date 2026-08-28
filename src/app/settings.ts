import "./styles.css";
import {
    mountSettings,
    tauriSettings,
    type Appearance,
    type WindowPorts,
} from "@/features/settings";
import { loadAppName } from "./app-name";
import { followTerminalFontSize } from "./font-size";
import {
    bindShortcut,
    clearShortcut,
    listenForShortcut,
    menuShortcuts,
    previewShortcut,
    resetAllShortcuts,
    resetShortcut,
    resolveShortcutConflict,
} from "./menu";
import { followSidebarDensity } from "./sidebar-density";
import { followTerminalFont, installedMonospaceFonts } from "./terminal-font";
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

// La police et la densité n'ont **que** cette surface : c'est ici qu'elles se choisissent.
// Elles sont suivies de la même façon quand même — la fenêtre les montre parce que le
// backend les dit, jamais parce qu'on vient de cliquer
// ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
const terminalFont = followTerminalFont();
terminalFont.ready.catch(() => undefined);

// Elle ne peint rien ici — il n'y a pas de sidebar dans cette fenêtre —, mais elle est posée
// sur la racine comme dans la fenêtre principale : les miniatures de densité de la section
// s'en servent, et poser l'attribut est le seul geste de ce module.
const density = followSidebarDensity(document.documentElement);
density.ready.catch(() => undefined);

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
        await Promise.all([theme.ready, fontSize.ready, terminalFont.ready, density.ready]);
        return {
            mode: theme.modes.current,
            fontSize: fontSize.changes.current,
            font: terminalFont.family.current,
            density: density.current,
        };
    },
    chooseThemeMode: (mode) => theme.modes.choose(mode),
    stepTerminalFontSize: (step) => fontSize.step(step),
    monospaceFonts: installedMonospaceFonts,
    chooseTerminalFont: (family) => terminalFont.choose(family),
    chooseSidebarDensity: (chosen) => density.choose(chosen),
    onAppearanceChanged: (listener) => {
        // Quatre abonnements pour un seul état affiché : les préférences sont annoncées
        // séparément par le backend — un `⌘+` n'a pas à faire repasser le thème. La scène,
        // elle, les porte ensemble, et chaque annonce la reforme **entière** : reconstruire
        // à partir des quatre valeurs en cours évite qu'un abonnement recopie une valeur
        // périmée d'un autre.
        const shown = (): Appearance => ({
            mode: theme.modes.current,
            fontSize: fontSize.changes.current,
            font: terminalFont.family.current,
            density: density.current,
        });
        theme.modes.subscribe(() => {
            listener(shown());
        });
        fontSize.changes.subscribe(() => {
            listener(shown());
        });
        terminalFont.family.subscribe(() => {
            listener(shown());
        });
        density.subscribe(() => {
            listener(shown());
        });
    },
    // Les sept verbes des raccourcis passent tels quels : le composition root les connaît
    // parce que c'est lui qui connaît `menu.ts`, et la fenêtre ne connaît que ces noms.
    shortcuts: menuShortcuts,
    listenForShortcut,
    previewShortcut,
    bindShortcut,
    clearShortcut,
    resetShortcut,
    resetAllShortcuts,
    resolveShortcutConflict,
};

// Le nom vient du backend, et la page attend sa réponse : comme dans la fenêtre principale,
// il n'y a pas de nom d'attente honnête à écrire, et `loadAppName` ne rejette jamais.
void loadAppName().then((appName) => {
    mount(root, appName, windowPorts);
});
