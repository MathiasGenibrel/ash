# Ash

Un terminal macOS qui montre ce que font tes agents.

Ash n'est pas un client d'IA et ne remplace pas ton shell : c'est une coquille
autour de lui. Tu lances `claude`, `codex`, `kimi` comme d'habitude, dans un vrai
bash. Ash s'occupe du reste — regrouper tes onglets par dépôt et par worktree, te dire
en permanence qui travaille, **qui attend une réponse**, et qui a fini, et te donner
le git qui va avec : quel agent a écrit quel commit, et qui travaille dans le worktree
que tu t'apprêtes à bousculer.

## Lancer le projet en développement

Ash ne se construit **que sur macOS**, et ça ne changera pas : Tauri se lie ici à
WKWebView et AppKit, et la sonde de `cwd` utilise `libproc`. Le crate ne compilerait
même pas dans un conteneur — inutile de chercher un `Dockerfile`.

Il faut trois choses installées :

| | Pourquoi |
|---|---|
| [Xcode Command Line Tools](https://developer.apple.com/xcode/) — `xcode-select --install` | le SDK macOS que Tauri lie |
| [Rust ≥ 1.97](https://rustup.rs) via `rustup` | le backend |
| [bun](https://bun.sh) | **le seul** gestionnaire de paquets de ce dépôt — n'utilise ni `npm`, ni `pnpm`, ni `yarn` |

Ensuite, deux commandes, **dans un vrai terminal** :

```bash
bun install
bun run tauri dev
```

La première installe les dépendances TypeScript ; la seconde compile le backend,
démarre Vite et ouvre la fenêtre. Elle **occupe le terminal** tant que l'application
tourne : ne la lance pas depuis un agent ou un outil qui attend la fin de la commande.

La première compilation Rust prend plusieurs minutes et quelques gigaoctets ; les
suivantes sont incrémentales. `bun run tauri dev` s'occupe seul du serveur Vite
(port 1420) — ne le lance pas à côté.

Le rechargement à chaud n'est pas symétrique, et c'est la surprise habituelle :
une modification **TypeScript ou CSS** apparaît immédiatement dans la fenêtre, une
modification **Rust** relance une compilation et redémarre l'application, en fermant
les onglets ouverts.

### Vérifier son travail

Six commandes, celles que la CI et les agents lancent :

```bash
bun run lint
bun run typecheck
bun test
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

Les trois premières couvrent le TypeScript, les trois dernières le Rust. Les commandes
`cargo` se lancent depuis `src-tauri/`, ou avec
`cargo --manifest-path src-tauri/Cargo.toml`.

Pour produire un bundle `.app` :

```bash
bun run tauri build
```

### Si `cargo` est introuvable

L'erreur ressemble à ceci, et vient toujours du `PATH` :

```
failed to run 'cargo metadata' command to get workspace directory:
No such file or directory (os error 2)
```

`rustup` ajoute `~/.cargo/bin` au `PATH` via un fichier de profil. Un shell ouvert
avant l'installation, ou un shell non interactif qui ne lit pas ce fichier — celui
d'un éditeur, d'un agent, d'un `Makefile` — ne l'aura pas. Sourcer suffit :

```bash
source ~/.cargo/env
```

## Documentation

- [Spécification](./docs/spec.md) — le produit, le modèle, l'interface, les jalons
- [Décisions d'architecture](./docs/adr/) — ce qui a été tranché, et ce que ça coûte
- [Briefs de design](./docs/design/) — ce qui a été demandé au design

## État

Cadrage terminé (2026-08-07), direction visuelle livrée et revue (2026-08-10).

La revue du design a ajouté un domaine entier — git — et donc cinq ADR (0011 à 0015)
et un jalon J5. Trois ADR ont été amendées, une reformulée. La spec est à jour.

**J0 est terminé.** Le squelette Tauri tient, et le spike xterm.js a levé le risque de
performance sous WKWebView — au passage, il a montré qu'une écriture sans contrôle de
flux perd des données au-delà de 50 Mo en attente. C'est devenu une contrainte de
conception du PTY, pas une note de bas de page. Voir
[l'amendement d'ADR-0002](./docs/adr/0002-tauri-rust-portable-pty.md).

**J1 est en cours** — PTY, onglets, raccourcis, sidebar par dépôt et worktree. Aucun
état d'agent : l'objectif est qu'Ash remplace le terminal quotidien avant qu'on
investisse dans les hooks.

| | |
|---|---|
| Un vrai shell dans un onglet | fait |
| Onglets multiples, raccourcis, menu natif | fait |
| Sonde `cwd` et processus en avant-plan | fait |
| Résolution `cwd` → worktree → dépôt | fait |
| Sidebar groupée par worktree et par dépôt | en revue |
| Métadonnées git du worktree, ligne de statut et thème | à faire |
