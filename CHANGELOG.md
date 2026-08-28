# Changelog

Toutes les modifications notables d'Ash sont consignées dans ce fichier.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), et le projet suit
le [versionnage sémantique](https://semver.org/lang/fr/).

Le numéro de version est décidé dans `src-tauri/Cargo.toml`, et nulle part ailleurs :
`package.json` le répète, et `scripts/release/version.ts --check vX.Y.Z` refuse un tag qui
ne concorderait pas avec les deux.

## [Non publié]

### Outillage

- Pousser un tag `vX.Y.Z` produit une release GitHub : `.github/workflows/release.yml`
  enchaîne `preflight → verify → build → publish`, et rien d'autre ne le déclenche. La
  release naît en brouillon et n'est rendue visible qu'en dernier geste, une fois l'archive
  attachée.
- `scripts/release/artifact.ts` décide seul le nom de l'archive (`Ash-X.Y.Z-macos-arm64.zip`),
  la cible construite et les chemins du bundle : le workflow les demande, il ne les recompose
  pas.

## [0.1.0] - 2026-08-28

Première version numérotée. Ash remplace déjà un terminal quotidien : ce qui suit décrit ce
qu'il fait aujourd'hui, pas ce qui a changé depuis une version précédente — il n'y en a pas.

### Ajouté

- Des shells dans de vrais PTY, avec leurs onglets, leur menu natif macOS et des raccourcis
  réglables.
- Une sonde qui suit le `cwd` et le processus en avant-plan de chaque onglet, et une sidebar
  qui groupe les onglets par worktree et les worktrees par dépôt.
- Les cinq états d'agent — `idle`, `working`, `waiting`, `done`, `error` — produits par la
  sonde et par les hooks des outils, avec les lignes filles des sous-agents.
- Des notifications macOS pour `waiting` et `error` quand Ash n'est pas au premier plan, dont
  le clic ramène sur l'agent concerné.
- Un git conscient des agents : métadonnées lues par surveillance de `.git`, popup de
  branches, graphe, tableau des worktrees, vue des conflits, onglet de merge, fiche de branche
  et journal d'attribution commit → agent → prompt.
- Les quotas de session et hebdomadaire de Claude Code, lus sur `api.anthropic.com` et
  coupables depuis les réglages.
- Une fenêtre de réglages : thème, police et densité, outils et hooks, notifications, usage.
- Une barre de statut composable, et l'ouverture de ce qu'un terminal imprime (URL, chemins).

### Outillage

- `scripts/release/version.ts --check vX.Y.Z` vérifie qu'un tag concorde avec
  `src-tauri/Cargo.toml` et `package.json`, et nomme le fichier fautif sinon.
- `scripts/release/release-notes.ts X.Y.Z` — ou `vX.Y.Z`, la forme d'une version étant décidée
  dans `version.ts` et demandée par les autres — écrit sur la sortie standard le corps de la
  section correspondante de ce fichier, et échoue si elle est absente ou vide.
