import type { WorktreeMetadata } from "@/shared/ipc";
import type { GitBridge } from "./ports";

/**
 * Ce que la ligne de statut sait de l'état git des worktrees habités.
 *
 * Elle a besoin d'une réponse **maintenant**, à chaque rendu, et la boucle de sonde en
 * provoque un plusieurs fois par seconde. La commande `git_metadata`, elle, est `async` et
 * peut coûter un `git status` : la rappeler à chaque rendu lancerait un processus trois
 * fois par seconde et par worktree. Ce cache est ce qui sépare les deux rythmes — on
 * demande **une fois** par worktree, et c'est ensuite la surveillance d'ADR-0011 qui
 * pousse les changements.
 *
 * Rien n'est détenu ici au sens d'ADR-0009 : c'est le backend qui sait, ce cache ne fait
 * que garder sa dernière réponse à portée d'un rendu synchrone.
 */
export class WorktreeMetadataStore {
    /** La dernière réponse du backend, `null` compris — hors dépôt est une réponse. */
    private readonly known = new Map<string, WorktreeMetadata | null>();
    /** Les worktrees déjà demandés : sans ça, deux rendus lanceraient deux lectures. */
    private readonly asked = new Set<string>();

    /**
     * `onChange` est appelé quand une réponse arrive **après** le rendu qui l'a demandée :
     * c'est ce qui fait apparaître la branche une fois que git a répondu.
     */
    constructor(
        private readonly bridge: GitBridge,
        private readonly onChange: () => void,
    ) {
        void this.bridge
            .onMetadataChanged((changed) => {
                this.known.set(changed.worktreeRoot, changed.metadata);
                this.asked.add(changed.worktreeRoot);
                this.onChange();
            })
            .catch(() => {
                // Pas d'abonnement : la ligne montrera l'état lu à l'ouverture de l'onglet
                // et n'en bougera plus. C'est une dégradation visible, pas une raison
                // d'empêcher la fenêtre d'ouvrir.
            });
    }

    /**
     * Ce qu'on sait du worktree, sans attendre.
     *
     * Rend `null` tant que le backend n'a pas répondu — ce qui se lit `no repo` le temps
     * d'un aller-retour, puis se corrige tout seul. Afficher une ligne vide en attendant
     * serait pire : elle sauterait à chaque changement d'onglet.
     */
    of(worktreeRoot: string | null): WorktreeMetadata | null {
        if (worktreeRoot === null) return null;

        const known = this.known.get(worktreeRoot);
        if (known !== undefined) return known;

        if (!this.asked.has(worktreeRoot)) {
            this.asked.add(worktreeRoot);
            void this.bridge
                .metadata(worktreeRoot)
                .then((metadata) => {
                    this.known.set(worktreeRoot, metadata);
                    this.onChange();
                })
                .catch(() => {
                    // Une commande qui échoue laisse le worktree « pas encore lu » : la
                    // surveillance le rattrapera si elle le suit, et un nouvel onglet
                    // reposera la question.
                    this.asked.delete(worktreeRoot);
                });
        }

        return null;
    }
}
