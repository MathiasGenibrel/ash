# ADR-0001 — Application graphique embarquant des PTY

- **Statut** : accepté (2026-08-07), confirmé par le design (2026-08-10)

## Contexte

Le besoin initial était formulé comme « un client bash poussé » pour piloter
plusieurs agents de code. Deux exigences ergonomiques sont apparues et se sont
révélées décisives :

- utilisation **clavier et souris** à parts égales ;
- raccourcis **`Cmd+1`, `Cmd+2`, …** à la macOS.

Or sur macOS, `Cmd+N` n'atteint quasiment jamais une application terminal :
l'émulateur l'intercepte. Une application TUI ne peut y accéder que si l'utilisateur
configure son émulateur pour traduire ces touches en séquences d'échappement — une
contrainte qui se propage à toute installation.

Par ailleurs, l'utilisateur a explicitement écarté la réécriture d'un terminal : il
veut *une autre interface autour* de son bash, pas un bash simulé.

## Décision

Ash est une **application graphique** qui embarque de **vrais PTY**. Chaque onglet
est un `bash` réel dans lequel l'utilisateur lance ses outils normalement ; rien
n'est simulé ni réinterprété.

Le clavier et la souris appartiennent à l'application, qui décide de ce qu'elle
transmet au PTY.

## Conséquences

- `Cmd+1..9`, le clic, le scroll et le drag fonctionnent nativement, sans configurer
  quoi que ce soit.
- La sidebar peut être une vraie interface (repliage, épinglage, hiérarchie) plutôt
  qu'un pane redessiné à la main.
- Ash doit fournir un émulateur de terminal correct : les outils visés sont des TUI
  plein écran. Le rendu est délégué à xterm.js (voir [ADR-0002](./0002-tauri-rust-portable-pty.md)).
- Ash n'est pas utilisable à distance ni en SSH. Accepté.
- Ce n'est plus « du bash ». La logique reste néanmoins scriptable et lisible ; le
  point d'extension par outil est traité en [ADR-0008](./0008-abstraction-adapter.md).

## Revue après le design (2026-08-10)

Confirmée sans réserve, et renforcée. Le design ajoute des surfaces qu'une TUI
n'aurait pas pu porter : un popup de branches filtrable ancré sur le pied de fenêtre,
un panneau bas repliable, un onglet de merge à trois colonnes, du markdown rendu.
Aucune n'est un terminal ; toutes vivent dans la même fenêtre que lui.

Le groupe de raccourcis git (`⌘⌃B` / `G` / `W` / `M` / `I`,
[ADR-0011](./0011-git-domaine-de-premier-plan.md)) confirme l'argument d'origine : ces
combinaisons sont hors de portée d'une TUI sans configurer l'émulateur, et `⌃B` seul —
la solution d'une TUI — est réclamé par tmux.

## Alternatives écartées

- **TUI + configuration de l'émulateur** (Ghostty/kitty/WezTerm traduisant `Cmd+1`
  en séquence d'échappement) : fonctionne, mais impose une configuration externe et
  une souris de moindre qualité, avec des conflits de gestion du drag/scroll entre
  Ash et le programme enfant.
- **tmux comme socle** : donne panes, detach et scrollback gratuitement, mais la
  souris et les raccourcis `Cmd` restent hors de portée, et la sémantique de tmux
  contraint l'UI.
- **TUI avec raccourcis `Ctrl`/leader** : portable partout, mais abandonne
  l'ergonomie macOS demandée.
- **Application native Swift** : raccourcis et perfs parfaits, mais itération UI
  beaucoup plus lente pour atteindre le rendu visé, et enfermement macOS total.
