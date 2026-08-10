# ADR-0011 — Git est un domaine de premier plan, intégré à Ash

- **Statut** : accepté (2026-08-10)
- **Issue de** : la direction visuelle, section 4
- **Impose** : [ADR-0012](./0012-worktree-unite-de-travail.md),
  [ADR-0014](./0014-attribution-locale-des-commits.md)

## Contexte

Jusqu'ici, git n'entrait dans Ash que par deux petites portes : il servait à résoudre
la racine d'un workspace ([ADR-0004](./0004-workspace-racine-git.md)) et à afficher
une branche sous le nom du dépôt (spec §5.3). Tout le reste — changer de branche,
lire l'historique, résoudre un conflit — se faisait dans le terminal, comme avant.

Le design a montré que cette frontière ne tient pas, pour une raison précise :
**superviser des agents qui écrivent du code, c'est superviser ce qu'ils écrivent
dans git**. Trois situations le rendent concret.

- Un `checkout` pendant qu'un agent écrit déplace des fichiers sous ses pieds. Aucun
  client git, aucun IDE ne peut prévenir : ils ne savent pas qu'un agent tourne.
  Ash, si.
- Un historique produit par plusieurs agents est illisible sans savoir **qui** a écrit
  quoi. La colonne « auteur » d'un graphe git classique dit toujours le nom de
  l'utilisateur, même quand le commit a été écrit par `codex` à 3 h du matin.
- Un rebase qui s'arrête sur conflit est exactement le moment où l'on voudrait
  passer la main à un agent — avec les bons chemins et le bon contexte.

Dans les trois cas, l'information manquante est celle qu'Ash est le seul à détenir.

## Décision

Ash intègre un **domaine git de premier plan**, borné par une règle de périmètre :

> Ash n'intègre une opération git que si la présence d'agents change ce qu'il faut
> en dire, ou ce qu'il faut en faire.

Surface retenue :

| Écran | Raccourci | Ce que la présence d'agents change |
|---|---|---|
| Popup de branches | `⌘⌃B` | l'avertissement nommant l'agent qui travaille avant un checkout |
| Graphe | `⌘⌃G` | la colonne `by` : quel agent, et le prompt qui a produit le commit |
| Worktrees | `⌘⌃W` | les colonnes `agents now` et `last worked by`, la détection du `stale` |
| Onglet merge | `⌘⌃M` | passer les hunks restants à un agent ([ADR-0015](./0015-ash-compose-l-utilisateur-envoie.md)) |
| Fiche de branche | `⌘⌃I` | le journal d'agents ([ADR-0013](./0013-fiche-de-branche-dans-le-depot.md)) |

Reste **hors** d'Ash, et donc dans le terminal : la zone de préparation (`add`,
`reset`), l'écriture d'un commit, la gestion des remotes, les tags, le `stash`, la
configuration git. Rien de tout cela ne change parce qu'un agent tourne.

## Conséquences

- Le produit gagne un troisième pilier. Il ne se décrit plus comme « navigation +
  supervision » mais comme **navigation, supervision, et git conscient des agents**.
- Le volume d'ingénierie est comparable à celui des jalons J1 à J4 réunis. Un jalon
  J5 est ajouté ; il vient après que la supervision soit fiable, pas avant.
- La règle de périmètre est ce qui empêche la dérive vers un client git complet. Elle
  doit être appliquée à chaque demande d'ajout, y compris aux demandes raisonnables :
  une UI de commit serait confortable, mais un agent qui tourne n'y change rien.
- Ash lit git en permanence pour plusieurs dépôts. Le coût d'observation dépasse
  celui de la sonde `cwd` : il faut surveiller `.git/HEAD`, `.git/refs`,
  `.git/rebase-merge`, sur `n` dépôts. Un `git status` par cycle de sonde est exclu.
- Ash affiche des états git transitoires (`rebasing onto main · 2/5`) qui n'existent
  que dans les fichiers de contrôle du dépôt. S'ils sont mal lus, Ash ment sur un
  sujet où l'utilisateur ne pardonne pas.

## Alternatives écartées

- **Déléguer à un client existant** (`lazygit`, `tig`, `gitui` lancé dans un onglet
  Ash) : gratuit, excellent, et déjà installé chez l'utilisateur. Écarté parce que
  c'est précisément le contraire de la décision : ces outils ignorent les agents, et
  c'est la seule chose qu'Ash avait à apporter. Ils restent lançables dans un onglet,
  et couvrent le hors-périmètre ci-dessus.
- **Lecture seule** (afficher branche, graphe et worktrees, sans jamais écrire) :
  supprime toute la classe de bugs où Ash abîme un dépôt. Écarté parce que les deux
  moments qui comptent — le checkout risqué et le rebase arrêté — sont des écritures.
  Un Ash qui montre le conflit sans permettre de le traiter renvoie au terminal
  exactement quand il devenait utile.
- **Ouvrir l'IDE** (un bouton « ouvrir dans JetBrains / VS Code » sur le conflit) :
  zéro ingénierie, et l'outil de merge y est meilleur. Écarté parce que la fenêtre
  d'Ash est celle qui sait quels agents tournent ; sortir vers l'IDE perd le contexte
  au moment de s'en servir. Reste un complément légitime, pas un remplacement.
- **Tout intégrer** (staging, commit, remotes, stash) : cohérent en apparence, mais
  aucune de ces opérations n'est modifiée par la présence d'un agent. Ash y serait un
  client git médiocre de plus.
