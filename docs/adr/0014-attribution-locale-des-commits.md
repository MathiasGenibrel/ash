# ADR-0014 — L'attribution d'un commit à un agent est un journal local

- **Statut** : accepté (2026-08-10)
- **Découle de** : [ADR-0011](./0011-git-domaine-de-premier-plan.md)
- **Amende** : [ADR-0009](./0009-cycle-de-vie-des-agents.md) — la persistance

## Contexte

La colonne `by` du graphe est la seule chose qu'un client git n'a pas : elle dit que
`8f3a1c2` a été écrit par `claude`, il y a quatre minutes, et le panneau de droite
garde le prompt qui l'a produit. C'est ce qui rend relisible un historique écrit par
plusieurs agents — sans elle, git affiche le nom de l'utilisateur pour tout, y compris
pour ce que `codex` a commité pendant la nuit.

Cette donnée n'existe nulle part. Il faut décider où on l'écrit, et le choix engage
le produit bien au-delà d'un affichage : la spec §3 affirmait jusqu'ici que **rien**
ne survit à la fermeture, sauf les worktrees épinglés.

Trois supports possibles : le commit lui-même, `git notes`, ou un journal local.

Une contrainte domine : **le rebase**. C'est l'opération centrale du design, et elle
réécrit les `sha`. Tout mécanisme dont la clé est le `sha` seul perd l'attribution
exactement dans le scénario que le produit met en avant.

## Décision

L'attribution vit dans un **journal local, append-only, sous `~/.ash/`**, un fichier
par dépôt. Rien n'est écrit dans le dépôt de l'utilisateur.

Ash observe la naissance des commits — il connaît le `cwd` de chaque onglet, l'agent
en avant-plan, et l'instant — et enregistre :

```
repo, sha, author_date, subject, agent, tab_id, session_started, prompt
```

La résolution à l'affichage se fait en deux temps :

1. correspondance par `sha` ;
2. à défaut — donc après un rebase, un amend ou un cherry-pick — correspondance par
   **(`author_date`, `subject`)**, que git préserve dans ces trois opérations.

Un commit sans correspondance n'est pas orphelin : il affiche simplement le nom
d'auteur git, comme dans n'importe quel client. La colonne `by` ne montre un nom
d'agent que quand Ash l'a réellement observé.

## Conséquences

- **La spec §3 est fausse en l'état.** Il existe désormais un état qui survit aux
  sessions. [ADR-0009](./0009-cycle-de-vie-des-agents.md) tient toujours — les PTY
  meurent avec l'application, il n'y a pas de démon — mais « rien d'autre ne survit à
  la fermeture » doit être réécrit.
- L'attribution est **locale à la machine**. Un collègue ne la voit pas ; un
  changement de poste la perd. C'est cohérent avec deux choses : la promesse de §9, et
  le fait qu'un prompt est une donnée personnelle qu'on ne publie pas d'office dans
  l'historique d'une équipe.
- L'attribution fonctionne **sans hook**, donc pour tous les outils, y compris ceux en
  adaptateur `generic` ([ADR-0008](./0008-abstraction-adapter.md)). Elle ne dépend que
  de la sonde. C'est le seul volet du produit qui échappe à la question ouverte n°1.
- Le journal contient des prompts. C'est un fichier à traiter comme tel : pas de
  synchronisation, pas de télémétrie, purge explicite possible.
- La correspondance de repli est heuristique. Deux commits de même sujet à la même
  seconde sont indiscernables. Cas rare, conséquence bénigne : un nom d'agent
  possiblement faux dans une colonne d'affichage.
- Il faut détecter les commits : surveiller `.git/logs/HEAD` par dépôt, pas sonder
  `git log`.

## Alternatives écartées

- **`git notes`** : le mécanisme prévu par git pour attacher de la donnée à un commit
  sans le réécrire, et il peut être poussé. Écarté parce que les notes **ne suivent
  pas le rebase** : elles restent attachées aux anciens `sha` et disparaissent de la
  branche réécrite. Perdre l'attribution pendant un rebase, dans un produit dont
  l'écran 4d est un rebase, est disqualifiant.
- **Trailer dans le message de commit** (`Ash-Agent: claude`) : voyage parfaitement,
  survit au rebase, lisible par tous. Écarté pour deux raisons. Ash ne rédige pas les
  commits — c'est l'agent qui commite — donc il faudrait installer un hook
  `prepare-commit-msg` **dans le dépôt de l'utilisateur**, une empreinte bien plus
  intrusive que celle de [ADR-0007](./0007-etats-par-hooks.md). Et ça publie le nom de
  l'agent, voire le prompt, dans l'historique partagé d'une équipe qui n'a rien
  demandé. À reconsidérer si l'attribution partagée devient un besoin explicite : ce
  serait alors une option, jamais le défaut.
- **Ne rien stocker, déduire à l'affichage** (croiser l'horodatage du commit avec les
  sessions d'agent en mémoire) : zéro persistance, la spec §3 reste vraie. Écarté
  parce que ça ne marche que pour la session en cours, alors que la valeur de la
  colonne est justement de relire l'historique d'hier.
- **Une base de données** (SQLite) plutôt qu'un journal append-only : plus commode à
  interroger. Repoussé, pas exclu — le volume est faible et un fichier texte reste
  inspectable et supprimable à la main, ce qui compte pour un fichier qui contient des
  prompts.
