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
 * Ce qu'on rend à qui suit le thème.
 *
 * Presque tout se contente du CSS : `data-theme` change, les tokens changent, la fenêtre
 * est repeinte. Ce qui reste — xterm.js, qui compose ses cellules lui-même et ne résout
 * pas un `var(--ash-…)` — a besoin d'être **prévenu**, pour relire la table et se
 * repeindre. D'où cet abonnement, et une seule détection : celle de ce module.
 */
export interface ThemeChanges {
    /** Prévient après chaque changement de palette, une fois `data-theme` posé. */
    subscribe(listener: () => void): () => void;
}

/**
 * Le **mode**, pour qui doit le montrer et non seulement le peindre.
 *
 * La fenêtre de réglages en a besoin là où la fenêtre principale se contentait de la palette
 * (§9) : elle affiche lequel des trois est en vigueur, et propose de le changer. Les deux
 * choses restent des lectures — le mode est détenu par `features::theme`
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)), et [`ThemeModes.choose`] ne
 * fait que le lui demander : la valeur ne bouge ici qu'au retour de l'annonce, celle que le
 * menu natif fait déjà partir.
 *
 * `subscribe` prévient à chaque changement de **mode**, là où `ThemeChanges` prévient à chaque
 * changement de **palette** : passer de *système* à *sombre* sur un macOS déjà sombre change
 * le premier sans toucher au second, et c'est exactement ce que la section `appearance` doit
 * montrer.
 */
export interface ThemeModes {
    readonly current: ThemeMode;
    subscribe(listener: (mode: ThemeMode) => void): () => void;
    /** Demande un mode au backend. Rejette si l'appel n'aboutit pas. */
    choose(mode: ThemeMode): Promise<void>;
}

/** Ce que `followThemeMode` rend : de quoi suivre, et de quoi attendre le backend. */
export interface ThemeBinding {
    changes: ThemeChanges;
    /** Le mode lui-même, pour la fenêtre qui le montre. */
    modes: ThemeModes;
    /** Le raccordement au mode que le backend détient. Rejette s'il n'a pas lieu. */
    ready: Promise<void>;
}

/**
 * Relie la racine du document au mode que le backend détient.
 *
 * La palette est posée **tout de suite**, avant tout aller-retour : sans ça, une fenêtre
 * ouverte sur un macOS sombre serait peinte en clair le temps d'un appel de commande.
 * C'est aussi pourquoi la fonction n'est pas `async` : ses abonnés doivent pouvoir se
 * brancher sur une palette déjà posée.
 *
 * L'abonnement à l'event est pris avant la lecture de la commande pour ne pas perdre un
 * changement qui arriverait entre les deux.
 */
export function followThemeMode(root: HTMLElement): ThemeBinding {
    const system = window.matchMedia("(prefers-color-scheme: dark)");
    const listeners = new Set<() => void>();
    const modeListeners = new Set<(mode: ThemeMode) => void>();
    let mode: ThemeMode = "system";
    let painted: Theme | null = null;

    const draw = (): void => {
        const theme = resolveTheme(mode, system.matches);
        applyTheme(root, theme);
        // Le mode peut changer sans que la palette bouge — passer de *système* à *sombre*
        // sur un macOS déjà sombre. Repeindre chaque terminal pour rien serait sans
        // conséquence visible, mais dirait une bascule qui n'a pas eu lieu.
        if (theme === painted) return;
        painted = theme;
        for (const listener of listeners) listener();
    };
    draw();

    /**
     * Retient un mode annoncé par le backend, et le fait savoir.
     *
     * `draw` est appelée dans tous les cas — elle sait ne rien repeindre pour rien — mais les
     * abonnés au **mode**, eux, ne sont prévenus que s'il a bougé : la relecture au démarrage
     * rend presque toujours la valeur déjà posée.
     */
    const announce = (next: ThemeMode): void => {
        const changed = next !== mode;
        mode = next;
        draw();
        if (!changed) return;
        for (const listener of modeListeners) listener(mode);
    };

    // Basculer macOS de clair à sombre pendant qu'Ash tourne doit changer l'application :
    // en mode *système*, c'est cet abonnement qui le fait, et lui seul.
    system.addEventListener("change", draw);

    const ready = (async (): Promise<void> => {
        await listen<unknown>(THEME_MODE_EVENT, (event) => {
            announce(parseThemeMode(event.payload) ?? mode);
        });

        announce(parseThemeMode(await invoke<unknown>("theme_mode")) ?? mode);
    })();

    return {
        changes: {
            subscribe: (listener) => {
                listeners.add(listener);
                return () => {
                    listeners.delete(listener);
                };
            },
        },
        modes: {
            get current(): ThemeMode {
                return mode;
            },
            subscribe: (listener) => {
                modeListeners.add(listener);
                return () => {
                    modeListeners.delete(listener);
                };
            },
            // Rien n'est posé ici : le backend retient le mode et l'annonce à **toutes** les
            // fenêtres, et c'est l'annonce qui repasse par `announce`. C'est ce qui fait qu'un
            // choix venu de cette fenêtre et un choix venu du menu natif suivent le même
            // chemin, donc qu'ils ne peuvent pas se contredire.
            choose: (chosen) => invoke<void>("theme_set_mode", { mode: chosen }),
        },
        ready,
    };
}
