import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

/**
 * L'alias `@/` est déclaré à trois endroits : ici, dans `tsconfig.json`, et — via les
 * `paths` du tsconfig, que Bun lit nativement — dans `bun test`. En oublier un casse
 * les tests ou le build sans message clair.
 */
const alias = {
    "@": fileURLToPath(new URL("./src", import.meta.url)),
};

export default defineConfig({
    resolve: { alias },

    // Port fixe : `tauri.conf.json` pointe dessus. Laisser Vite en choisir un autre
    // quand 1420 est pris donnerait une fenêtre blanche sans erreur.
    server: {
        port: 1420,
        strictPort: true,
        watch: {
            // Le watcher n'a rien à faire dans l'arbre Rust : `target/` y pèse
            // plusieurs gigaoctets et cargo le réécrit en permanence.
            ignored: ["**/src-tauri/**"],
        },
    },

    build: {
        // Safari 15 est la base de WKWebView sur les macOS que l'app vise.
        target: "safari15",
        sourcemap: true,

        // Deux pages, parce qu'il y a deux fenêtres : la fenêtre de réglages a son propre
        // document (`settings.html`), que le backend ouvre par `WebviewUrl::App`. Sans
        // cette entrée, `vite build` ne produirait que `index.html` et la seconde fenêtre
        // serait blanche en bundle — alors qu'elle marcherait en développement, où Vite
        // sert le dossier entier.
        rollupOptions: {
            input: {
                main: fileURLToPath(new URL("./index.html", import.meta.url)),
                settings: fileURLToPath(new URL("./settings.html", import.meta.url)),
            },
        },
    },

    // Tauri sert le bundle depuis un chemin local, pas depuis la racine d'un domaine.
    base: "./",
});
