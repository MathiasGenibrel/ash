# ADR-0017 — Ash lit le jeton de l'outil qu'il supervise

- **Statut** : accepté (2026-08-20)
- **Découle de** : [ADR-0016](./0016-ash-sort-sur-le-reseau.md),
  [ADR-0007](./0007-etats-par-hooks.md)

## Contexte

[ADR-0016](./0016-ash-sort-sur-le-reseau.md) autorise l'appel réseau. Elle ne dit rien de
l'**authentification**, et les quotas en exigent une : ils appartiennent au compte de
l'utilisateur, et c'est le compte de Claude Code.

**C'est la première fois qu'Ash lit un secret.** Le dépôt a une règle transverse sur ce
qu'Ash a le droit d'**écrire** chez l'utilisateur — entrées marquées, sauvegarde, jamais
silencieux ([ADR-0007](./0007-etats-par-hooks.md),
[ADR-0013](./0013-fiche-de-branche-dans-le-depot.md)). Rien ne disait ce qu'il a le droit
de **lire**, parce que jusqu'ici il ne lisait que des fichiers de configuration et des
journaux.

Trois sources étaient possibles :

| Source | Ce qu'elle coûte |
|---|---|
| Le trousseau macOS de Claude Code | Ash lit le secret d'une autre application |
| `claude setup-token`, collé dans les réglages d'Ash | Une manipulation à faire, sans laquelle rien ne s'affiche |
| `ANTHROPIC_API_KEY` | Ce n'est pas la même authentification, et elle ne mesure aucun abonnement |

## Décision

**Ash lit l'item de trousseau de Claude Code**, à quatre conditions cumulatives.

### 1. macOS pose la question, et Ash ne la contourne pas

Le trousseau demande l'autorisation à la première lecture par une application qui n'a pas
créé l'item. **C'est le consentement**, et il est meilleur que tout ce qu'Ash pourrait
afficher : il est posé par le système et non par la partie intéressée, il nomme l'item
exact, et il est révocable à tout moment dans *Trousseaux d'accès* sans passer par Ash.

Ash ne cherche donc **pas** à l'éviter, à le pré-autoriser, ni à l'expliquer par une
fenêtre qui s'afficherait juste avant pour préparer le terrain.

### 2. Le jeton ne transite que vers son émetteur

Il ne sort que vers l'hôte nommé par ADR-0016, et vers rien d'autre. **Jamais** dans un
journal, dans un fichier écrit par Ash, dans un `argv`, dans un message d'erreur, ni dans
un rapport de panique.

### 3. Ash ne le copie pas

Il ne le range **jamais** dans son propre trousseau, ni dans `~/.ash/`, ni dans aucun
fichier. Deux copies d'un secret, ce sont deux endroits à révoquer — et le second serait
celui que l'utilisateur oublierait.

**Le garder en mémoire entre deux appels est autorisé, et c'est une conséquence de la
condition 1.** Relire le trousseau à chaque cycle rouvrirait le dialogue de macOS, ou au
mieux le solliciterait toutes les minutes : la condition 1 dit qu'Ash ne contourne pas ce
dialogue, pas qu'il doit le déclencher en boucle. Ce qui est interdit est la **persistance**
— ce qui survit à l'extinction d'Ash. Un jeton en mémoire meurt avec le processus, et il est
**relâché** dès qu'un `401` prouve qu'il ne vaut plus rien, ce qui force une relecture au
lieu de réessayer indéfiniment avec un secret périmé.

### 4. Un refus est définitif et silencieux

Si l'utilisateur refuse l'accès, **les quotas n'existent pas**. Ash ne redemande pas, ne
pose aucune bannière, n'affiche aucune invite, et ne réessaie pas au prochain démarrage en
espérant un clic distrait. C'est la condition 3 d'ADR-0016 appliquée à sa lettre : ce qu'on
n'a pas disparaît.

### Ce qui rend la lecture acceptable, et qu'il faut dire

**Ash supervise déjà cet outil.** Il écrit ses hooks dans son `settings.json`, lit le
transcript de ses conversations, reconnaît ses processus, et reçoit ses événements. Lire
son jeton n'ouvre pas une porte qui serait restée fermée : elle l'était déjà, largement.

Mais c'est le premier **secret**, et un secret n'est pas un fichier de configuration. C'est
pour cette raison que la décision se prend dans une ADR plutôt qu'au fil d'une PR : le jour
où quelqu'un voudra lire un deuxième secret, il tombera sur ce texte et sur ses quatre
conditions, au lieu de trouver un précédent tacite.

## Conséquences

- **Ash-dev a son propre identifiant de paquet, donc sa propre autorisation.** Le dialogue
  macOS reparaîtra en développement, une fois par build installé. C'est attendu, et c'est
  le même effet de bord voulu que pour les notifications et le stockage.
- **Le jour où l'item change de nom ou de forme, les quotas disparaissent en silence.**
  C'est la conduite voulue, et elle se diagnostiquera mal. La fenêtre de réglages doit donc
  pouvoir dire **« le jeton n'est pas lisible »**, exactement comme elle dit déjà « macOS
  ne nous le dit pas » pour l'autorisation de notification. Sans cette ligne, l'utilisateur
  n'aurait aucun moyen de distinguer un refus, un item absent et une panne.
- **Un utilisateur sans abonnement, ou authentifié autrement, n'a pas cet item.** Mêmes
  conséquences : rien ne s'affiche, rien n'est signalé comme une erreur.
- **Cas à traiter — plusieurs comptes.** ADR-0007 prévoit deux dossiers de configuration
  (`claude` et `claude-perso`), donc deux comptes. Le trousseau, lui, ne porte qu'un jeton :
  les quotas affichés seront ceux du compte que Claude Code utilise, et **Ash n'a aucun
  moyen de savoir lequel c'est**. C'est une limite à documenter dans la fenêtre de réglages,
  pas à résoudre : afficher un quota en le rattachant au mauvais compte serait pire que de
  ne rien rattacher du tout.
- **La lecture du trousseau est un appel système bloquant**, et il peut attendre un clic
  humain. Il tombe donc sous la condition 1 d'ADR-0016 : jamais sur un chemin de rendu, de
  sonde ou de hook. En pratique, il vit sur le même fil de fond que l'appel qu'il sert.

## Alternatives écartées

- **`claude setup-token`, collé dans les réglages d'Ash.** C'est l'option la plus propre
  sur le papier, et de loin : un jeton **fabriqué exprès pour un outil tiers**, révocable
  seul, sans qu'Ash lise le secret de personne. Écartée pour une raison d'usage et non de
  principe — elle déplace le coût sur l'utilisateur pour une jauge périphérique, et
  personne ne va fabriquer un jeton pour un pourcentage dans une barre d'état. La
  fonctionnalité serait éteinte pour tout le monde, y compris pour ceux qui l'auraient
  voulue.
  **À reprendre sans hésiter** si la lecture du trousseau se révèle fragile, ou le jour où
  Ash aura besoin d'un jeton pour autre chose qu'un chiffre décoratif.
- **La clé d'API (`ANTHROPIC_API_KEY`).** Ce n'est pas la même authentification : elle
  facture à l'appel et ne mesure aucun quota d'abonnement. Elle ne répond donc pas à la
  question posée, même quand elle est présente.
- **Demander à l'utilisateur de coller lui-même le contenu de son trousseau.** Le pire des
  deux mondes : la manipulation de l'option écartée plus haut, sans le jeton dédié qui la
  justifiait, et un secret de plus qui se balade dans un presse-papiers.
- **Lire le jeton une fois et le garder dans `~/.ash/`.** Écartée par la condition 3 : ce
  serait une seconde copie, hors du trousseau, qu'une révocation côté Claude Code ne
  toucherait pas.
