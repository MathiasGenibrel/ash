import type { ActiveTab } from "@/features/terminal";
import { branchOf, locationLabel } from "@/shared/tab-context";

/**
 * Ce que la bande de titre écrit : `<nom> — <dépôt> / <branche>` de l'onglet actif
 * (spec §4.2).
 *
 * La règle est ici, dans le composition root, et pas dans une feature : la bande surplombe
 * les deux colonnes, elle n'appartient donc ni à la sidebar ni à la zone terminal. C'est une
 * fonction pure parce qu'elle a quatre cas que la maquette ne dessine pas, et qu'aucun ne se
 * vérifierait dans le DOM.
 *
 * Elle ne détient rien : le dépôt vient de la localisation que le backend a résolue, la
 * branche de la surveillance d'[ADR-0011](../../docs/adr/0011-git-domaine-de-premier-plan.md), toutes
 * deux telles que la feature terminal les reçoit
 * ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)). Elle ne **nomme** rien non
 * plus : `shared/tab-context` dit comment s'écrivent un lieu et une branche, et la ligne de
 * statut le lit au même endroit — sans quoi les deux finiraient par désigner deux endroits
 * différents. Ce qui reste ici est la seule chose propre à la bande : la **phrase**, et ce
 * qu'elle tait quand git n'a pas encore répondu.
 *
 * Le nom de l'agent n'y figure pas : reconnaître `claude` dans un avant-plan qui s'annonce
 * `2.1.233` demande [ADR-0006](../../docs/adr/0006-decouverte-automatique-des-agents.md), qui n'est pas
 * faite. Le processus brut, lui, est déjà dans la ligne de statut.
 *
 * **Le nom est un paramètre, jamais un littéral** (décision du 2026-08-17). La maquette
 * dessinait `ash` en minuscules ; ce mot est abandonné, parce qu'il faisait deux noms pour
 * une application. Ce qui s'écrit ici est le nom **affiché**, dont `APP_NAME` côté Rust est
 * la seule source (voir `CLAUDE.md`) : la bande dit donc `Ash` dans l'application
 * installée et `Ash-dev` dans une compilation de développement — et c'est le résultat
 * voulu, pas un effet de bord. Ash est le terminal quotidien de son auteur : deux fenêtres
 * qui se ressemblent doivent se distinguer là où l'œil se pose, et la bande de titre est
 * précisément là.
 *
 * Le recevoir plutôt que le lire est ce qui garde la fonction pure, donc ses sept cas
 * vérifiables sans backend — c'est le composition root qui va chercher le nom, une fois, au
 * démarrage (`app-name.ts`).
 */
export function windowTitle(active: ActiveTab | null, appName: string): string {
    // Pas d'onglet : rien à dire de plus que le nom. Un titre inventé serait faux, et une
    // bande vide ferait sauter la hauteur du titre à l'ouverture du premier onglet.
    if (active === null) return appName;

    const where = locationLabel(active.tab);

    // Sans métadonnées — un onglet ouvert à `~`, un dossier hors dépôt, ou le temps de
    // l'aller-retour qui lit les fichiers de contrôle — la bande dit **où** l'on est et se
    // tait sur le reste. Elle ne ment donc jamais, et elle ne clignote pas : le titre
    // s'allonge d'une branche quand git répond, il ne passe pas par le vide.
    if (active.metadata === null) return `${appName} — ${where}`;

    return `${appName} — ${where} / ${branchOf(active.metadata).label}`;
}
