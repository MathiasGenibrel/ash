import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Le thème de la fenêtre : clair, sombre, ou celui du système.
 *
 * **Le mode vit en Rust** (`src-tauri/src/features/theme/`), et ce module ne fait que le
 * rendre : c'est la même règle que pour les onglets — le frontend affiche un état, il ne
 * le détient pas ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Le choix
 * se fait dans le menu natif, arrive ici par l'event `ash://theme-mode`, et se relit au
 * démarrage par la commande `theme_mode`.
 *
 * Ce qui **est** décidé ici, et nulle part ailleurs, c'est la résolution du mode
 * *système* en une palette concrète : seule la webview sait de quelle humeur est macOS,
 * et elle l'apprend par `matchMedia`, qui la lui redit à chaque bascule — sans
 * redémarrage. Le CSS n'a donc que deux paliers à définir, un par palette, au lieu de
 * répéter le palier sombre sous une media query (voir `styles.css`).
 *
 * Il vit dans `app/` et non dans `shared/ipc/` pour la même raison que `menu.ts` : un
 * thème est un objet de fenêtre, dont le seul lecteur est le composition root.
 */

/** Nom de l'event du backend. Contrat avec `features::theme::commands::THEME_MODE_EVENT`. */
const THEME_MODE_EVENT = "ash://theme-mode";

/** Ce que l'utilisateur choisit. */
export type ThemeMode = "light" | "dark" | "system";

/** Ce qui se peint : une palette, jamais « système ». */
export type Theme = "light" | "dark";

/**
 * La palette qu'un mode donne, sur un système donné.
 *
 * Un mode explicite l'emporte toujours sur la préférence du système — c'est tout l'objet
 * d'un choix explicite. Le mode *système* n'est pas une troisième palette : c'est
 * l'absence de choix, donc celui de macOS.
 */
export function resolveTheme(mode: ThemeMode, prefersDark: boolean): Theme {
    return mode === "system" ? (prefersDark ? "dark" : "light") : mode;
}

/**
 * Le mode tel que le backend l'a sérialisé.
 *
 * Ce qui traverse la frontière est du JSON, donc `unknown` : un mode inconnu — un backend
 * plus récent que la webview — ne doit pas laisser la fenêtre sans palette.
 */
export function parseThemeMode(value: unknown): ThemeMode | null {
    return value === "light" || value === "dark" || value === "system" ? value : null;
}

/** Pose la palette sur la racine du document. `styles.css` fait le reste. */
export function applyTheme(root: HTMLElement, theme: Theme): void {
    root.dataset["theme"] = theme;
}

/**
 * Relie la racine du document au mode que le backend détient.
 *
 * La palette est posée **tout de suite**, avant tout aller-retour : sans ça, une fenêtre
 * ouverte sur un macOS sombre serait peinte en clair le temps d'un appel de commande.
 *
 * L'abonnement est pris avant la lecture pour ne pas perdre un changement qui arriverait
 * entre les deux.
 */
export async function followThemeMode(root: HTMLElement): Promise<void> {
    const system = window.matchMedia("(prefers-color-scheme: dark)");
    let mode: ThemeMode = "system";

    const draw = (): void => {
        applyTheme(root, resolveTheme(mode, system.matches));
    };
    draw();

    // Basculer macOS de clair à sombre pendant qu'Ash tourne doit changer l'application :
    // en mode *système*, c'est cet abonnement qui le fait, et lui seul.
    system.addEventListener("change", draw);

    await listen<unknown>(THEME_MODE_EVENT, (event) => {
        mode = parseThemeMode(event.payload) ?? mode;
        draw();
    });

    mode = parseThemeMode(await invoke<unknown>("theme_mode")) ?? mode;
    draw();
}
