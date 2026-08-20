import "./styles.css";
import { mountBranchCard, mountBranches, mountWorktreeTable } from "@/features/git";
import { mountBottomPanel, type BottomPanelState } from "@/features/panel";
import { revealTool } from "@/features/settings";
import { mountSidebar } from "@/features/sidebar";
import type { BranchCard, SidebarRows } from "@/shared/ipc";
import {
    mountTerminals,
    type FontFamilySignal,
    type TabId,
    type TabInfo,
    type Terminals,
} from "@/features/terminal";
import { loadAppName } from "./app-name";
import { followTerminalFontSize, type FontSizeChanges } from "./font-size";
import {
    NEW_TAB_ACTION,
    onMenuAction,
    onShortcutsChanged,
    shortcutKeys,
    shortcutOwner,
    type MenuAction,
} from "./menu";
import { onSelectTab } from "./select-tab";
import { installShortcuts } from "./shortcuts";
import { followSidebarDensity } from "./sidebar-density";
import { fontStack, followTerminalFont } from "./terminal-font";
import { followThemeMode, type ThemeChanges } from "./theme";
import { createTitleBar } from "./titlebar";
import { windowTitle } from "./window-title";
import { followSidebarRows, type SidebarRowsBinding } from "./sidebar-rows";
import { followSidebarColumn, type SidebarColumnBinding } from "./sidebar-column";
import { followBottomPanel, type BottomPanelBinding } from "./bottom-panel";
import { followBranchCard, type BranchCardBinding } from "./branch-card";
import { listWorktrees, worktreeRemoval } from "./worktrees";

/**
 * Composition root du frontend.
 *
 * C'est ici, et nulle part ailleurs, que les features sont instanciées et câblées
 * entre elles. Une feature ne va pas chercher sa voisine : elle reçoit ce dont elle a
 * besoin. Voir `.claude/docs/architecture.md`.
 *
 * Le menu applicatif est déclaré en Rust ; c'est ici qu'on relie ses actions à la
 * feature terminal. La feature ne connaît pas le menu, et le menu ne connaît pas la
 * feature.
 */
function mount(
    root: HTMLElement,
    theme: ThemeChanges,
    fontSize: FontSizeChanges,
    fontFamily: FontFamilySignal,
    sidebarRows: SidebarRowsBinding,
    sidebarColumn: SidebarColumnBinding,
    bottomPanel: BottomPanelBinding,
    branchCard: BranchCardBinding,
    appName: string,
): void {
    // Deux rangées : la bande de titre, puis les deux colonnes. La bande traverse toute la
    // largeur — c'est ce qui la laisse saisissable à droite des pastilles, et ce qui la
    // rend indifférente à `⌘B`.
    root.classList.add("ash-shell");

    const layout = document.createElement("div");
    layout.className = "ash-layout";

    const host = document.createElement("div");
    host.className = "terminal-host";

    // Le thème, la taille et la police sont passés, pas cherchés : ce sont trois préférences
    // d'apparence de l'**application**, détenues par le backend
    // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)), et un terminal ne peint
    // pas en CSS — il lui faut l'avis pour relire la palette, changer de taille et refaire
    // sa grille, onglets déjà ouverts compris.
    // Le panneau bas (spec §4.3) se pose **entre** les terminaux et la ligne de statut, et
    // c'est là toute son histoire : la rangée du milieu de `.terminal-workbench` annonce sa
    // hauteur, le terminal absorbe la différence, sa pile rétrécit, son `ResizeObserver`
    // refait la grille, et le PTY reçoit son `SIGWINCH`. Ce chemin est le seul, et il existait
    // déjà — le panneau n'en ouvre pas un second
    // ([ADR-0003](../../docs/adr/0003-zone-terminal-unique.md)).
    //
    // Les deux features ne se connaissent pas : la feature terminal reçoit un élément, le
    // panneau reçoit une hauteur de zone à mesurer. Elles se rencontrent ici, et nulle part
    // ailleurs. Le panneau ne détient rien non plus — ses trois gestes partent au backend et
    // reviennent par son annonce, comme la colonne de gauche.
    const panel = mountBottomPanel({
        showView: (view) => {
            bottomPanel.showView(view);
        },
        setHeight: (height) => {
            bottomPanel.setHeight(height);
        },
        close: () => {
            bottomPanel.close();
        },
        // La zone terminal, et non la fenêtre : les bornes du panneau (15 % à 70 %) parlent de
        // la place qu'il partage avec les terminaux, pas de celle qu'occupe la bande de titre.
        areaHeight: () => host.getBoundingClientRect().height,
    });

    const terminals = mountTerminals(host, theme, fontSize, fontFamily, panel.element);

    // La sidebar ne connaît pas la feature terminal, et la feature terminal ne connaît pas
    // la sidebar : elles se rencontrent ici, et nulle part ailleurs. La sidebar ne
    // s'abonne à rien côté Tauri — elle reçoit les onglets **déjà situés** par le backend
    // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    const sidebar = mountSidebar({
        selectTab: (tabId) => void terminals.selectTab(tabId),
        newTab: () => void terminals.openTab("current-worktree"),
        // Le clic sur une ligne épinglée sans onglet (spec §5.2) : la sidebar nomme le
        // worktree, la feature terminal ouvre le PTY. Les deux ne se connaissent pas plus ici
        // qu'ailleurs — c'est le même câble que `selectTab`, dans l'autre sens.
        openTabIn: (worktreeRoot) => void terminals.openTab({ directory: worktreeRoot }),
        // Les deux gestes qui survivent à la fermeture partent au backend, et rien n'est posé
        // au passage : la colonne se redessine sur son annonce
        // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
        //
        // Un geste qui n'aboutit pas ne laisse **aucune moitié d'état** : le backend n'a rien
        // retenu, il n'annonce rien, et la ligne reste exactement comme elle était. Il n'y a
        // donc rien à rattraper ici, et une bannière parlerait d'une épingle au moment où
        // l'utilisateur regarde son terminal.
        setPinned: (worktreeRoot, pinned) => {
            sidebarRows.pin(worktreeRoot, pinned).catch(() => undefined);
        },
        setCollapsed: (key, collapsed) => {
            sidebarRows.collapse(key, collapsed).catch(() => undefined);
        },
        // Le marqueur « non instrumenté » d'une ligne d'agent (ADR-0006) : la sidebar nomme
        // l'outil, la fenêtre de réglages agit. C'est ici que les deux features se
        // rencontrent, et nulle part ailleurs — la sidebar ne connaît pas `settings`.
        instrument: revealTool,
        // La largeur et le repli suivent le chemin des épingles : le geste part, le backend
        // retient, et la colonne se redessine sur son annonce. `⌘B` et la poignée passent par
        // le même état, donc il n'y a qu'un détenteur et pas deux notions de « repliée ».
        setColumnWidth: (width) => {
            sidebarColumn.setWidth(width);
        },
        setColumnCollapsed: (collapsed) => {
            sidebarColumn.setCollapsed(collapsed);
        },
        toggleColumn: () => {
            sidebarColumn.toggle();
        },
    });
    // La colonne se dessine sur deux sources — les onglets que la sonde pousse, et l'état
    // gardé d'une session à l'autre —, donc les deux dernières valeurs se retiennent ici : un
    // avis n'apporte jamais que sa moitié.
    let tabs: readonly TabInfo[] = [];
    let activeTabId: TabId | null = null;
    let kept: SidebarRows = sidebarRows.changes.current;

    const drawSidebar = (): void => {
        sidebar.render(tabs, activeTabId, kept);
    };

    terminals.onTabs((nextTabs, nextActive) => {
        tabs = nextTabs;
        activeTabId = nextActive;
        drawSidebar();
    });
    sidebarRows.changes.subscribe((next) => {
        kept = next;
        drawSidebar();
    });
    // La colonne s'apprend par la même sorte d'annonce, et c'est elle — jamais le geste qui
    // l'a demandée — qui met la ligne de statut d'accord avec la colonne : repliée, la sidebar
    // ne nomme plus les agents, et la ligne reprend celui qui attend.
    sidebarColumn.subscribe((column) => {
        sidebar.setColumn(column);
        terminals.setSidebarCollapsed(column.collapsed);
    });
    sidebar.setColumn(sidebarColumn.current);
    terminals.setSidebarCollapsed(sidebarColumn.current.collapsed);
    drawSidebar();

    // La bande de titre dit qui l'on est et où l'on est : `<nom> — <dépôt> / <branche>` de
    // l'onglet actif (spec §4.2, amendée le 2026-08-17). Elle est reliée ici, comme la
    // sidebar, et pour la même raison — c'est du chrome de fenêtre, il surplombe les deux
    // colonnes, et ni la feature terminal ni la bande n'ont à se connaître. La règle qui
    // compose le texte est dans `window-title.ts` ; ici, il n'y a qu'un câble.
    //
    // Le nom traverse en paramètre plutôt que d'être relu à chaque titre : il est constant
    // pour toute la session, et le relire à chaque changement d'onglet ferait un
    // aller-retour Tauri par `cd`.
    // La fiche de branche (#31) se pose dans le **corps** du panneau, sur la vue `branch`.
    // Les deux features ne se connaissent pas : le panneau expose une boîte dont il garantit
    // la hauteur, la fiche est une vue qui n'en sait rien, et elles se rencontrent ici — comme
    // la sidebar et la feature terminal.
    //
    // Rien ne la pousse : une fiche est un fichier, relu **quand on la regarde**. Les deux
    // moments sont l'ouverture de la vue et le changement d'onglet actif, parce que la fiche
    // suit le worktree de l'onglet ([ADR-0012](../../docs/adr/0012-worktree-unite-de-travail.md)).
    const card = mountBranchCard({
        writeLog: () => {
            void showCard(branchCard.writeLog(cardWorktree ?? ""));
        },
        place: (local) => {
            void showCard(branchCard.place(cardWorktree ?? "", local));
        },
    });
    panel.body.append(card.element);

    let cardWorktree: string | null = null;
    let cardShown = false;

    const showCard = async (asked: Promise<BranchCard | null>): Promise<void> => {
        const shown = cardWorktree;
        const answer = await asked;
        // La réponse d'un worktree qu'on ne regarde plus est **jetée** : deux `cd` rapprochés
        // rendraient sinon la fiche du premier par-dessus celle du second.
        if (shown === cardWorktree) card.render(answer);
    };

    const drawCard = (): void => {
        card.element.hidden = !cardShown;
        if (!cardShown) return;
        if (cardWorktree === null) {
            card.render(null);
            return;
        }
        void showCard(branchCard.read(cardWorktree));
    };

    const titleBar = createTitleBar(windowTitle(null, appName));

    // La popup de branches (spec §7.1), reliée ici comme la sidebar et la bande de titre : la
    // feature terminal ne connaît pas `features/git`, et `features/git` ne connaît ni les
    // onglets ni la ligne de statut. Ce qui les relie tient en quatre câbles — où l'on est,
    // où s'ancrer, à qui rendre les doigts, et qui prévenir quand le dépôt a bougé.
    //
    // Le worktree courant est relu **à chaque ouverture** et non capturé : l'onglet actif
    // change, et une popup qui garderait la racine de son premier montage parlerait d'un
    // autre dépôt que celui affiché.
    let here: string | null = null;
    const branches = mountBranches(root, {
        worktreeRoot: () => here,
        anchor: () => terminals.branchAnchor(),
        // Les doigts reviennent au terminal en se refermant : rien dans Ash ne garde le
        // clavier après un geste (ADR-0010).
        restoreFocus: () => {
            host.querySelector<HTMLElement>(".xterm-helper-textarea")?.focus();
        },
        // Un checkout réussi change la branche du worktree. La surveillance de `.git`
        // d'ADR-0011 le verra d'elle-même ; ce câble ne fait que redessiner ce qui est déjà
        // en main, sans rien redemander à personne.
        onRepositoryChanged: drawSidebar,
    });
    terminals.onBranchesRequested(() => {
        branches.toggle();
    });

    terminals.onActiveTab((active) => {
        here = active?.tab.location?.worktreeRoot ?? null;
        titleBar.setTitle(windowTitle(active, appName));
        const worktree = active?.tab.location?.worktreeRoot ?? null;
        if (worktree === cardWorktree) return;
        cardWorktree = worktree;
        drawCard();
    });

    // Le tableau des worktrees (spec §7.3) se pose dans le corps du panneau, et ne connaît ni
    // Tauri ni les autres features : il reçoit ce qu'il sait demander. Les deux colonnes qui
    // font l'écran — `agents now` et `last worked by` — sont composées par le backend
    // ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)) ; ici, il n'y a que des
    // câbles.
    const worktrees = mountWorktreeTable({
        list: listWorktrees,
        removal: worktreeRemoval,
        // Le clic sur un agent va à son onglet — un geste de l'utilisateur, comme sur une
        // ligne de la sidebar (ADR-0010). Un onglet qui n'existe plus ne change rien.
        selectTab: (tabId) => void terminals.selectTab(tabId),
        openTabIn: (worktreeRoot) => void terminals.openTab({ directory: worktreeRoot }),
        // **Le point de jonction avec la fiche de branche (#31)** : le tableau demande la
        // fiche, et tout ce que la fenêtre sait en faire aujourd'hui est montrer la vue qui
        // la portera. Le jour où elle existera, c'est ici — et seulement ici — qu'on lui
        // passera le worktree et sa branche.
        showCard: () => {
            bottomPanel.showView("branch");
        },
        now: () => Date.now(),
    });

    // Le panneau s'apprend par l'annonce du backend, jamais par le geste qui l'a demandée :
    // c'est ce qui laisse un seul détenteur à l'ouverture, et ce qui fera que le clic sur un
    // onglet et le raccourci de #32 ne pourront pas se contredire.
    //
    // C'est aussi cette annonce qui décide **quand** le tableau se relit : une vue fermée n'a
    // rien à demander au backend, et une vue qui s'ouvre doit dire la vérité de l'instant.
    // Les quatre vues du panneau partageront ce corps ; celle-ci retire ce qu'elle y a posé
    // dès qu'une autre est montrée.
    const showPanelBody = (next: BottomPanelState): void => {
        const mine = next.open && next.view === "worktrees";
        if (mine) {
            if (worktrees.element.parentElement !== panel.body) {
                panel.body.replaceChildren(worktrees.element);
            }
            worktrees.refresh();
        } else if (worktrees.element.parentElement === panel.body) {
            worktrees.element.remove();
        }
    };

    bottomPanel.subscribe((next) => {
        panel.setPanel(next);
        const showing = next.open && next.view === "branch";
        const changed = showing !== cardShown;
        cardShown = showing;
        if (changed) drawCard();
        showPanelBody(next);
    });
    panel.setPanel(bottomPanel.current);
    showPanelBody(bottomPanel.current);

    layout.append(sidebar.element, sidebar.separator, host);
    root.append(titleBar.element, layout);

    // **Après l'accrochage au document, et pas avant** : les bornes du panneau se lisent sur
    // la hauteur de la zone terminal, et un élément qui n'est pas encore dans le document en
    // mesure zéro — le panneau se serait ouvert sur un pixel.
    panel.layOut();
    // La fenêtre qui rétrécit ne redessine pas le panneau — elle le **replace** dans ses
    // bornes, et rend ses lignes au terminal sans jamais réécrire la hauteur réglée.
    window.addEventListener("resize", () => {
        panel.layOut();
    });

    const fail = (error: unknown): void => {
        // Un shell qui ne démarre pas laisse l'application sans rien à montrer : le dire
        // vaut mieux qu'une fenêtre noire dont l'utilisateur ne peut rien conclure.
        const banner = document.createElement("p");
        banner.className = "ash-banner";
        banner.textContent = `${appName} : le shell n'a pas démarré — ${
            error instanceof Error ? error.message : String(error)
        }`;
        host.append(banner);
    };

    // Le premier onglet part de `~`, faute d'onglet actif dont reprendre le répertoire.
    terminals.openTab("home").catch(fail);

    const play = (action: MenuAction): void => {
        dispatch(terminals, sidebarColumn, action).catch(fail);
    };

    onMenuAction(play).catch(fail);
    // Le clic sur une bannière macOS ramène sur l'agent qui a interrompu (spec §8). Il
    // arrive par le même genre de chemin qu'une action de menu — un geste de l'utilisateur
    // hors de la webview — et il se joue par la même méthode que le clic sur une ligne de la
    // sidebar : un onglet qui n'existe plus ne change rien.
    onSelectTab((tabId) => {
        void terminals.selectTab(tabId);
    }).catch(fail);
    // `⌃⇥` et `⌃⇧⇥` arrivent par le clavier de la webview, faute d'être captées par le
    // menu natif — voir `shortcuts.ts`. Ce qu'elles jouent est **demandé au backend** : la
    // webview arrête la frappe, le backend nomme l'action, et la table de `dispatch` la joue.
    // Il n'y a donc toujours qu'un seul chemin d'effet, et plus aucune combinaison écrite ici
    // — une liaison déplacée cesse aussitôt de répondre à son ancienne touche.
    installShortcuts(document, shortcutOwner, play);

    // Le pied de la colonne annonce le raccourci de « nouvel onglet ». Il le **demande**,
    // et le redemande à chaque changement de liaison : écrit en dur, il mentait dès le
    // premier rebinding — et il vit dans cette fenêtre-ci, que la fenêtre de réglages ne
    // connaît pas ([ADR-0009](../../docs/adr/0009-cycle-de-vie-des-agents.md)).
    const showNewTabShortcut = (): void => {
        shortcutKeys(NEW_TAB_ACTION)
            .then((keys) => {
                sidebar.showNewTabShortcut(keys);
            })
            .catch(() => undefined);
    };
    showNewTabShortcut();
    onShortcutsChanged(showNewTabShortcut).catch(fail);
}

function dispatch(
    terminals: Terminals,
    sidebarColumn: SidebarColumnBinding,
    action: MenuAction,
): Promise<void> {
    switch (action.kind) {
        case "new-tab":
            return terminals.openTab("current-worktree");
        case "new-home-tab":
            return terminals.openTab("home");
        case "close-tab":
            return terminals.closeActiveTab();
        case "clear-scrollback":
            return terminals.clearActiveScrollback();
        case "select-tab":
            return terminals.selectTabAt(action.position);
        case "next-tab":
            return terminals.cycleTab(1);
        case "previous-tab":
            return terminals.cycleTab(-1);
        case "toggle-sidebar":
            // Le repli **part au backend** et revient par son annonce : c'est là qu'il vit
            // depuis qu'il survit au redémarrage, et c'est ce qui fait que `⌘B` et la poignée
            // du bord ne peuvent pas se contredire. Ce que l'annonce entraîne — la colonne
            // redessinée, la ligne de statut qui reprend l'agent qui attend — est branché une
            // fois pour toutes dans `mount`.
            sidebarColumn.toggle();
            return Promise.resolve();
    }
}

/**
 * Monte le banc de mesure du spike xterm.js au lieu de l'application.
 *
 * Derrière un drapeau, et éteint par défaut : l'application ne doit pas démarrer sur un
 * banc de mesure. Se relance avec `VITE_SPIKE=1 bun run tauri dev` — voir
 * `docs/spike-xterm.md`. L'import est dynamique pour que le banc ne pèse pas dans le
 * bundle quand le drapeau est absent.
 */
async function mountSpike(root: HTMLElement): Promise<void> {
    const output = document.createElement("pre");
    output.className = "spike-log";
    output.textContent = "spike xterm.js — mesure en cours…\n";

    const host = document.createElement("div");
    host.className = "spike-host";
    root.classList.add("spike");
    root.append(output, host);

    const log = (line: string): void => {
        output.textContent += `${line}\n`;
    };

    const { runBench } = await import("@/spike/bench");
    await runBench(host, log);
}

const root = document.querySelector<HTMLElement>("#root");

// Le point de montage vient d'`index.html`, pas de l'utilisateur : son absence est un
// bug de build, pas une erreur à rattraper.
if (root === null) {
    throw new Error("index.html n'expose pas #root");
}

// Avant tout montage : la palette d'abord, pour ne pas peindre une fenêtre en clair sur
// un macOS sombre le temps du premier aller-retour. Un échec du raccordement au backend
// laisse le thème du système, ce qui est exactement le défaut — il n'y a rien à rattraper.
const theme = followThemeMode(document.documentElement);
theme.ready.catch(() => undefined);

// Même forme, et pour la même raison : la taille gardée de la session précédente arrive
// par un aller-retour, et les premiers terminaux naissent avant sa réponse — ils s'y
// ajusteront comme à un `⌘+`. Un échec du raccordement laisse la taille par défaut, ce
// qui est exactement ce qu'un premier démarrage donne : il n'y a rien à rattraper.
const fontSize = followTerminalFontSize();
fontSize.ready.catch(() => undefined);

// Et la même forme une troisième fois, pour ce que la colonne garde d'une session à l'autre :
// les worktrees épinglés et les lignes repliées (spec §3.1, §5.2). La demande part avant
// l'attente de la police, comme les deux autres, pour que les allers-retours se recouvrent.
// Un échec du raccordement donne une colonne sans épingle — c'est exactement ce qu'un premier
// démarrage montre, donc il n'y a rien à rattraper.
const sidebarRows = followSidebarRows();
sidebarRows.ready.catch(() => undefined);

// Et une quatrième fois, pour la largeur de la colonne et son repli (spec §9). Un échec du
// raccordement donne une colonne de 240 px ouverte — exactement ce qu'un premier démarrage
// montre, donc il n'y a rien à rattraper là non plus.
const sidebarColumn = followSidebarColumn();
sidebarColumn.ready.catch(() => undefined);

// Et une cinquième, pour le panneau bas — sa hauteur, son ouverture et sa vue (spec §4.3).
// Un échec du raccordement laisse un panneau **fermé**, donc un terminal qui garde toute sa
// hauteur : c'est exactement ce qu'un premier démarrage montre, et le défaut le plus sûr.
const bottomPanel = followBottomPanel();
bottomPanel.ready.catch(() => undefined);

// La fiche de branche, elle, n'a rien à raccorder : elle n'a pas d'event, et se lit quand on
// la regarde. Le binding n'est qu'un nom de commande par geste (ADR-0013).
const branchCard = followBranchCard();

// La police du terminal et la densité de la sidebar suivent le même chemin que les trois
// au-dessus : elles sont détenues par le backend, demandées ici, et posées quand il répond.
// Toutes deux se règlent dans la fenêtre de réglages (spec §9), qui est un **autre document** :
// c'est l'annonce de Tauri qui les fait arriver jusqu'ici, pas un appel entre fenêtres.
const terminalFont = followTerminalFont();
terminalFont.ready.catch(() => undefined);

// La densité, elle, n'a rien à passer à une feature : elle se pose sur la racine, et le CSS
// fait le reste — comme la palette.
const density = followSidebarDensity(document.documentElement);
density.ready.catch(() => undefined);

/**
 * La police telle que la feature terminal la reçoit : une **pile**, jamais la seule famille.
 *
 * Composée ici parce que `app/` est déjà le seul endroit à savoir ce qu'Ash embarque, et
 * parce qu'une police désinstallée entre deux démarrages doit laisser un terminal aligné
 * plutôt qu'un rendu proportionnel.
 */
const fontFamily: FontFamilySignal = {
    get current(): string {
        return fontStack(terminalFont.family.current);
    },
    subscribe: (listener) => terminalFont.family.subscribe((family) => {
        listener(fontStack(family));
    }),
};

// Le nom de l'application, lui, est **attendu** au lieu d'être posé par défaut puis
// corrigé : il est constant pour toute la session, donc il n'y a pas de « valeur d'attente »
// honnête à écrire dans la bande — un `Ash` remplacé par `Ash-dev` au premier aller-retour
// serait un clignotement, et sur le seul mot qui distingue les deux applications. La
// demande part **ici**, avant l'attente de la police, pour que les deux se recouvrent : le
// démarrage ne s'allonge donc pas d'un aller-retour, il attend le plus lent des deux.
const appName = loadAppName();

// **Rien ne se monte avant que JetBrains Mono ne soit retombée.** Une vue de terminal
// mesure sa cellule une fois, à sa construction, et ne la remesure jamais : la construire
// trop tôt fige la largeur d'une face de repli, et donne des glyphes rognés à une lamelle
// verticale et un `➜` rendu `?`. Le mécanisme est documenté là où est la mesure, dans
// `features/terminal/xterm-view.ts`, avec la raison pour laquelle la vue ne peut pas s'en
// tirer seule.
//
// L'attente est ici parce que c'est ici qu'est l'ordre de démarrage, et parce que c'est le
// seul endroit où la condition se tient en une fois : les onglets ouverts plus tard
// héritent d'une mesure déjà juste. Elle ne coûte rien — la police est livrée dans le
// bundle, pas téléchargée, et la promesse retombe en quelques millisecondes. `finally` et
// non `then` : une police qui échoue à charger doit donner un terminal en police de repli,
// pas une fenêtre vide.
void document.fonts.ready.finally(() => {
    if (import.meta.env.VITE_SPIKE === "1") {
        // Le banc a la même contrainte, et une raison de plus : il chronomètre un rendu,
        // et une grille mesurée sur une face de repli n'est pas la grille qu'on mesure.
        mountSpike(root).catch((error: unknown) => {
            root.textContent = `spike — ÉCHEC : ${error instanceof Error ? error.message : String(error)}`;
        });
    } else {
        // `loadAppName` ne rejette jamais — elle se replie sur un nom plutôt que de laisser
        // une fenêtre sans rien —, donc il n'y a pas d'échec à rattraper ici.
        void appName.then((name) => {
            mount(
                root,
                theme.changes,
                fontSize.changes,
                fontFamily,
                sidebarRows,
                sidebarColumn,
                bottomPanel,
                branchCard,
                name,
            );
        });
    }
});
