# ADR-0007 — Les états viennent des hooks de l'outil, pas de la sortie

- **Statut** : accepté (2026-08-07)
- **Complété par** : [ADR-0008](./0008-abstraction-adapter.md)

## Contexte

La valeur d'Ash tient à une seule information : cet agent **travaille**, **attend une
réponse**, ou **a fini**. Sans elle, Ash n'est qu'un terminal à onglets.

Or les outils visés n'exposent pas la même chose. Claude Code dispose de véritables
hooks (`Notification`, `Stop`, `PreToolUse`, `SessionEnd`) : l'agent peut déclarer
son état lui-même. Les autres restent à qualifier.

Quatre sources possibles : les hooks, une heuristique sur le flux PTY, des signaux
système, ou les fichiers de session écrits sur disque.

## Décision

**Les hooks font autorité.** Ash déclare dans `~/.ash/config.toml` les commandes
reconnues et le dossier de configuration de chacune, puis y écrit un **bloc
délimité** :

```
~/.ash/config.toml
  [[command]] match="claude"       config="~/.claude"
  [[command]] match="claude-perso" config="~/.claude-perso"

→ dans chaque settings.json :
  // ash:begin  (ne pas éditer)
  hooks → ash-event --tab $ASH_TAB_ID <state>
  // ash:end
```

La **corrélation hook → onglet** se fait par `ASH_TAB_ID`, variable d'environnement
posée par Ash à la création du `bash` et héritée par toute la descendance —
y compris par les processus de hook. Aucune devinette par `cwd` ou par horodatage.

La sonde système ne sert qu'à détecter la **disparition** du processus, pas à
inférer un état.

## Conséquences

- Les états sont exacts, y compris `waiting`, qui est indevinable de l'extérieur et
  qui est justement le seul état méritant d'interrompre l'utilisateur.
- Ça marche même quand `claude` est lancé hors d'Ash — le bloc vit dans la
  configuration de l'outil, pas dans l'environnement.
- **Ash écrit dans les fichiers de l'utilisateur.** C'est assumé, et encadré :
  sauvegarde `.bak` avant écriture, bloc délimité, désinstallation en un geste, et
  refus d'écrire silencieusement si le bloc a été modifié à la main.
- Les deux comptes Claude nécessitent deux installations de bloc, une par dossier de
  configuration. Le modèle `[[command]]` le couvre nativement, et s'étend à `n`
  comptes.
- **Dépendance forte à ce que chaque outil expose.** Pour Claude Code c'est acquis.
  Pour codex, kimi et opencode, ça reste à qualifier — c'est le principal risque du
  projet, et la raison d'être de [ADR-0008](./0008-abstraction-adapter.md).
- Un outil sans hook ne pourra fournir que `idle` / `done` / `error` via la sonde.
  Si ce n'est pas suffisant à l'usage, il faudra réintroduire un moteur heuristique —
  décision explicitement repoussée, pas exclue.

## Alternatives écartées

- **Heuristique sur le flux PTY** (détecter un spinner, « esc to interrupt », un
  motif de question, un silence après le prompt) : un seul mécanisme universel, qui
  marcherait même sur un outil inconnu et sans rien écrire nulle part. Écarté parce
  que fragile par nature — ça casse à chaque changement de rendu d'un outil — et
  parce que les faux positifs sur `waiting` détruiraient la confiance dans la
  notification, qui est le cœur du produit.
- **Signaux système seuls** (état du processus, CPU, dernière écriture PTY) :
  increvable et sans maintenance, mais incapable de distinguer « attend une réponse »
  de « terminé » — exactement l'information recherchée.
- **Lecture des transcripts de session** (`~/.claude/projects/**/*.jsonl` et
  équivalents) : très riche et sans écriture dans les configs, mais format non
  documenté, instable, différent par outil, et sujet à la latence de flush.
- **Protocole imposé aux agents** (marqueurs OSC émis par l'agent) : signal parfait
  et uniforme, mais n'existe que si l'outil coopère — impossible sur un binaire fermé.
