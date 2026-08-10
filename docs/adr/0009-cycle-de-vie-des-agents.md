# ADR-0009 — Les agents meurent avec l'application (v1)

- **Statut** : accepté pour la v1 (2026-08-07)
- **Amendé par** : [ADR-0014](./0014-attribution-locale-des-commits.md) — la persistance

## Contexte

Trois agents tournent, l'utilisateur ferme la fenêtre — ou l'application plante.
Que devient le travail en cours ?

La persistance est le point où une application de terminal devient beaucoup plus
coûteuse : elle impose de séparer la détention des PTY de leur affichage, donc un
démon, un protocole, un cycle de vie, des versions potentiellement désynchronisées.

## Décision

En v1, les PTY vivent **dans le processus Ash**. Fermer l'application les termine,
avec une confirmation explicite s'il reste des agents actifs.

Rien n'est restauré au redémarrage, à l'exception des workspaces épinglés et de leur
état replié.

## Conséquences

- Architecture à un seul processus : pas d'IPC, pas de démon à superviser, pas de
  question de compatibilité entre une UI et un démon de versions différentes. Le
  débogage reste simple.
- **Un plantage d'Ash coûte les sessions d'agent en cours.** C'est le vrai prix, et
  il est réel : un agent qui tourne depuis vingt minutes est une perte sensible. La
  stabilité de l'application devient donc une exigence de premier ordre, pas un
  confort.
- Ash n'est utilisable ni à distance, ni en reprise depuis un autre poste.
- La confirmation à la fermeture doit être précise (quels agents, depuis combien de
  temps), pas un dialogue générique.

## Amendement (2026-08-10) — ce qui survit désormais

La décision tient : les PTY vivent dans le processus Ash, il n'y a pas de démon, et
fermer l'application termine les agents. Rien de tout cela ne change.

En revanche, « rien n'est restauré au redémarrage » est devenu faux. Le design
introduit deux persistances, qui ne sont pas des sessions mais des **traces** :

- le journal d'attribution commit → agent → prompt, dans `~/.ash/`
  ([ADR-0014](./0014-attribution-locale-des-commits.md)) ;
- la fiche de branche, dans le dépôt
  ([ADR-0013](./0013-fiche-de-branche-dans-le-depot.md)).

La distinction à tenir est nette, et vaut règle : **Ash persiste ce que les agents ont
fait, jamais ce qu'ils étaient en train de faire.** Un historique se relit après un
redémarrage ; une session en cours, non.

Cet amendement rend le chemin de sortie ci-dessous un peu plus attirant — la valeur
perdue à la fermeture n'a pas changé, mais on voit maintenant mieux ce qu'on garde.

## Chemin de sortie

Cette décision est la plus susceptible d'être revue à l'usage. Deux échappatoires,
par ordre de coût croissant :

1. **Reprise assistée** — mémoriser onglets, `cwd` et dernière commande, et proposer
   de relancer au démarrage. Presque gratuit, mais relance des processus sans
   restaurer le contexte des agents : le contexte est perdu quand même.
2. **Démon `ashd`** — détient les PTY, le scrollback et l'état ; la fenêtre devient
   une vue qui s'attache et se détache. Persistance réelle, résistance au crash de
   l'UI, et à terme plusieurs fenêtres. C'est le gros morceau.

Pour que le second reste possible, la frontière entre la détention des PTY
(`src-tauri/pty.rs`, `probe.rs`, `events.rs`) et leur affichage doit rester nette
dès J1 : aucun état d'agent ne doit vivre uniquement côté TypeScript.

## Alternatives écartées

- **Démon dès la v1** : la bonne architecture à terme, mais déplace tout le budget
  du jalon J1 vers de l'infrastructure, alors que la valeur à valider d'abord est la
  fiabilité des états.
- **tmux comme couche de persistance** (chaque onglet est un pane tmux détaché) :
  persistance et scrollback gratuits, reprise en SSH possible. Écarté parce que tmux
  devient une dépendance dure et impose sa sémantique de resize et de layout à une
  UI qui a la sienne.
