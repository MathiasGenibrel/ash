# Architecture — Ash

Style : **feature folders des deux côtés de la frontière Tauri**, retenu au démarrage
du projet.

Le code n'existe pas encore. Ce document décrit l'organisation à tenir dès la première
tâche, pas un état observé. Il est la référence de `dev-integration` et
`dev-architecture`.

## Organisation

```
src-tauri/src/
  main.rs                composition root : assemblage, configuration, démarrage
  features/
    pty/                 PTY et cycle de vie des onglets shell
    probe/               sonde fg_pid + cwd (libproc)             — ADR-0005
    notifications/       bannières macOS, autorisation, clic
                         (UNUserNotificationCenter)               — spec §8
    agents/              découverte, machine à états, trait Adapter — ADR-0006/7/8
      adapters/          claude-code, codex, generic
    git/                 refs, worktrees, graphe, état de rebase   — ADR-0011/12
    journal/             attribution commit → agent → prompt       — ADR-0014
    hooks/               les entrées marquées d'Ash dans les settings.json, leur pose
                         et leur retrait — à l'octet près              — spec §10
    theme/               l'apparence de la fenêtre — mode clair / sombre /
                         système, taille de police du terminal — et sa persistance
    sidebar/             ce qui survit à la fermeture : worktrees épinglés et
                         lignes repliées (`~/.ash/state.json`)      — spec §3.1/5.2
  shared/                réellement transverse, et justifié
src/
  app/                   composition root du frontend
  features/
    terminal/            xterm.js, pile de terminaux, ligne de statut
    sidebar/             dépôts, worktrees, agents, subagents
    git/                 popup de branches, graphe, merge, fiche
    settings/            la fenêtre de réglages
  shared/
    ipc/                 le contrat Rust ↔ TypeScript
    agent-state/         la présentation des cinq états — sidebar et ligne de statut
```

À l'intérieur d'une feature :

| Fichier / dossier | Rôle |
|---|---|
| `mod.rs` / `index.ts` | API publique de la feature — la seule surface importable de l'extérieur |
| `domain.rs` / `model.ts` | règles, invariants, machine à états — sans dépendance technique |
| `commands.rs` | les `#[tauri::command]` exposés au frontend, et rien d'autre |
| `ports.rs` | les traits que la feature exige de son environnement |
| `<adapter>.rs` | les implémentations concrètes de ces traits |

**Ne crée pas de dossier vide pour respecter la forme.** Une feature qui n'a pas de
port n'a pas de `ports.rs` ; une feature de trois fichiers reste trois fichiers.

## Le découpage suit les ADR, pas une taxonomie technique

Chaque feature correspond à une décision tracée. C'est ce qui rend le code navigable
pour un agent : la question « où vit la résolution du workspace ? » se répond en lisant
[ADR-0004](../../docs/adr/0004-workspace-racine-git.md), et la réponse est un dossier.

| Feature | ADR | Ce qu'elle possède |
|---|---|---|
| `pty` | [0001](../../docs/adr/0001-application-graphique-avec-pty-embarques.md), [0002](../../docs/adr/0002-tauri-rust-portable-pty.md) | la création d'un bash, son environnement (`ASH_TAB_ID`, `ASH_SOCK`), son cycle de vie |
| `probe` | [0005](../../docs/adr/0005-sonde-cwd-libproc.md) | `tcgetpgrp`, `proc_pidinfo`, et **rien** qui interprète un état |
| `agents` | [0006](../../docs/adr/0006-decouverte-automatique-des-agents.md), [0007](../../docs/adr/0007-etats-par-hooks.md), [0008](../../docs/adr/0008-abstraction-adapter.md) | le vocabulaire commun `idle/working/waiting/done/error`, le trait `Adapter`, le socket d'events, et **la décision** — une machine à états par onglet, que `pty` consulte par son port `AgentStates` |
| `git` | [0011](../../docs/adr/0011-git-domaine-de-premier-plan.md), [0012](../../docs/adr/0012-worktree-unite-de-travail.md) | refs, worktrees, dépôt commun, état de rebase, couloirs du graphe |
| `journal` | [0014](../../docs/adr/0014-attribution-locale-des-commits.md) | l'écriture et la relecture de l'attribution |
| `sidebar` | [0009](../../docs/adr/0009-cycle-de-vie-des-agents.md), [0012](../../docs/adr/0012-worktree-unite-de-travail.md) | les deux seuls faits de la colonne qui survivent à la fermeture — épingles et lignes repliées — et **rien d'autre** (spec §3.1). Le mot « workspace », retiré du vocabulaire par [0012](../../docs/adr/0012-worktree-unite-de-travail.md), n'est pas le nom de cette feature : elle ne détient pas les worktrees, seulement deux faits sur leurs lignes |
| `hooks` | [0007](../../docs/adr/0007-etats-par-hooks.md), [0013](../../docs/adr/0013-fiche-de-branche-dans-le-depot.md) | le marqueur par entrée, le `.bak`, le diff montré avant toute écriture |

Le fait que `hooks` porte à la fois les `settings.json` et le bloc `<!-- ash:log -->`
n'est pas un hasard : c'est la même règle transverse, et elle doit avoir un seul
propriétaire dans le code.

## Frontières

Une feature n'importe **pas** les fichiers internes d'une autre feature. La
communication passe par :

- son **API publique** — `mod.rs` en Rust, `index.ts` en TypeScript
- un **contrat** partagé (type, port) dans `shared/`
- un **service** explicite, injecté depuis la composition root

```rust
// ✗ dépendance sur l'intérieur d'une autre feature
use crate::features::git::worktree::internal::parse_gitdir_file;
// ✓ dépendance sur son API publique
use crate::features::git::{Repo, resolve_repo};
```

C'est cette règle qui rend une feature remplaçable et testable en isolation. Sans elle,
le découpage n'est qu'un renommage de dossiers.

### La frontière Rust ↔ TypeScript

C'est la plus importante du projet.

- Une feature Rust n'expose au frontend que ses `#[tauri::command]` et ses events,
  déclarés dans son `commands.rs`. Le reste du module est privé au crate.
- Le TypeScript ne connaît que les noms de commandes et les types de `shared/ipc/`. Il
  n'a aucune connaissance de la structure interne du backend.
- **Un refus traverse en chaîne, et ça se teste.** `PtyError` et `SettingsError`
  sérialisent tous deux `self.to_string()`, et le frontend en fait
  `error instanceof Error ? error.message : String(error)`. Un `Serialize` dérivé —
  donc un objet balisé — y donnerait `[object Object]` **à l'écran**, sans que `strict`,
  `noUncheckedIndexedAccess` ni le générateur de types ci-dessous ne s'en aperçoivent :
  le `catch` reçoit un `unknown`, et `String()` accepte tout. `ts-rs` synchronise la
  forme d'un type **déclaré**, pas la valeur qu'un `catch` reçoit. Un type d'erreur qui
  traverse la frontière garde donc un test qui assert la **forme sur le fil**, pas
  seulement la phrase (`settings/error.rs` en a un).

#### Les types du contrat sont écrits deux fois, et confrontés par le compilateur

Le contrat est **écrit deux fois**, et c'est assumé : les interfaces à la main portent
la prose qui explique le produit — pourquoi `location` peut être `null`, ce que
`upstream` absent veut dire — et une génération pure les remplacerait par des formes
muettes. Ce qui ne se défendait pas, c'est que rien ne les rattachait aux `struct` : un
champ renommé côté Rust laissait le TypeScript compiler, et rendait `undefined` à
l'exécution. Le dépôt l'a payé deux fois (#16, #48).

Le dispositif tient en trois pièces :

- **`ts-rs`**, en `[dev-dependencies]` uniquement. Chaque forme sérialisée porte
  `#[cfg_attr(test, derive(ts_rs::TS), ts(export))]` — donc sous `cfg(test)`, donc
  l'application livrée ne lie rien. `cargo test` écrit les types dans
  `src/shared/ipc/generated/`, qui est **versionné** ; le chemin est dans
  `src-tauri/.cargo/config.toml` plutôt que répété trente fois.
- **`src/shared/ipc/mirroring.ts`** : `Mirrors<Rust, HandWritten>`, une comparaison de
  types dans les **deux sens** — un champ oublié à la main se voit dans un sens, un
  champ inventé dans l'autre.
- **Un `mirror.ts` par endroit qui recopie une forme** — `shared/ipc/`,
  `features/settings/`, `features/terminal/`, `app/`. C'est la feature qui recopie qui
  prouve qu'elle recopie encore. Ces fichiers ne produisent aucun JavaScript.

**L'ordre des vérifications compte** : `cargo test` régénère, `bun run typecheck`
compare. Les deux sont déjà obligatoires ; les inverser laisse comparer un contrat
périmé, et c'est la seule maille du filet. `src/shared/ipc/mirror.test.ts` prouve que le
filet mord — il donne à `tsc` les vrais fichiers, puis les mêmes avec un champ Rust
renommé.

Deux autres voies ont été instruites et écartées :

- **`tauri-specta`** — sa seule ligne compatible Tauri 2 est en `2.0.0-rc`, et le dépôt
  refuse déjà les `rc` ailleurs (voir le commentaire de `notify` dans `Cargo.toml`). Il
  aurait de plus fallu réécrire le composition root, que rien ne teste.
- **Une description JSON produite de chaque côté et comparée** — sans dépendance, mais
  elle ne peut décrire que ce qu'un exemplaire montre. Mesuré sur `TabInfo` : les cinq
  états d'agent s'y réduisent à `state: string`, et un `location` absent à
  `location: null`. Il aurait fallu un exemplaire par variante et par branche
  d'`Option`, écrit à la main — c'est-à-dire le test artisanal d'`AgentState`, répété
  trente fois.

Le test artisanal d'`AgentState` (`features/agents/state.rs`) est **conservé** : son
`match` exhaustif force à nommer un état ajouté, ce que `ts-rs` ne fait pas.

### Aucun état d'agent ne vit uniquement côté TypeScript

Conséquence directe de [ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md) : le
jour où l'on voudra un démon `ashd`, la frontière entre **détenir** les PTY et leur
état, et **afficher** cet état, devra déjà être nette.

Le frontend rend un état ; il ne le détient pas. Concrètement : la machine à états d'un
agent, la remontée d'état vers la ligne de workspace, la résolution du worktree — tout
cela est en Rust, et le TypeScript en reçoit le résultat. Un `useState` qui devient la
source de vérité d'un état d'agent est un bug d'architecture, pas un détail
d'implémentation.

Le corollaire est agréable : cette logique se teste avec `cargo test`, sans webview,
sans navigateur, sans mock de framework.

## `shared/` n'est pas un fourre-tout

Un module va dans `shared/` **seulement** s'il sert au moins deux features **et** ne
porte aucune règle propre à l'une d'elles. Préfère des noms de rôle (`shared/ipc`,
`shared/result`, `shared/fs`) à des noms d'absence de rôle (`utils`, `helpers`,
`misc`).

Quand `shared/` grossit, c'est en général le signe qu'une feature n'a pas été nommée.
Cherche la capacité manquante avant d'ajouter un fichier de plus.

## Injection de dépendances

Mécanisme : **injection par constructeur et par paramètres, sans conteneur**.
Composition root : `src-tauri/src/main.rs` côté Rust, `src/app/` côté TypeScript.

Ash est presque entièrement fait d'effets système — PTY, `libproc`, horloge, système de
fichiers, git, socket unix. C'est précisément ce qui rend les ports indispensables :
sans eux, aucune règle métier n'est testable sans lancer un vrai processus.

```rust
// ✓ ce dont la feature a besoin est visible dans sa signature
pub struct AgentRegistry<P: Probe, C: Clock> {
    probe: P,
    clock: C,
}

// ✗ dépendance cachée derrière un appel direct au système
fn refresh(&mut self) {
    let pid = unsafe { tcgetpgrp(self.master_fd) };   // intestable
}
```

À privilégier : les traits aux frontières entre une feature et le système, une
composition root explicite, des dépendances visibles dans la signature.

À éviter : un état global (`static`, `lazy_static`, singleton TypeScript), un appel
système au cœur d'une règle métier, un mock rendu nécessaire uniquement par une
mauvaise frontière.

Sur `impl Trait` / génériques contre `Box<dyn Trait>` : pas de dogme. Les génériques
quand la substitution est connue à la compilation (le cas courant : réel en production,
fake en test), `Box<dyn>` quand la collection est hétérogène — c'est le cas du trait
`Adapter`, dont plusieurs implémentations coexistent au même moment.

## Design patterns

Les patterns sont des outils, pas des objectifs. Avant d'en ajouter un, vérifie qu'il
existe **au moins une** de ces quatre conditions : une variation réelle, une frontière
métier, un besoin de substitution, ou une réduction démontrable du couplage. Sans cela,
le pattern ajoute de l'indirection sans rien retirer.

| Pattern | À utiliser quand | Signal d'abus |
|---|---|---|
| Strategy | Comportements réellement interchangeables | Une seule implémentation, ou un `match` déguisé |
| Adapter | Isoler une dépendance externe du domaine | Recopie à l'identique de l'API adaptée |
| Port (trait) | Frontière entre une feature et un effet système | Un trait par struct, sans substitution réelle |
| State machine | Transitions explicites avec invariants | Un `enum` à deux variantes sans règle |
| Test Data Builder | Données de test lisibles et valides par défaut | Builder pour un objet à deux champs sans invariant |

Le trait `Adapter` de [ADR-0008](../../docs/adr/0008-abstraction-adapter.md) est le cas
d'école de la première ligne, et il est **décidé** : il existe dès J1 avec une seule
implémentation, et c'est assumé dans l'ADR. C'est la seule abstraction du projet dont
la justification est antérieure au code.

Évite les abstractions spéculatives et les traits créés uniquement pour doubler une
struct dans un test. Si un test doit doubler cinq collaborateurs, le problème est le
graphe de dépendances, pas l'absence de traits.

## Frontières exécutables (piste)

La règle de non-import entre features peut être rendue vérifiable plutôt que
déclarative :

- côté Rust, la visibilité fait déjà une partie du travail : garde les modules internes
  en `pub(super)` ou privés, et n'expose que le `mod.rs` ;
- côté TypeScript, `eslint-plugin-boundaries` ou `dependency-cruiser`.

**Rien n'a été installé** — c'est une piste, avec son coût de configuration.
