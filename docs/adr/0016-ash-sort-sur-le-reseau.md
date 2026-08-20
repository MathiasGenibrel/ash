# ADR-0016 — Ash sort sur le réseau, à quatre conditions

- **Statut** : accepté (2026-08-20)
- **Découle de** : [ADR-0002](./0002-tauri-rust-portable-pty.md)
- **Complétée par** : [ADR-0017](./0017-ash-lit-le-jeton-de-l-outil.md)

## Contexte

**Ash n'a jamais fait un seul appel réseau.** Ce n'est pas un oubli : c'est une propriété
de ce qu'il est. Tout ce qu'il sait, il le lit sur la machine — les PTY qu'il tient, la
sonde d'[ADR-0005](./0005-sonde-cwd-libproc.md), les fichiers de contrôle de `.git`, les
hooks d'[ADR-0007](./0007-etats-par-hooks.md). Le `Cargo.toml` en porte la trace : `libc`
et les `objc2` y sont *nommées* parce qu'elles étaient déjà dans l'arbre, et
`tauri-plugin-notification` a été **retiré** plutôt que gardé au chaud. `tokio` lui-même
n'y est qu'avec `features = ["sync"]` — pas de runtime, pas de socket.

Le premier besoin qui casse ça est celui des **quotas de session et hebdomadaires** de la
spec §4.2. Contrairement au contexte d'une conversation, ils ne sont ni dans un hook, ni
dans un transcript, ni dans aucun fichier de la machine : ils vivent chez Anthropic, et il
faut les demander. Deux vérifications ont été faites avant d'en arriver là :

- **`claude` n'expose aucune commande d'usage.** Il n'y a rien à déléguer à l'outil.
- **`ccstatusline` — que l'utilisateur emploie aujourd'hui — appelle l'API directement.**
  La valeur existe donc déjà dans son terminal ; Ash la lui retirerait en ne la portant
  pas.

La question n'est donc pas « faut-il appeler ». C'est **à quelles conditions**, une fois
pour toutes, parce que ce sera vrai du deuxième appel réseau comme du premier.

## Décision

Ash a le droit d'appeler le réseau, à **quatre conditions cumulatives**.

### 1. Jamais sur un chemin de rendu, de sonde ou de hook

Aucun rendu, aucune passe de la boucle de sonde, aucun hook n'attend une réponse réseau.
La valeur est rafraîchie en fond et **lue** là où elle s'affiche. Un réseau lent, coupé ou
qui ne répond jamais ne ralentit rien de visible.

C'est la condition la plus dure à tenir et la plus facile à perdre : il suffit d'un
`await` posé au mauvais endroit. C'est aussi celle qui décide si Ash reste un terminal.

### 2. Jamais quand personne ne regarde

L'appel ne part que si la fenêtre d'Ash est **au premier plan**. Le superviseur sait déjà
si elle l'est — c'est ce niveau qui décide des bannières de la spec §8, et il est déjà
poussé à toutes les machines à états.

Ash est le terminal quotidien de son auteur : il tourne toute la journée, souvent derrière
autre chose. Une jauge que personne ne regarde n'a pas besoin d'être à jour, et un appel
par minute pendant huit heures d'inattention est du réseau et de la batterie dépensés pour
un chiffre que personne ne lira.

**Le corollaire fait partie de la condition** : le retour au premier plan **déclenche** un
appel, sans attendre la fin du cycle courant. Sans lui, se taire en arrière-plan
reviendrait à afficher une valeur vieille de deux heures pendant la seconde où
l'utilisateur la regarde enfin — c'est-à-dire à violer la condition 3, qui interdit de
faire passer une valeur périmée pour fraîche. Le premier plan est donc un **front**, pas
seulement un niveau : il éteint le sondage en le quittant, et il en redemande un en
revenant.

### 3. Jamais silencieux, dans les deux sens

- **En échec** : une valeur qu'on n'a pas **disparaît**. Elle ne s'invente pas, ne se
  remplace pas par un zéro, ne se fige pas sur la dernière valeur connue en la faisant
  passer pour fraîche. C'est la même règle que pour la jauge de contexte
  ([#146](https://github.com/MathiasGenibrel/ash/issues/146)) : pas de tiret, pas de
  libellé grisé, rien.
- **En succès** : l'utilisateur doit pouvoir savoir qu'Ash appelle, et **le couper**. Un
  interrupteur dans la fenêtre de réglages, détenu par la feature concernée et persisté
  comme les trois de la spec §9. Il existe **dès la première fonctionnalité réseau**, pas
  au jour où quelqu'un le demande.

### 4. Une destination nommée par besoin

Il n'y a **pas** de client HTTP générique offert au reste du code. Chaque appel nomme son
hôte dans le code de la feature qui en a besoin, et une feature qui n'a pas de raison
d'appeler n'a aucun moyen de le faire.

C'est le même raisonnement que la frontière de sécurité documentée autour du seul appel à
`git` (`features/git/git_cli.rs`) : visiter un dépôt hostile ne doit rien exécuter, et
ouvrir Ash ne doit rien appeler d'autre que ce qu'une ADR a nommé.

### Le client : `ureq`

`ureq` **3.4.0**, publiée le **2026-08-08**. Bloquante, sans runtime asynchrone, une
poignée de dépendances transitives.

Bloquante *est* la bonne propriété ici, pas un pis-aller : l'appel est un sondage de fond
sur un fil dédié, et la condition 1 dit déjà que personne ne l'attend. Un runtime
asynchrone n'achèterait rien.

**TLS par le vérificateur de la plateforme**, et non par un magasin de racines embarqué :
un poste sous MDM, ou derrière un proxy d'entreprise qui ré-signe, doit fonctionner sans
qu'Ash ait son propre avis sur les autorités de certification. `HTTPS_PROXY` est honoré
pour la même raison.

## Conséquences

- **Tout ce qui existait continue de fonctionner hors ligne.** Les PTY, la sonde, git, les
  hooks, les états, les notifications : rien de tout cela ne demande le réseau, et rien ne
  le demandera. Hors ligne, Ash perd exactement les fonctionnalités qui sont nées en ligne,
  et rien d'autre.
- **La fenêtre de réglages gagne une section.** Elle dira ce qu'Ash appelle, et portera
  l'interrupteur de la condition 3.
- **Une dépendance de plus dans l'arbre**, la première depuis les `objc2`. Elle est
  assumée : la règle du dépôt est de ne pas ajouter à la légère, pas de ne jamais ajouter,
  et un module `unsafe` de plus coûterait plus cher qu'une petite crate (voir les
  alternatives).
- **Cas à traiter** : Ash-dev fait les mêmes appels qu'Ash. Deux applications, deux
  identités, deux autorisations — et deux fois le trafic si les deux tournent, ce qui est
  le cas normal pendant qu'on développe. La condition 2 le borne largement : une seule des
  deux fenêtres est devant à la fois.
- **L'adresse exacte de l'API d'usage n'est pas décidée ici.** Cette ADR fixe les
  conditions, pas l'URL ; l'endpoint est à établir à l'implémentation
  ([#147](https://github.com/MathiasGenibrel/ash/issues/147)).

## Alternatives écartées

- **Ne pas appeler du tout, et se passer des quotas.** C'était l'état jusqu'ici, et c'est
  la seule alternative honnête sur le fond. Écartée parce que la valeur est réelle et
  qu'elle existe **déjà** chez l'utilisateur : `ccstatusline` la lui affiche aujourd'hui, et
  Ash la lui retirerait en devenant son terminal.
- **Déléguer à `claude`.** Il n'y a rien à déléguer : la CLI n'a ni `claude usage` ni
  équivalent (vérifié le 2026-08-20). L'option n'existe pas.
- **`reqwest`.** Le standard de fait, et le mauvais outil ici : il exige un vrai runtime
  tokio là où Ash n'a que `features = ["sync"]`, et tire hyper, une pile TLS et une longue
  chaîne transitive pour un `GET` par minute. Faire entrer un runtime asynchrone dans une
  application qui n'en a pas est une décision bien plus lourde que celle qu'on prend.
- **`NSURLSession` par `objc2`.** Zéro dépendance nouvelle — `objc2-foundation` est déjà
  là — et c'est son seul argument. Écartée parce qu'elle achète cette économie avec un
  **troisième module `unsafe`** après la sonde et les notifications, et avec du code à
  écrire là où `ureq` en fournit. Les deux modules `unsafe` existants sont là parce
  qu'aucune bibliothèque sûre ne faisait le travail ; ce n'est pas le cas ici.
- **Lancer `curl`.** macOS le fournit, et ça ne coûte aucune dépendance. Écartée pour une
  raison de sécurité, pas de style : un secret passé dans un `argv` est lisible par `ps`
  pour tout processus de la machine, et Ash a déjà **une** frontière de sécurité autour de
  son unique appel à un binaire externe. En ouvrir une seconde pour économiser une petite
  crate est un mauvais échange.
