# Ash — Brief design : écran de réglages

Conçois les réglages de **Ash**, une application macOS de terminal augmenté qui
supervise des agents de code (Claude Code, Codex, Kimi, opencode).

## Rappel du produit

Ash entoure un vrai shell, il ne le remplace pas. L'utilisateur lance ses agents
lui-même (`claude`, `claude-perso`, `codex`…) ; Ash les découvre automatiquement et
affiche dans une sidebar leur état : `working` / `waiting` / `done` / `idle` /
`error`. L'écran principal est une sidebar de workspaces à gauche et un terminal
unique à droite.

Ces réglages sont la seule partie « formulaire » du produit. Ils doivent rester dans
la même langue visuelle : terminal-natif, dense, monospace, sobre. Surtout pas un
panneau de préférences macOS générique.

## Forme générale

Fenêtre de réglages séparée (pas une modale par-dessus le terminal), avec une
navigation latérale à quatre sections :

```
Outils · Raccourcis · Apparence · Notifications
```

La fenêtre doit rester lisible en 800 × 600.

---

## Section 1 — Outils

C'est ici que l'utilisateur déclare quelles commandes sont des agents.

**Point essentiel** : il a *deux* abonnements Claude, donc deux commandes distinctes
pointant vers deux dossiers de configuration différents. Le design doit rendre ce
cas évident et non exceptionnel — on doit pouvoir en ajouter `n`.

Chaque entrée comporte :

- le nom de la commande à reconnaître — ex. `claude`, `claude-perso`
- un libellé d'affichage optionnel — ex. « Perso », « Pro »
- l'adaptateur — `claude-code` | `codex` | `generic`
- le chemin du dossier de configuration — ex. `~/.claude`, `~/.claude-perso`
- le résultat de la vérification de ce chemin
- l'état des hooks, et l'action associée

### 1a. Vérification d'un chemin saisi à la main — à concevoir en priorité

Un chemin tapé à la main peut être n'importe quoi. Ash doit le **vérifier** et le
dire visuellement, avant toute écriture. La vérification contrôle, dans l'ordre :

1. le dossier existe et est lisible ;
2. il porte bien la signature de l'adaptateur choisi (pour `claude-code` : un
   `settings.json`, un dossier `projects/`, etc.) ;
3. la commande associée existe dans le `PATH` et répond ;
4. la commande, lancée avec ce dossier de configuration, l'utilise réellement —
   c'est le seul test qui prouve que le couple *commande + chemin* est cohérent.

Conçois les **cinq états** de ce résultat, tous visibles sur la ligne de l'entrée :

| État | Ce qu'il montre |
|---|---|
| non vérifié | chemin modifié, pas encore testé |
| vérification en cours | — |
| valide | ce qui a été reconnu, en une ligne : « Claude Code 2.1.198 · 4 projets · dernière activité 2 h » |
| valide avec réserve | le dossier est bon mais quelque chose cloche : « commande introuvable dans le PATH » |
| invalide | dit précisément **ce qui** a échoué, et propose une correction : « ce dossier ne ressemble pas à une config Claude Code — adaptateur `generic` à la place ? » |

Le test 4 est le plus coûteux : il faut lancer la commande. Le design doit donc
prévoir qu'un résultat **arrive en deux temps** — une validation immédiate sur les
tests 1 à 3, puis une confirmation plus lente.

Une entrée non vérifiée ou invalide **ne doit pas pouvoir recevoir les hooks**. Le
design doit montrer ce blocage sans le cacher.

Prévois aussi un bouton « tout revérifier » pour la liste entière, et le fait que la
vérification se relance automatiquement à chaque changement de chemin ou
d'adaptateur.

### 1b. Réinitialiser un provider — à concevoir

Un bouton par entrée qui remet le chemin par défaut de l'adaptateur choisi
(`claude-code` → `~/.claude`, `codex` → son chemin par défaut, etc.), puis relance
la vérification.

**Cas limite à traiter explicitement** : l'utilisateur a deux entrées `claude-code`.
Réinitialiser la seconde la ferait pointer sur le même dossier que la première.
Montre comment Ash le signale — ce n'est pas une erreur système, mais deux entrées
sur le même dossier n'ont plus de sens. Le doublon doit être visible sur **les deux
lignes** concernées, pas seulement sur celle qu'on vient de toucher.

Ce cas n'est pas tordu, c'est le cas nominal de cet utilisateur : le défaut d'un
adaptateur ne peut être bon que pour l'une de ses deux entrées. Si ta proposition
suggère un chemin par défaut *par entrée* plutôt que *par adaptateur*, dis-le.

### 1c. Hooks

Pour poser ses états, Ash écrit un bloc délimité dans le `settings.json` du dossier
de configuration. Conçois les **cinq états** de cette ligne :

1. installés et à jour → action : retirer ;
2. absents → action : installer ;
3. version antérieure → action : mettre à jour ;
4. bloc modifié à la main, donc conflit → Ash refuse d'écrire et propose le diff ;
5. bloqué parce que le chemin n'est pas vérifié → l'action d'installation est
   indisponible, et on dit pourquoi.

Montre aussi qu'une sauvegarde `.bak` est faite avant toute écriture.

### 1d. Divers

- l'ajout d'une entrée, et l'état vide de la liste ;
- l'adaptateur `generic` présenté comme **dégradé** : l'outil sera visible mais
  seulement en `idle` / `done` / `error`, jamais « attend une réponse ».

C'est la section où l'utilisateur autorise une application à écrire dans ses
fichiers de configuration. Le design doit être franc là-dessus : montrer quel
fichier, ce qui y est écrit, et comment tout retirer.

---

## Section 2 — Raccourcis

Liste éditable, groupée :

```
Navigation   Cmd+1..9        sélectionner le n-ième onglet
             Cmd+B           replier / déplier la sidebar
Onglets      Cmd+N           nouvel onglet dans le workspace courant
             Cmd+Shift+N     nouvel onglet à ~ (nouveau workspace)
             Cmd+W           fermer l'onglet
Terminal     Cmd+K           effacer le scrollback
```

Conçois :

- la ligne au repos — action à gauche, combinaison à droite, en monospace ;
- la ligne en cours de capture — « appuyez sur une combinaison », avec échappement ;
- un conflit : deux actions sur la même combinaison, avec la résolution proposée ;
- le retour au réglage par défaut, par ligne et global ;
- l'avertissement quand la combinaison choisie est déjà prise par macOS ou
  interceptée par le terminal.

---

## Section 3 — Apparence

- **Thème** : Système / Clair / Sombre — avec un aperçu réel, pas trois boutons
  radio. L'aperçu doit montrer la sidebar avec ses états d'agent, puisque c'est là
  que le thème compte vraiment.
- Police du terminal et taille (liste des monospace installées).
- Largeur de la sidebar.
- Densité de la sidebar : confortable / compacte.

---

## Section 4 — Notifications

Ce qui déclenche une notification macOS quand Ash n'est pas au premier plan :

| Événement | Défaut |
|---|---|
| `waiting` — un agent attend une réponse | activé |
| `error` — un agent a échoué | activé |
| `done` — un agent a terminé | désactivé |

Plus : l'état « permission macOS non accordée », avec le chemin pour l'accorder.

---

## Contraintes

- macOS. Thème **clair et sombre** à concevoir pour tout ce qui précède.
- Le thème clair est nouveau : montre aussi comment la sidebar de l'écran principal
  et ses cinq états d'agent se traduisent en clair, sans perdre la hiérarchie ni
  l'urgence de `waiting`.
- Monospace partout, y compris dans les formulaires. Libellés secondaires discrets.
- Pas d'icônes décoratives, pas d'ombres portées, pas de dégradés.
- Tout doit être atteignable au clavier comme à la souris.
- Les chemins de fichiers sont des données de première classe : lisibles en entier,
  copiables, tronqués par le milieu si nécessaire.

---

## Écrans à produire

1. **Outils** — cas nominal : `claude` (vérifié, hooks installés), `claude-perso`
   (vérifié, hooks installés), `codex` (vérifié, hooks absents).
2. **Outils** — la planche des cinq états de vérification d'un chemin, côte à côte,
   en gros.
3. **Outils** — une entrée invalide, avec la correction proposée, et l'action
   d'installation des hooks bloquée en conséquence.
4. **Outils** — le doublon après réinitialisation : deux entrées `claude-code` sur
   le même dossier.
5. **Outils** — le conflit de hooks (bloc modifié à la main), avec le diff.
6. **Outils** — formulaire d'ajout, et état vide de la section.
7. **Raccourcis** — liste au repos, une ligne en cours de capture, un conflit.
8. **Apparence** — les trois thèmes, avec l'aperçu de sidebar.
9. **Notifications** — dont l'état « permission non accordée ».
10. **Sidebar de l'écran principal en thème clair**, avec ses cinq états côte à côte.
