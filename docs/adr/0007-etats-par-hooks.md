# ADR-0007 — Les états viennent des hooks de l'outil, pas de la sortie

- **Statut** : accepté (2026-08-07), précisé le 2026-08-11, amendé le 2026-08-12, amendé le
  2026-08-13, précisé le 2026-08-24
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
  hooks → ash-event <state> --tab $ASH_TAB_ID
  // ash:end
```

La **corrélation hook → onglet** se fait par `ASH_TAB_ID`, variable d'environnement
posée par Ash à la création du `bash` et héritée par toute la descendance —
y compris par les processus de hook. Aucune devinette par `cwd` ou par horodatage.

La sonde système ne sert pas à inférer ce que l'agent **fait**.

### Précision du 2026-08-11 — ce que la sonde a le droit de dire

La rédaction initiale disait « la sonde ne sert qu'à détecter la **disparition** du
processus ». Trop large : elle se lisait comme interdisant `working`, que
`features/pty/registry.rs` produit depuis le jalon J1.

Ce que cette ADR écarte, c'est l'**analyse de la sortie** du PTY — reconnaître un
spinner, un « esc to interrupt », un motif de question, un silence après le prompt. C'est
la première alternative écartée ci-dessous, et elle reste écartée. `tcgetpgrp` n'en est
pas : il ne lit rien de ce que l'agent écrit, il répond à une question de **présence** —
le shell tient-il encore l'avant-plan de son terminal, ou l'a-t-il cédé à autre chose.

La sonde peut donc dire qu'un agent est **là** (`working`), qu'il n'y en a pas (`idle`),
et qu'il est **parti** et comment (`done` / `error`). Elle ne peut rien dire de plus.

**`waiting` n'a jamais d'autre source qu'un hook**, et c'est la partie de cette décision
qui ne se relâche pas : c'est le seul état qui justifie d'interrompre l'utilisateur, et
un faux positif y détruirait la confiance dans la notification.

Voir l'[amendement correspondant d'ADR-0008](./0008-abstraction-adapter.md).

### Amendement du 2026-08-12 — des marqueurs **par entrée**, plus un bloc délimité

La rédaction initiale disait « un **bloc délimité** », et l'implémentation l'a tenue au
mot : `ash:begin` / `ash:end` encadraient une région du `settings.json`, et rien n'était
jamais écrit hors de cette région.

**Ce que le code a montré.** Le premier utilisateur réel avait déjà un hook dans son
`~/.claude/settings.json` — un `PreToolUse` posé par un autre outil. Ash refusait alors
d'écrire, avec raison : ajouter une seconde clé `"hooks"` au même objet JSON aurait
désactivé la sienne en silence, puisque le dernier arrivé l'emporte. Mais la seule issue
offerte était « déplace-les toi-même dans le bloc d'Ash, ou vise un autre dossier ». La
fonction centrale du produit devenait donc **inatteignable** pour exactement le public
qu'il vise : celui qui outille déjà son agent. Un refus n'est pas une impasse ; il l'était
devenu.

**Ce qui est décidé.** Ash n'encadre plus une région du fichier : il marque **ses propres
objets** dans les tableaux de l'utilisateur. Chaque entrée qu'il écrit porte son marqueur,
donc se reconnaît elle-même, donc peut cohabiter ligne à ligne avec celles de
l'utilisateur et se retirer sans toucher au reste.

```
→ dans chaque settings.json, dans les tableaux d'événements de l'outil :
  "hooks": { "PreToolUse": [
      {"hooks":[{"command":"… ash-event working --tab \"$ASH_TAB_ID\" # ash:hook v1"}]},
      { … le hook de l'utilisateur, intact … } ] }
```

Le marqueur vit **dans la ligne de commande**, en commentaire de shell, et non dans une
clé JSON de plus. La commande d'un hook est déjà une ligne de shell — elle contient
`"$ASH_TAB_ID"` —, donc un `#` en fin de ligne n'y change rien ; une clé inconnue posée au
milieu d'un objet de hooks serait, elle, à la merci d'un schéma strict, et l'utilisateur
perdrait alors tous ses réglages à cause d'Ash. L'asymétrie des deux risques tranche.

**La garantie est reformulée, pas retirée :**

| Avant | Après |
|---|---|
| Ash n'écrit que **dans son bloc** | Ash n'écrit que **ce qui lui appartient**, et sait le reconnaître |

C'est un affaiblissement du **mécanisme**, pas de la **promesse**, et la promesse vit
toujours dans les types plutôt que dans la prudence des appelants
(`features/hooks/document.rs`) : un document ne se compose que de modifications qui
retirent ou remplacent une plage **portant le marqueur**, ou qui ajoutent un texte **le
portant** — et, hors de la feature, un document ne se compose pas du tout, ses
constructeurs étant `pub(super)`. Le retrait, lui, préfère recomposer le texte qu'Ash
avait écrit et le comparer octet par octet ; quand la comparaison échoue — le fichier a
été réindenté, l'entrée retouchée — il se replie sur les bornes de l'entrée marquée
elle-même et de son séparateur, jamais au-delà.

**« Jamais silencieux » ne change pas, et devient plus vrai.** Un fichier qui porte des
hooks qui ne sont pas ceux d'Ash n'est plus un refus : c'est un **conflit**, au même titre
qu'une entrée d'Ash éditée à la main — du point de vue de celui qui regarde, ce sont deux
fois « il y a là quelque chose que je n'ai pas mis, montre-le-moi ». L'écran le nomme,
ouvre le **diff de ce qu'Ash écrirait sur le fichier tel qu'il est**, et laisse trancher :
fusionner en gardant tout, ou retirer les entrées d'Ash. Rien ne s'écrit sans ce geste, et
la copie `.bak` le précède toujours.

**Ce qui reste un refus** : un fichier qui n'est pas un objet JSON, un chemin occupé par
autre chose qu'un conteneur, un fichier illisible. On ne devine pas où écrire.

### Amendement du 2026-08-13 — une seconde clé, **subordonnée** à l'onglet

La rédaction initiale dit que la corrélation hook → onglet se fait par `ASH_TAB_ID`, sans
devinette par `cwd` ni par horodatage. Elle ne dit rien de ce qui se passe **à
l'intérieur** d'un onglet, et le silence devenait un obstacle.

**Ce que la préparation de #19 a montré.** Un sous-agent de Claude Code n'est pas un autre
processus dans un autre terminal : c'est le **même** `claude`, dans le **même** onglet.
`ASH_TAB_ID` est donc rigoureusement identique pour le parent et pour tous ses enfants, et
le `session_id` de l'outil aussi. Lu au pied de la lettre, « l'onglet est la seule
corrélation » interdisait de représenter les sous-agents — non parce que l'information
manque, mais parce que la règle ne prévoyait pas de niveau en dessous.

Or l'information est là. Tout hook de Claude Code reçoit sur son entrée standard un
`agent_id` et un `agent_type` **dès qu'il se déclenche dans un sous-agent**, et
`SubagentStop` les porte aussi — donc on sait *lequel* des enfants s'arrête, y compris
quand plusieurs tournent en parallèle.

| Avant | Après |
|---|---|
| L'onglet est la **seule** corrélation | L'onglet est la **seule corrélation d'un événement à un onglet**. À l'intérieur d'un onglet déjà corrélé, un identifiant fourni par l'outil peut désigner un enfant |

**Ce n'est pas un affaiblissement, et voici pourquoi.** Ce qui est interdit reste
interdit : deviner à quel onglet un événement appartient. `agent_id` ne fait pas ça — il
n'entre en jeu qu'après que `ASH_TAB_ID` a tranché, et il ne peut pas rattacher un
événement à un onglet auquel il n'appartenait pas. C'est une hiérarchie, pas une seconde
source de vérité. Une trame sans ces champs reste valide et concerne l'agent principal,
exactement comme aujourd'hui.

**Trois garde-fous, qui font partie de la décision :**

- **Un événement de sous-agent ne produit jamais d'état d'onglet.** `SubagentStop` figure
  dans `tempting_events()` — la liste des mots qu'un adaptateur a interdiction de
  reconnaître — et **il y reste**. Un sous-agent qui finit ne rend pas `claude`
  disponible ; le traduire en `done` serait exactement la déduction que cette ADR refuse.
  Le cycle de vie des enfants passe par une méthode **distincte** du trait `Adapter`, et
  la suite contractuelle gagne l'exigence de vérifier qu'aucun événement d'enfant ne fuit
  vers l'état du parent.
- **Un sous-agent n'est jamais `waiting`.** Il ne peut pas interroger l'utilisateur. La
  partie de cette décision qui ne se relâche pas — `waiting` n'a jamais d'autre source
  qu'un hook, et justifie seul d'interrompre — n'est donc pas touchée : un enfant ne peut
  pas rendre son parent `waiting`.
- **`agent_id` ne sert qu'à distinguer des frères dans un onglet.** Il n'est ni une clé
  de persistance, ni un identifiant stable entre deux sessions, ni quelque chose qu'on
  corrèle à un commit — l'attribution d'ADR-0014 continue de ne connaître que l'onglet et
  l'identifiant de l'outil.

**Un mélange qui existait déjà, et que cet amendement rend visible.** Le hook `PreToolUse`
d'Ash prend tous les outils, donc il se déclenche aussi à l'intérieur des sous-agents :
un enfant qui lance un outil marque l'onglet parent `working`. Ce n'est pas faux — le
parent travaille bien — mais les états du parent et des enfants sont **déjà** confondus,
sans que rien ne le signale. Représenter les sous-agents ne crée pas ce mélange, il le
nomme.

### Précision du 2026-08-24 — pour un outil **instrumenté**, la présence ne dit plus `working`

La précision du 2026-08-11 dit que la sonde peut affirmer qu'un agent est **là**
(`working`). Elle reste vraie, et elle est ici **bornée** — pas retirée.

**Ce que le code a montré.** Ouvrir `claude` dans un onglet donnait deux choses fausses, et
les deux avaient la même cause : les six hooks installés — `UserPromptSubmit`, `PreToolUse`,
`Notification`, `Stop`, `SessionEnd`, `SubagentStop` — partent tous **pendant** un tour, donc
aucun ne part au démarrage d'une session.

- L'onglet était montré `working`, glyphe qui tourne, alors que l'agent attendait un prompt.
  L'état venait de la sonde : `fg_proc` reconnu ⇒ `working`, ce que la précision du
  2026-08-11 autorise en toutes lettres.
- La jauge de contexte n'existait pas. La mesure se lit dans le transcript **à l'arrivée d'un
  hook** ; tant qu'aucun n'était parti, il n'y avait pas de chemin de transcript, donc pas de
  jauge — y compris sur une session **reprise**, dont le contexte est déjà à moitié plein.

**Ce qui est précisé.** Pour un outil instrumenté, ce sont les hooks qui disent ce que
l'agent fait ; la **présence seule** ne produit plus `working`.

| Avant | Après |
|---|---|
| La sonde peut dire qu'un agent est **là** (`working`) | La sonde le dit encore — **pour un outil sans hooks**. Un outil instrumenté annonce lui-même l'ouverture de sa session, et à partir de là ce sont ses hooks, et eux seuls, qui disent ce qu'il fait |

L'outil l'annonce par un **septième hook**, `SessionStart`, qui écrit un verbe qui n'est pas
un état — `session-start`, à côté du `subagent-stop` de l'amendement du 2026-08-13 — et se
lit par une **troisième méthode** du trait `Adapter`, distincte d'`interpret` et de
`child_event`. La suite contractuelle vérifie qu'un même mot ne passe jamais par deux de ces
portes, et qu'un adaptateur sans instrumentation n'en reconnaît aucun.

**Ce n'est pas un affaiblissement de la sonde, et voici pourquoi.** Ce qu'elle savait faire,
elle continue de le faire : elle sert un onglet tant qu'aucun agent n'y a parlé — `vim`,
`htop`, un `make` —, et elle constate la **disparition** d'un processus, qui referme la
session. Ce qu'elle perd est un seul cas, celui où un autre producteur, meilleur, existe.

**Trois garde-fous, qui font partie de la précision :**

- **`SessionStart` ne produit aucun état, et surtout pas `waiting`.** `waiting` garde sa
  source unique — un hook qui déclare une attente — et l'ouverture d'une session n'en est
  pas une. Aucune notification n'en sort (spec §8).
- **Aucun sixième état n'apparaît.** « Un agent est là, il n'a rien en vol » est `idle` :
  le mot change de sens — « rien n'est en cours » plutôt que « shell sans agent » — mais la
  liste des cinq ne bouge pas, et rien ne tourne à l'écran dans les deux cas.
- **Un outil non instrumenté ne perd rien.** Il n'envoie aucun verbe de session, donc rien
  ne naît dans son onglet et la sonde continue d'y répondre `working` sur la seule présence.
  C'est exactement la raison d'être des deux producteurs de la spec §6.2.

**Ce que la même précision rapporte, et qui n'est pas un effet de bord.** L'entrée standard
de `SessionStart` porte un `transcript_path`, comme celle des six autres : une session
**reprise** montre donc son contexte déjà consommé dès la première seconde, sans qu'aucun
prompt n'ait été envoyé. Une session neuve, dont le transcript ne porte encore aucun tour,
n'affiche rien — pas une jauge à 0 %.

## Conséquences

- Les états sont exacts, y compris `waiting`, qui est indevinable de l'extérieur et
  qui est justement le seul état méritant d'interrompre l'utilisateur.
- Ça marche même quand `claude` est lancé hors d'Ash — le bloc vit dans la
  configuration de l'outil, pas dans l'environnement.
- **Ash écrit dans les fichiers de l'utilisateur.** C'est assumé, et encadré :
  sauvegarde `.bak` avant écriture, marqueur sur chacune de ses entrées (amendement du
  2026-08-12), désinstallation en un geste qui rend le fichier à l'octet près, et rien
  d'écrit sans que le diff ait été montré et un choix fait.
- Les deux comptes Claude nécessitent deux installations de bloc, une par dossier de
  configuration. Le modèle `[[command]]` le couvre nativement, et s'étend à `n`
  comptes.
- **Dépendance forte à ce que chaque outil expose.** Pour Claude Code c'est acquis.
  Pour codex, kimi et opencode, ça reste à qualifier — c'est le principal risque du
  projet, et la raison d'être de [ADR-0008](./0008-abstraction-adapter.md).
- Un outil sans hook ne pourra fournir que `idle` / `working` / `done` / `error` via la
  sonde — jamais `waiting`. C'est aussi le seul cas où la présence produit encore `working`
  (précision du 2026-08-24).
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
