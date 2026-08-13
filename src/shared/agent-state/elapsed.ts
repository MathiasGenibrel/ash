/**
 * Depuis combien de temps — la mise en forme d'une durée d'agent.
 *
 * Elle vit dans `shared/` pour la raison qui y range déjà la présentation des cinq états :
 * elle sert **deux** features. La ligne de statut écrit `claude · working · 15m22s` pour
 * l'onglet actif (spec §4.2) ; la sidebar écrit la même durée sur chaque ligne de sous-agent
 * (spec §6.5). Écrite deux fois, elle finirait par ne plus dire la même chose des deux côtés
 * — `2h05m` ici, `125m` là — pour la même valeur.
 *
 * Rien n'est **produit** ici : le backend envoie une **date d'entrée** absolue, une seule
 * fois, et l'écart jusqu'à maintenant est un fait d'affichage
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)). C'est ce qui laisse la
 * fiche d'un onglet identique d'une passe de sonde à l'autre : une durée transportée ferait
 * partir `ash://tab-changed` chaque seconde pour chaque onglet actif.
 */

/**
 * `45s`, `15m22s`, `2h05m` — au plus deux unités, et jamais plus de sept caractères.
 *
 * La ligne de statut fait 25 px de haut et partage sa largeur avec un chemin et un état
 * git ; la sidebar en fait 240. La seconde disparaît au-delà de l'heure, où elle n'apprend
 * plus rien.
 */
export function formatElapsed(millis: number): string {
    const seconds = Math.floor(millis / 1000);
    if (seconds < 60) return `${seconds}s`;

    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m${pad(seconds % 60)}s`;

    return `${Math.floor(minutes / 60)}h${pad(minutes % 60)}m`;
}

function pad(value: number): string {
    return value.toString().padStart(2, "0");
}

/**
 * La durée écoulée depuis une date d'entrée, ou `null` quand il n'y a rien à écrire.
 *
 * `null` sur une date **à venir** — une horloge recalée entre le backend et le rendu, un
 * event arrivé d'avance : écrire `-3s` serait pire que ne rien écrire. Ce qui décide, pour
 * un onglet à son invite ou pour toute autre absence d'activité, appartient à l'appelant :
 * cette fonction ne sait rien des cinq états.
 */
export function elapsedSince(since: number, now: number): string | null {
    const elapsed = now - since;
    return elapsed < 0 ? null : formatElapsed(elapsed);
}
