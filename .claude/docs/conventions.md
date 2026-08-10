# Conventions — Ash

## Rust

- **Édition** : 2021 ou plus récente. Un seul crate (`src-tauri/`) tant qu'il n'y a pas
  de raison de le découper.
- **Pas de `unwrap()` ni de `expect()`** hors tests et composition root. Les erreurs
  remontent en `Result`, avec un type d'erreur par feature — pas un `anyhow::Error`
  générique traversant tout le programme. Une erreur de sonde et une erreur de rebase
  ne se traitent pas pareil, donc elles ne se typent pas pareil.
- **`unsafe` est isolé.** Les appels `libproc` et `tcgetpgrp` sont du FFI : ils vivent
  dans un module dédié de `features/probe/`, derrière une fonction sûre et testée. Aucun
  `unsafe` ailleurs.
- **`clippy` en `-D warnings`.** Un `#[allow]` local doit porter un commentaire disant
  pourquoi.
- **`cargo fmt` tranche le formatage.** Ce n'est pas un sujet de revue.

## TypeScript

- **Strictness** : `strict: true`, plus `noUncheckedIndexedAccess` et
  `exactOptionalPropertyTypes`. Ils attrapent des classes d'erreurs que `strict` seul
  laisse passer, et coûtent beaucoup moins cher à activer maintenant qu'après.
- **Modules** : ESM.
- **Alias** : `@/` → `src/`. Un alias doit être déclaré dans **chacun** de ces
  fichiers, sinon le build ou les tests casseront, souvent sans message clair :
  `tsconfig.json`, la configuration Vite, la configuration de test.

Préfère l'alias à une remontée relative profonde (`../../../`) : un fichier déplacé
casse la seconde, pas le premier.

## Imports

Les imports internes d'une autre feature sont interdits, des deux côtés de la
frontière. Passe par son API publique. Voir
[`architecture.md`](./architecture.md).

Côté Rust, la visibilité est ton alliée : garde les modules internes privés ou en
`pub(super)`, et n'expose que ce que le `mod.rs` réexporte. Une frontière que le
compilateur fait respecter vaut mieux qu'une frontière écrite dans un document.

## Nommage

- **Fichiers Rust** : `snake_case`, un module par responsabilité.
- **Fichiers TypeScript** : `kebab-case`. Ne mélange pas avec du `PascalCase`.
- **Types** : `PascalCase` des deux côtés · fonctions et variables : `snake_case` en
  Rust, `camelCase` en TypeScript.
- Un nom de feature décrit une **capacité** (`agents`, `git`, `journal`), pas une
  couche technique (`services`, `managers`, `helpers`).

**Le vocabulaire du produit est celui de la spec, et il est traduit une seule fois.**
Les états d'agent sont `idle`, `working`, `waiting`, `done`, `error` — en anglais, dans
le code, dans le contrat IPC et dans l'interface. La documentation du projet est en
français ; le code ne l'est pas. Ne réinvente pas un synonyme (`busy`, `blocked`,
`pending`) : le design de la fiche de branche montre justement `blocked`/`finished`
comme le côté *theirs* d'un conflit, pas comme le vocabulaire retenu.

Même règle pour `worktree`, `repo`, `tab`, `agent`, `subagent`, `adapter`. Le mot
« workspace » a été abandonné par la spec : ne le réintroduis pas.

## Tests

- **Rust** : module `#[cfg(test)]` en fin de fichier pour l'unitaire,
  `src-tauri/tests/` pour l'intégration.
- **TypeScript** : `*.test.ts` à côté du code testé.
- Structure `Given / When / Then` obligatoire des deux côtés. Détail dans
  [`testing.md`](./testing.md).

## Commits

- Style : **Conventional Commits**
- Langue : anglais
- Portée : le nom de la feature — `feat(sidebar):`, `fix(probe):`, `refactor(git):`
- Exemple : `feat(sidebar): bubble waiting state to the workspace row`

## Branches

- Base : `main`
- Motif : `<type>/<slug>` — ex. `feat/pty-tabs`, une branche par worktree
- Forge : GitHub, CLI `gh`

## Documentation du projet

`docs/spec.md` et `docs/adr/` sont la mémoire du projet. Deux règles :

- **Une ADR ne se réécrit pas en silence.** Si une tâche montre qu'une décision est
  fausse, on ajoute une section d'amendement datée, ou une nouvelle ADR qui l'amende —
  on ne modifie pas le raisonnement d'origine. C'est la pratique déjà en place dans
  `docs/adr/`, et elle vaut pour les agents comme pour les humains.
- **Une décision non évidente prise pendant une tâche s'écrit.** En commentaire à
  l'endroit concerné si elle est locale ; dans `.claude/docs/` si elle engage le
  projet ; en ADR si elle engage l'architecture.

## Ce qui n'est pas une convention

Le formatage automatique n'est pas un sujet de revue : `cargo fmt` et le formateur
TypeScript tranchent. Ne passe pas de temps sur les guillemets, les points-virgules ou
la largeur de ligne, et ne reformate pas des fichiers hors du périmètre d'une tâche —
un diff de formatage noie les modifications réelles.
