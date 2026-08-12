import "./styles.css";
import { mountSettings, tauriSettings } from "@/features/settings";
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
function mount(root: HTMLElement): void {
    root.classList.add("ash-shell");
    root.append(createTitleBar("settings — ash"));
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

mount(root);
