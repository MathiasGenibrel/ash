# ADR-0012 — Le worktree est l'unité de travail, le dépôt est le groupe

- **Statut** : accepté (2026-08-10)
- **Découle de** : [ADR-0011](./0011-git-domaine-de-premier-plan.md)
- **Amende** : [ADR-0004](./0004-workspace-racine-git.md)

## Contexte

[ADR-0004](./0004-workspace-racine-git.md) posait une hiérarchie à deux niveaux :
workspace (= racine git) → onglets. Elle supposait implicitement qu'un dépôt n'a
qu'une racine.

C'est faux dès qu'on utilise `git worktree`, et le design en fait un usage central :
un agent par branche, chacun dans son propre dossier, sans se marcher dessus. C'est
d'ailleurs la seule manière propre de faire tourner trois agents sur trois branches
du même projet — ce qui est le cas nominal d'Ash, pas un cas exotique.

Avec trois worktrees d'`omelette-web`, la règle de l'ADR-0004 produit trois
workspaces sans lien visible, alignés alphabétiquement entre des projets sans rapport.
L'information « ces trois dossiers sont le même projet » disparaît exactement quand
elle devient utile.

Deux détails techniques pèsent sur la décision :

- dans un worktree lié, `.git` est un **fichier** contenant `gitdir: …`, pas un
  dossier. Remonter jusqu'au premier `.git` trouve donc bien la racine du worktree,
  mais ne dit rien du dépôt auquel il appartient ;
- deux worktrees ne peuvent pas être sur la même branche. La branche identifie donc
  déjà le worktree — sauf pour le worktree principal, qui peut être détaché.

## Décision

La hiérarchie passe à **trois niveaux** : dépôt → worktree → onglets.

- Le **worktree** reste l'unité à laquelle un onglet se rattache. C'est lui qui porte
  la branche, l'état de l'arbre, les agents. C'est le « workspace » de l'ADR-0004,
  renommé pour ce qu'il est.
- Le **dépôt** est un groupe d'affichage, résolu en lisant le `gitdir:` du fichier
  `.git` puis en remontant au `commondir`. Il n'a pas d'onglets en propre.
- Un dépôt sans worktree lié s'affiche **à plat**, comme avant : un seul niveau
  visible. La hiérarchie à trois niveaux n'apparaît que quand elle a un sens.

Chaque ligne de worktree porte sa branche et le **suffixe de son dossier**
(`·sidebar`, `·toc`) : c'est ce qui reste lisible en périphérie à 240 px quand deux
worktrees du même dépôt sont côte à côte.

L'épinglage et le repliage restent des propriétés du **worktree**, pas du dépôt.

## Conséquences

- La sidebar dit enfin la vérité : trois agents sur trois branches d'`omelette-web`
  se lisent comme un projet, pas comme trois.
- Ash peut afficher deux colonnes que `git worktree list` ne donne pas — `agents now`
  et `last worked by` — parce qu'il connaît le `cwd` de chaque onglet
  ([ADR-0005](./0005-sonde-cwd-libproc.md)). C'est ce qui rend le tableau utile.
- Un worktree sans agent depuis plusieurs jours **et** avec des fichiers modifiés est
  signalé `stale`. Ash le signale, ne le supprime jamais.
- La suppression d'un worktree devient une opération à part entière : elle doit dire
  ce qu'elle emporte (fichiers modifiés, agent en cours) avant de le faire.
- Coût : un niveau de plus dans le modèle, dans la sidebar, dans la navigation
  clavier, et dans l'ordre de `Cmd+1..9` — qui reste la question ouverte n°5 de la
  spec, désormais un cran plus épineuse.
- Le worktree principal n'a pas de suffixe naturel. Le design lui en donne un tiré du
  nom du dossier (`·web`) ; c'est cohérent, mais ça reste une convention d'Ash.

## Alternatives écartées

- **Garder deux niveaux, ignorer les worktrees** : rien à écrire, et les worktrees
  restent utilisables. Écarté parce que le cas « un agent par branche » est le cas
  nominal du produit, et qu'il est exactement celui qu'on rend illisible.
- **Le dépôt est l'unité, les worktrees sont un détail** (un seul nœud par dépôt,
  les worktrees en onglets) : sidebar plus courte, mais deux worktrees ont des états
  d'arbre différents, parfois un rebase en cours dans l'un et rien dans l'autre. Un
  seul nœud ne peut pas porter deux vérités.
- **Hiérarchie à profondeur libre** (grouper par dossier parent, récursivement) :
  générique et joli sur le papier, mais la profondeur devient imprévisible et dépend
  de l'organisation du disque plutôt que de celle du travail.

## Amendement du 2026-08-26 — « à plat » veut dire *au plus un* worktree

La décision écrit qu'« un dépôt **sans worktree lié** s'affiche à plat ». Le critère
devient : un dépôt qui n'héberge **qu'un seul** worktree s'affiche à plat. Sa ligne unique
porte le nom du dépôt et ce que la ligne de worktree portait — l'état de l'arbre, ou
l'opération en cours ; le suffixe du dossier ne s'y affiche que s'il ajoute quelque chose,
c'est-à-dire quand le dossier du worktree ne porte pas déjà le nom du dépôt. C'est la même
règle que celle qui retire les suffixes d'un groupe où ils ne distinguent plus rien, et elle
couvre le cas courant sans avoir à le nommer : l'arbre principal vit dans le dossier du
dépôt, donc `ash ·ash` ne dirait rien de plus que `ash`. La colonne ne demande **pas** si
c'est l'arbre principal — le fait existe côté Rust mais ne traverse pas la frontière, et le
redériver contredirait [ADR-0009](./0009-cycle-de-vie-des-agents.md). Le compteur
`1 worktree` disparaît avec le niveau qui le portait.

Ce n'est pas une contradiction, c'est l'application de la phrase suivante de la décision :
« la hiérarchie à trois niveaux n'apparaît que quand elle a un sens ». Et l'alternative
écartée — « le dépôt est l'unité, les worktrees sont un détail » — l'était pour un motif
précis : « deux worktrees ont des états d'arbre différents, parfois un rebase en cours dans
l'un et rien dans l'autre. Un seul nœud ne peut pas porter deux vérités. » Avec un seul
worktree, il n'y a qu'une vérité, et le motif du refus ne s'applique pas. Ce que le niveau
intermédiaire produisait dans ce cas, c'était `ash` → `ash ·ash` → `claude` : trois lignes
pour une seule.

Le cas est le nominal de l'usage réel, et non un cas limite : l'agent de tête est un
orchestrateur, il vit dans l'arbre principal, et ce sont ses sous-agents qui partent en
worktree — sans onglet propre, donc sans ligne de worktree.

Ce qui ne change pas :

- le **worktree** reste l'unité à laquelle un onglet se rattache. Le backend continue de
  situer chaque onglet dans son worktree ; c'est la colonne, et elle seule, qui décide de ne
  pas dessiner un niveau ;
- l'**épinglage** et le **repli** restent des propriétés du worktree. La ligne aplatie est
  celle du worktree, sous le nom du dépôt : elle replie, épingle et ouvre ce worktree. Les
  deux clés de repli restent distinctes — la racine du worktree d'un côté, `repo:<id>` de
  l'autre —, donc un dépôt qui passe d'une forme à l'autre ne perd aucun des deux ;
- le **niveau intermédiaire revient dès le second worktree**, sans redémarrage, et repart
  quand le dernier onglet d'un des deux se ferme sans qu'une épingle le retienne.

