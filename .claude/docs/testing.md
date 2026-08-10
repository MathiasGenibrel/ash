# Tests — Ash

Les tests protègent des **comportements**. Aucun test n'est ajouté pour gonfler un
compteur de couverture, et aucun test n'est conservé s'il ne protège rien.

## Structure obligatoire

`Given / When / Then`, avec les trois commentaires, et un nom qui décrit un
comportement — pas une méthode, pas un mock.

**TypeScript** (`bun test`)

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

**Rust** (`cargo test`) — le nom porte la phrase, puisque Rust n'a pas de chaîne de
description :

```rust
#[test]
fn given_a_working_agent_when_the_process_disappears_with_code_zero_then_it_becomes_done() {
    // Given
    let mut agent = AgentBuilder::new().command("claude").state(State::Working).build();
    // When
    agent.on_process_exit(ExitStatus::Code(0));
    // Then
    assert_eq!(agent.state(), State::Done);
}
```

## Ce qui mérite un test dans ce projet

Ash est presque entièrement fait d'effets système, donc la question « qu'est-ce qui a
de la valeur à tester » a une réponse précise ici :

| À tester | Pourquoi |
|---|---|
| La machine à états d'un agent | C'est la valeur du produit. Chaque transition de la spec §6.2 et chaque règle de §6.4 est un test |
| La remontée d'état vers la ligne de dépôt/worktree | `waiting` l'emporte sur tout — une régression rend un agent en attente invisible |
| La résolution worktree → dépôt | `.git` fichier contre dossier, `gitdir:`, `commondir`. Cas limites nombreux, conséquences visibles |
| Le parsing de l'état d'un rebase | Ash affiche `2/5` et `3 conflicted files` à partir de fichiers de contrôle. S'il se trompe, il ment sur un sujet qui ne pardonne pas |
| L'écriture d'un bloc délimité | `.bak`, marqueurs, refus si édité à la main. C'est un fichier de l'utilisateur |
| La correspondance de repli du journal | `(author_date, subject)` après rebase — la règle est heuristique, donc elle a besoin d'exemples |
| Chaque correction de bug | Le test vient avec le correctif, et décrit le symptôme |

## Ce qui n'en mérite pas

Getters, DTO, constantes, délégation sans logique, câblage de Tauri, sérialisation
`serde` d'une structure sans invariant, et tout test dont l'unique garantie est qu'un
mock a été appelé — celui-là verrouille l'implémentation actuelle et cassera au premier
refactoring correct.

**N'écris pas de test qui relance un vrai `claude`.** Ni un vrai PTY dans un test
unitaire, ni un vrai dépôt distant, ni une vraie notification macOS.

## Les ports rendent tout le reste testable

C'est la raison d'être des traits décrits dans
[`architecture.md`](./architecture.md). Un test qui a besoin d'un vrai processus n'est
pas un test unitaire, c'est un pari sur l'environnement de la machine.

```rust
// ✓ la règle se teste sans processus
let registry = AgentRegistry::new(FakeProbe::with_foreground("claude"), FixedClock::at(t0));

// ✗ la règle appelle le système : intestable, et lente
let registry = AgentRegistry::new();   // ouvre un PTY dans son constructeur
```

Les fakes respectent le **contrat** du port, ils ne simulent pas ses appels un par un.
Un `FakeProbe` qui rend un `cwd` et un nom de processus est plus utile — et plus
robuste au refactoring — qu'un mock qui vérifie que `proc_pidinfo` a été appelé deux
fois.

Quand plusieurs implémentations partagent un contrat — c'est le cas du trait `Adapter`
et de ses `claude-code` / `codex` / `generic` — écris une **suite de tests
contractuels** commune, puis les comportements propres à chacune.

## Tests d'intégration

Ils ont leur place ici, et il faut savoir laquelle :

- **Rust, `src-tauri/tests/`** — un vrai dépôt git créé dans un dossier temporaire est
  le bon niveau pour tester la résolution worktree/dépôt et le parsing d'un rebase. Un
  fake de système de fichiers ne reproduirait pas fidèlement ce que git écrit.
- **Un vrai PTY** — un test qui lance `bash -c 'echo hi'` dans un PTY et vérifie la
  sortie est légitime pour la feature `pty`, et pour elle seule.

Ces tests sont plus lents : garde-les hors du chemin d'itération courant
(`cargo test --lib` pendant qu'on code, la suite complète avant la PR).

## Test Data Builders

À créer dès qu'un objet a plusieurs champs ou un invariant — c'est le cas de `Tab`,
`Agent`, `Worktree`, `Vcs`.

- Défauts **valides** et **déterministes** — jamais aléatoires, jamais `Instant::now()`.
- On ne surcharge que les propriétés utiles au scénario : c'est ce qui rend le `Given`
  lisible.
- Ils vivent dans le code de test, pas dans le code de production.

Le temps est une dépendance comme une autre. `since`, les durées affichées, la règle
des 30 secondes de `done`, les 60 secondes sans événement : tout cela se teste avec une
horloge injectée, jamais avec un `sleep`.

## E2E

**Il n'y en a pas, et c'est un choix.** Piloter une fenêtre Tauri demande
`tauri-driver` et un binaire construit ; le produit n'a pas encore de parcours à
protéger. À rediscuter au jalon J2, quand les états d'agent seront le cœur du produit
et qu'une régression y sera coûteuse.

En attendant, la validation de bout en bout est manuelle et confiée à l'agent `qa` :
build, lancement réel de l'application, parcours touché par la tâche.

## Pas de runner Gherkin

Il n'y en a pas dans ce projet et **il n'en faut pas** : la structure BDD vit dans les
tests `bun test` et `cargo test`. N'installe pas Cucumber pour « faire du Gherkin ».
