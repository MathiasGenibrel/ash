# ADR-0013 — La fiche de branche vit dans le dépôt, en markdown

- **Statut** : accepté (2026-08-10)
- **Découle de** : [ADR-0012](./0012-worktree-unite-de-travail.md)
- **Modifie** : l'empreinte système de la spec §9

## Contexte

Quand trois agents travaillent sur trois branches, l'intention de chaque branche
s'évapore. Le design y répond par une **fiche par worktree** : le pourquoi, une liste
de tâches, un schéma d'états, ce qui est tranché, ce qui est hors périmètre, la
commande de vérification, et un journal de ce que les agents ont fait.

Deux questions se posent, dans cet ordre. **Où vit ce fichier ?** Et **dans quel
format ?**

La première est la plus lourde. Jusqu'ici, la spec §9 promettait qu'Ash est retirable
sans laisser de traces : il touche les `settings.json` des outils, l'environnement des
bash qu'il crée, `~/.ash/`, et **rien d'autre**. Écrire dans le dépôt de l'utilisateur
casse cette promesse — et pas dans un coin privé : le fichier part avec la branche,
donc avec la *pull request*, donc sous les yeux de gens qui n'ont pas Ash.

## Décision

La fiche est un fichier **`.ash/worktree.md`, versionné dans le dépôt**, committé avec
la branche à laquelle il se rapporte.

Le format est du **markdown standard**, et rien d'autre :

- front matter YAML pour les métadonnées (`type`, `issue`, `branch`, `base`, `status`) ;
- GFM pour le corps — les cases `- [ ]` deviennent la barre de progression, un tableau
  reste un tableau ;
- clôtures `mermaid` pour les schémas.

Pas de MDX, pas de HTML. Rien qui soit propre à Ash : le rendu n'invente aucune
syntaxe, il met en forme du markdown que n'importe quel éditeur affiche déjà.

Ash n'écrit que **dans une seule zone**, délimitée comme les hooks
([ADR-0007](./0007-etats-par-hooks.md)) :

```markdown
<!-- ash:log -->
| agent | work | when |
|---|---|---|
| claude | 4 commits · 15m22s | now |
<!-- /ash:log -->
```

Même régime que pour les `settings.json` : sauvegarde avant écriture, jamais de
modification hors marqueurs, refus d'écrire si le bloc a été édité à la main.

Tout le reste du fichier appartient à l'utilisateur et aux agents.

## Conséquences

- **La spec §9 change de nature.** Ash n'est plus « retirable sans laisser de
  traces » : il est retirable de la *machine* sans traces, mais un dépôt où Ash a
  servi garde ses fiches. C'est assumé — la fiche a précisément de la valeur parce
  qu'elle voyage avec la branche au lieu de rester dans un outil.
- Un agent qui reprend la branche lit la fiche et sait quoi faire. C'est le vrai
  bénéfice, et il ne fonctionne que si le fichier est dans le dépôt.
- Le markdown se relit et se diffe sans Ash. Si Ash disparaît, la fiche reste un
  document lisible ; un format propriétaire aurait laissé un déchet.
- Ash doit prévoir un **mode local** : `.ash/` peut être gitignoré, et l'équipe peut
  ne pas vouloir de ce fichier. Dans ce cas la fiche vit dans `~/.ash/worktrees/` et
  perd son unique avantage. Ash ne doit ni forcer, ni imposer un `.gitignore`.
- **Le bloc `<!-- ash:log -->` peut lui-même partir en conflit** quand deux branches
  ont chacune leur journal et qu'on les fusionne. Ash génère alors un conflit dans un
  fichier qu'il gère seul. La règle est simple et doit être tenue : Ash ne résout
  jamais ce conflit tout seul ; il le traite comme n'importe quel autre.
- Rendre du mermaid impose une dépendance de rendu côté UI, à mettre en regard du
  risque de performance déjà identifié ([ADR-0002](./0002-tauri-rust-portable-pty.md)).

## Alternatives écartées

- **Fichier local dans `~/.ash/`, indexé par worktree** : préserve intégralement la
  promesse de §9, aucun risque de conflit, aucune trace chez les collègues. Écarté
  parce que la fiche perd alors les deux choses qui la justifient — elle ne suit pas
  la branche, et un agent qui travaille sur une autre machine ne la voit pas. C'est
  malgré tout le mode de repli retenu ci-dessus.
- **MDX ou HTML** : composants riches, mise en forme libre. Écarté pour trois
  raisons : les agents écrivent et relisent du markdown sans se tromper, il diffe
  proprement, et MDX comme HTML laissent passer du code arbitraire dans un fichier
  que des agents éditent seuls. Il faudrait en plus maintenir un moteur de rendu.
- **Un ticket dans le tracker** (la fiche vit dans Jira, Linear, OpenProject) : c'est
  déjà là où l'intention est censée vivre. Écarté parce que ça impose une intégration
  par tracker, une authentification, et un aller-retour réseau pour lire deux lignes —
  alors que l'agent, lui, a le dépôt sous la main. Le front matter garde un champ
  `issue` pour faire le lien, sans dépendance.
- **Un trailer dans le message du dernier commit** : voyage parfaitement, zéro
  fichier. Écarté parce qu'un message de commit n'est pas un document vivant : on ne
  coche pas une case dans un commit déjà écrit.
