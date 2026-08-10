# Spike — xterm.js sous WKWebView

> Mesure du 2026-08-10. Issue #2, jalon J0. Verdict : **xterm.js tient**, sous deux
> conditions. L'amendement correspondant est dans
> [ADR-0002](./adr/0002-tauri-rust-portable-pty.md).

La spec désigne deux fois le rendu de xterm.js dans WKWebView comme le risque à lever
avant de construire quoi que ce soit dessus. Ce document dit comment il a été mesuré,
ce qui a été trouvé, et ce qui ne l'a pas été.

## Protocole

Le banc est dans `src-tauri/src/spike.rs` et `src/spike/bench.ts`. Il est **jetable** :
il part avec le spike, ou reste pour re-mesurer, mais il ne sert de modèle à rien.

- **La sortie est générée en Rust** et poussée par un `Channel` Tauri, c'est-à-dire par
  le chemin que le PTY empruntera. La générer en TypeScript aurait mesuré le moteur de
  rendu seul, pas la chaîne.
- **Morceaux de 64 Kio** — l'ordre de grandeur de ce qu'un `read()` sur un master PTY
  macOS rend sous forte charge.
- **Un million de lignes par combinaison**, six combinaisons : deux moteurs de rendu
  (DOM, WebGL) × trois charges.
- **Grille 326 × 75** — 24 450 cellules, fenêtre maximisée, `scrollback` 10 000.
- **Aucune temporisation** : on cherche le plafond.

Les trois charges :

| Charge | Ce qu'elle imite | Octets par ligne |
|---|---|---|
| `test` | un `bun test` verbeux, un peu de couleur | ~60 |
| `cat` | `cat` d'un fichier source, lignes longues, aucun échappement | ~80 |
| `color` | sortie saturée de séquences ANSI — pire cas de l'analyseur | ~153 |

Trois mesures, et pourquoi celles-là :

- **débit soutenu** — un `cat` ne doit pas prendre plus de temps dans Ash qu'ailleurs ;
- **temps de trame** — c'est lui qui décide si la fenêtre « rame ». On garde p95 et
  maximum : une moyenne flatteuse cache une trame de 300 ms, qui elle se voit ;
- **latence frappe → peinture, pendant le flux** — le vrai grief contre un terminal
  lent, c'est de taper pendant que ça défile. Mesurée sous charge, jamais au repos.

Le débit est chronométré jusqu'au **rappel de `write()`**, seul signal fiable de
« consommé » : xterm.js met les écritures en file et rend en différé, donc chronométrer
l'appel aurait mesuré la vitesse à laquelle on remplit une file d'attente.

## Résultats

`devicePixelRatio` 1 · un million de lignes par combinaison · backend en profil debug.

| Moteur | Charge | Durée | Mo/s | Lignes/s | Trame p50/p95/max (ms) | Frappe p50/p95/max (ms) | Trames > 34 ms |
|---|---|---|---|---|---|---|---|
| DOM | `test` | 2,27 s | 25,6 | 440 335 | 13 / 16 / 33 | 17 / 28 / 28 | 0 |
| DOM | `cat` | 3,30 s | 23,4 | 302 663 | 14 / 17 / 23 | 22 / 30 / 38 | 0 |
| DOM | `color` | 6,40 s | 24,0 | 156 348 | 13 / 15 / 23 | 18 / 27 / 28 | 0 |
| **WebGL** | `test` | **1,64 s** | **35,4** | **608 643** | 13 / 15 / 27 | 20 / 22 / 22 | 0 |
| **WebGL** | `cat` | **2,08 s** | **37,2** | **481 696** | 13 / 15 / 18 | 20 / 22 / 22 | 0 |
| **WebGL** | `color` | 6,30 s | 24,4 | 158 680 | 13 / 15 / 23 | 20 / 21 / 22 | 0 |

Le contexte WebGL n'a jamais été perdu : le repli câblé sur `onContextLoss` ne s'est
pas déclenché une fois.

## Verdict

**xterm.js tient, avec réglages.** Les deux réglages ne sont pas des options.

### 1. Le contrôle de flux est obligatoire

C'est la découverte du spike, et elle vaut plus que les chiffres.

Au-delà de **50 Mo de données non consommées**, `Terminal.write()` lève
`write data discarded, use flow control to avoid losing data` et **jette la sortie**.
Le premier jet du banc poussait tout sans attendre : il s'est bloqué exactement là,
sur un `cat` d'un million de lignes.

Ce n'est pas une bizarrerie du banc. C'est le régime qu'un `cat` d'un gros fichier
impose à un vrai PTY, et **un terminal qui perd de la sortie est un terminal cassé**.

Conséquence pour la feature `pty` (#3) : la boucle de lecture doit être **acquittée par
le rappel de `write()`**, pas par le retour de `read()`. Le banc utilise une fenêtre de
huit morceaux de 64 Kio — 512 Kio en vol, cent fois sous le seuil, et assez pour que le
canal ne soit jamais à sec. C'est le point de départ à reprendre, pas une valeur sacrée.

### 2. L'addon WebGL est retenu

Il fonctionne dans WKWebView, et il donne environ **50 % de débit en plus** sur les
charges textuelles (35–37 Mo/s contre 23–26). Sur la charge colorée les deux moteurs se
rejoignent : elle est limitée par l'analyseur ANSI, pas par le rendu — aucun moteur ne
rattrapera ça.

Le repli sur `onContextLoss` reste indispensable : WKWebView peut perdre son contexte
WebGL sous pression mémoire, et sans écoute la perte se lirait comme un écran figé.

## Ce que le spike n'a pas mesuré

Ces trous sont la partie honnête du verdict.

- **`devicePixelRatio` valait 1.** Sur un écran Retina, le coût de rendu par cellule est
  de l'ordre de quatre fois supérieur. **C'est le trou le plus important** : la mesure
  est à refaire sur l'écran intégré d'un portable avant de considérer le risque clos.
- **Le redimensionnement à chaud sous une TUI plein écran** n'a pas été testé : il n'y
  avait pas de PTY à redimensionner. C'est le vrai critère du panneau bas (#24), et il
  reste entier.
- **Le `scrollback` en usage prolongé** : 10 000 lignes sur un terminal, pas quinze
  onglets ouverts une journée.
- **Le backend tournait en profil debug.** Les chiffres sont donc pessimistes du côté
  Rust — la génération de la sortie est comptée dans la durée.

## Rejouer la mesure

```bash
bun install
VITE_SPIKE=1 bun run tauri dev    # écrit spike-results.json à la racine
```

Le banc est derrière un drapeau, éteint par défaut : une application qui démarre sur un
banc de mesure est une application cassée. Sans `VITE_SPIKE`, ni le banc ni xterm.js
n'entrent dans le bundle — l'import est dynamique.

Le rapport est écrit sur disque plutôt qu'affiché : un chiffre lu sur une capture
d'écran n'est pas une mesure.
