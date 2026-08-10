# ADR-0015 — Ash compose, l'utilisateur envoie

- **Statut** : accepté (2026-08-10)
- **Amende** : [ADR-0010](./0010-sidebar-informe-terminal-agit.md)
- **Découle de** : [ADR-0011](./0011-git-domaine-de-premier-plan.md)

## Contexte

[ADR-0010](./0010-sidebar-informe-terminal-agit.md) posait une règle absolue : *« la
sidebar informe et navigue, elle n'écrit jamais dans un PTY »*. Elle protégeait contre
un risque précis et réel — reconstruire des boutons à partir de ce qu'un hook rapporte,
puis frapper une touche à l'aveugle dans une interface qui a peut-être changé, alors
que l'agent a les droits d'écrire du code et de lancer des commandes.

Le design fait apparaître un cas que cette ADR n'avait pas envisagé, et qui ne relève
pas du même risque. Un rebase s'arrête sur trois fichiers en conflit. La bonne action
est souvent de passer le travail à l'agent qui tourne déjà là — mais un agent à qui on
demande « résous les conflits » sans lui donner les chemins, le commit d'arrêt et la
commande de test **redemande systématiquement**. Composer ce prompt à la main prend une
minute, et on oublie toujours un des trois éléments.

Ash a les trois. C'est même la seule chose qu'il a que personne d'autre n'a.

La différence avec ce que l'ADR-0010 écartait est celle-ci : là, Ash **fabriquait une
réponse** en devinant l'état de l'interface d'un outil. Ici, Ash **rédige un texte** que
l'utilisateur lit avant de l'envoyer.

## Décision

Ash a le droit d'écrire du texte dans un PTY, à trois conditions cumulatives :

1. **Visible** — le texte apparaît dans le terminal, à sa place, tel qu'il sera envoyé.
   Pas de zone de saisie parallèle, pas d'aperçu dans un panneau.
2. **Éditable** — l'utilisateur peut le modifier ou l'effacer entièrement (`⌥⌫`) avant
   de l'envoyer, comme s'il l'avait tapé.
3. **Jamais validé par Ash** — Ash ne presse jamais `⏎`. Le seul envoi possible est
   celui de l'utilisateur.

Le libellé qui l'accompagne dit exactement ce qui se passe : *« ash typed this for you
— not sent yet »*.

Ce qui reste interdit, et qui est le cœur intact de l'ADR-0010 :

- reconstruire les options d'un agent en attente et frapper la touche correspondante ;
- toute barre de commande qui enverrait du texte à plusieurs agents à la fois ;
- tout envoi qu'Ash déclencherait de lui-même, y compris différé.

**Corollaire sur la file d'attente.** Quand l'onglet visé est occupé par un tour
d'agent, Ash écrit quand même le texte et annonce le délai (« queued behind the current
turn »). Il attend la fin du tour pour que la frappe atterrisse dans le prompt et non
au milieu d'une sortie — c'est un problème de placement, pas d'autorisation. L'envoi
reste celui de l'utilisateur, avant comme après.

**Corollaire sur la pause d'agent.** L'avertissement d'un checkout risqué propose de
mettre l'agent en pause. « Pause » signifie `SIGSTOP` sur le groupe de processus en
avant-plan, et rien d'autre : aucune touche envoyée au PTY, aucune interprétation de
l'interface de l'outil. Envoyer `Esc` parce qu'on suppose que ça interrompt serait
exactement la faute que l'ADR-0010 interdit.

## Conséquences

- Le gain est réel et mesurable : le prompt de conflit porte les chemins, le commit
  d'arrêt et la commande de test. Trois choses qu'on oublie et qui coûtent un
  aller-retour chacune.
- La franchise est la même que pour les hooks : **on voit ce qui va partir avant que ça
  parte**. C'est la ligne de conduite commune aux deux endroits où Ash écrit chez
  l'utilisateur, et elle mérite d'être formulée une fois pour toutes.
- L'utilisateur reste seul responsable de ce qui est envoyé. Aucune séquence où Ash
  agit sans qu'un geste humain l'ait déclenchée.
- **Cas à traiter** : l'utilisateur tape pendant qu'Ash écrit. Ash doit refuser de
  composer dans un prompt non vide plutôt que d'insérer au milieu de la frappe.
- **Cas à traiter** : l'onglet visé n'est pas celui affiché. Composer doit toujours
  sélectionner l'onglet de destination — écrire dans un terminal qu'on ne regarde pas
  viole la première condition.
- Le texte composé est un artefact d'Ash dans le scrollback de l'utilisateur. S'il
  l'efface, il ne reste rien ; s'il l'envoie, c'est une commande comme une autre.

## Alternatives écartées

- **Tenir l'ADR-0010 à la lettre** (afficher le prompt à copier dans un panneau,
  l'utilisateur le colle) : préserve la règle sans exception. Écarté parce que le
  copier-coller est un geste de plus pour un résultat identique, et parce que le
  panneau devient une seconde zone de texte à côté du terminal — précisément la
  confusion que l'ADR-0010 voulait éviter en écartant la barre de commande.
- **Envoyer directement** (Ash presse `⏎`) : le confort maximal, et le prompt est
  correct dans la quasi-totalité des cas. Écarté sans hésiter : c'est le pas qui fait
  d'Ash un acteur, avec un agent qui a les droits d'écriture derrière.
- **Confirmation par dialogue** (« envoyer ce prompt à claude ? oui / non ») : sûr, et
  plus explicite. Écarté parce qu'un dialogue se clique sans lire, alors qu'un texte
  posé dans le terminal se relit forcément — et s'édite, ce qu'un dialogue ne permet
  pas.
