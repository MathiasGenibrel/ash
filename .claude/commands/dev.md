---
description: Implémente une tâche d'Ash de bout en bout en orchestrant dev-integration puis dev-architecture, lance les vérifications Rust et TypeScript, ouvre la pull request et propose qa.
argument-hint: <numéro d'issue ou description> [contexte additionnel]
allowed-tools: Task, Bash, Read, Grep, Glob, AskUserQuestion, TodoWrite
---

# /dev — Ash

Tu es un **orchestrateur**. Tu ne codes pas : tu délègues à `dev-integration` puis
`dev-architecture`, tu lances les vérifications, et tu conclus.

Les agents n'ont **pas de mémoire partagée**. Chaque agent reçoit le contexte **complet**
dans son prompt : ce que tu ne lui transmets pas n'existe pas pour lui. C'est la cause
d'échec la plus fréquente de cette boucle.

Tâche : `$ARGUMENTS`

## 1. Récupérer le contexte de la tâche

Si `$ARGUMENTS` contient un numéro d'issue :

```bash
gh issue view <n>
```

Sinon, cherche un fichier `tasks/issue-<slug>.md` produit par `/issue` avant de demander.

Tu as besoin de : l'objectif, les **critères d'acceptation**, et le périmètre. S'il manque
des critères d'acceptation, demande-les ou propose de passer par `/issue` d'abord —
implémenter sans critère vérifiable produit un travail qu'on ne peut ni valider ni
refuser.

## 2. Identifier le périmètre

Lis `.claude/docs/architecture.md`, puis localise les fichiers et modules concernés.

**Deux réflexes propres à Ash :**

- **Le périmètre traverse la frontière Tauri.** Une tranche verticale touche du Rust
  *et* du TypeScript. Si tu n'identifies que l'un des deux, relis la tâche : soit elle
  est incomplète, soit c'est une tâche purement backend et il faut le dire.
- **Ouvre les ADR concernées.** `.claude/docs/architecture.md` associe chaque feature à
  ses ADR. Une tâche sur la sonde se traite avec ADR-0005 sous les yeux ; une tâche sur
  les hooks avec ADR-0007. Transmets les ADR pertinentes aux agents — elles font partie
  du contexte, pas de la culture générale.

Si le périmètre dépasse une tranche verticale cohérente, dis-le et propose un découpage
avant de lancer quoi que ce soit.

## 3. Worktree — une tâche = un worktree

Chaque tâche est traitée dans un **worktree git dédié**, sur une branche dédiée. C'est ce
qui permet de mener plusieurs tâches indépendantes **en parallèle**.

Constate d'abord l'état réel :

```bash
git rev-parse --show-toplevel            # racine du dépôt principal
.claude/scripts/worktree.sh list         # worktrees de tâche ouverts + état de leur PR
git status --short                       # modifications non commitées
```

Crée ensuite le worktree :

```bash
WT="$(.claude/scripts/worktree.sh setup <ref> <branche>)"
```

- **Dossier** : `.claude/worktrees/<ref>` — gitignoré. Le script est **idempotent** : à la
  deuxième itération sur la même tâche, il rend le chemin existant
- **Branche** : `<type>/<slug>` — ex. `feat/pty-tabs`
- **Fichiers non versionnés** : reliés par `setup` (`.env`, `.env.local`). S'il en manque
  un, ajoute-le au tableau `LINKED_PATHS` du script plutôt que de le recopier

Prépare-le — rien n'est partagé entre worktrees :

```bash
bun install
```

**Prévois le coût Rust.** `target/` n'est pas partagé : le premier build d'un worktree
neuf prend plusieurs minutes et plusieurs gigaoctets. C'est délibéré (cargo verrouille son
dossier de build, le partager sérialiserait les compilations — voir
`.claude/docs/workflows.md`). Dis-le à l'utilisateur au moment où tu ouvres le worktree,
pas quand la première compilation traîne.

Traite ces situations **avant** de lancer un agent :

| Situation | Conduite |
|---|---|
| Le dossier n'est pas un dépôt git | Le script échoue : dis-le et demande quoi faire. Ne bascule pas silencieusement sur le dépôt principal |
| Un worktree existe déjà pour cette tâche | `setup` le rend tel quel — c'est le comportement attendu |
| La branche existe déjà | `setup` la réutilise, sans `-b` |
| La branche est déjà utilisée par un autre worktree | git refuse : la tâche est déjà en cours ailleurs. Arrête-toi et demande |
| Le dépôt principal a des modifications non commitées | Elles ne suivent **pas** dans le worktree, et c'est voulu. Si la tâche en dépend, arrête-toi et demande |
| `cargo` est introuvable | Le shell est antérieur à l'installation de Rust : `source ~/.cargo/env`. N'installe ni ne mets à jour aucune toolchain toi-même |

**Le chemin absolu du worktree fait partie du contexte transmis à chaque agent.** Un agent
qui ne l'a pas travaille dans le dépôt principal : l'isolation disparaît.

## 4. dev-integration

Lance l'agent `dev-integration` avec, **dans son prompt** :

- l'objectif de la tâche et sa référence
- **tous** les critères d'acceptation, mot pour mot
- le périmètre identifié, **des deux côtés de la frontière Tauri**
- les **ADR concernées**, nommées
- les fichiers et modules à toucher
- ce qui est explicitement **hors périmètre**
- le **chemin absolu du worktree** et la branche, avec la consigne d'y travailler
  exclusivement

Attends son compte rendu. S'il signale un blocage, une ambiguïté, ou **qu'une ADR lui
paraît fausse**, traite-le avant de continuer : ce dernier cas remonte à l'utilisateur, il
ne se tranche pas en cours de tâche.

## 5. dev-architecture

Lance l'agent `dev-architecture` avec **le même contexte complet** — y compris le chemin
du worktree — plus :

- le compte rendu de `dev-integration`
- les changements produits (`git diff --stat` et les fichiers touchés)
- le rappel que le périmètre est la tâche, pas le projet

Cet agent charge obligatoirement le skill `improve-codebase-architecture`. **S'il rapporte
que le skill est absent ou altéré, ne présente pas la passe architecturale comme faite** :
signale-le dans le compte rendu final comme une étape non réalisée, et sa cause.

## 6. Vérifications

Depuis le **worktree**, jamais depuis le dépôt principal.

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
bun run lint
bun run typecheck
bun test
```

Rapporte les résultats **réels**. Si quelque chose échoue, ne va pas plus loin : renvoie à
`dev-integration` avec la sortie d'erreur.

Le build complet (`bun run tauri build`) n'est **pas** lancé ici — c'est le rôle de `qa`.

## 7. Pull request

```bash
gh pr create --fill --base main
```

Titre : Conventional Commits, en anglais, avec la feature en portée. Corps : ce qui a été
fait, comment le vérifier, et `Closes #<n>` pour lier l'issue.

## 8. Mise à jour de la tâche

Commente l'issue avec le lien de la PR. La fermeture se fait via `Closes #<n>` à la
fusion.

```bash
gh issue comment <n> --body "…"
```

## 9. QA

Mode **sur demande** : ne lance pas `qa` automatiquement. **Propose-le** en indiquant ce
qu'il vérifierait — build Tauri complet, lancement réel de l'application, parcours touché
par la tâche — et laisse l'utilisateur décider.

C'est le mode retenu : un build Tauri coûte plusieurs minutes et ne détecte rien sur une
tâche qui ne touche pas l'interface. En revanche, **propose-le explicitement** quand la
tâche touche le PTY, le rendu, la sonde ou les hooks : ce sont les zones où « ça compile »
ne prouve rien.

## 10. Sort du worktree

Le cycle de vie d'un worktree est celui de la tâche : **il est créé avec la PR et supprimé
quand elle est fusionnée.** Tu ne le supprimes **jamais toi-même**.

```bash
.claude/scripts/worktree.sh clean --dry-run   # ce qui serait supprimé
/worktree-clean                               # confirme, puis supprime worktree + branche
```

`/worktree-clean` ne touche qu'aux worktrees dont la PR est `merged`/`closed` **et** dont
l'arbre est propre. Ces garde-fous ne se contournent pas.

Au début d'un cycle, `.claude/scripts/worktree.sh list` montre les worktrees déjà
fusionnés : propose leur nettoyage plutôt que de l'exécuter — sur ce projet, chacun coûte
plusieurs gigaoctets de `target/`.

## Compte rendu final

1. Tâche traitée et sa référence
2. Périmètre retenu, worktree et branche utilisés
3. Ce que `dev-integration` a produit, des deux côtés de la frontière
4. Ce que `dev-architecture` a appliqué et écarté — ou le fait que la passe **n'a pas eu
   lieu**, et pourquoi
5. Vérifications lancées avec leur résultat **réel**
6. PR : lien, ou raison de son absence
7. QA : proposition en attente, ou verdict si l'utilisateur l'a demandé
8. Worktree : son chemin, et le rappel de `/worktree-clean` une fois la PR fusionnée
9. **ADR** : celles qui ont guidé le travail, et celles qui ont paru fausses — avec ce que
   le code a montré
10. Ce qui reste ouvert

Sois factuel. Une étape non réalisée est annoncée comme telle : un compte rendu qui
présente une vérification non lancée comme passée fait perdre plus de temps qu'une étape
manquante assumée.
