# CLAUDE.md

Guide de travail pour Claude Code sur **Ash**.

Ash est une application macOS qui entoure un shell plutôt que de le remplacer : elle
supervise les agents de code lancés dans de vrais PTY, et apporte un git conscient de
ces agents. Voir [`docs/spec.md`](./docs/spec.md) et [`docs/adr/`](./docs/adr/).

**J0 et J1 sont terminés : Ash peut déjà remplacer un terminal quotidien.** De vrais
shells dans des onglets avec leurs raccourcis et un menu natif, une sonde qui suit le
`cwd` et le processus en avant-plan, la résolution d'un `cwd` vers son worktree et son
dépôt, une sidebar qui groupe les onglets par worktree et les worktrees par dépôt, les
métadonnées git lues par surveillance de fichiers, une ligne de statut, et trois thèmes.
Le spike xterm.js a levé le risque de [ADR-0002](./docs/adr/0002-tauri-rust-portable-pty.md)
— voir son amendement.

**Les cinq états ont désormais tous un producteur.** `idle` et `working` viennent de la
sonde ; `waiting`, `done` et `error` viennent des **hooks**
([ADR-0007](./docs/adr/0007-etats-par-hooks.md)), traduits par l'adaptateur de l'outil puis
arbitrés par une machine à états — une par onglet, tenue par `features/agents`, que
`features/pty` consulte par son port `AgentStates`. Un état ne se déduit **jamais** de la
sortie du PTY, et `waiting` n'a **jamais** d'autre source qu'un hook. N'invente aucune
source d'état.

**La notification macOS existe, et son clic ramène sur l'agent** (spec §8) : `waiting` et
`error` posent une bannière quand Ash n'est pas au premier plan, sur le **changement** d'état
et jamais sur sa lecture, et `done` ne notifie pas. Elle passe par le port `Notifier`
d'`agents`, que le composition root branche sur `features/notifications` —
`UNUserNotificationCenter`, le **second module `unsafe` du crate** après la sonde. La
bannière emporte l'identifiant de son onglet, macOS le rend au clic par un délégué
asynchrone, et le backend émet `ash://select-tab` : aucun fil n'attend jamais un geste de
l'utilisateur, et rien ne sélectionne sans lui.

**Rien de tout cela ne fonctionne en `bun run tauri dev`**, et c'est irréductible :
`UNUserNotificationCenter` exige une application empaquetée et *tue* le processus qui le
demande sans l'être. Un garde le franchit avant tout appel, la fenêtre de réglages dit alors
« macOS ne nous le dit pas », et la fonctionnalité ne se vérifie que sur
`bun run tauri build`. `tauri-plugin-notification` a été **retiré** : son
`permission_state()` de bureau rendait `Granted` en dur, et deux couches se disputant le
délégué global au processus est une panne silencieuse.

**Les sous-agents ont leurs lignes filles** (spec §6.5) : sous une ligne d'agent, une ligne
par sous-agent en cours — son libellé, son état, sa durée —, inerte, un clic sélectionnant le
parent (ADR-0003 : un onglet porte au plus un PTY). Elles viennent du **sixième hook**,
`SubagentStop`, qui écrit un verbe qui n'est **pas** un état : le cycle de vie des enfants
passe par `Adapter::child_event`, distincte d'`interpret`, et la suite contractuelle vérifie
qu'aucun événement d'enfant n'atteint l'état de l'onglet. Un sous-agent n'est jamais
`waiting`, et son échec n'a aucune source — c'est un angle mort documenté dans
`agents/subagents.rs`. Une ligne fille finie reste visible dix secondes ; la durée est un
réglage, injecté au superviseur, que la fenêtre de réglages ne porte pas encore.

**Les agents sont désormais reconnus, pas déclarés** (ADR-0006) : la sonde rend le chemin de
l'exécutable, son nom et son `argv[0]`, et `features/agents/providers.rs` les compare — dans
cet ordre, du plus fiable au moins fiable — à une table embarquée. Le chemin passe avant le
nom parce que l'installateur officiel de Claude Code pose un binaire nommé d'après sa version
(`~/.local/share/claude/versions/2.1.234`) : une table de noms ne matcherait jamais
l'installation la plus courante. `settings` concilie la table avec les entrées déclarées à la
main, qui **l'emportent**, et dit si la configuration de l'outil porte le marqueur
`# ash:hook v`. Reconnaître est de la **lecture** : aucun fichier écrit, aucune autorisation
macOS, aucun scan de disque. Un agent reconnu mais non instrumenté porte un marqueur discret
dans la sidebar, dont le geste ouvre les réglages sur cet outil — la sidebar informe, l'écran
agit (ADR-0010), et rien ne s'écrit sans un geste explicite.

Ce qui reste à faire du côté des agents : la remontée d'état dans la sidebar, et le
branchement de cette reconnaissance sur la machine à états — c'est elle qui donnera enfin son
producteur à `AgentEvent::AgentStarted` (voir `agents/supervisor.rs`).

**L'entrée dans un état est datée, et la ligne de statut affiche sa durée** (`working ·
15m22s`). Ce qui traverse la frontière est une **date absolue** — `TabInfo.stateSince`, en
millisecondes depuis l'époque Unix — envoyée une seule fois, au changement d'état : le
`TabInfo` est comparé entier pour décider s'il faut émettre, donc une durée vivante ferait
partir `ash://tab-changed` chaque seconde pour chaque onglet actif. Le compteur qui
s'incrémente est un fait d'affichage, et il le reste.

`ash-event` lit **l'entrée standard** que tout hook lui donne, et en tire `agent_id` /
`agent_type` (ADR-0007, amendement du 2026-08-13) — les deux clés que les lignes filles
consomment, bornées à 256 octets pour qu'une clé démesurée ne fasse jamais partir une trame
sans son enfant. Il n'attend jamais cette entrée — rien de ce qui s'y passe
ne peut retenir un hook, donc bloquer un agent.

## Stack

- **Type de projet** : application de bureau macOS
- **Coquille** : Tauri v2 ([ADR-0002](./docs/adr/0002-tauri-rust-portable-pty.md))
- **Backend** : Rust — `portable-pty`, `libc` (la sonde), `notify` (la surveillance de
  `.git`), `objc2` + `objc2-user-notifications` (les bannières). Le socket unix et le
  binaire `ash-event`
  ([ADR-0007](./docs/adr/0007-etats-par-hooks.md)) n'existent pas encore : c'est J2
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
bun run app                       # lancer l'app en développement  → Ash-dev
bun run package:debug             # bundle macOS de développement  → Ash-dev.app
bun run package                   # bundle macOS installable       → Ash.app

bun run lint                      # lint TypeScript
bun run typecheck                 # tsc --noEmit
bun test                          # tests TypeScript

cargo fmt --check                 # format Rust
cargo clippy -- -D warnings       # lint Rust
cargo test                        # tests Rust

bun run smoke                     # l'application s'ouvre-t-elle vraiment ?
```

**La septième est obligatoire dès qu'une tâche touche `lib.rs`, `menu.rs` ou un
`commands.rs`.** Les six autres ont toutes été vertes le jour où Ash ne démarrait plus du
tout — un `state()` appelé avant son `manage()`, qui ne panique qu'au lancement. Le
composition root n'a pas de test unitaire, et il n'en aura pas : assembler une
application Tauri en demande une vraie.

`bun run smoke` compile, démarre Vite au besoin, lance le binaire, et vérifie qu'il
survit à son démarrage **et** qu'il a lancé son shell. Il ouvre une fenêtre pendant
quelques secondes — c'est le prix, `run()` crée la fenêtre et c'est là que les pannes de
câblage sortent. Il ne remplace pas l'agent `qa` : il ne regarde rien, il ne clique nulle
part.

### Ash et Ash-dev sont deux applications

**Ash est le terminal quotidien de son auteur, et il tourne pendant qu'on le développe.**
Une compilation de développement qui porterait le même nom et la même icône ferait taper
une commande dans la mauvaise fenêtre, attribuer un bug au mauvais binaire, et rendre à
l'agent `qa` un verdict sur une application qu'il n'a pas construite. Les deux identités
sont donc distinctes, et ça ne se rediscute pas dans une tâche :

| | Installé | Développement |
|---|---|---|
| Nom, fenêtre, menu | **Ash** | **Ash-dev** |
| Icône | celle du dépôt | la **même aux couleurs inversées** (fond clair) |
| Identifiant de paquet | `com.mg-studio.ash` | `com.mg-studio.ash.dev` |
| Construit par | `bun run package` | `bun run app`, `bun run package:debug` |

Ce qui les sépare tient en deux fils, et un seul interrupteur — `debug_assertions`, que
`tauri build --debug` laisse allumé et que `tauri build` éteint :

- `src-tauri/tauri.dev.conf.json` porte le nom du paquet, son identifiant et son icône.
  C'est une configuration **surchargée**, passée par `--config` dans les scripts `app` et
  `package:debug` de `package.json`. Elle ne surcharge que des valeurs scalaires : y
  redéclarer `app.windows` remplacerait le tableau entier, donc aussi la taille de la
  fenêtre et son style de barre de titre.
- `APP_NAME`, dans `src-tauri/src/lib.rs`, porte le nom **affiché** — le menu applicatif
  et le titre de la fenêtre. C'est la seule source de ce nom côté code.

**N'utilise plus `bun run tauri dev` ni `bun run tauri build --debug` :** ils
court-circuitent le `--config`, et rendent une application nommée `Ash` qui écrase
l'installée dans le Dock, le centre de notifications et LaunchServices. Passe toujours
par les scripts.

L'identifiant distinct a un effet de bord voulu : Ash-dev a ses propres autorisations de
notification et son propre stockage — développer ne touche donc pas aux réglages de l'Ash
installé.

**Ce que le nom ne sépare pas encore : les hooks.** Le bloc écrit dans un `settings.json`
nomme le `ash-event` **d'à côté de l'application qui l'a posé**, et les deux builds
portent le même marqueur `# ash:hook v`. Installer les hooks depuis Ash-dev sur
`~/.claude` remplace donc ceux de l'Ash installé, qui n'a alors plus d'état d'agent
jusqu'à réinstallation. Tant qu'un marqueur par build n'existe pas, on n'instrumente
depuis Ash-dev qu'un dossier de configuration jetable (`CLAUDE_CONFIG_DIR`), jamais celui
de l'utilisateur.

Deux points restent hors de portée, et il faut les savoir plutôt que s'en étonner :
`bun run app` produit un binaire **non empaqueté**, donc le Dock l'affiche sous le nom
`ash` avec une icône générique — c'est le menu applicatif et le titre de la fenêtre qui
disent `Ash-dev` — et les bannières macOS n'y fonctionnent pas du tout (voir plus haut).
La seule façon de voir l'icône inversée et de tester les notifications est
`bun run package:debug`.

Cibler un seul test pendant une itération :

```bash
bun test src/features/sidebar/tree.test.ts
cargo test -p ash --lib features::probe
```

Les commandes `cargo` se lancent depuis `src-tauri/`, ou avec
`cargo --manifest-path src-tauri/Cargo.toml`.

**`cargo` n'est pas dans le `PATH` de tous les shells.** `rustup` l'ajoute via un fichier
de profil que les shells non interactifs — celui d'un agent, d'un éditeur, d'un
`Makefile` — ne lisent pas toujours. Le symptôme est un `No such file or directory
(os error 2)`, y compris via `bun run tauri dev`, qui lance `cargo metadata`. Le remède
tient en une ligne, à mettre en tête de session :

```bash
source ~/.cargo/env
```

## Structure

Architecture : **feature folders des deux côtés de la frontière Tauri**, retenue au
démarrage du projet.

`✓` existe, le reste est la cible. **Ne crée pas un dossier prévu tant qu'une tâche ne le
demande pas** : la cible dit où les choses iront, pas où elles sont.

```
src-tauri/src/
  main.rs                composition root : assemblage, configuration, démarrage
  menu.rs              ✓ menu natif macOS et routage de ses actions
  features/
    pty/               ✓ PTY, onglets, boucle de sonde 300 ms       — ADR-0003
    probe/             ✓ sonde fg_pid + cwd (libc)                  — ADR-0005
    notifications/     ✓ bannières macOS, autorisation, clic
                         (UNUserNotificationCenter)                 — spec §8
    git/               ✓ résolution worktree/dépôt, surveillance de
                         `.git`, métadonnées                        — ADR-0011/12
    theme/             ✓ clair / sombre / système, persisté
    agents/              découverte, états, trait Adapter           — ADR-0006/7/8
      adapters/          claude-code, codex, generic
    journal/             attribution commit → agent → prompt        — ADR-0014
    hooks/               installation du bloc dans settings.json
  shared/                réellement transverse, et justifié
src/
  app/                 ✓ composition root, tokens des thèmes, menu
  features/
    terminal/          ✓ xterm.js, barre d'onglets, ligne de statut
    sidebar/           ✓ dépôts, worktrees, onglets
    git/                 popup de branches, graphe, merge, fiche
    settings/            la fenêtre de réglages
  shared/
    ipc/               ✓ le contrat Rust ↔ TypeScript, et ses builders
    agent-state/       ✓ présentation des cinq états, partagée par
                         la sidebar et la ligne de statut
```

`features/git/` est déjà la plus grosse : elle porte la résolution, la surveillance de
fichiers, le parsing des fichiers de contrôle et **le seul appel à `git`** du dépôt. Cet
appel est encadré par une frontière de sécurité documentée dans `git_cli.rs` — Ash lance
`git status` sur un simple `cd`, donc visiter un dépôt hostile ne doit rien exécuter. Si
tu ajoutes un verbe git, repose la question pour lui.

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

Les deux exemples ci-dessous sont tirés du code réel — copie leur forme, et les vrais
noms.

```ts
it("Given a collapsed worktree whose agent is waiting, when its row bubbles a state, then waiting wins", () => {
    // Given
    const states: AgentState[] = ["idle", "waiting", "working"];
    // When
    const bubbled = bubbleState(states);
    // Then
    expect(bubbled).toBe("waiting");
});
```

```rust
#[test]
fn given_a_worktree_git_file_when_resolving_then_it_finds_the_worktree_and_the_common_repo() {
    // Given
    let tree = FakeFs::new()
        .plain_repo("/dev/ash")
        .dir("/dev/ash/.git/worktrees/sidebar")
        .file("/wt/ash-sidebar/.git", "gitdir: /dev/ash/.git/worktrees/sidebar");
    // When
    let located = resolve_worktree(&tree, Path::new("/wt/ash-sidebar/src"));
    // Then
    let repo = located.unwrap().repo.unwrap();
    assert_eq!(repo.git_dir, Path::new("/dev/ash/.git"));
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
- **Ash n'écrit que ce qui lui appartient, et sait le reconnaître. Sauvegarde, jamais
  silencieux.** Partout où Ash écrit dans un fichier de l'utilisateur
  ([ADR-0007](./docs/adr/0007-etats-par-hooks.md),
  [ADR-0013](./docs/adr/0013-fiche-de-branche-dans-le-depot.md)). La forme dépend du
  fichier : **entrées marquées** dans un `settings.json`, dont chacune se reconnaît
  seule et cohabite avec celles de l'utilisateur ; **bloc délimité** dans un `.md`, où
  il n'y a rien à entrelacer.
- **Les états d'agent viennent des hooks, jamais d'une analyse de la sortie du PTY**
  ([ADR-0007](./docs/adr/0007-etats-par-hooks.md)).
- **Un onglet porte au plus un PTY, et le panneau bas n'en contient jamais**
  ([ADR-0003](./docs/adr/0003-zone-terminal-unique.md)).
