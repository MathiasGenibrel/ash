# Ash

Un terminal macOS qui montre ce que font tes agents.

Ash n'est pas un client d'IA et ne remplace pas ton shell : c'est une coquille
autour de lui. Tu lances `claude`, `codex`, `kimi` comme d'habitude, dans un vrai
bash. Ash s'occupe du reste — regrouper tes onglets par dépôt et par worktree, te dire
en permanence qui travaille, **qui attend une réponse**, et qui a fini, et te donner
le git qui va avec : quel agent a écrit quel commit, et qui travaille dans le worktree
que tu t'apprêtes à bousculer.

## Installer Ash

Chaque tag `vX.Y.Z` publie une release dans ce dépôt, avec une archive
`Ash-X.Y.Z-macos-arm64.zip` — `Ash.app`, pour Mac Apple Silicon. Il n'y a **aucun mécanisme
de mise à jour** : on retélécharge.

Le plus simple est de la récupérer en ligne de commande, et ce n'est pas une coquetterie :

```bash
gh release download vX.Y.Z --repo MathiasGenibrel/ash --pattern '*.zip'
unzip Ash-X.Y.Z-macos-arm64.zip -d /Applications
```

**Pourquoi pas le navigateur.** macOS pose l'attribut étendu `com.apple.quarantine` sur les
fichiers téléchargés, mais ce n'est pas le transfert qui le pose : c'est LaunchServices, pour
le compte de l'application qui télécharge — un navigateur, un client de messagerie, AirDrop.
`curl`, `gh` ou `scp` ne le demandent pas, donc le fichier ne le porte jamais. La même archive
donne donc « Ash est endommagé et ne peut pas être ouvert » d'un côté, et rien du tout de
l'autre : c'est un symptôme du **canal**, pas d'un défaut de l'archive.

Si l'archive est déjà passée par un navigateur, l'attribut se retire :

```bash
xattr -d com.apple.quarantine /Applications/Ash.app
```

**Ash n'est pas signé Developer ID** aujourd'hui, et n'est pas notarisé. C'est ce qui rend la
quarantaine fatale plutôt que simplement bavarde, et c'est assumé pour l'instant.

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
bun run app
```

La première installe les dépendances TypeScript ; la seconde compile le backend,
démarre Vite et ouvre la fenêtre. Elle **occupe le terminal** tant que l'application
tourne : ne la lance pas depuis un agent ou un outil qui attend la fin de la commande.

La première compilation Rust prend plusieurs minutes et quelques gigaoctets ; les
suivantes sont incrémentales. `bun run app` s'occupe seul du serveur Vite
(port 1420) — ne le lance pas à côté.

Le rechargement à chaud n'est pas symétrique, et c'est la surprise habituelle :
une modification **TypeScript ou CSS** apparaît immédiatement dans la fenêtre, une
modification **Rust** relance une compilation et redémarre l'application, en fermant
les onglets ouverts.

## Toutes les commandes

Tout passe par `bun run`, y compris le Rust : les scripts savent depuis où lancer
`cargo`, et ça évite un piège décrit plus bas.

| Commande | Ce qu'elle fait |
|---|---|
| `bun install` | dépendances TypeScript |
| `bun run app` | lance Ash en développement — compile le backend, démarre Vite, ouvre la fenêtre |
| `bun run package` | produit `Ash.app` en release |
| `bun run package:debug` | idem, en debug — plus rapide à compiler, plus lent à l'exécution |
| `bun run verify` | toutes les vérifications du dépôt, TypeScript **et** Rust |
| `bun run verify:full` | les mêmes, plus le smoke |
| `bun run lint` · `typecheck` · `test` | TypeScript, une par une |
| `bun run rust:fmt` · `rust:lint` · `rust:test` | Rust, une par une |
| `bun run smoke` | lance réellement l'application et vérifie qu'elle survit |

**Ne renomme ni `dev` ni `build`.** Ce sont les deux scripts que Tauri appelle
lui-même — `beforeDevCommand` et `beforeBuildCommand` dans `src-tauri/tauri.conf.json`.
`build` ne construit que la partie web (`tsc --noEmit && vite build`) ; il ne produit
aucune application macOS. C'est `bun run package` qui empaquette.

### Produire un paquet

```bash
bun run package
```

Elle enchaîne le build web, compile les deux binaires Rust en release — `ash` et
`ash-event` — et empaquette le tout dans :

```
src-tauri/target/release/bundle/macos/Ash.app
```

**Les deux binaires comptent.** `ash-event` est celui que les hooks des agents appellent,
par **chemin absolu** : c'est lui qui rapporte à Ash qu'un agent attend ou a fini. Un
`Ash.app` déplacé après l'installation des hooks les casse en silence — réinstalle-les
depuis les réglages, le nouveau chemin remplacera l'ancien.

**Les notifications macOS n'existent que dans un paquet.** L'API native refuse de
fonctionner hors d'un bundle, au point de terminer le processus ; un garde la
court-circuite donc en développement. Si tu veux voir une bannière, il faut passer par
`bun run package`, pas par `bun run app`.

### Vérifier son travail

```bash
bun run verify
```

Le formatage, le lint, les types et les tests, des deux côtés de la frontière. La liste
exacte est décidée dans `package.json`, et nulle part ailleurs : la réécrire ici en ferait
une seconde liste, qui divergerait. La commande s'arrête à la première qui échoue.

Une vérification reste dehors, à lancer **dès qu'on touche à l'assemblage de l'application** —
`lib.rs`, `menu.rs`, un `commands.rs` :

```bash
bun run smoke
```

Elle compile, lance réellement Ash, et vérifie qu'il survit à son démarrage et qu'il a
ouvert son shell. Une fenêtre apparaît quelques secondes. Tout le reste peut être
vertes pendant que l'application ne s'ouvre pas — c'est arrivé, et c'est pour ça qu'elle
existe.

### Le piège des commandes `cargo` lancées à la main

`bun run rust:test` fait un `cd src-tauri` avant d'appeler `cargo`, et ce n'est pas une
coquetterie.

`cargo` cherche son fichier de configuration en remontant depuis le **répertoire
courant**, pas depuis le manifeste. `src-tauri/.cargo/config.toml` y pose
`TS_RS_EXPORT_DIR`, qui dit où déposer les types TypeScript tirés du Rust. Lancé depuis
la racine du dépôt avec `--manifest-path`, `cargo test` ne lit pas ce fichier et écrit
les types **au mauvais endroit, en silence**.

`fmt` et `clippy` ne lisent pas cette variable : eux peuvent se lancer depuis la racine,
et les scripts le font.

Si tu lances `cargo` toi-même, fais-le donc depuis `src-tauri/`.

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
