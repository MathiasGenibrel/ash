# ADR-0004 — Le workspace est la racine git du `cwd`, suivie en direct

- **Statut** : accepté (2026-08-07)
- **Impose** : [ADR-0005](./0005-sonde-cwd-libproc.md)
- **Amendé par** : [ADR-0012](./0012-worktree-unite-de-travail.md) — le niveau dépôt

## Contexte

La sidebar regroupe les onglets par « workspace ». L'utilisateur décrit les
workspaces comme des dossiers : `Cmd+N` ouvre un onglet dans le workspace courant,
`Cmd+Shift+N` ouvre à `~` et crée donc un nouveau workspace.

Mais dans un shell, on fait `cd`. Un onglet ouvert dans `~/Projects/ash` peut se
retrouver dans `~/Projects/website` trois secondes plus tard. À quel workspace
appartient-il alors ?

Trois politiques possibles : figer à la création, suivre le `cwd` brut, ou suivre la
racine du dépôt.

## Décision

Le workspace d'un onglet est la **racine du dépôt git** contenant son `cwd`, résolue
en remontant jusqu'au premier `.git`. À défaut de dépôt, c'est le `cwd` lui-même.

La résolution est **continue** : si le `cwd` sort du dépôt courant, l'onglet migre
vers le workspace correspondant.

## Conséquences

- Naviguer *dans* un projet (`cd website`, `cd ..`, `cd src`) ne change rien : la
  sidebar reste stable. Changer de projet fait migrer l'onglet, ce qui est le
  comportement attendu.
- La sidebar peut afficher nom du dépôt **et** branche — l'information qui a du sens
  quand on supervise des agents qui modifient du code.
- Un onglet peut changer de groupe pendant qu'un agent y tourne. C'est correct mais
  visuellement surprenant : le design devra rendre la migration lisible.
- Un onglet ouvert à `~` crée un workspace `~` jusqu'au premier `cd`. À valider à
  l'usage (question ouverte de la spec).
- Impose un suivi du `cwd` en temps réel, traité en
  [ADR-0005](./0005-sonde-cwd-libproc.md).

## Amendement (2026-08-10) — les worktrees

La décision supposait qu'un dépôt n'a qu'une racine. C'est faux avec `git worktree`,
et le design en fait le cas nominal : un agent par branche, chacun dans son dossier.

[ADR-0012](./0012-worktree-unite-de-travail.md) ajoute donc un niveau au-dessus. Ce
qui est appelé « workspace » ici devient le **worktree** ; les worktrees d'un même
dépôt sont groupés sous un nœud **dépôt**, qui n'a pas d'onglets en propre.

L'algorithme de résolution ci-dessus reste juste, avec une précision : dans un worktree
lié, `.git` est un **fichier** contenant `gitdir: …`. Remonter au premier `.git` donne
bien la racine du worktree ; retrouver le dépôt demande une étape de plus, lire ce
`gitdir:` puis son `commondir`.

## Alternatives écartées

- **Figé à la création de l'onglet** : parfaitement prévisible, mais un `cd` vers un
  autre projet laisse l'onglet mal classé, à déplacer à la main.
- **`cwd` brut** : `cd website` puis `cd ..` créerait deux workspaces pour le même
  projet — la sidebar se remplirait de bruit.
- **Workspaces déclarés en configuration** : sidebar stable et paramétrable, mais
  contredit le `Cmd+Shift+N` qui doit faire naître un workspace tout seul.
