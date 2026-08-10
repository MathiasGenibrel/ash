# ADR-0003 — Un seul terminal à la fois, pas de splits de terminaux

- **Statut** : accepté (2026-08-07), **reformulé après le design (2026-08-10)**

## Contexte

La maquette d'origine montrait deux terminaux côte à côte dans un même onglet
(un agent à gauche, un serveur de développement à droite). En clarifiant, il est
apparu que l'intention réelle était autre : une **sidebar** de navigation à gauche,
et le shell à droite — « soit un bash classique si aucune session d'agent n'est
lancée, soit claude, codex, peu importe ».

Le modèle de découpe est le plus gros morceau d'UI d'un émulateur de terminal :
arbre de layout, redimensionnement, focus, redistribution à la fermeture d'un pane.

## Décision

**Un seul terminal est visible à la fois** : celui de l'onglet sélectionné. Aucun
split de terminaux, ni horizontal ni vertical.

Ce qui aurait été un second pane devient un **autre onglet du même worktree**, atteint
par `Cmd+1..9` ou par la sidebar.

La règle porte sur les **terminaux**, pas sur la mise en page. Les surfaces qui ne
sont pas des terminaux — le panneau bas, un onglet non-PTY — ne sont pas concernées :
elles n'ont ni PTY, ni focus clavier ambigu, ni question « où va ce que je tape ».
C'est exactement ce que la décision protégeait.

## Conséquences

- Pas d'arbre de layout à implémenter, pas de gestion de resize inter-panes. Le
  budget d'ingénierie va au différenciateur réel : la supervision des agents.
- La sidebar devient le véritable outil de navigation, ce qui est cohérent avec le
  reste du produit.
- L'utilisateur ne peut pas voir un agent et son serveur de développement en même
  temps. Accepté : c'est un aller-retour `Cmd+1` / `Cmd+2`.
- Si le besoin de simultanéité de deux **terminaux** revient à l'usage, l'option la
  moins coûteuse restera un panneau auxiliaire portant un second PTY, plutôt qu'un
  vrai système de splits. Non retenu.

## Reformulation après le design (2026-08-10)

La rédaction d'origine — « une seule zone terminal, pas de splits » — interdisait par
accident ce que le design fait, et qui n'a rien à voir avec des splits de terminaux :

- un **panneau bas repliable** qui prend de la hauteur au terminal, pour le graphe
  (`⌘⌃G`), les worktrees (`⌘⌃W`) et la fiche de branche (`⌘⌃I`) ;
- un **onglet de merge** à trois colonnes (`yours` / `result` / `theirs`) qui n'a pas
  de PTY du tout.

Deux règles encadrent ces surfaces, et remplacent l'interdiction trop large :

1. **Un onglet, au plus un PTY.** Un onglet est soit un terminal, soit une surface
   d'outil (merge). Jamais deux terminaux dans un onglet.
2. **Le panneau bas ne contient jamais de terminal.** Il se replie et rend sa hauteur
   au terminal ; il ne prend jamais le focus clavier par lui-même.

Ce que la décision d'origine évitait — l'arbre de layout, le redimensionnement
inter-panes, la redistribution à la fermeture — reste évité : le panneau bas est une
hauteur unique et réglable, pas un arbre.

Le seul coût nouveau est réel : redimensionner le terminal à chaud quand le panneau
s'ouvre, ce qui déclenche un `SIGWINCH` vers une TUI plein écran en cours
d'exécution. À vérifier au jalon où le panneau arrive.

## Alternatives écartées

- **Splits libres** (`Cmd+D`, drag à la souris, façon iTerm) : flexibilité maximale,
  mais retarde considérablement la partie supervision.
- **Deux zones fixes** (principale + auxiliaire repliable) : couvre la maquette pour
  une fraction du travail. Gardé en réserve, pas retenu en v1.
- **Grille par workspace** (tous les agents d'un workspace en mosaïque) : très adapté
  à la supervision parallèle, mais s'éloigne du modèle d'onglets décrit par
  l'utilisateur (`Cmd+N` dans le workspace courant).
