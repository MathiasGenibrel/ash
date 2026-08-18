import type { Instrumented, RecognizedAgent } from "@/shared/ipc";

/**
 * Le marqueur qu'une ligne d'agent porte quand rien ne l'instrumente (ADR-0006, ADR-0007).
 *
 * **La sidebar informe, l'écran agit** ([ADR-0010](../../../docs/adr/0010-sidebar-informe-terminal-agit.md)) :
 * la colonne signale, discrètement, qu'un agent reconnu ne dira jamais qu'il attend une
 * réponse — et le geste ouvre la fenêtre de réglages, seul endroit d'où Ash écrit chez
 * l'utilisateur. Pas de bandeau dans la zone terminal, qui recréerait un second chemin
 * d'écriture ; pas de modal, pas de vol de focus, pas de notification.
 *
 * Ce module est pur pour la même raison que [`./visible`] : la règle qui décide *si* la ligne
 * signale quelque chose, et *ce qu'elle dit*, ne se vérifierait pas dans un test de rendu —
 * `bun test` n'a pas de DOM.
 */

/** Ce que la ligne montre, ou `null` quand elle n'a rien à signaler. */
export interface InstrumentationMark {
    /** Le glyphe posé à droite du nom. Discret, et distinct des glyphes d'état. */
    readonly glyph: string;
    /** La phrase entière — infobulle et nom accessible. */
    readonly title: string;
    /**
     * L'outil que le geste nomme, ou `null` quand il n'y a **pas** de geste.
     *
     * `null` quand aucun adaptateur de cette version ne sait instrumenter cet outil : la
     * ligne le dit, et n'offre pas un bouton qui n'écrirait jamais rien.
     *
     * Le nom voyage **dans le marqueur** et non à côté : « la ligne a un geste » et « voici
     * l'outil qu'il nomme » sont un seul fait, et les séparer obligerait la vue à les
     * recoller — donc à affirmer, sans que le type le prouve, qu'ils sont d'accord.
     */
    readonly instrument: InstrumentTarget | null;
}

/** L'outil qu'un geste d'instrumentation désigne — le strict nécessaire pour l'écran. */
export interface InstrumentTarget {
    readonly command: string;
    readonly adapter: string;
}

/**
 * Ce que la phrase doit apprendre à quelqu'un qui lit sa sidebar.
 *
 * Elle nomme la conséquence avant la cause : ce qui manque n'est pas « un bloc dans un
 * fichier », c'est l'état `waiting`. Sans ce mot, un agent qui ne demande jamais rien se lit
 * comme une panne d'Ash.
 */
const WHY = "idle and working still show; waiting never will";

/**
 * Le marqueur d'un onglet, à partir de ce que le backend a reconnu.
 *
 * `null` dans les deux cas où il n'y a rien à dire : aucun outil reconnu — un shell, un
 * `vim` —, et un outil dont la configuration porte déjà le marqueur d'Ash.
 */
export function instrumentationMark(agent: RecognizedAgent | null): InstrumentationMark | null {
    if (agent === null) return null;
    return MARKS[agent.instrumented](agent);
}

const MARKS: Record<Instrumented, (agent: RecognizedAgent) => InstrumentationMark | null> = {
    // Rien à signaler : les hooks sont posés, l'onglet montrera les cinq états.
    installed: () => null,
    missing: ({ command, adapter }) => ({
        glyph: "!",
        title: `${command} is not instrumented — ${WHY}. open settings to install ash's hooks.`,
        instrument: { command, adapter },
    }),
    // Aucun geste : `generic` est l'adaptateur de l'outil dont on ne sait rien, et il ne pose
    // aucun hook (ADR-0008). Proposer d'instrumenter mènerait à un bouton éteint.
    unsupported: ({ command }) => ({
        glyph: "!",
        title: `ash has no adapter for ${command} yet — ${WHY}.`,
        instrument: null,
    }),
};
