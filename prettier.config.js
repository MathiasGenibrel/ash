/**
 * Prettier tranche le formatage du frontend, comme `cargo fmt` tranche celui du Rust.
 *
 * Ce fichier est en JavaScript et non en `.prettierrc.json` pour une raison unique : les
 * deux valeurs ci-dessous sont des décisions, et le JSON n'accepte pas de commentaire.
 *
 * Tout le reste est laissé aux défauts de Prettier — guillemets doubles, point-virgules,
 * virgules finales partout, parenthèses autour du paramètre d'une lambda : le dépôt les
 * suivait déjà, il n'y avait rien à décider.
 *
 * @type {import("prettier").Config}
 */
export default {
    // Le dépôt indente à quatre espaces des deux côtés de la frontière Tauri —
    // `tsconfig.json`, `vite.config.ts`, `eslint.config.js`, `tauri.conf.json`. Le défaut
    // de Prettier est deux : le poser ici fait du passage un reformatage, pas un
    // changement de convention.
    tabWidth: 4,

    // Le code s'arrête net à cent colonnes : au-delà, il ne reste que 1,3 % des lignes.
    // Le défaut de Prettier (80) recouperait un quart du frontend sans que personne
    // l'ait demandé.
    printWidth: 100,
};
