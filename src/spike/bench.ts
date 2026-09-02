/**
 * Banc de mesure du spike xterm.js — **code jetable**.
 *
 * Question posée : xterm.js, dans WKWebView, tient-il la sortie qu'un agent de code
 * produit réellement ? La spec désigne deux fois ce point comme le risque à lever avant
 * tout le reste.
 *
 * Ce que le banc mesure, et pourquoi :
 *
 * - **débit soutenu** — un `cat` ou un `bun test` verbeux ne doit pas prendre plus de
 *   temps dans Ash que dans Terminal.app ;
 * - **temps de trame sous charge** — c'est lui qui décide si la fenêtre « rame ». Une
 *   trame de 300 ms est visible ; une moyenne flatteuse la cache, donc on garde p95 et
 *   maximum ;
 * - **latence frappe → peinture sous charge** — le vrai grief contre un terminal lent :
 *   taper pendant que ça défile. Mesuré pendant le flux, pas au repos.
 *
 * Le flux vient du backend par un `Channel` Tauri, c'est-à-dire par le chemin que le
 * PTY empruntera. Générer la sortie en TypeScript aurait mesuré le moteur de rendu seul.
 */

import { Channel, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Terminal } from "@xterm/xterm";
import { WebglAddon } from "@xterm/addon-webgl";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

type Frame = { kind: "chunk"; data: string } | { kind: "done"; bytes: number; lines: number };

type Workload = "test" | "cat" | "color";
type Renderer = "dom" | "webgl";

export interface Measurement {
    renderer: Renderer;
    workload: Workload;
    lines: number;
    bytes: number;
    /** La taille de la grille change tout le coût de rendu : elle fait partie de la mesure. */
    cols: number;
    rows: number;
    seconds: number;
    linesPerSecond: number;
    megabytesPerSecond: number;
    frameMs: Percentiles;
    keystrokeMs: Percentiles;
    droppedFrames: number;
    webglFallback: boolean;
}

interface Percentiles {
    p50: number;
    p95: number;
    max: number;
    samples: number;
}

/**
 * Un million de lignes, et pas cent mille.
 *
 * Le premier jet mesurait 100 000 lignes : elles s'écoulaient en 0,26 s, ce qui laissait
 * six échantillons de trame et **une seule** frappe par run. Un p95 sur six points n'est
 * pas un p95. Il faut plusieurs secondes de charge continue pour que la distribution des
 * trames et des latences veuille dire quelque chose.
 */
const LINES_PER_RUN = 1_000_000;

/** Intervalle entre deux frappes simulées pendant le flux. */
const KEYSTROKE_EVERY_MS = 100;

/** Au-delà, la trame est perdue pour l'œil : 60 Hz laisse 16,7 ms. */
const DROPPED_FRAME_MS = 34;

function percentiles(values: number[]): Percentiles {
    if (values.length === 0) return { p50: 0, p95: 0, max: 0, samples: 0 };
    const sorted = [...values].sort((a, b) => a - b);
    const at = (q: number): number =>
        sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
    return {
        p50: round(at(0.5)),
        p95: round(at(0.95)),
        max: round(sorted[sorted.length - 1] ?? 0),
        samples: sorted.length,
    };
}

const round = (n: number): number => Math.round(n * 100) / 100;

/** Attend la peinture suivante — `rAF` est appelé juste avant, donc on en enchaîne deux. */
const nextPaint = (): Promise<number> =>
    new Promise((resolve) => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now())));
    });

function createTerminal(
    host: HTMLElement,
    renderer: Renderer,
): { term: Terminal; fallback: boolean } {
    const term = new Terminal({
        fontFamily: '"JetBrains Mono", ui-monospace, monospace',
        fontSize: 13,
        // Le défaut d'xterm.js est 1000 ; un terminal de travail en garde bien plus, et
        // le scrollback pèse sur le rendu. On mesure la configuration visée, pas le défaut.
        scrollback: 10_000,
        allowProposedApi: true,
    });

    term.open(host);

    // `activate` seul ne redimensionne rien : sans `fit()`, xterm reste au défaut 80×24.
    // La première passe a mesuré cette grille-là — soit un quart de la surface d'une
    // fenêtre de travail, et donc un coût de rendu sans rapport.
    const fit = new FitAddon();
    term.loadAddon(fit);
    fit.fit();

    let fallback = false;
    if (renderer === "webgl") {
        try {
            const addon = new WebglAddon();
            // WKWebView peut perdre le contexte WebGL sous pression mémoire. Sans cette
            // écoute, la perte se lirait comme un écran figé plutôt que comme un repli.
            addon.onContextLoss(() => {
                fallback = true;
                addon.dispose();
            });
            term.loadAddon(addon);
        } catch {
            fallback = true;
        }
    }

    return { term, fallback };
}

async function measure(
    host: HTMLElement,
    renderer: Renderer,
    workload: Workload,
): Promise<Measurement> {
    const { term, fallback } = createTerminal(host, renderer);

    const frameTimes: number[] = [];
    const keystrokeTimes: number[] = [];
    let running = true;

    // Cadence des trames pendant tout le flux.
    let last = performance.now();
    const tick = (now: number): void => {
        frameTimes.push(now - last);
        last = now;
        if (running) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);

    // Frappes simulées pendant le flux : c'est sous charge que la latence compte.
    const typing = (async (): Promise<void> => {
        while (running) {
            await new Promise((r) => setTimeout(r, KEYSTROKE_EVERY_MS));
            if (!running) break;
            const started = performance.now();
            await new Promise<void>((resolve) => term.write("x", resolve));
            await nextPaint();
            keystrokeTimes.push(performance.now() - started);
        }
    })();

    const channel = new Channel<Frame>();
    let bytes = 0;
    let lines = 0;
    let pending = 0;
    let ended = false;
    let resolveDone: () => void = () => {};
    const done = new Promise<void>((resolve) => {
        resolveDone = resolve;
    });

    const settle = (): void => {
        if (ended && pending === 0) resolveDone();
    };

    channel.onmessage = (frame): void => {
        if (frame.kind === "chunk") {
            pending += 1;
            // Le rappel de `write` est le seul signal fiable de « consommé » : xterm.js
            // met les écritures en file et rend en différé. Chronométrer l'appel seul
            // mesurerait la vitesse à laquelle on remplit une file d'attente.
            //
            // C'est aussi le point d'acquittement : rendre le crédit avant que xterm ait
            // digéré le morceau annulerait le contrôle de flux et ramènerait la perte de
            // données que la fenêtre est là pour empêcher.
            term.write(frame.data, () => {
                pending -= 1;
                void invoke("spike_ack");
                settle();
            });
            return;
        }
        bytes = frame.bytes;
        lines = frame.lines;
        ended = true;
        settle();
    };

    // Un run bloqué doit échouer bruyamment. Le premier jet du banc s'est figé dix
    // minutes parce que `write()` levait et que le rappel n'arrivait jamais : sans
    // garde-fou, un blocage se lit comme une mesure lente.
    const guard = new Promise<never>((_, reject) => {
        setTimeout(
            () => reject(new Error(`${renderer}/${workload} : bloqué au-delà de 120 s`)),
            120_000,
        );
    });

    const started = performance.now();
    await Promise.race([
        invoke("spike_stream", { channel, workload, lines: LINES_PER_RUN }),
        guard,
    ]);
    await Promise.race([done, guard]);
    await nextPaint();
    const seconds = (performance.now() - started) / 1000;
    const { cols, rows } = term;

    running = false;
    await typing;
    term.dispose();
    host.replaceChildren();

    return {
        renderer,
        workload,
        lines,
        bytes,
        cols,
        rows,
        seconds: round(seconds),
        linesPerSecond: Math.round(lines / seconds),
        megabytesPerSecond: round(bytes / 1024 / 1024 / seconds),
        frameMs: percentiles(frameTimes),
        keystrokeMs: percentiles(keystrokeTimes),
        droppedFrames: frameTimes.filter((t) => t > DROPPED_FRAME_MS).length,
        webglFallback: fallback,
    };
}

export async function runBench(
    host: HTMLElement,
    log: (line: string) => void,
): Promise<Measurement[]> {
    // Le coût de rendu suit le nombre de cellules. Mesurer dans une fenêtre par défaut
    // flatterait le résultat : on mesure la fenêtre plein écran, celle où un agent
    // déverse réellement sa sortie.
    await getCurrentWindow().maximize();
    await nextPaint();

    const results: Measurement[] = [];

    for (const renderer of ["dom", "webgl"] as const) {
        for (const workload of ["test", "cat", "color"] as const) {
            log(`… ${renderer} / ${workload}`);
            const m = await measure(host, renderer, workload);
            results.push(m);
            log(
                `${renderer}/${workload} — ${m.megabytesPerSecond} Mo/s · ` +
                    `${m.linesPerSecond.toLocaleString("fr")} lignes/s · ` +
                    `trame p95 ${m.frameMs.p95} ms (max ${m.frameMs.max}) · ` +
                    `frappe p95 ${m.keystrokeMs.p95} ms${m.webglFallback ? " · WEBGL PERDU" : ""}`,
            );
        }
    }

    const path = await invoke<string>("spike_report", {
        report: {
            userAgent: navigator.userAgent,
            devicePixelRatio: window.devicePixelRatio,
            linesPerRun: LINES_PER_RUN,
            results,
        },
    });
    log(`rapport écrit : ${path}`);

    return results;
}
