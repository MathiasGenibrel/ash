import "./styles.css";
import { mountSettings, tauriSettings } from "@/features/settings";
import { loadAppName } from "./app-name";
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
 */
function mount(root: HTMLElement, appName: string): void {
    root.classList.add("ash-shell");
    // Son titre ne bouge jamais : la fenêtre de réglages n'a pas d'onglet actif dont suivre
    // le contexte. Seul le nom en vient d'ailleurs — `APP_NAME` côté Rust, comme la bande de
    // la fenêtre principale —, et le même mot est posé sur la fenêtre par
    // `features::settings::commands::open` : c'est ce dernier que macOS met dans le menu
    // Fenêtre, celui-ci est ce qu'on lit dans la page.
    root.append(createTitleBar(`settings — ${appName}`).element);
    mountSettings(root, tauriSettings);
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

// Le nom vient du backend, et la page attend sa réponse : comme dans la fenêtre principale,
// il n'y a pas de nom d'attente honnête à écrire, et `loadAppName` ne rejette jamais.
void loadAppName().then((appName) => {
    mount(root, appName);
});
