# ADR-0010 — La sidebar informe, le terminal agit

- **Statut** : accepté (2026-08-07)
- **Amendé par** : [ADR-0015](./0015-ash-compose-l-utilisateur-envoie.md) — Ash peut
  composer un texte, jamais l'envoyer

## Contexte

Une fois qu'Ash sait qu'un agent attend une réponse — et, dans le cas de Claude Code,
qu'il connaît même les options proposées via le hook — la tentation est forte
d'afficher des boutons dans la sidebar et de répondre sans aller dans le terminal.
Répondre à trois agents en attente deviendrait instantané.

Le même raisonnement vaut pour une barre de commande globale qui enverrait du texte
à un ou plusieurs agents à la fois.

## Décision

La sidebar **informe** et **navigue**. Elle n'écrit jamais dans un PTY.

Cliquer sur un agent sélectionne son onglet et place le focus dans le terminal.
L'utilisateur répond dans l'interface réelle de l'outil.

Corollaire pour les subagents : leurs lignes sont purement informatives et non
cliquables — ils n'ont pas de PTY vers lequel naviguer, donc rien à sélectionner.
Le clic sélectionne le parent.

## Conséquences

- Aucune réinterprétation de l'interface d'un outil, donc aucun risque de valider
  autre chose que ce qui est affiché. C'est décisif quand un agent a les permissions
  d'écrire du code et de lancer des commandes.
- Le comportement est identique pour `claude`, `codex`, `kimi`, et pour tout outil
  installé demain : Ash n'a rien à savoir de leur UI.
- La surface d'interface se réduit fortement : pas de boutons contextuels, pas de
  seconde zone de saisie, pas de question « où va ce que je tape ».
- Coût réel : aucun raccourci. Répondre « oui » à trois agents demande trois
  allers-retours. Accepté — la valeur d'Ash est de *savoir* qu'ils attendent, pas de
  répondre à la chaîne.
- Le design doit donc rendre la navigation très rapide : le clic et `Cmd+1..9` sont
  le seul chemin vers l'action.

## Amendement (2026-08-10) — « n'écrit jamais dans un PTY » est trop large

La phrase de la décision est à lire comme : *la sidebar ne **valide** jamais rien à la
place de l'utilisateur*. Elle interdisait par accident un cas qu'elle n'avait pas
envisagé, et qui ne porte pas le même risque : Ash **rédigeant** un prompt de
résolution de conflit dans l'onglet d'un agent, sans jamais presser `⏎`.

[ADR-0015](./0015-ash-compose-l-utilisateur-envoie.md) pose les trois conditions —
visible, éditable, jamais validé par Ash — et laisse intact tout le reste : pas de
boutons reconstruits depuis les options d'un hook, pas de barre de commande globale,
aucun envoi déclenché par Ash.

Le corollaire sur les subagents est inchangé : leurs lignes restent informatives et
non cliquables.

## Alternatives écartées

- **Actions rapides sur les cas sûrs** (boutons reconstruits depuis les options
  portées par le hook, écrivant la touche correspondante dans le PTY) : très
  confortable, mais revient à frapper à l'aveugle — si l'interface de l'outil a
  changé, on valide autre chose que ce qu'on croit. Inacceptable pour un agent qui
  a les droits d'écriture.
- **Barre de commande globale** (envoyer une consigne à plusieurs agents à la fois) :
  utile pour lancer la même instruction sur trois dépôts, mais introduit une seconde
  façon de saisir du texte à côté du terminal, source de confusion permanente.
- **Actions de cycle de vie seulement** (interrompre, tuer, relancer, ouvrir le
  dossier) : sans ambiguïté et sans risque d'interprétation. Non retenu en v1 parce
  que ça ne règle pas le cas fréquent — l'agent qui attend — mais c'est l'extension
  la plus défendable si le besoin se fait sentir.
