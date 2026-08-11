/**
 * Quand la vérification se relance toute seule.
 *
 * La maquette l'écrit en une phrase : *« ash re-runs on its own 400 ms after the last key,
 * or right away on ⏎ »*. C'est un **debounce**, pas un intervalle : rien ne tourne tant que
 * personne ne tape, et une frappe annule le report de la précédente.
 *
 * **Deux entrées, et pas une seule qui saurait réagir à une touche.** [`Relaunch.soon`] est
 * pour ce qui peut encore être suivi d'autre chose — une frappe ; [`Relaunch.now`] est pour
 * ce qui ne le sera pas : `⏎`, un menu d'adaptateur qu'on referme, et demain un chemin
 * choisi dans le Finder, qui ne passe par aucune touche. La question du sélecteur de
 * fichiers (spec §9.9) est donc déjà répondue par la forme, sans que `Browse…` existe.
 *
 * **Le report est par entrée.** Deux cartes se vérifient indépendamment ; un report unique
 * ferait qu'une frappe dans l'une annule la vérification de l'autre.
 *
 * Le temps est **injecté**. C'est ce qui permet de prouver la règle sans faire dormir un
 * test — la discipline de `shared/time.rs` côté Rust, et la même ici.
 */

/** Reporter une action. Rend de quoi l'annuler. */
export interface Timer {
    after(delay: number, action: () => void): () => void;
}

/** Le délai de la maquette, en millisecondes. */
export const RELAUNCH_DELAY = 400;

/** Le vrai temps du navigateur. */
export const windowTimer: Timer = {
    after(delay, action) {
        const handle = window.setTimeout(action, delay);
        return () => {
            window.clearTimeout(handle);
        };
    },
};

export interface Relaunch {
    /** Une raison qui peut encore être suivie d'une autre — une frappe. */
    soon(key: string): void;
    /** Une raison qui ne le sera pas — `⏎`, un menu, un sélecteur de fichiers. */
    now(key: string): void;
    /** Oublie un report en cours — l'entrée a disparu, ou l'écran a changé. */
    cancel(key: string): void;
    /** Oublie tous les reports en cours. */
    cancelAll(): void;
}

/**
 * Monte le déclencheur : `run` est appelé une fois par salve, avec la clé de l'entrée.
 */
export function createRelaunch(
    run: (key: string) => void,
    timer: Timer = windowTimer,
    delay: number = RELAUNCH_DELAY,
): Relaunch {
    const pending = new Map<string, () => void>();

    function cancel(key: string): void {
        pending.get(key)?.();
        pending.delete(key);
    }

    return {
        soon(key) {
            // Le report précédent est annulé avant d'en poser un nouveau : sans ça, taper
            // huit caractères lancerait huit vérifications, dont sept décriraient un chemin
            // que l'utilisateur n'a jamais fini d'écrire.
            cancel(key);
            const stop = timer.after(delay, () => {
                pending.delete(key);
                run(key);
            });
            pending.set(key, stop);
        },
        now(key) {
            cancel(key);
            run(key);
        },
        cancel,
        cancelAll() {
            for (const key of [...pending.keys()]) cancel(key);
        },
    };
}
