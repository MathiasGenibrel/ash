# Workflows — Ash

## La boucle

```
/issue                     formuler et valider une tâche
   │
   ▼
/dev  ou  /dev-with-plan
   │
   ├─ (plan écrit dans tasks/ + validation explicite)      ← /dev-with-plan uniquement
   ├─ worktree dédié        une tâche = un worktree = une branche
   ├─ dev-integration       tranche verticale + tests à valeur réelle
   ├─ dev-architecture      skill improve-codebase-architecture → améliorations pertinentes
   ├─ vérifications         cargo fmt/clippy/test · bun lint/typecheck/test
   ├─ pull request          gh pr create
   └─ qa                    proposé, pas lancé
```

Les agents n'ont **pas** de mémoire partagée. `/dev` repasse le contexte complet à
chaque agent : la description de la tâche, les critères d'acceptation, le périmètre, le
chemin du worktree, et pour `dev-architecture` également les changements produits par
`dev-integration`.

## Worktrees — une tâche = un worktree

Chaque tâche est traitée dans un **worktree git dédié**, sur sa propre branche, jamais
dans le dépôt principal (que l'utilisateur garde pour lui). Plusieurs tâches sans
dépendance entre elles avancent donc **en parallèle** : fichiers, branche, dépendances
et artefacts de build isolés.

```bash
WT="$(.claude/scripts/worktree.sh setup 42 feat/pty-tabs)"   # idempotent
.claude/scripts/worktree.sh path 42     # retrouver un worktree existant
.claude/scripts/worktree.sh list        # worktrees + branche + état de la PR
bun install                             # depuis le worktree
```

Convention : dossier `.claude/worktrees/<ref>`, **gitignoré** — invisible de git, et à
exclure des globs de lint, de watch et de test s'ils balaient la racine. `setup` relie
les fichiers non versionnés nécessaires (`.env`, `.env.local`) depuis le dépôt
principal.

### Le coût Rust d'un worktree, et pourquoi on l'accepte

Un worktree neuf recompile tout : `target/` n'est pas partagé, et une première
compilation de Tauri prend plusieurs minutes et plusieurs gigaoctets.

**C'est délibéré.** Cargo prend un verrou exclusif sur son dossier de build : partager
`CARGO_TARGET_DIR` entre worktrees sérialiserait les compilations, ce qui annule
précisément le bénéfice du parallélisme. On échange du disque contre de l'isolation.

Trois conséquences pratiques :

- lance `bun install` **et** une première compilation dès l'ouverture du worktree, pas
  au milieu de la tâche ;
- `sccache` est la façon de partager le cache **sans** partager le verrou. Rien n'est
  installé — c'est une piste, à évaluer quand le temps de build deviendra gênant ;
- `/worktree-clean` régulièrement : chaque worktree abandonné coûte plusieurs Go.

### Cycle de vie

Le worktree naît avec la tâche et **disparaît quand la PR est fusionnée** — via
`/worktree-clean`, jamais par un agent.

```bash
.claude/scripts/worktree.sh clean --dry-run   # simule le nettoyage
/worktree-clean                               # confirme, puis supprime worktree + branche
```

`clean` ne supprime que les worktrees dont la PR est `merged`/`closed` **et** dont
l'arbre est propre ; il ignore les worktrees `agent-*` du harnais Claude Code. Un
worktree conservé signale du travail non intégré : c'est une protection, on ne la
contourne pas.

Les agents ne sortent **jamais** de leur worktree : ni écriture dans le dépôt principal,
ni lecture du travail en cours d'un autre worktree, ni `git checkout` — une branche déjà
sortie dans un worktree ne peut pas l'être ailleurs.

## Commandes de vérification

```bash
bun run verify
```

La liste des vérifications est décidée dans `package.json`, et nulle part ailleurs — la
réécrire ici en ferait une seconde liste, qui divergerait le jour où la première change.

Cibler pendant une itération :

```bash
cargo test --lib features::agents          # unitaires d'une feature, sans l'intégration
bun test src/features/sidebar/state.test.ts
```

`cargo` se lance depuis `src-tauri/`, ou avec `--manifest-path src-tauri/Cargo.toml`.

## Toolchain

**Rust 1.97.1** via `rustup`, avec `clippy` et `rustfmt`. Xcode et les Command Line
Tools fournissent le linker. Rien d'autre n'est nécessaire pour compiler.

`rustup` câble le `PATH` depuis `~/.zshenv`. Si `cargo` est introuvable dans un shell,
c'est que ce shell a été ouvert avant l'installation : `source ~/.cargo/env`, ou
utilise `~/.cargo/bin/cargo`.

Aucun agent n'installe ni ne met à jour une toolchain de sa propre initiative — ni
`rustup update`, ni une toolchain nightly, ni une target supplémentaire. Ce sont des
changements qui affectent toutes les tâches en parallèle, pas seulement la sienne.

### Pas de Docker

Ash ne se construit pas dans un conteneur, et ce n'est pas un manque d'outillage :

- Tauri se lie sur macOS à WKWebView (`WebKit.framework`) et AppKit, qui n'existent que
  dans le SDK macOS. Docker Desktop sur Mac est une VM Linux ; cross-compiler vers macOS
  demanderait `osxcross` et un SDK Apple dont la licence restreint la redistribution.
- La sonde d'[ADR-0005](../../docs/adr/0005-sonde-cwd-libproc.md) utilise `libproc`
  (`proc_pidinfo`) et `tcgetpgrp` : absents de Linux. Le crate ne compilerait pas, donc
  `cargo test` échouerait aussi, pas seulement `cargo build`.
- Les vérifications qui comptent observent un vrai PTY, un vrai dépôt git et une vraie
  fenêtre. Une VM Linux sans serveur graphique ne peut rien en dire.

Si une tâche propose « on n'a qu'à conteneuriser », la réponse est non, avec ces
raisons.

## Tâches

Tracker : **issues GitHub**.

```bash
gh issue view <n>                      # lire une tâche
gh issue create --title … --body …     # créer — après validation uniquement
```

Référence des tâches : `#<n>`. `/issue` rédige et fait valider **avant** toute création
distante ; il ne crée jamais une issue sans accord explicite.

`tasks/` contient les plans produits par `/dev-with-plan` (`tasks/plan-<slug>.md`) et,
le cas échéant, les propositions de tâche non encore créées.

## Branches et pull requests

- Forge : **GitHub**, CLI `gh`
- Base : `main`
- Motif de branche : `<type>/<slug>` — ex. `feat/pty-tabs`, une branche par worktree

```bash
gh pr create --fill --base main
```

Titre : Conventional Commits, en anglais. Corps : ce qui a été fait, comment le
vérifier, et `Closes #<n>` pour lier l'issue.

## Commits

Conventional Commits, en anglais, avec le nom de la feature en portée. Exemple :
`feat(sidebar): bubble waiting state to the workspace row`

## QA

L'agent `qa` exécute les validations coûteuses : `bun run package:debug`, lancement réel
de l'application, parcours touché par la tâche.

Il construit et observe **Ash-dev** — nom distinct, icône aux couleurs inversées,
identifiant `com.mg-studio.ash.dev` —, jamais l'`Ash` installé qui te sert de terminal
quotidien. Voir « Ash et Ash-dev sont deux applications » dans `CLAUDE.md`.

Mode : **sur demande**. `/dev` le **propose** en fin de tâche, et tu décides. C'est le
mode retenu parce qu'un build Tauri complet coûte plusieurs minutes et ne détecte rien
sur une tâche qui ne touche pas l'interface.

## Ce que les agents ne font pas

- sortir du worktree de leur tâche, ou supprimer un worktree dont le travail n'est pas
  fusionné
- installer une toolchain (Rust, `tauri-cli`) ou une dépendance sans demande explicite
- renégocier une ADR en silence — une décision fausse s'amende par écrit
- refactorer hors du périmètre de la tâche
- ajouter un pattern sans variation réelle, frontière métier ou besoin de substitution
- ajouter un test sans risque ou comportement identifiable
- lancer un vrai `claude` ou un vrai agent dans un test
- pousser ou publier quoi que ce soit sans validation
