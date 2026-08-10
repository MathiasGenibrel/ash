# CLAUDE.md

Guide de travail pour Claude Code sur **Ash**.

Ash est une application macOS qui entoure un shell plutôt que de le remplacer : elle
supervise les agents de code lancés dans de vrais PTY, et apporte un git conscient de
ces agents. Voir [`docs/spec.md`](./docs/spec.md) et [`docs/adr/`](./docs/adr/).

**Le squelette est en place, le produit ne l'est pas.** La fenêtre Tauri s'ouvre et les
six commandes de vérification passent ; aucune feature n'existe encore. Le jalon J0 se
termine par le spike xterm.js, qui peut encore invalider [ADR-0002](./docs/adr/0002-tauri-rust-portable-pty.md).

## Stack

- **Type de projet** : application de bureau macOS
- **Coquille** : Tauri v2 ([ADR-0002](./docs/adr/0002-tauri-rust-portable-pty.md))
- **Backend** : Rust — `portable-pty`, `libproc`, socket unix
- **Frontend** : TypeScript + xterm.js, dans la webview système (WKWebView)
- **Gestionnaire de paquets** : **bun** — n'utilise aucun autre gestionnaire dans ce dépôt
- **Tests** : `cargo test` côté Rust, `bun test` côté TypeScript
- **Police du terminal** : JetBrains Mono par défaut

Toolchain en place : **Rust 1.97.1** (`rustup`, avec `clippy` et `rustfmt`), Xcode et
les Command Line Tools. Rien d'autre à installer pour compiler.

Le projet ne se construit **pas** dans Docker, et ça ne changera pas : Tauri se lie ici
à WKWebView et AppKit, qui n'existent que dans le SDK macOS, et la sonde d'
[ADR-0005](./docs/adr/0005-sonde-cwd-libproc.md) utilise `libproc`, absent de Linux. Le
crate ne compilerait même pas dans un conteneur.

## Commandes

```bash
bun install                       # dépendances TypeScript
bun run tauri dev                 # lancer l'app en développement
bun run tauri build               # bundle macOS

bun run lint                      # lint TypeScript
bun run typecheck                 # tsc --noEmit
bun test                          # tests TypeScript

cargo fmt --check                 # format Rust
cargo clippy -- -D warnings       # lint Rust
cargo test                        # tests Rust
```

Cibler un seul test pendant une itération :

```bash
bun test src/features/sidebar/state.test.ts
cargo test -p ash --lib features::probe
```

Les commandes `cargo` se lancent depuis `src-tauri/`, ou avec
`cargo --manifest-path src-tauri/Cargo.toml`.

## Structure

Architecture : **feature folders des deux côtés de la frontière Tauri**, retenue au
démarrage du projet.

```
src-tauri/src/
  main.rs                composition root : assemblage, configuration, démarrage
  features/
    pty/                 PTY et cycle de vie des onglets shell
    probe/               sonde fg_pid + cwd (libproc)          — ADR-0005
    agents/              découverte, états, trait Adapter      — ADR-0006/7/8
      adapters/          claude-code, codex, generic
    git/                 refs, worktrees, graphe, rebase       — ADR-0011/12
    journal/             attribution commit → agent → prompt   — ADR-0014
    hooks/               installation du bloc dans settings.json
  shared/                réellement transverse, et justifié
src/
  app/                   composition root du frontend
  features/
    terminal/            xterm.js, barre d'onglets
    sidebar/             dépôts, worktrees, agents, subagents
    git/                 popup de branches, graphe, merge, fiche
    settings/            la fenêtre de réglages
  shared/
    ipc/                 le contrat Rust ↔ TypeScript
```

Détail et justification : [`.claude/docs/architecture.md`](./.claude/docs/architecture.md).

### Frontières

**Une feature n'importe pas les fichiers internes d'une autre feature.** La
communication passe par son API publique (`mod.rs` côté Rust, `index.ts` côté TS), un
contrat partagé, ou un service injecté depuis la composition root.

```ts
// ✗ import de l'intérieur d'une autre feature
import { parseRebaseState } from "../git/internal/rebase-parser";
// ✓ import de son API publique
import { type RebaseState } from "@/features/git";
```

**La frontière Rust ↔ TypeScript est la plus importante du projet.** Une feature Rust
n'expose au frontend que ses `#[tauri::command]` et ses events, déclarés dans son
`commands.rs`. Le TypeScript ne connaît que ces noms et les types du contrat partagé —
jamais la structure interne du backend.

**Aucun état d'agent ne vit uniquement côté TypeScript.** C'est une conséquence directe
de [ADR-0009](./docs/adr/0009-cycle-de-vie-des-agents.md) : le jour où l'on voudra un
démon `ashd`, la frontière entre la détention des PTY et leur affichage devra déjà être
nette. Le frontend rend un état ; il ne le détient pas.

`shared/` n'est pas un fourre-tout : un module n'y va que s'il sert au moins deux
features **et** ne porte aucune règle propre à l'une d'elles.

## Conventions

**Rust**

- Pas de `unwrap()` ni de `expect()` hors tests et composition root. Les erreurs
  remontent en `Result` avec un type d'erreur par feature.
- Les effets système — PTY, `libproc`, horloge, système de fichiers, git — passent par
  un **trait** que la feature possède. C'est ce qui rend la sonde testable sans lancer
  de processus.
- `cargo fmt` tranche le formatage. `clippy` est en `-D warnings`.

**TypeScript**

- `strict: true`, plus `noUncheckedIndexedAccess` et `exactOptionalPropertyTypes`.
- Alias `@/` → `src/`, à déclarer dans `tsconfig.json` **et** dans la configuration de
  test — en oublier un casse les tests sans message clair.

**Nommage des tests** : `*.test.ts` à côté du code testé côté TypeScript ; module
`#[cfg(test)]` en fin de fichier côté Rust, et `src-tauri/tests/` pour l'intégration.

**Commits** : Conventional Commits, en anglais. Exemple :
`feat(sidebar): bubble waiting state to the workspace row`

## Tests

Les tests protègent des comportements. Aucun test n'est ajouté pour gonfler un
compteur de couverture.

**Structure `Given / When / Then` obligatoire**, et un nom qui décrit un comportement.

```ts
it("Given a collapsed workspace whose agent is waiting, when the row renders, then it shows the waiting state", () => {
    // Given
    const workspace = WorkspaceBuilder.create().collapsed().withAgent("waiting").build();
    // When
    const state = bubbleState(workspace);
    // Then
    expect(state).toBe("waiting");
});
```

```rust
#[test]
fn given_a_worktree_git_file_when_resolving_then_it_finds_the_common_repo() {
    // Given
    let tree = FakeFs::with_worktree("/wt/ash-sidebar", "gitdir: /dev/ash/.git/worktrees/sidebar");
    // When
    let repo = resolve_repo(&tree, Path::new("/wt/ash-sidebar/src"));
    // Then
    assert_eq!(repo.unwrap(), Path::new("/dev/ash/.git"));
}
```

Ce qui mérite un test : les règles d'état des agents, la remontée d'état dans la
sidebar, la résolution worktree/dépôt, le parsing de l'état d'un rebase, les
transitions de la machine à états, les corrections de bugs. Ce qui n'en mérite pas :
getters, DTO, constantes, câblage de Tauri, et tout test dont la seule garantie est
qu'un mock a été appelé.

**Test Data Builders** : à créer dès qu'un objet a plusieurs champs ou un invariant.
Défauts valides et **déterministes**, surcharge des seules propriétés utiles.

**Pas de suite E2E** pour l'instant. Piloter une fenêtre Tauri demande `tauri-driver`,
et le produit n'a pas encore de parcours à protéger. À rediscuter au jalon J2.

Détail : [`.claude/docs/testing.md`](./.claude/docs/testing.md).

## Worktrees, branches et pull requests

**Une tâche = un worktree = une branche.** Chaque tâche est traitée dans
`.claude/worktrees/<ref>` (gitignoré), créé par `/dev` via
`.claude/scripts/worktree.sh setup <ref> <branche>` depuis `origin/main`. Plusieurs
tâches indépendantes avancent ainsi **en parallèle** sans conflit de fichiers.

Le script relie les fichiers non versionnés (`.env`, `.env.local`) ; les dépendances,
elles, ne sont pas partagées : `bun install` se relance dans chaque worktree.

**Le `target/` de Cargo n'est pas partagé non plus, et c'est voulu** : cargo prend un
verrou exclusif sur son dossier de build, donc partager `CARGO_TARGET_DIR` entre
worktrees sérialiserait les compilations — exactement ce que le parallélisme cherchait
à éviter. Le prix est le disque (plusieurs Go par worktree) et une première
compilation longue. `sccache` est la façon de partager le cache sans partager le
verrou ; rien n'est installé.

Aucun agent ne supprime de worktree : `/worktree-clean` le fait une fois la PR fusionnée.

- **Forge** : GitHub (CLI `gh`)
- **Branche de base** : `main`
- **Nommage** : `<type>/<slug>` — ex. `feat/pty-tabs`
- **Créer la PR** : `gh pr create --fill --base main`
- **Lier la tâche** : `Closes #<n>` dans la description

**Tracker** : issues GitHub. Les tâches sont créées et mises à jour à distance après
validation.

## Boucle agentique

| Command | Rôle |
|---|---|
| `/issue` | Transforme un besoin en tâche claire, avec critères d'acceptation vérifiables. Rien n'est créé à distance sans validation |
| `/dev` | Orchestre : `dev-integration` → `dev-architecture` → vérifications → PR → propose `qa` |
| `/dev-with-plan` | Comme `/dev`, mais écrit d'abord un plan dans `tasks/` et attend une validation explicite avant toute ligne de code |
| `/worktree-clean` | Supprime les worktrees dont la PR est fusionnée/fermée et l'arbre propre |

| Agent | Rôle |
|---|---|
| `dev-integration` | Implémente la plus petite tranche verticale cohérente, avec les tests qui ont une valeur |
| `dev-architecture` | Charge le skill `improve-codebase-architecture` et applique les améliorations pertinentes sur la même branche |
| `qa` | Validations coûteuses (build, lancement réel de l'app, parcours) — sur demande |

Les agents n'ont **pas** de mémoire partagée : `/dev` repasse le contexte complet à
chacun. `dev-architecture` s'arrête si
`.claude/skills/improve-codebase-architecture/` est absent ou altéré.

## Ce qui est déjà décidé, et ne se rediscute pas dans une tâche

Les 15 ADR de [`docs/adr/`](./docs/adr/) sont des décisions prises. Une tâche les
applique ; elle ne les renégocie pas. Si une tâche révèle qu'une ADR est fausse, la
conduite est d'écrire l'amendement — pas de coder contre elle en silence.

Les quatre règles qui reviennent le plus souvent :

- **Ash ne valide rien à la place de l'utilisateur.** Il peut rédiger un texte dans un
  terminal, il ne presse jamais `⏎` ([ADR-0015](./docs/adr/0015-ash-compose-l-utilisateur-envoie.md)).
- **Bloc délimité, sauvegarde, jamais silencieux.** Partout où Ash écrit dans un
  fichier de l'utilisateur ([ADR-0007](./docs/adr/0007-etats-par-hooks.md),
  [ADR-0013](./docs/adr/0013-fiche-de-branche-dans-le-depot.md)).
- **Les états d'agent viennent des hooks, jamais d'une analyse de la sortie du PTY**
  ([ADR-0007](./docs/adr/0007-etats-par-hooks.md)).
- **Un onglet porte au plus un PTY, et le panneau bas n'en contient jamais**
  ([ADR-0003](./docs/adr/0003-zone-terminal-unique.md)).
