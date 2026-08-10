---
description: Comme /dev sur Ash, mais écrit d'abord un plan d'implémentation dans tasks/ et attend une validation explicite avant toute modification de code, puis orchestre dev-integration et dev-architecture et ouvre la pull request.
argument-hint: <numéro d'issue ou description> [contexte additionnel]
allowed-tools: Task, Bash, Read, Write, Grep, Glob, AskUserQuestion, TodoWrite
---

# /dev-with-plan — Ash

Même workflow que `/dev`, avec une **phase de plan** en tête. Utile quand la tâche est
risquée, touche les deux côtés de la frontière Tauri, ou quand une erreur de cadrage
coûterait cher.

Sur ce projet, c'est le mode par défaut pour trois familles de tâches : tout ce qui touche
au **PTY et au rendu** (le risque n°1, ADR-0002), tout ce qui **écrit dans un fichier de
l'utilisateur** (hooks, fiche de branche), et toute tâche qui **inaugure une feature**.

Tu es un **orchestrateur** : tu ne codes pas.

Tâche : `$ARGUMENTS`

## 1. Analyser

Si `$ARGUMENTS` contient un numéro d'issue :

```bash
gh issue view <n>
```

Sinon, cherche un `tasks/issue-<slug>.md` produit par `/issue`.

Puis explore le code réellement concerné : lis `.claude/docs/architecture.md`, localise
les modules, repère les points d'entrée et les tests existants. **Lis les ADR que
`.claude/docs/architecture.md` associe aux features touchées** — sur ce projet, la
décision précède le code, et un plan écrit sans les ADR propose des étapes qui
contredisent une décision déjà prise.

Le plan ne vaut que par la précision de cette lecture : un plan écrit sans avoir ouvert
les fichiers énumère des étapes plausibles plutôt que des étapes vraies.

**Lecture seule à ce stade.** Aucune modification de code avant la validation du plan.

## 2. Écrire le plan

Dans `tasks/plan-<slug>.md` :

```markdown
# Plan — <titre de la tâche>

Référence : <#n ou slug>
ADR concernées : <ADR-00xx, ADR-00yy>

## Objectif
Ce que la tâche doit rendre possible.

## Périmètre
Ce qui est couvert. Ce qui est explicitement **hors périmètre**.

## Découpage Rust / TypeScript
| Côté | Ce qui change | Pourquoi ici |
|---|---|---|
| Rust | … | l'état est détenu par le backend (ADR-0009) |
| TypeScript | … | rendu uniquement |

## Contrat IPC touché
Les commandes et events ajoutés ou modifiés, avec leurs types. Vide si aucun.

## Fichiers concernés
| Fichier | Nature du changement |
|---|---|
| `src-tauri/src/features/…/mod.rs` | création / modification / suppression |

## Risques
Ce qui peut casser, ce qui est incertain, ce qui dépend du système (PTY, libproc, git).

## Stratégie de tests
Ce qui sera testé et **pourquoi c'est utile**. Ce qui ne sera **pas** testé, et pourquoi.
Niveau par comportement : unitaire Rust / unitaire TS / intégration Rust.
Les ports à introduire pour rendre la règle testable sans processus réel.

## Étapes
1. …

## Critères d'acceptation
- [ ] …
```

Quatre exigences sur ce plan :

- les **fichiers concernés** sont des chemins qui existent (ou dont le dossier parent
  existe) — pas des noms plausibles ;
- le **découpage Rust / TypeScript** est explicite. Si tout le plan est côté TypeScript,
  vérifie qu'aucun état d'agent n'y migre (ADR-0009) ;
- la **stratégie de tests** dit aussi ce qui ne sera pas testé. Un plan qui promet de
  « tout tester » ne documente rien ;
- si le plan **contredit une ADR**, dis-le en tête du document. Ne le contourne pas en
  silence : c'est une décision de l'utilisateur, et elle se prend maintenant.

Respecte la doctrine de `.claude/docs/testing.md` : `Given / When / Then`, comportements,
Test Data Builders dans le `Given`, aucun test trivial.

## 3. Faire valider — étape bloquante

Présente le plan et **attends une validation explicite**.

Ne commence **aucune** modification de code avant. Si l'utilisateur demande des
ajustements, mets le plan à jour et redemande. C'est tout l'intérêt de cette command : le
désaccord se règle sur un document, pas sur un diff.

Un silence ou une réponse ambiguë ne vaut pas validation.

## 4. Suite du workflow

Une fois le plan validé, déroule exactement `/dev`, en transmettant **le plan** en plus du
contexte de la tâche. Le fichier de plan vit dans le dépôt principal, pas dans le
worktree : transmets son **contenu intégral** dans le prompt des agents, jamais son seul
chemin.

1. **Worktree** — une tâche = un worktree. Ouvre `.claude/worktrees/<ref>` sur la branche
   `<type>/<slug>`, basée sur `origin/main` :

   ```bash
   .claude/scripts/worktree.sh list
   WT="$(.claude/scripts/worktree.sh setup <ref> <branche>)"
   bun install
   ```

   `setup` est **idempotent** et relie les fichiers non versionnés. Rappelle le coût du
   premier build Rust (`target/` non partagé, plusieurs minutes et plusieurs Go). Le
   **chemin absolu** du worktree fait partie du contexte transmis à chaque agent.
2. **`dev-integration`** — contexte complet **+ le plan validé** : objectif, critères
   d'acceptation mot pour mot, périmètre, hors périmètre, ADR concernées, découpage
   Rust/TS, contrat IPC, fichiers, stratégie de tests, étapes, chemin du worktree
3. **`dev-architecture`** — même contexte complet, plus le compte rendu de
   `dev-integration` et les changements produits. Cet agent charge obligatoirement le
   skill `improve-codebase-architecture` ; s'il rapporte que le skill est absent ou
   altéré, la passe architecturale **n'a pas eu lieu** et doit être annoncée comme telle
4. **Vérifications** :
   ```bash
   cargo fmt --check && cargo clippy -- -D warnings && cargo test
   bun run lint && bun run typecheck && bun test
   ```
5. **Pull request** — `gh pr create --fill --base main`, avec `Closes #<n>`
6. **Issue** — commentaire avec le lien de la PR
7. **QA** — `qa` **proposé**, pas lancé. Insiste s'il la tâche touche le PTY, le rendu, la
   sonde ou les hooks

## 5. Compte rendu final

Reprends le plan et confronte-le à ce qui a été réellement fait :

1. Chemin du plan validé, chemin du worktree et branche
2. **Écarts entre plan et réalisation, avec leur raison** — c'est l'information la plus
   utile du compte rendu
3. Ce que `dev-integration` a produit, des deux côtés
4. Ce que `dev-architecture` a appliqué et écarté, ou le fait que la passe n'a pas eu lieu
5. Vérifications lancées et résultats **réels**
6. PR : lien ou raison de son absence
7. QA : proposition en attente
8. ADR : celles qui ont guidé, celles qui ont paru fausses
9. Ce qui reste ouvert

Le plan reste dans `tasks/` : il documente la décision de cadrage, y compris les écarts
assumés.

## Limites

- Aucune modification de code avant validation explicite du plan
- Le plan ne remplace pas les critères d'acceptation : s'ils manquent, demande-les ou
  passe par `/issue`
- Aucun refactor hors du périmètre écrit dans le plan
- Aucune dépendance (crate ou paquet) ni toolchain installée sans demande explicite, en
  dehors de `bun install` dans le worktree
- Aucun agent ne supprime de worktree : `/worktree-clean` le fait une fois la PR fusionnée
- Une étape non réalisée est annoncée comme telle, jamais présentée comme faite
