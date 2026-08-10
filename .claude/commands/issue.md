---
description: Transforme un besoin en tâche claire pour Ash — description fonctionnelle, critères d'acceptation vérifiables, Gherkin quand il exprime un comportement — puis la crée comme issue GitHub après validation.
argument-hint: <besoin ou description libre>
allowed-tools: Read, Grep, Glob, Bash, Write, AskUserQuestion, TodoWrite
---

# /issue — Ash

Transforme `$ARGUMENTS` en **une tâche claire**. Tu ne codes pas, tu ne modifies aucun
fichier applicatif.

Tracker : **issues GitHub** (CLI `gh`).

## 1. Comprendre le besoin

Lis le dépôt pour situer le besoin. Sur ce projet, l'ordre de lecture est particulier et
il compte :

1. `docs/spec.md` — le produit est spécifié, en détail. Le besoin y correspond
   probablement déjà à une section
2. `docs/adr/` — la décision qui encadre le besoin est probablement déjà prise
3. `.claude/docs/architecture.md` — quelle feature, de quel côté de la frontière

Une tâche bien formée cite la section de spec et les ADR concernées. Une tâche vague fait
perdre un cycle entier à `/dev`.

**Trois vérifications avant de rédiger :**

- **Le besoin est-il déjà spécifié ?** Si oui, la tâche consiste à l'implémenter, et ses
  critères d'acceptation se dérivent de la spec — ne les réinvente pas.
- **Contredit-il une ADR ?** Si oui, ce n'est pas une tâche, c'est un amendement d'ADR à
  discuter d'abord. Dis-le et arrête-toi.
- **Est-ce une question ouverte de la spec §12 ?** Si oui, la tâche doit dire ce qu'elle
  tranche, et la spec devra être mise à jour en même temps.

Si le besoin est ambigu, pose les questions maintenant. C'est le moment le moins coûteux
du cycle.

Si `$ARGUMENTS` couvre en réalité **plusieurs** tâches, propose un découpage plutôt qu'une
tâche fourre-tout. Sur Ash, le découpage naturel est le **jalon** (J1 terminal, J2 états,
J3 attention, J4 ouverture, J5 git) : une tâche qui traverse deux jalons est presque
toujours deux tâches.

## 2. Rédiger

Privilégie une **description fonctionnelle** : ce que l'utilisateur doit pouvoir faire, et
pourquoi. Un ticket qui décrit une solution technique enferme l'implémentation avant même
la discussion.

```markdown
## Contexte
Pourquoi ce besoin existe, du point de vue de l'utilisateur.
Spec : §<n> · ADR : <ADR-00xx>

## Comportement attendu
Ce qui doit être possible après la tâche.

## Critères d'acceptation
- [ ] Critère observable et vérifiable
- [ ] …

## Scénarios
Given …
When …
Then …

## Hors périmètre
Ce que la tâche ne couvre pas — évite l'élargissement silencieux.
```

**Critères d'acceptation** : chacun doit être **vérifiable**. « La sidebar affiche l'état
des agents » n'est pas un critère ; « un agent passé en `waiting` fait apparaître l'état
sur la ligne de son worktree replié en moins d'une seconde » en est un.

Sur ce produit, les bons critères parlent souvent de **temps** et de **périphérie** : le
design est fait pour être lu du coin de l'œil, et la spec chiffre plusieurs choses
(sonde ~300 ms, `waiting` vu en moins de 10 s, `done` visible 30 s, rafraîchissement git
au plus toutes les 5 s). Reprends ces chiffres au lieu d'en inventer.

**Gherkin** : utilise-le quand il exprime réellement un comportement. Ne l'utilise pas
pour habiller une consigne technique — `Given le code est refactoré` n'est pas un
scénario. Une tâche purement technique n'a pas besoin de Gherkin.

Note aussi ce qui est **hors périmètre** : c'est ce qui protège la tâche de
l'élargissement en cours de route.

## 3. Faire valider

Présente la tâche rédigée **et attends la validation explicite**. Rien n'est créé à
distance avant.

Propose les ajustements possibles (périmètre, critères, découpage) plutôt que de demander
un simple oui/non : c'est à ce moment que la tâche gagne en qualité.

## 4. Créer

Après validation :

```bash
gh issue create --title "<titre>" --body "<corps>"
```

Rends le numéro (`#<n>`) et l'URL de l'issue créée.

Si aucun remote GitHub n'est configuré, `gh` échouera. Dans ce cas, écris la tâche en
local :

```
tasks/issue-<slug>.md
```

Puis dis **explicitement** qu'il s'agit d'une **proposition locale** et qu'**aucune issue
n'a été créée**. Ne formule jamais « issue créée » : l'utilisateur chercherait un ticket
qui n'existe pas, et le découvrirait au pire moment. Indique que `/dev` peut prendre ce
fichier comme source de contexte.

## Limites

- Aucune création à distance sans validation explicite
- Aucune modification de fichier applicatif, ni de la spec, ni d'une ADR
- Aucun commit, aucun push, aucun worktree créé — c'est `/dev` qui ouvre le worktree
- Pas de critère d'acceptation invérifiable
- Pas de Gherkin décoratif sur une tâche technique
- Une tâche qui contredit une ADR n'est pas créée : elle est signalée comme un amendement
  à discuter
- Ne présente jamais une proposition locale comme une issue créée
