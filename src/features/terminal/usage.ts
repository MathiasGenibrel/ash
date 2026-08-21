import { badge, column, paint, row, text, type UiComponent } from "@/shared/ui";
import type { AccountUsage, Quota, SessionUsage } from "@/shared/ipc";
import { DEFAULT_STATUS_BAR_SEGMENTS, type StatusBarSegments } from "./status-bar";

/**
 * L'usage, à droite de la ligne de statut (spec §4.2, vues 5 et 5b de la maquette).
 *
 * Cinq morceaux, dans cet ordre : le quota de **session** (`s 63% · 2h14`), le quota
 * **hebdomadaire** (`w 28% · 3d 09h`), la **jauge de contexte** de la conversation, son
 * libellé (`ctx 41%`), et le **modèle** qui tourne (`Opus 5 1M`). Chacun n'apparaît que si sa
 * donnée existe, et une donnée absente ne
 * laisse **rien** derrière elle — ni tiret, ni zéro, ni dernière valeur connue
 * ([ADR-0016](../../../docs/adr/0016-ash-sort-sur-le-reseau.md), condition 3). L'écran ne
 * signale pas d'erreur non plus : il ne sait pas laquelle des quatre raisons s'applique.
 *
 * **Deux rythmes cohabitent ici, et c'est ce que ce fichier est venu séparer.** La jauge de
 * contexte suit l'onglet — elle arrive avec sa fiche, par `ash://tab-changed` —, tandis que
 * les deux quotas sont ceux du **compte** : ils ne dépendent d'aucune sélection, et changer
 * d'onglet ne les touche pas. Ils vivent donc dans des nœuds **persistants**, créés une fois
 * et mis à jour en place : un changement d'onglet ne peut pas les faire clignoter ni repartir
 * leur transition, parce qu'il ne les détruit jamais. C'est une propriété de structure, pas
 * une discipline de rendu — la ligne de statut, elle, refait tous ses morceaux à chaque
 * passage.
 *
 * **Rien n'est produit ici.** `usedTokens` et les deux pourcentages viennent du backend ; les
 * `2h14` et `3d 09h` sont dérivés à l'affichage d'une **date absolue**, exactement comme la
 * durée d'état de `status-line.ts` — un décompte transporté ferait repartir
 * `ash://account-usage` chaque seconde pour animer un compteur
 * ([ADR-0009](../../../docs/adr/0009-cycle-de-vie-des-agents.md)).
 *
 * ## Ce que cette tranche ne porte pas
 *
 * Le menu contextuel « show in the status bar » de la vue 5c existe désormais, et il est
 * dans `status-bar.ts` : ce qui vit ici est ce que ses interrupteurs **font** à la droite de
 * la ligne. Les défauts, eux, ne sont plus une constante de ce fichier — ils sont détenus
 * par `features::theme`, et le weekly masqué est le leur.
 *
 * `⌘⌥U`, écrit au pied du popover, reste un **indice** : la vue d'usage complète n'existe
 * pas, et aucune liaison n'est réclamée pour cette combinaison — les liaisons vivent dans
 * `features/shortcuts`, et une combinaison non réclamée n'a rien à y faire.
 */

/** Lequel des deux quotas — l'ordre de la maquette est celui de cette union. */
export type QuotaKind = "session" | "weekly";

/** Une pastille de quota, telle qu'elle se lit : `s 63% · 2h14`. */
export interface QuotaSegment {
    readonly kind: QuotaKind;
    /** La lettre colorée qui ouvre la pastille — `s` en `--ash-working`, `w` en `--ash-done`. */
    readonly letter: string;
    /** `63%` — arrondi, parce que l'hôte rend parfois un pourcentage fractionnaire. */
    readonly percent: string;
    /**
     * `2h14`, ou `null` quand il n'y a pas de date de remise à zéro — ou qu'elle est
     * passée. Le pourcentage, lui, s'affiche quand même : n'avoir qu'une des deux moitiés
     * vaut mieux que n'en avoir aucune.
     */
    readonly resets: string | null;
    /** Entre `0` et `1` : la barre pleine du popover. */
    readonly ratio: number;
}

/**
 * Ce que la conversation occupe de sa fenêtre — la jauge, et le mot qui la double.
 *
 * **Le pourcentage et le palier ne sont pas deux champs, mais un seul** : sans dénominateur,
 * aucun des deux ne veut dire quoi que ce soit, et les porter à plat obligerait chaque
 * appelant à vérifier deux fois la même absence — donc à pouvoir se tromper une fois sur
 * deux. [`share`](ContextGauge.share) à `null` est l'unique façon de dire « Ash sait combien,
 * mais pas sur combien », et il n'existe aucun état où une barre se peindrait sans son
 * pourcentage, ni un seuil sans son rapport.
 */
export interface ContextGauge {
    /**
     * Ce que la conversation occupe, sans le mot qui le nomme — `41%`, `57k`.
     *
     * C'est **la** valeur ; [`label`] n'en est que la mise en mots. Les deux sont posées
     * ensemble par [`composeContextGauge`], et dans ce sens-là : la ligne de statut écrit le
     * libellé entier, la colonne de droite du menu contextuel n'écrit que la mesure — le nom
     * du segment y est déjà dans la colonne du milieu, et `context bar    ctx 41%` se lirait
     * deux fois. Aucune des deux ne se retrouve à partir de l'autre.
     */
    readonly measure: string;
    /** `ctx 41%` quand la fenêtre est connue, `ctx 57k` sinon — [`measure`] et son mot. */
    readonly label: string;
    /**
     * Le nom court du modèle qui a produit le dernier tour — `Opus 5`, `Opus 5 1M`.
     *
     * **Il est ici, et pas à côté**, parce qu'il bat au rythme de la jauge : les deux
     * arrivent avec la fiche de l'onglet, et changent quand on en change. Le porter à part
     * ferait deux entrées pour un même rythme, donc deux façons de les désynchroniser.
     *
     * `null` quand l'adaptateur ne sait pas nommer ce qui a tourné, ou que rien ne l'a nommé.
     * Le segment disparaît alors entièrement — ni tiret, ni `unknown`, ni dernière valeur
     * connue. C'est la règle de [`share`], appliquée à l'autre absence.
     *
     * **Il n'ouvre rien** ([ADR-0015](../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)) :
     * changer de modèle se fait dans le terminal, par `/model`, et un segment de barre d'état
     * qui prendrait cette décision la prendrait à la place de l'utilisateur.
     */
    readonly model: string | null;
    /**
     * La part de la fenêtre que la conversation occupe, ou `null` quand la fenêtre est
     * inconnue — aucune source ne nomme de modèle reconnu.
     *
     * C'est la correction du bug qui faisait lire `ctx 28%` à une conversation occupant 6 %
     * de sa fenêtre : un dénominateur absent se dit, il ne se suppose pas.
     */
    readonly share: ContextShare | null;
}

/** Le rapport d'une conversation à sa fenêtre, quand cette fenêtre est connue. */
export interface ContextShare {
    /** Entre `0` et `100`, arrondi : la **même** valeur pour la largeur et pour le libellé. */
    readonly percent: number;
    readonly level: ContextLevel;
}

/**
 * Les trois paliers de la maquette. Ils nomment ce que la couleur **dit**, pas sa teinte :
 * `--ash-working`, puis `--ash-warning`, puis `--ash-accent`.
 *
 * À `90 %` la maquette est formelle — corail, et *« aucune alerte modale »*. Un contexte
 * plein n'est pas une panne : il annonce un compactage, que l'outil fera tout seul.
 */
export type ContextLevel = "fresh" | "loaded" | "compacting";

/** `≥ 70 %` : chargé. */
export const LOADED_AT = 70;
/** `≥ 90 %` : bientôt compacté. */
export const COMPACTING_AT = 90;

/**
 * Les deux quotas, dans l'ordre de la maquette, sans ceux qu'on n'a pas.
 *
 * `now` n'entre que dans les décomptes : les pourcentages, eux, sont ceux que le backend a
 * mesurés.
 */
export function composeQuotas(usage: AccountUsage, now: number): readonly QuotaSegment[] {
    return [
        segment("session", "s", usage.session, now),
        segment("weekly", "w", usage.weekly, now),
    ].filter((quota): quota is QuotaSegment => quota !== null);
}

/**
 * Ce que la **barre** montre des quotas — le popover, lui, les montre toujours tous les deux.
 *
 * C'est le seul endroit où le choix de l'utilisateur mord sur les quotas, et c'est ce qui
 * fait tenir le critère « décocher le quota hebdomadaire ne le retire pas du popover » :
 * `composeUsagePopover` ne passe pas par ici.
 */
export function inStatusBar(
    quotas: readonly QuotaSegment[],
    segments: StatusBarSegments,
): readonly QuotaSegment[] {
    return quotas.filter((quota) => segments[quota.kind]);
}

function segment(
    kind: QuotaKind,
    letter: string,
    quota: Quota | null,
    now: number,
): QuotaSegment | null {
    if (quota === null) return null;

    const percent = clamp(quota.percent);
    return {
        kind,
        letter,
        percent: `${String(percent)}%`,
        resets: remainingUntil(quota.resetsAt, now),
        ratio: percent / 100,
    };
}

/**
 * La jauge de l'onglet actif, ou `null` quand il n'y a rien à montrer.
 *
 * `null` couvre les trois absences que rien ne doit distinguer à l'écran — outil sans
 * transcript, aucun hook encore passé, mesure sans résultat. Ce sont les cas où Ash ne sait
 * **rien** de cette conversation.
 *
 * **Une fenêtre inconnue n'en fait pas partie**, et c'est ce que cette tranche est venue
 * corriger : `usedTokens` est exact, et l'effacer parce qu'on n'a pas de dénominateur serait
 * perdre ce qu'Ash sait vraiment. Le libellé montre alors la mesure sans la mettre en rapport
 * — `ctx 57k` —, sans barre et sans couleur de seuil. Une fenêtre **annoncée vide** est
 * traitée pareil : zéro ne se divise pas, et ce n'est pas une donnée.
 *
 * Le seuil est lu sur le pourcentage **affiché**, et non sur le rapport brut : une jauge qui
 * écrirait `70%` en restant bleue se lirait comme un bug, et c'est le chiffre qui est la
 * promesse. Le dépassement, lui, est ramené à `100 %` : une conversation compactée peut
 * déclarer plus que sa fenêtre le temps d'un tour, et `ctx 143%` ne veut rien dire.
 */
export function composeContextGauge(usage: SessionUsage | null): ContextGauge | null {
    if (usage === null) return null;

    const window = usage.windowTokens;
    if (window === null || window <= 0) {
        const measure = abbreviate(usage.usedTokens);
        return {
            measure,
            label: `ctx ${measure}`,
            model: usage.model,
            share: null,
        };
    }

    const percent = clamp((usage.usedTokens / window) * 100);
    const measure = `${String(percent)}%`;
    return {
        measure,
        // Le libellé est composé **à partir de** la mesure, et jamais l'inverse : la retrouver
        // en retirant `ctx ` d'un texte déjà écrit ferait deux règles à tenir d'accord, dont
        // l'une se lirait à l'envers.
        label: `ctx ${measure}`,
        // Le nom traverse tel quel : le backend l'a déjà mis en mots, et il n'y a rien à en
        // dériver. Les deux absences sont **indépendantes** — une fenêtre inconnue n'efface
        // pas le nom, et un modèle qu'on ne sait pas nommer n'efface pas le pourcentage.
        model: usage.model,
        share: {
            percent,
            level:
                percent >= COMPACTING_AT
                    ? "compacting"
                    : percent >= LOADED_AT
                      ? "loaded"
                      : "fresh",
        },
    };
}

/**
 * `57k`, `900` — un compte de tokens écrit court, et sans décimale.
 *
 * La règle est volontairement plate : au-dessus de mille, les milliers arrondis suivis d'un
 * `k` ; en dessous, le nombre tel quel. Une décimale (`57.2k`) ferait battre le dernier
 * chiffre à chaque tour d'agent pour une précision que personne ne lit dans une barre d'état
 * de 12 px.
 */
function abbreviate(tokens: number): string {
    return tokens >= 1000 ? `${String(Math.round(tokens / 1000))}k` : String(Math.round(tokens));
}

/**
 * `14m`, `2h14`, `3d 09h` — au plus deux unités, et jamais de seconde.
 *
 * C'est le miroir de `formatElapsed` de `@/shared/agent-state` : même mécanisme — une date
 * absolue entre, un fait d'affichage sort —, unités différentes. Elle n'est pas réutilisée
 * telle quelle parce qu'elle s'arrête à l'heure : un quota hebdomadaire s'y lirait `65h00m`,
 * et sa seconde y ferait battre la ligne pour un chiffre qui bouge une fois par minute. Elle
 * reste donc **ici**, dans la seule feature qui la lit — `shared/` demande deux features.
 *
 * C'est la **troisième** fois que la question se pose, et la troisième réponse identique :
 * `aged` de `features/git/table-view.ts` écrit `3d ago` pour la même raison. Trois grammaires,
 * trois échelles, aucun risque de divergence — les trois ne rendent jamais la même valeur. Le
 * jour où deux d'entre elles répondraient à la même question, c'est **celles-là** qu'il
 * faudrait fondre, pas les trois.
 */
export function formatRemaining(millis: number): string {
    const minutes = Math.floor(millis / 60_000);
    if (minutes < 60) return `${String(minutes)}m`;

    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${String(hours)}h${pad(minutes % 60)}`;

    return `${String(Math.floor(hours / 24))}d ${pad(hours % 24)}h`;
}

/**
 * Le temps qui reste avant la remise à zéro, ou `null` quand il n'y a rien à écrire.
 *
 * `null` sur une date **passée** autant que sur une date absente : l'hôte a pu ne rien dire,
 * ou le fil de fond ne pas avoir encore rappelé depuis la fin de la fenêtre. Écrire `0m`
 * dans le second cas annoncerait une remise à zéro qu'Ash n'a pas constatée.
 */
export function remainingUntil(resetsAt: number | null, now: number): string | null {
    if (resetsAt === null) return null;
    const left = resetsAt - now;
    return left <= 0 ? null : formatRemaining(left);
}

function pad(value: number): string {
    return value.toString().padStart(2, "0");
}

function clamp(percent: number): number {
    return Math.min(100, Math.max(0, Math.round(percent)));
}

/* ------------------------------------------------------------------------------------- *
 * Le popover (vue 5b) — deux lignes, et rien de plus.
 * ------------------------------------------------------------------------------------- */

/**
 * Ce que le clic sur une pastille ouvre : les **deux** quotas, chacun avec son décompte et
 * sa barre, puis un pied.
 *
 * Les deux barres mesurent le **quota**, jamais le temps écoulé dans la fenêtre : `5h window`
 * est un libellé de la maquette, et la durée d'une fenêtre n'existe nulle part — ni ici, ni
 * côté backend ([`Quota`] ne porte que `percent` et `resetsAt`). Une troisième barre qui
 * prétendrait la montrer serait inventée de bout en bout.
 */
export function composeUsagePopover(quotas: readonly QuotaSegment[]): UiComponent {
    const card = column().class("status-usage-card");

    for (const quota of quotas) {
        const line = row(
            badge(quota.letter).class("status-usage-mark", `is-${quota.kind}`),
            badge(quota.kind).class("status-usage-name"),
        )
            .spacer()
            .add(badge(quota.percent).class("status-usage-share"))
            .class("status-usage-line");

        // Rien à la place d'un décompte absent : une ligne vide dirait qu'on attend une
        // valeur, alors qu'il n'y en a pas.
        if (quota.resets !== null) {
            line.add(badge(`resets in ${quota.resets}`).class("status-usage-resets"));
        }

        card.add(
            line,
            row(
                row()
                    .class("status-usage-fill", `is-${quota.kind}`)
                    .attr("style", `width: ${String(Math.round(quota.ratio * 100))}%`),
            ).class("status-usage-rail"),
        );
    }

    return card.add(
        row(text("5h window"))
            .spacer()
            .add(badge("⌘⌥U").class("status-usage-key"))
            .class("status-usage-foot"),
    );
}

/* ------------------------------------------------------------------------------------- *
 * Le rendu — des nœuds posés une fois, et mis à jour en place.
 * ------------------------------------------------------------------------------------- */

/**
 * Le groupe de droite de la ligne de statut.
 *
 * Il ne décide rien : il pose ce que les deux composeurs ci-dessus ont décidé. Ce qu'il tient
 * en propre est ce que le modèle n'a pas — les éléments eux-mêmes, et le popover ouvert.
 *
 * **Deux entrées, une par rythme, et de la même forme** : [`showContext`] pour ce qui vient
 * avec l'onglet, [`showQuotas`] pour ce qui vient du compte. Les deux reçoivent une valeur
 * déjà composée, et c'est ce qui dit où poser un segment de plus — dans `StatusLineModel`
 * s'il parle de l'onglet, dans `composeQuotas` s'il parle du compte, jamais dans cette classe.
 *
 * **Il ne reconstruit jamais ses nœuds.** C'est ce qui rend les deux critères de la tâche
 * structurels plutôt que respectés : la transition de 700 ms de la jauge ne peut pas repartir
 * d'un rendu à l'autre, et un changement d'onglet ne peut pas faire clignoter des quotas
 * qu'il ne touche pas.
 */
export class UsageSegments {
    readonly element: HTMLElement;

    private readonly pills = new Map<QuotaKind, QuotaPill>();
    private readonly gauge: HTMLElement;
    private readonly fill: HTMLElement;
    private readonly label: HTMLElement;
    private readonly model: HTMLElement;

    /** Les deux quotas tels qu'ils ont été composés — le popover les relit à l'ouverture. */
    private quotas: readonly QuotaSegment[] = [];
    /**
     * La dernière jauge posée, gardée pour une seule raison : quand les interrupteurs
     * changent, il faut réappliquer une valeur que personne ne renvoie — l'onglet n'a pas
     * bougé, donc `ash://tab-changed` n'a rien à dire.
     */
    private gaugeShown: ContextGauge | null = null;
    /**
     * Ce que la ligne montre (spec §4.2, vue 5c) — **lu, jamais détenu** : il vient de
     * `features::theme`, et les défauts posés ici ne servent qu'au battement d'avant la
     * première réponse du backend.
     */
    private segments: StatusBarSegments = DEFAULT_STATUS_BAR_SEGMENTS;
    private popover: HTMLElement | null = null;
    /** Ce qui est réellement à l'écran — voir [`fold`]. */
    private shownQuotas = 0;
    private shownGauge = false;

    /**
     * `beforeOpen` est appelé juste avant que le popover s'ouvre — c'est par là que la ligne
     * de statut referme son menu contextuel. Deux panneaux ne sont jamais ouverts ensemble,
     * et l'arbitrage est chez celui qui les possède tous les deux, pas ici.
     */
    constructor(private readonly beforeOpen: () => void = () => undefined) {
        this.element = document.createElement("div");
        this.element.className = "status-usage";

        for (const kind of ["session", "weekly"] as const) {
            const pill = quotaPill(kind, () => {
                this.togglePopover();
            });
            this.pills.set(kind, pill);
            this.element.append(pill.element);
        }

        this.gauge = document.createElement("span");
        this.gauge.className = "status-usage-gauge";
        // La jauge ne dit rien qu'un lecteur d'écran puisse lire : `ctx 41%`, juste à côté,
        // le dit en toutes lettres. Deux voix pour un chiffre en feraient entendre deux.
        this.gauge.setAttribute("aria-hidden", "true");
        this.fill = document.createElement("span");
        this.fill.className = "status-usage-fill";
        this.gauge.append(this.fill);

        this.label = document.createElement("span");
        this.label.className = "status-usage-label";

        // Un `<span>`, et non un bouton comme les pastilles de quota : il n'ouvre rien, donc
        // il n'a rien à faire sur le chemin de `tab` ni dans l'arbre d'accessibilité comme un
        // élément actionnable. Changer de modèle se dit `/model`, dans le terminal, et Ash
        // n'appuie sur rien à la place de l'utilisateur (ADR-0015).
        this.model = document.createElement("span");
        this.model.className = "status-usage-model";

        this.element.append(this.gauge, this.label, this.model);
        this.showContext(null);
    }

    /**
     * Les quotas du compte, déjà composés — c'est le **rythme du compte**, celui de l'event
     * `ash://account-usage` et du battement de seconde qui fait avancer les décomptes.
     *
     * Le jumeau de [`showContext`], et volontairement de la même forme : les deux entrées
     * reçoivent une valeur composée ailleurs, cette classe n'en décide aucune. Le même appel
     * sert l'event et le battement — la composition est pure, et écrire une valeur identique
     * ne touche pas le DOM.
     */
    showQuotas(quotas: readonly QuotaSegment[]): void {
        this.quotas = quotas;
        const shown = new Map(
            inStatusBar(this.quotas, this.segments).map((quota) => [quota.kind, quota]),
        );

        for (const [kind, pill] of this.pills) {
            pill.show(shown.get(kind) ?? null);
        }
        this.shownQuotas = shown.size;
        this.fold();

        // Le popover ouvert suit la même valeur que la barre : il n'a pas de source à lui.
        // Et si les deux quotas viennent de disparaître, il disparaît avec eux — un cadre
        // vide serait la façon de dire « il manque quelque chose », ce qu'ADR-0016 refuse.
        if (this.popover === null) return;
        if (this.quotas.length === 0) this.closePopover();
        else this.paintPopover(this.popover);
    }

    /**
     * La jauge de l'onglet actif, déjà composée — le **rythme de l'onglet**, celui de
     * `ash://tab-changed`. `null` efface le segment : pas de jauge à zéro, pas de `ctx —`.
     */
    showContext(gauge: ContextGauge | null): void {
        this.gaugeShown = gauge;
        // La **barre** ne sort que s'il y a un rapport à montrer ; le **libellé** sort dès
        // qu'il y a une mesure. C'est toute la différence entre « Ash ne sait rien » et « Ash
        // sait combien, mais pas sur combien » — et le seul endroit où elle se voit.
        // Deux conditions par nœud, et elles ne disent pas la même chose : la donnée peut
        // manquer, ou l'utilisateur peut avoir décoché le segment. La seconde ne s'écrit
        // qu'ici — le popover, lui, ne connaît que la première.
        const share = gauge?.share ?? null;
        // Le **troisième** masquage, et la troisième absence : Ash peut mesurer sans connaître
        // la fenêtre, et connaître la fenêtre sans savoir nommer le modèle. Les trois se
        // décident séparément parce qu'elles disent trois choses différentes.
        const named = gauge?.model ?? null;
        // Les deux segments que porte ce groupe, décidés **avant** d'être posés : ce que
        // `fold` doit savoir, c'est ce qui a été décidé, et le relire sur un `hidden` déjà
        // écrit reviendrait à demander au DOM ce qu'on vient de lui dire.
        const showsContext = gauge !== null && this.segments.context;
        const showsModel = named !== null && this.segments.model;

        this.gauge.hidden = share === null || !this.segments.context;
        this.label.hidden = !showsContext;
        this.model.hidden = !showsModel;
        this.shownGauge = showsContext || showsModel;
        this.fold();

        if (gauge !== null) write(this.label, gauge.label);
        if (named !== null) write(this.model, named);

        if (share === null) {
            // Le palier part avec le rapport — qu'il n'y ait rien du tout, ou une mesure sans
            // dénominateur. Le laisser sur le groupe garderait un `compacting` qui ne décrit
            // plus rien, que la première règle posée sur une pastille lirait, et qui
            // annoncerait un compactage sur un chiffre qu'Ash n'a pas.
            delete this.element.dataset["context"];
            return;
        }

        this.element.dataset["context"] = share.level;
        const width = `${String(share.percent)}%`;
        if (this.fill.style.width !== width) this.fill.style.width = width;
    }

    /**
     * Le groupe entier s'efface quand il n'a rien à montrer.
     *
     * Ce n'est pas une coquetterie : la ligne est un `flex` à `gap: 14 px`, et un élément
     * vide compte quand même pour un `gap`. Sans ce repli, un onglet sans usage — un shell à
     * son invite — pousserait le rappel de sidebar repliée de 14 px vers la gauche. La ligne
     * doit rester celle d'avant, au pixel.
     */
    private fold(): void {
        this.element.hidden = this.shownQuotas === 0 && !this.shownGauge;
    }

    /**
     * Ce que la ligne montre vient de changer — la réponse du backend, ou une bascule du
     * menu contextuel.
     *
     * Les deux valeurs déjà posées sont réappliquées telles quelles : ni les quotas ni la
     * jauge n'ont bougé, et rien d'autre ne les renverra.
     */
    showSegments(segments: StatusBarSegments): void {
        this.segments = segments;
        this.showQuotas(this.quotas);
        this.showContext(this.gaugeShown);
    }

    /** Referme le popover, s'il est ouvert — le clic ailleurs, et le clic droit. */
    closePopover(): void {
        if (this.popover === null) return;
        document.removeEventListener("pointerdown", this.onPointerDown, true);
        document.removeEventListener("contextmenu", this.onContextMenu, true);
        this.popover.remove();
        this.popover = null;
    }

    private togglePopover(): void {
        if (this.popover !== null) {
            this.closePopover();
            return;
        }

        this.beforeOpen();

        const card = document.createElement("div");
        card.className = "status-usage-popover";
        card.setAttribute("role", "dialog");
        card.setAttribute("aria-label", "account usage");
        this.paintPopover(card);

        // Posé dans le `body`, et non dans la ligne de statut : celle-ci coupe ce qui la
        // dépasse (`overflow: hidden`), et un popover ancré au-dessus d'elle en dépasse par
        // construction.
        document.body.append(card);
        this.popover = card;
        this.anchor(card);

        document.addEventListener("pointerdown", this.onPointerDown, true);
        document.addEventListener("contextmenu", this.onContextMenu, true);
    }

    private paintPopover(card: HTMLElement): void {
        card.replaceChildren(paint(composeUsagePopover(this.quotas).build()));
    }

    /**
     * Au-dessus du groupe, aligné sur son bord droit — et ramené dans la fenêtre s'il
     * déborde. Même règle que la popup de branches : l'ancre est au **pied** de la fenêtre,
     * ouvrir vers le bas la ferait sortir de l'écran.
     */
    private anchor(card: HTMLElement): void {
        const bounds = this.element.getBoundingClientRect();
        card.style.right = `${String(Math.round(Math.max(8, window.innerWidth - bounds.right)))}px`;
        card.style.bottom = `${String(Math.round(window.innerHeight - bounds.top + 6))}px`;
    }

    /**
     * Un clic ailleurs referme — y compris sur la jauge, qui n'ouvre rien.
     *
     * Seules les **pastilles** sont exclues, et pas le groupe entier : leur propre `click`
     * bascule déjà, et les deux gestes se seraient annulés — le popover se serait refermé
     * puis rouvert dans le même battement.
     */
    private readonly onPointerDown = (event: Event): void => {
        const target = event.target;
        if (!(target instanceof Node)) return;
        if (this.popover?.contains(target) === true) return;
        for (const pill of this.pills.values()) {
            if (pill.element.contains(target)) return;
        }
        this.closePopover();
    };

    /** Le clic droit referme, où qu'il tombe — y compris sur la pastille qui a ouvert. */
    private readonly onContextMenu = (): void => {
        this.closePopover();
    };
}

/** Une pastille de la barre : la lettre, le pourcentage, le décompte. */
interface QuotaPill {
    readonly element: HTMLElement;
    show(quota: QuotaSegment | null): void;
}

function quotaPill(kind: QuotaKind, onClick: () => void): QuotaPill {
    // Un vrai `<button>`, comme l'ancre de branche : c'est ce qui le met sur le chemin de
    // `tab` et dans l'arbre d'accessibilité sans une ligne de code.
    const element = document.createElement("button");
    element.type = "button";
    element.className = "status-usage-quota";
    element.dataset["quota"] = kind;
    element.title = `${kind} usage`;
    element.addEventListener("click", onClick);

    const mark = span("status-usage-mark");
    const percent = span("status-usage-percent");
    const resets = span("status-usage-resets");
    element.append(mark, percent, resets);

    return {
        element,
        show: (quota): void => {
            element.hidden = quota === null;
            if (quota === null) return;
            write(mark, quota.letter);
            write(percent, quota.percent);
            write(resets, quota.resets === null ? "" : ` · ${quota.resets}`);
        },
    };
}

function span(className: string): HTMLElement {
    const element = document.createElement("span");
    element.className = className;
    return element;
}

/**
 * Écrire, mais seulement si le mot a changé.
 *
 * Ce n'est pas une optimisation : c'est ce qui fait qu'un rendu déclenché par un changement
 * d'onglet ne touche pas un nœud de quota, alors même qu'il le traverse.
 */
function write(element: HTMLElement, value: string): void {
    if (element.textContent !== value) element.textContent = value;
}
