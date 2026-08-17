#!/usr/bin/env bash
#
# La septième vérification : Ash s'ouvre-t-il vraiment ?
#
# Les six autres ont toutes été vertes le jour où l'application ne démarrait plus du
# tout — un `state()` appelé avant son `manage()` dans le composition root, qui ne
# panique qu'au lancement. Rien ne pouvait le voir : assembler une application Tauri
# demande une vraie application, et la doctrine du projet range le câblage Tauri parmi
# ce qui ne mérite pas de test unitaire.
#
# Ce script ne remplace pas l'agent `qa` — il ne regarde rien, il ne clique nulle part.
# Il répond à une seule question, celle qui manquait : **le processus survit-il à son
# propre démarrage, et la chaîne va-t-elle jusqu'au shell ?**
#
# Il ouvre brièvement une fenêtre. C'est le prix, et il n'y a pas de contournement :
# `run()` crée la fenêtre, et c'est précisément là que les pannes de câblage sortent.
#
#   scripts/smoke.sh              # ~15 s après un build à chaud
#   SMOKE_TIMEOUT=30 scripts/smoke.sh
#
set -uo pipefail

TIMEOUT="${SMOKE_TIMEOUT:-20}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="$(mktemp -t ash-smoke)"
VITE_LOG="$(mktemp -t ash-smoke-vite)"
APP_PID=""
VITE_PID=""

say() { printf '\033[1msmoke:\033[0m %s\n' "$1"; }
fail() { printf '\033[31msmoke: %s\033[0m\n' "$1" >&2; exit 1; }

cleanup() {
    [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null
    [ -n "$VITE_PID" ] && kill "$VITE_PID" 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT INT TERM

# `cargo` n'est pas dans le PATH de tous les shells — voir CLAUDE.md.
[ -x "$HOME/.cargo/env" ] || [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env" 2>/dev/null
command -v cargo >/dev/null || fail "cargo introuvable — 'source ~/.cargo/env'"

cd "$ROOT"

say "compilation du backend"
cargo build --manifest-path src-tauri/Cargo.toml --quiet || fail "la compilation a échoué"

# Le binaire de développement charge la webview depuis le serveur Vite (`devUrl`). Sans
# lui, la fenêtre resterait vide et le shell ne serait jamais demandé — on ne testerait
# plus que la moitié du démarrage.
if lsof -ti :1420 >/dev/null 2>&1; then
    say "serveur Vite déjà en écoute sur 1420, réutilisé"
else
    say "démarrage du serveur Vite"
    bun run dev >"$VITE_LOG" 2>&1 &
    VITE_PID=$!
    for _ in $(seq 1 40); do
        lsof -ti :1420 >/dev/null 2>&1 && break
        sleep 0.25
    done
    lsof -ti :1420 >/dev/null 2>&1 || fail "Vite n'écoute pas sur 1420 — voir $VITE_LOG"
fi

say "lancement de l'application"
./src-tauri/target/debug/ash >"$LOG" 2>&1 &
APP_PID=$!

# On ne cherche jamais le processus par son nom : l'utilisateur peut avoir sa propre
# instance d'Ash ouverte, et la tuer serait impardonnable. Le binaire lancé ci-dessus est
# celui de développement — il se présente comme `Ash-dev`, et n'a ni le nom, ni l'icône, ni
# l'identifiant de paquet de l'Ash installé (voir CLAUDE.md).
deadline=$((SECONDS + TIMEOUT))
shell_seen=""
while [ $SECONDS -lt $deadline ]; do
    kill -0 "$APP_PID" 2>/dev/null || break
    if [ -z "$shell_seen" ] && pgrep -P "$APP_PID" >/dev/null 2>&1; then
        shell_seen="$(pgrep -P "$APP_PID" -l 2>/dev/null | head -1)"
        break
    fi
    sleep 0.5
done

if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "--- sortie de l'application ---" >&2
    cat "$LOG" >&2
    fail "l'application s'est arrêtée pendant son démarrage"
fi

if grep -q "panicked at" "$LOG"; then
    echo "--- sortie de l'application ---" >&2
    cat "$LOG" >&2
    fail "panique au démarrage"
fi

[ -n "$shell_seen" ] || fail "aucun shell lancé en ${TIMEOUT}s — la webview n'a pas demandé d'onglet (log : $LOG)"

say "l'application tient, et son shell tourne — $shell_seen"
rm -f "$LOG" "$VITE_LOG"
