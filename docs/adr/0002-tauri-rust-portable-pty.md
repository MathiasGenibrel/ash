# ADR-0002 — Tauri + Rust (portable-pty) comme coquille

- **Statut** : accepté (2026-08-07), confirmé par le design (2026-08-10)
- **Dépend de** : [ADR-0001](./0001-application-graphique-avec-pty-embarques.md)

## Contexte

Une fois actée l'idée d'une application graphique embarquant des PTY, il faut choisir
la coquille. Ce choix détermine le langage du backend PTY, le poids de l'application,
et ce qu'on débogue quand un agent se comporte mal.

Contraintes de contexte : outil personnel avant tout, itération UI rapide souhaitée
(le design visé est riche), et une exigence explicite de **patterns anticipant
l'extension** à d'autres fournisseurs.

## Décision

**Tauri**, avec un backend **Rust** utilisant `portable-pty` (la crate de WezTerm),
et une UI en **TypeScript + xterm.js** dans la webview système.

```
src-tauri/src/        Rust
  main.rs             composition root
  features/
    pty/              portable-pty → bash
    probe/            sonde fg_pid + cwd (libproc)
    agents/           trait Adapter, machine à états, socket unix ← ash-event
    git/              refs, worktrees, graphe, état de rebase     (ADR-0011)
    journal/          attribution commit → agent                  (ADR-0014)
    hooks/            bloc délimité dans les settings.json
src/                  TypeScript
  app/                composition root
  features/
    terminal/         xterm.js
    sidebar/          dépôts, worktrees, agents, subagents
    git/              popup de branches, graphe, merge, fiche
    settings/         la fenêtre de réglages
  shared/ipc/         le contrat Rust ↔ TypeScript
```

Le découpage est en feature folders des deux côtés, chaque feature correspondant à une
décision tracée. Détail : `.claude/docs/architecture.md`.

## Conséquences

- Binaire de l'ordre de 15 Mo, empreinte mémoire faible, démarrage immédiat.
- L'UI reste en HTML/CSS/TS : itération rapide sur la sidebar et les états, qui sont
  la vraie surface de design du produit.
- Tout le code hors UI est en Rust. `portable-pty` est éprouvé (WezTerm), les
  bindings `libproc` pour la sonde `cwd` le sont aussi.
- **Risque à lever tôt** : le rendu de xterm.js sous WKWebView. L'addon WebGL n'y est
  pas garanti ; sur une sortie très verbeuse la performance peut se dégrader.
  À mesurer au jalon J1, avant que le reste soit construit dessus.
- WKWebView a ses particularités de rendu (polices, scroll) — à surveiller au design.

## Revue après le design (2026-08-10)

Le choix tient, et l'argument « l'UI est la vraie surface de design » sort renforcé :
le design ajoute un popup filtrable, un graphe à couloirs, un onglet de merge à trois
colonnes et du markdown rendu. Tout cela s'itère en HTML/CSS, pas en Swift.

Trois ajustements :

- **Une dépendance de rendu en plus.** Les clôtures `mermaid` de la fiche de branche
  ([ADR-0013](./0013-fiche-de-branche-dans-le-depot.md)) demandent un moteur de
  diagrammes dans la webview. À charger paresseusement : la fiche est un panneau, pas
  l'écran principal.
- **Le graphe est du Rust.** Le calcul des couloirs se fait côté backend et n'envoie à
  l'UI que des lignes prêtes à dessiner. Sur un dépôt de plusieurs milliers de commits,
  le faire en TypeScript dans la webview reproduirait le risque de performance
  identifié ci-dessous, mais sur une surface qu'on maîtrise pourtant entièrement.
- **Le risque xterm.js sous WKWebView est inchangé** et reste le premier à lever au
  jalon J1. Rien dans le design ne le réduit ; le panneau bas le rend légèrement plus
  aigu, puisque le terminal est alors redimensionné à chaud.

## Amendement du 2026-08-10 — le risque xterm.js est levé

Le spike du jalon J0 a mesuré xterm.js 6.0.0 sous WKWebView, sur un million de lignes
par charge, dans une grille de 326 × 75 (24 450 cellules), la sortie venant du backend
Rust par un `Channel` Tauri. Protocole et chiffres complets :
[`docs/spike-xterm.md`](../spike-xterm.md).

**xterm.js tient, à deux conditions.** Aucune trame au-delà de 34 ms sur les six
combinaisons, latence de frappe **sous charge** à 22–30 ms au p95, et un débit compris
entre 24 et 37 Mo/s selon la charge. Le pire cas — sortie saturée de couleurs — plafonne
à 156 000 lignes/s, ce qui reste au-dessus de tout ce qu'un agent produit.

Les deux conditions ne sont pas négociables :

1. **Le contrôle de flux est obligatoire.** Au-delà de 50 Mo de données non consommées,
   `Terminal.write()` lève « write data discarded, use flow control to avoid losing
   data » et **jette la sortie**. Le premier jet du banc, qui poussait sans attendre,
   s'est bloqué sur exactement cette erreur. La lecture du PTY devra donc être
   acquittée par le rappel de `write()`, pas par le simple retour de `read()`.
   C'est une contrainte de conception pour la feature `pty`, pas un réglage.
2. **L'addon WebGL est retenu.** Il fonctionne dans WKWebView — le repli sur perte de
   contexte n'a jamais été déclenché — et donne 35–37 Mo/s contre 23–26 pour le
   rendu DOM sur les charges textuelles, soit environ 50 % de mieux. Sur la charge
   colorée les deux moteurs se rejoignent : elle est limitée par l'analyseur ANSI, pas
   par le rendu.

**Ce que le spike n'a pas mesuré**, et qui reste à vérifier :

- l'écran de mesure avait un `devicePixelRatio` de **1**. Sur un écran Retina, le coût
  de rendu par cellule est de l'ordre de quatre fois supérieur ; c'est le trou le plus
  important de cette mesure ;
- le redimensionnement à chaud sous une TUI plein écran, que le panneau bas rend aigu
  ([ADR-0003](./0003-zone-terminal-unique.md)). Il n'y avait pas encore de PTY à
  redimensionner ;
- le coût d'un `scrollback` long en usage prolongé — la mesure utilisait 10 000 lignes
  sur un seul terminal, pas quinze onglets ouverts une journée.

La décision d'origine est **confirmée** : Tauri v2, Rust, xterm.js. Rien n'est
renégocié.

## Alternatives écartées

- **Electron + node-pty** : le chemin le plus balisé (c'est VS Code), DevTools
  familiers, zéro Rust. Écarté pour le poids (~150 Mo, ~300 Mo de RAM), le démarrage
  plus lent, et le rebuild natif de `node-pty` à chaque version d'Electron.
- **Bun + webview natif** : très léger et cohérent avec l'outillage existant, mais
  le support PTY y est nettement moins éprouvé et le packaging macOS entièrement
  manuel.
- **Tauri + sidecar bash** (logique métier en scripts bash appelés par l'app) :
  séduisant pour garder l'esprit « client bash », mais introduit une frontière IPC
  supplémentaire et un contrat à tenir, pour un gain surtout esthétique.
