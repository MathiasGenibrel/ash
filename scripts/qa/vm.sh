#!/usr/bin/env bash
#
# Une seconde session graphique pour la QA — Ash-dev tourne dans une VM, pas sur le bureau.
#
# L'agent `qa` construit **et lance** Ash-dev. Le build ne dérange personne, c'est du CPU ;
# le lancement, lui, prend le focus, le Dock et le WindowServer de la machine qui sert de
# terminal quotidien. C'est pour ça que `qa` est « sur demande », donc rare.
#
# Aucun pilote ne règle ça sur macOS — `tauri-driver` ne supporte que Linux et Windows, et
# `safaridriver` ne s'attache pas à une `WKWebView` tierce. Ce qui le règle est une seconde
# session graphique : **l'hôte construit, la VM lance**. La VM n'a donc ni Rust, ni Xcode,
# ni second `target/`.
#
#   scripts/qa/vm.sh doctor            # que manque-t-il ?
#   scripts/qa/vm.sh up                # démarre la VM (sans écran), rend son adresse
#   scripts/qa/vm.sh install           # copie l'Ash-dev.app construit sur l'hôte
#   scripts/qa/vm.sh fixture           # un dépôt git avec deux worktrees
#   scripts/qa/vm.sh run               # lance Ash-dev et joue les cinq états
#   scripts/qa/vm.sh shot five-states  # un PNG dans .qa-vm/shots/
#   scripts/qa/vm.sh down              # arrête la VM
#
# `up`, `install`, `fixture` et `down` sont idempotents : les relancer ne casse rien.
#
# Ce que ce script ne fait **jamais** :
#   - télécharger l'image de base (des dizaines de Go). Il exige qu'elle soit déjà tirée,
#     et dit la commande à taper ;
#   - construire Ash-dev. C'est `bun run package:debug`, sur l'hôte, avant `install` ;
#   - ouvrir une fenêtre sur le bureau de l'hôte. La seule sous-commande qui en ouvre une
#     est `console`, qui sert à préparer l'image une fois pour toutes et n'appartient à
#     aucun cycle de QA.
#
# Documentation, coûts et limites : .claude/docs/qa-vm.md
#
set -euo pipefail

# --- codes de retour, pour que l'appelant sache de quoi il s'agit -------------------------
#
# Ils sont une **interface** : l'agent `qa` agit dessus, et `.claude/agents/qa.md` en porte
# la table. Ce qui les sépare est *qui* doit corriger, pas *où* ça a cassé :
#
#   1 usage      — l'appelant a mal appelé ; il n'y a rien à installer, rien à démarrer.
#   2 prérequis  — il manque quelque chose sur l'hôte (tart, image, build, expect). C'est
#                  la seule famille où `doctor` dit quoi taper, et où l'agent rend la main.
#   3 VM         — tart n'a pas fait ce qu'on lui demandait : clonage, adresse, ssh, arrêt.
#                  La VM, pas ce qu'elle contient.
#   4 étape      — une étape a échoué **dans** la VM. C'est là qu'un défaut d'Ash se voit.
#
# Aucun chemin de ce script ne fabrique de chemin temporaire : tout ce qu'il écrit sur
# l'hôte est sous `$STATE_DIR`, construit une fois à partir de `$ROOT`, et tout ce qu'il
# supprime est nommé en toutes lettres. C'est ce qui évite qu'une chaîne vide, sous `set -u`,
# fasse viser la racine.
readonly EXIT_USAGE=1
readonly EXIT_PREREQ=2
readonly EXIT_VM=3
readonly EXIT_STEP=4

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly ROOT

# Tout ce que ce script écrit sur l'hôte tient dans un seul dossier gitignoré.
readonly STATE_DIR="$ROOT/.qa-vm"
readonly SHOTS_DIR="$STATE_DIR/shots"
readonly KEY="$STATE_DIR/id_ed25519"
readonly BOOT_LOG="$STATE_DIR/boot.log"

readonly VM_NAME="${ASH_VM_NAME:-ash-qa}"
readonly VM_IMAGE="${ASH_VM_IMAGE:-ghcr.io/cirruslabs/macos-sequoia-base:latest}"
readonly VM_USER="${ASH_VM_USER:-admin}"
readonly VM_PASSWORD="${ASH_VM_PASSWORD:-admin}"

# Ce que l'hôte construit, et l'unique build que la QA a le droit d'installer : `Ash-dev`,
# identifiant `com.mg-studio.ash.dev`, icône aux couleurs inversées. Jamais `Ash` — c'est
# l'application que l'utilisateur a dans son Dock (voir CLAUDE.md).
readonly APP_SOURCE="$ROOT/src-tauri/target/debug/bundle/macos/Ash-dev.app"
readonly APP_TARGET="/Applications/Ash-dev.app"

# Dans la VM. `$HOME` y est celui de `$VM_USER`, résolu côté guest.
readonly GUEST_FIXTURE="fixture"
readonly GUEST_QA_DIR=".ash-qa"

# Le binaire que les hooks appellent (ADR-0007), tel qu'il vit dans le bundle installé.
# Un seul endroit le nomme : c'est ce que `run` et `state` envoient tous les deux.
readonly GUEST_EVENT="$APP_TARGET/Contents/MacOS/ash-event"

# Le `~` de la VM, écrit en toutes lettres — et **seulement** pour l'AppleScript.
#
# Partout ailleurs le guest résout `$HOME` lui-même, ce qui est plus juste. Mais un
# AppleScript est du texte composé sur l'hôte et joué dans la session graphique : il n'a
# aucun shell pour développer une variable. D'où cette seconde écriture du même fait, à un
# seul endroit plutôt qu'à chaque frappe.
readonly GUEST_HOME="/Users/$VM_USER"

# Les messages vont sur la **sortie d'erreur**, sans exception : plusieurs sous-commandes
# rendent une valeur sur la sortie standard — une adresse IP, un chemin de capture — et
# elles sont lues par `$(…)`. Un mot d'avancement mêlé à une adresse ferait viser un hôte
# qui n'existe pas.
say() { printf '\033[1mqa-vm:\033[0m %s\n' "$1" >&2; }
warn() { printf '\033[33mqa-vm: %s\033[0m\n' "$1" >&2; }
fail() {
    printf '\033[31mqa-vm: %s\033[0m\n' "$1" >&2
    exit "${2:-$EXIT_STEP}"
}

# --- ce qui a le droit de traverser vers la VM ---------------------------------------------
#
# Tout ce que ce script compose finit dans un `bash -s` distant, dans un `sed`, ou dans un
# AppleScript joué par `System Events`. Une valeur qu'on n'a pas jugée y devient donc du
# **code** : `state 1 "x; rm -rf ~"` exécuterait sa moitié droite dans la VM.
#
# La conduite est celle des trois frontières de sécurité du dépôt (`git_cli.rs`,
# `token.rs`/`api.rs`, `links/target.rs`) : **une fonction décide, tous les autres
# demandent.** Ajouter une sous-commande qui prend un argument, c'est ajouter un appel à
# `checked` — pas une nouvelle expression régulière au site d'appel.

# Un nom de fichier ou de VM : ni chemin, ni espace, ni point de départ (`..`).
readonly SAFE_NAME='^[A-Za-z0-9][A-Za-z0-9._-]*$'
# Un rang d'onglet, à partir de 1 — il part dans une adresse `sed`.
readonly SAFE_INDEX='^[1-9][0-9]*$'
# Les verbes qu'`ash-event` transporte.
#
# Cette liste est **détenue par le Rust** (`features/agents/adapters/claude_code.rs`), et un
# script shell ne peut pas l'importer : on la redit donc en nommant sa source, jamais comme
# si elle venait de nulle part. `interpret` ne connaît que les quatre états ; `session-start`
# et `subagent-stop` sont des verbes de session et d'enfant, pas des états — c'est pour ça
# qu'`idle` n'est pas déclarable.
readonly SAFE_VERB='^(session-start|subagent-stop|working|waiting|done|error)$'

# `checked` **échoue le script**, elle ne rend rien : appelée dans un `$(…)`, son `exit` ne
# tuerait que le sous-shell et l'appelant continuerait avec une valeur vide.
checked() {
    local what="$1" pattern="$2" value="${3:-}"
    [ -n "$value" ] && printf '%s' "$value" | grep -Eq "$pattern" ||
        fail "$what invalide : « ${3:-} » — attendu $pattern" "$EXIT_USAGE"
}

# Les quatre variables d'environnement traversent elles aussi : `ASH_VM_NAME` part en
# argument de `tart` et en motif de `grep`, `ASH_VM_USER` part dans une chaîne AppleScript.
# On les juge une fois, avant toute sous-commande, plutôt qu'à chaque usage.
check_configuration() {
    checked "ASH_VM_NAME" "$SAFE_NAME" "$VM_NAME"
    checked "ASH_VM_USER" "$SAFE_NAME" "$VM_USER"
}

# --- prérequis de l'hôte ------------------------------------------------------------------

has_tart() { command -v tart >/dev/null 2>&1; }

# L'image de base pèse des dizaines de Go. On ne la tire jamais implicitement : on constate
# son absence, et on donne la commande.
image_pulled() { tart list --source oci --quiet 2>/dev/null | grep -qxF "$VM_IMAGE"; }

require_tart() {
    has_tart || fail \
        "tart est introuvable. Installe-le : brew install cirruslabs/cli/tart" "$EXIT_PREREQ"
}

require_image() {
    image_pulled || fail \
        "l'image $VM_IMAGE n'est pas tirée localement (des dizaines de Go).
       Tire-la explicitement, en connaissant le coût : tart pull $VM_IMAGE" "$EXIT_PREREQ"
}

require_app() {
    [ -d "$APP_SOURCE" ] || fail \
        "Ash-dev.app n'a pas été construit : $APP_SOURCE est absent.
       Construis-le sur l'hôte — bun run package:debug — jamais dans la VM." "$EXIT_PREREQ"
}

vm_exists() { tart list --quiet 2>/dev/null | grep -qxF "$VM_NAME"; }

# `tart list` rend une colonne d'état. On ne se fie pas à la présence d'un processus `tart`
# sur l'hôte : plusieurs VM peuvent tourner, et le plafond d'Apple est de **2 VM macOS**
# par hôte.
vm_running() {
    # `tart get` prend le nom en **argument** : il n'y a donc aucun nom à apparier dans une
    # sortie, et rien qui puisse être lu comme une expression régulière. C'est ce qui manquait
    # à la version qui cherchait `"Name":"…"` dans `tart list --format json` — tart indente
    # son JSON (`"Name" : "ash-qa"`), donc le motif compact ne matchait jamais et `up` se
    # croyait toujours devant une VM arrêtée.
    #
    # L'espace autour du deux-points est tolérée des deux côtés : ce qu'on lit est un format
    # d'affichage, pas un contrat.
    tart get "$VM_NAME" --format json 2>/dev/null |
        grep -qE '"State"[[:space:]]*:[[:space:]]*"running"'
}

vm_ip() { tart ip "$VM_NAME" --wait "${1:-60}" 2>/dev/null; }

# --- ssh sans la moindre saisie -----------------------------------------------------------
#
# Pilotage par ssh : les images de base de Cirrus ouvrent un compte admin avec ssh actif,
# et c'est la seule couture qui ne demande ni écran, ni clic, ni focus.
#
# La clé est **propre à la QA**, posée dans `.qa-vm/` : la clé quotidienne de l'utilisateur
# n'a rien à faire dans une VM jetable. Sa première installation passe par `expect`, qui
# ship avec macOS — pas de `sshpass` à installer.

ssh_options=(
    -i "$KEY"
    # Sans ces deux-là, ssh propose d'abord **toutes** les clés de l'agent de l'utilisateur,
    # et le `sshd` de la VM atteint son `MaxAuthTries` avant d'avoir essayé la nôtre :
    # « Too many authentication failures », sur une clé pourtant installée. Le `-i` ne
    # restreint rien à lui seul — il ajoute, il ne remplace pas.
    -o IdentitiesOnly=yes
    -o IdentityAgent=none
    -o BatchMode=yes
    -o StrictHostKeyChecking=no
    -o UserKnownHostsFile=/dev/null
    -o LogLevel=ERROR
    -o ConnectTimeout=10
)

# Les mêmes options, mais pour `rsync -e`, qui prend une **chaîne** et la redécoupe.
#
# `"${ssh_options[*]}"` les collerait à l'espace : le chemin de la clé contient celui du
# dépôt, et un dépôt rangé sous « Mon projet » ferait alors viser deux fichiers qui
# n'existent pas. La chaîne est donc dérivée du tableau — jamais réécrite à côté.
ssh_transport() {
    local transport="ssh" option
    for option in "${ssh_options[@]}"; do
        transport="$transport $(printf '%q' "$option")"
    done
    printf '%s' "$transport"
}

ensure_key() {
    [ -d "$STATE_DIR" ] || mkdir -p "$STATE_DIR"
    [ -f "$KEY" ] && return 0
    say "génération de la clé de QA (jamais celle de l'utilisateur)"
    ssh-keygen -t ed25519 -N "" -C "ash-qa" -f "$KEY" >/dev/null
}

ensure_ssh() {
    ensure_key
    local ip
    ip="$(vm_ip 60)" || true
    [ -n "$ip" ] || fail "la VM n'a pas rendu d'adresse — est-elle démarrée ? (up)" "$EXIT_VM"

    if ssh "${ssh_options[@]}" "$VM_USER@$ip" true 2>/dev/null; then
        printf '%s\n' "$ip"
        return 0
    fi

    say "installation de la clé publique dans la VM"
    expect -f - "$ip" "$VM_USER" "$VM_PASSWORD" "$KEY.pub" <<'EXPECT' >/dev/null || true
        set timeout 60
        set ip       [lindex $argv 0]
        set user     [lindex $argv 1]
        set password [lindex $argv 2]
        set pubkey   [lindex $argv 3]
        spawn ssh-copy-id -i $pubkey -o StrictHostKeyChecking=no \
            -o UserKnownHostsFile=/dev/null \
            -o IdentitiesOnly=yes -o IdentityAgent=none $user@$ip
        expect {
            "*assword:" { send "$password\r"; exp_continue }
            "*(yes/no*"  { send "yes\r";      exp_continue }
            eof {}
        }
EXPECT

    ssh "${ssh_options[@]}" "$VM_USER@$ip" true 2>/dev/null || fail \
        "ssh refuse encore la clé. Vérifie ASH_VM_USER / ASH_VM_PASSWORD, ou prépare
       l'image à la main : scripts/qa/vm.sh console" "$EXIT_VM"
    printf '%s\n' "$ip"
}

# Une commande dans la VM. Le shell distant est un `bash -s` nourri par l'entrée standard :
# rien à échapper, donc rien à casser.
guest() {
    local ip
    ip="$(cat "$STATE_DIR/ip" 2>/dev/null || true)"
    [ -n "$ip" ] || ip="$(ensure_ssh)"
    ssh "${ssh_options[@]}" "$VM_USER@$ip" "bash -s"
}

remember_ip() {
    local ip
    ip="$(ensure_ssh)"
    printf '%s\n' "$ip" >"$STATE_DIR/ip"
    printf '%s\n' "$ip"
}

# AppleScript joué dans la session graphique de la VM.
#
# **Ça demande l'autorisation d'accessibilité pour sshd, une fois, dans l'image** : sans
# elle, `System Events` refuse la frappe (erreur -1719). Voir `console` et la doc.
guest_osascript() { guest <<GUEST
set -euo pipefail
osascript <<'APPLESCRIPT'
$1
APPLESCRIPT
GUEST
}

# --- sous-commandes -----------------------------------------------------------------------

cmd_doctor() {
    local verdict=0
    if has_tart; then
        say "tart : $(tart --version 2>/dev/null || echo présent)"
    else
        warn "tart : absent (brew install cirruslabs/cli/tart)"
        verdict=$EXIT_PREREQ
    fi
    if has_tart && image_pulled; then
        say "image : $VM_IMAGE tirée"
    else
        warn "image : $VM_IMAGE absente (tart pull $VM_IMAGE — des dizaines de Go)"
        verdict=$EXIT_PREREQ
    fi
    if has_tart && vm_exists; then
        say "VM : $VM_NAME existe ($(vm_running && echo running || echo stopped))"
    else
        warn "VM : $VM_NAME n'existe pas encore (up la clonera depuis l'image)"
    fi
    if [ -d "$APP_SOURCE" ]; then
        say "build : $APP_SOURCE"
    else
        warn "build : Ash-dev.app absent (bun run package:debug)"
        verdict=$EXIT_PREREQ
    fi
    command -v expect >/dev/null || { warn "expect : absent — la clé ssh ne pourra pas être posée"; verdict=$EXIT_PREREQ; }
    return $verdict
}

cmd_up() {
    require_tart
    mkdir -p "$SHOTS_DIR"

    if ! vm_exists; then
        require_image
        say "clonage de $VM_IMAGE vers $VM_NAME"
        tart clone "$VM_IMAGE" "$VM_NAME" || fail "le clonage a échoué" "$EXIT_VM"
    fi

    local runner=""
    if vm_running; then
        say "$VM_NAME tourne déjà"
    else
        # `--no-graphics` : la VM a bien un écran virtuel — c'est l'hôte qui n'en montre
        # pas la fenêtre. C'est tout l'objet de la tâche.
        say "démarrage de $VM_NAME (sans fenêtre sur l'hôte)"
        nohup tart run "$VM_NAME" --no-graphics >"$BOOT_LOG" 2>&1 &
        runner=$!
        disown
        # Le refus le plus probable est le plafond d'Apple — **2 VM macOS par hôte** —, et
        # il tombe en une seconde. Sans ce regard, `tart ip --wait` ferait attendre deux
        # minutes une VM déjà morte, puis dirait « pas d'adresse » au lieu de la vraie
        # raison, qui est dans le journal.
        sleep 2
        kill -0 "$runner" 2>/dev/null || fail \
            "tart run s'est arrêté aussitôt (plafond de 2 VM macOS par hôte ?) :
       $(tail -n 3 "$BOOT_LOG" 2>/dev/null || echo "$BOOT_LOG est vide")" "$EXIT_VM"
    fi

    local ip
    ip="$(vm_ip 120)" || true
    [ -n "$ip" ] || fail "la VM n'a pas rendu d'adresse en 120 s — voir $BOOT_LOG" "$EXIT_VM"
    remember_ip >/dev/null
    say "adresse : $ip"
    printf '%s\n' "$ip"
}

cmd_down() {
    require_tart
    if ! vm_exists || ! vm_running; then
        say "$VM_NAME est déjà arrêtée"
        rm -f "$STATE_DIR/ip"
        return 0
    fi
    say "arrêt de $VM_NAME"
    tart stop "$VM_NAME" --timeout 60 || fail "l'arrêt a échoué" "$EXIT_VM"
    rm -f "$STATE_DIR/ip"
}

# L'`.app` voyage par `scp`, pas par dossier partagé (`tart run --dir`).
#
# Trois raisons, et la troisième décide : un dossier virtiofs est monté sous
# `/Volumes/My Shared Files` et lu en écriture par l'hôte pendant que le guest exécute —
# deux cycles de vie pour un même bundle ; `/Applications` est ce que LaunchServices
# indexe, et c'est là qu'on veut voir vivre l'application ; et surtout la VM reste
# **autonome** une fois installée, donc reproductible d'un cycle à l'autre.
#
# La quarantaine : ni `scp` ni un dossier partagé ne posent `com.apple.quarantine` — c'est
# LaunchServices qui la pose sur ce qu'un navigateur télécharge. On le **vérifie** au lieu
# de le supposer, et on ne retire l'attribut que s'il est là.
cmd_install() {
    require_tart
    require_app
    local ip
    ip="$(remember_ip)"
    say "copie de Ash-dev.app vers $ip:$APP_TARGET"

    # `rsync` plutôt que `scp -r` : il préserve les liens symboliques du bundle (Frameworks,
    # Resources), qu'un `scp -r` déréférence — un bundle recopié en dur ne se lance pas.
    rsync -a --delete -e "$(ssh_transport)" \
        "$APP_SOURCE/" "$VM_USER@$ip:$APP_TARGET/" || fail "la copie a échoué" "$EXIT_STEP"

    guest <<GUEST || fail "la vérification post-copie a échoué dans la VM" "$EXIT_STEP"
set -euo pipefail
if xattr -p com.apple.quarantine "$APP_TARGET" >/dev/null 2>&1; then
    echo "quarantaine posée — on la retire"
    xattr -dr com.apple.quarantine "$APP_TARGET"
else
    echo "quarantaine absente, comme attendu d'une copie par ssh"
fi
test -x "$APP_TARGET/Contents/MacOS/ash-event" \
    || { echo "ash-event manque dans le bundle" >&2; exit 1; }
GUEST

    install_shell_hook
    say "installé"
}

# Le crochet de shell : chaque onglet qu'Ash ouvre écrit son `ASH_TAB_ID`.
#
# C'est ce qui permet de piloter les états **par ssh**, sans frappe : `ash-event` a besoin
# de l'identifiant d'onglet, et rien d'autre ne l'expose hors du PTY. Bloc délimité et
# idempotent, comme Ash écrit dans les fichiers de l'utilisateur (ADR-0007) — ici c'est un
# `~` de VM jetable, mais la forme se tient.
install_shell_hook() {
    say "pose du crochet de shell (~/.zshrc de la VM)"
    guest <<GUEST || fail "le crochet de shell n'a pas pu être posé" "$EXIT_STEP"
set -euo pipefail
mkdir -p "\$HOME/$GUEST_QA_DIR"
touch "\$HOME/.zshrc"
/usr/bin/sed -i '' '/# >>> ash-qa >>>/,/# <<< ash-qa <<</d' "\$HOME/.zshrc"
cat >>"\$HOME/.zshrc" <<'HOOK'
# >>> ash-qa >>>
# Chaque onglet d'Ash annonce son identifiant : la QA pilote alors ash-event par ssh,
# sans frappe et sans deviner. Rien d'autre n'expose ASH_TAB_ID hors du PTY.
if [ -n "\${ASH_TAB_ID:-}" ]; then
  mkdir -p "\$HOME/$GUEST_QA_DIR"
  printf '%s\n' "\$ASH_TAB_ID" >>"\$HOME/$GUEST_QA_DIR/tabs"
fi
# <<< ash-qa <<<
HOOK
GUEST
}

# Le dépôt de fixture est **construit dans la VM**, pas copié depuis l'hôte.
#
# Un worktree git porte un `.git` qui nomme son dépôt par **chemin absolu**
# (`gitdir: /…/.git/worktrees/<nom>`), et le dépôt lui répond par un `gitdir` tout aussi
# absolu. Copier l'arbre depuis l'hôte livrerait donc des pointeurs qui désignent des
# chemins de l'hôte : la résolution worktree → dépôt d'Ash (ADR-0011) verrait des dépôts
# cassés, et la sidebar montrerait n'importe quoi. Le construire sur place les rend justes.
cmd_fixture() {
    require_tart
    say "dépôt de fixture : un dépôt, deux worktrees"
    guest <<GUEST || fail "la fixture n'a pas pu être construite (git est-il présent ?)" "$EXIT_STEP"
set -euo pipefail
command -v git >/dev/null || {
    echo "git est absent de la VM — choisis une image qui l'a, ou xcode-select --install" >&2
    exit 1
}
root="\$HOME/$GUEST_FIXTURE"
rm -rf "\$root"
mkdir -p "\$root"
cd "\$root"
git init -q -b main hello
cd hello
git config user.email qa@example.invalid
git config user.name "Ash QA"
printf 'hello\n' >README.md
git add README.md
git commit -qm "chore: seed the fixture repository"
git branch feature-a
git branch feature-b
git worktree add -q ../hello-feature-a feature-a
git worktree add -q ../hello-feature-b feature-b
git worktree list
GUEST
}

# `run` : Ash-dev se lance dans la VM, cinq onglets s'ouvrent, et les cinq états sortent
# **sans qu'aucun agent d'IA ne soit installé**.
#
# Ce n'est pas un contournement : ADR-0007 pose qu'un état vient d'un hook et jamais d'une
# analyse de la sortie du PTY. `ash-event` est donc le chemin nominal, et pas une doublure.
cmd_run() {
    require_tart
    # Ce que l'hôte veut faire répéter aux doublures d'usage (#190). Vide = rien à poser :
    # Ash lit alors le vrai trousseau et le vrai hôte, comme en usage normal.
    local RUN_USAGE="${ASH_DEV_USAGE:-}"
    local ip
    ip="$(remember_ip)"

    say "lancement d'Ash-dev dans la VM ($ip)"
    guest <<GUEST || fail "Ash-dev n'a pas démarré" "$EXIT_STEP"
set -euo pipefail
test -d "$APP_TARGET" || { echo "$APP_TARGET est absent — lance install" >&2; exit 1; }
rm -f "\$HOME/$GUEST_QA_DIR/tabs"
# L'exécutable du bundle s'appelle **ash**, pas Ash-dev : un pgrep sur le nom de
# l'application ne matche jamais, et chaque run ouvrait une instance de plus. On
# reconnaît la nôtre par son chemin complet, qui est le seul nom qui ne mente pas.
#
# Et on lance le binaire directement plutôt que par open : c'est le seul chemin qui
# puisse porter ASH_DEV_USAGE. launchctl setenv depuis une session ssh ne remonte pas
# jusqu'au domaine graphique, donc une variable posée là n'atteindrait jamais l'app.
mkdir -p "\$HOME/$GUEST_QA_DIR"
if ! pgrep -f "$APP_TARGET/Contents/MacOS/ash" >/dev/null; then
    # ASH_DEV_USAGE n'est posée QUE si l'hôte en a donné une non vide : une variable
    # présente mais vide est un refus explicite côté Ash (elle ne demande rien), et
    # l'application s'arrête au démarrage. Voir features/usage/rehearsal.rs.
    if [ -n "$RUN_USAGE" ]; then
        ASH_DEV_USAGE="$RUN_USAGE" nohup "$APP_TARGET/Contents/MacOS/ash" \
            >"\$HOME/$GUEST_QA_DIR/ash.log" 2>&1 &
    else
        nohup "$APP_TARGET/Contents/MacOS/ash" \
            >"\$HOME/$GUEST_QA_DIR/ash.log" 2>&1 &
    fi
    sleep 6
fi
GUEST

    # Le dialogue d'autorisation de notifications, première ouverture d'une application
    # empaquetée sur un `~` neuf. Il est **modal** : tant qu'il est là, plus aucune frappe
    # n'arrive à Ash. On l'accorde, une fois — la réponse est retenue par identifiant de
    # paquet, donc les cycles suivants ne le reverront pas.
    say "accord du dialogue de notifications, s'il est là"
    guest_osascript '
        tell application "System Events"
            repeat 10 times
                if exists (process "UserNotificationCenter") then
                    try
                        click button "Allow" of window 1 of process "UserNotificationCenter"
                    end try
                end if
                delay 0.5
            end repeat
        end tell' || warn "le dialogue n'a pas pu être cliqué — accessibilité accordée à sshd ?"

    play_scenario
    post_states
    # Le temps que la webview rende ce que le backend vient d'annoncer. Sans cette pause,
    # une capture prise juste après `run` attrape l'écran d'avant : observé, sur une
    # application lancée quelques secondes plus tôt.
    sleep 4
    say "les cinq états sont posés — capture maintenant : done et error s'effacent 30 s
       après avoir été vus (LINGER, agents/machine.rs)"
}

# Quatre onglets de plus, chacun dans un worktree : la sidebar montre alors ses trois
# niveaux — le dépôt, ses worktrees, leurs onglets.
play_scenario() {
    say "ouverture des onglets"
    local home="$GUEST_HOME"
    # Ash ouvre déjà un onglet au démarrage : on l'emmène dans le dépôt, puis on en ouvre
    # quatre autres. Cinq onglets, cinq états — et deux worktrees pour que la sidebar
    # montre ses trois niveaux.
    guest_osascript "
        tell application \"System Events\"
            tell process \"Ash-dev\" to set frontmost to true
            delay 1
            keystroke \"cd $home/$GUEST_FIXTURE/hello\"
            key code 36
            delay 1
            repeat with p in {\"$home/$GUEST_FIXTURE/hello-feature-a\", \"$home/$GUEST_FIXTURE/hello-feature-b\", \"$home/$GUEST_FIXTURE/hello-feature-a\", \"$home/$GUEST_FIXTURE/hello-feature-b\"}
                keystroke \"t\" using command down
                delay 1.5
                keystroke \"cd \" & (contents of p)
                key code 36
                delay 1
            end repeat
        end tell" || fail "les frappes n'ont pas abouti — accessibilité accordée à sshd ?" "$EXIT_STEP"
}

# Les cinq états, dans l'ordre, sur cinq onglets.
#
# Les verbes sont ceux de `features/agents/adapters/claude_code.rs` — voir `$SAFE_VERB`,
# qui redit la même liste au seul endroit qui la juge.
#
#   idle    : `session-start` — un verbe de session, pas un état. `idle` n'est jamais
#             déclarable (l'adaptateur le refuse) : il naît de l'ouverture d'une session,
#             ou de la sonde qui ne voit aucun agent.
#   working : déclarable. La sonde le produit aussi, quand un vrai processus tourne.
#   waiting : déclarable, et **jamais** d'une autre source qu'un hook (ADR-0007).
#   done    : déclarable.
#   error   : déclarable.
#
# `done` et `error` s'effacent d'eux-mêmes **30 s après avoir été vus** (LINGER, dans
# `agents/machine.rs`), et la fenêtre de la VM est au premier plan : la capture doit donc
# suivre tout de suite.
post_states() {
    say "envoi des cinq états par ash-event, sans aucun agent installé"
    guest <<GUEST || fail "les états n'ont pas pu être envoyés" "$EXIT_STEP"
set -euo pipefail
tabs="\$HOME/$GUEST_QA_DIR/tabs"
for _ in \$(seq 1 30); do
    [ -f "\$tabs" ] && [ "\$(wc -l <"\$tabs")" -ge 5 ] && break
    sleep 1
done
[ -f "\$tabs" ] || { echo "aucun onglet ne s'est annoncé — le crochet de shell est-il posé ?" >&2; exit 1; }
count="\$(wc -l <"\$tabs")"
[ "\$count" -ge 5 ] || { echo "seulement \$count onglet(s) annoncé(s) sur 5" >&2; exit 1; }

event="$GUEST_EVENT"
index=0
verbs=(session-start working waiting done error)
while read -r tab; do
    [ "\$index" -ge 5 ] && break
    # La redirection depuis /dev/null n'est pas une précaution de style : ash-event lit
    # son ENTREE STANDARD pour en tirer agent_id / agent_type (ADR-0007, amendement du
    # 2026-08-13). Sans elle il hérite de l'entrée de la boucle -- le fichier des onglets
    # -- et en avale les lignes restantes : un seul verbe part, la boucle s'arrête, et
    # rien ne le dit.
    "\$event" "\${verbs[\$index]}" --tab "\$tab" </dev/null
    index=\$((index + 1))
done <"\$tabs"
[ "\$index" -eq 5 ] || { echo "seulement \$index verbe(s) envoyé(s) sur 5" >&2; exit 1; }
echo "cinq verbes envoyés"
GUEST
}

# Une capture de l'écran **virtuel** de la VM, rapatriée sur l'hôte.
#
# `screencapture` exige l'autorisation d'enregistrement d'écran pour sshd : sans elle, il
# rend un PNG où ne figure que le fond d'écran. C'est le premier risque de la tâche, et il
# se lève en regardant l'image, pas en la supposant.
cmd_shot() {
    local name="${1:-}"
    [ -n "$name" ] || fail "usage : scripts/qa/vm.sh shot <nom>" "$EXIT_USAGE"
    checked "nom de capture" "$SAFE_NAME" "$name"

    require_tart
    mkdir -p "$SHOTS_DIR"
    local ip target
    ip="$(remember_ip)"
    target="$SHOTS_DIR/$name.png"

    guest <<'GUEST' || fail "screencapture a échoué dans la VM" "$EXIT_STEP"
set -euo pipefail
/usr/sbin/screencapture -x -t png /tmp/ash-qa-shot.png
test -s /tmp/ash-qa-shot.png
GUEST

    scp "${ssh_options[@]}" "$VM_USER@$ip:/tmp/ash-qa-shot.png" "$target" >/dev/null ||
        fail "la capture n'a pas pu être rapatriée" "$EXIT_STEP"
    say "capture : $target"
    printf '%s\n' "$target"
}

# Un état à la main, sur l'onglet de rang <n> (1 = le premier annoncé).
#
# Les deux arguments partent dans la VM — l'un dans une adresse `sed`, l'autre en argument
# d'`ash-event` — donc ils passent par `checked` avant tout, comme le nom d'une capture.
cmd_state() {
    local index="${1:-}" verb="${2:-}"
    [ -n "$index" ] && [ -n "$verb" ] || fail "usage : scripts/qa/vm.sh state <n> <verbe>" "$EXIT_USAGE"
    checked "rang d'onglet" "$SAFE_INDEX" "$index"
    checked "verbe" "$SAFE_VERB" "$verb"
    require_tart
    remember_ip >/dev/null
    guest <<GUEST || fail "l'état n'a pas pu être envoyé" "$EXIT_STEP"
set -euo pipefail
tab="\$(sed -n '${index}p' "\$HOME/$GUEST_QA_DIR/tabs")"
[ -n "\$tab" ] || { echo "aucun onglet de rang $index" >&2; exit 1; }
"$GUEST_EVENT" "$verb" --tab "\$tab"
GUEST
}

cmd_ssh() {
    require_tart
    local ip
    ip="$(remember_ip)"
    if [ "$#" -eq 0 ]; then
        say "ssh ${ssh_options[*]} $VM_USER@$ip"
        exec ssh "${ssh_options[@]}" "$VM_USER@$ip"
    fi
    exec ssh "${ssh_options[@]}" "$VM_USER@$ip" "$@"
}

# La seule sous-commande qui ouvre une fenêtre sur le bureau de l'hôte.
#
# Elle sert à **préparer l'image**, une fois : accorder l'accessibilité et l'enregistrement
# d'écran à sshd, ce qui ne se fait pas par ssh (une autorisation TCC se donne devant un
# écran). Elle n'appartient à aucun cycle de QA, et un cycle `up → install → run → shot →
# down` ne l'appelle jamais.
cmd_console() {
    require_tart
    vm_exists || fail "la VM $VM_NAME n'existe pas encore — lance up" "$EXIT_VM"
    # Le refus vient **avant** le mode d'emploi : imprimer trois étapes puis dire non
    # laisserait croire qu'il y a quelque chose à faire.
    if vm_running; then
        fail "la VM tourne déjà sans écran — arrête-la d'abord (down)" "$EXIT_VM"
    fi
    warn "cette commande ouvre une fenêtre sur ton bureau — c'est la seule qui le fasse."
    cat <<'STEPS'
À faire une fois, dans la VM, pour que la QA se pilote ensuite sans écran :
  1. Réglages système → Confidentialité et sécurité → Accessibilité
     → ajouter /usr/libexec/sshd-keygen-wrapper (⌘⇧G pour saisir le chemin)
  2. Réglages système → Confidentialité et sécurité → Enregistrement de l'écran
     → ajouter le même
  3. Vérifier que la session graphique de l'utilisateur est bien ouverte (connexion auto)
Puis ferme cette fenêtre : les cycles suivants n'en auront plus besoin.
STEPS
    tart run "$VM_NAME"
}

usage() {
    cat <<'USAGE'
usage : scripts/qa/vm.sh <commande>

  doctor            ce qui manque sur l'hôte, sans rien installer
  up                démarre la VM sans écran, rend son adresse (idempotent)
  install           copie l'Ash-dev.app construit sur l'hôte, pose le crochet de shell
  fixture           construit un dépôt git avec deux worktrees dans la VM
  run               lance Ash-dev, ouvre cinq onglets, produit les cinq états
  shot <nom>        un PNG de l'écran de la VM dans .qa-vm/shots/
  state <n> <verbe> un état à la main sur l'onglet de rang n
  ssh [commande]    un shell (ou une commande) dans la VM
  down              arrête la VM (idempotent)
  console           ouvre la VM AVEC écran — préparation d'image seulement

Verbes acceptés par `state` : session-start, subagent-stop, working, waiting, done, error
  (`idle` n'en est pas un : il vient de session-start ou de la sonde)

Codes de retour : 1 usage · 2 prérequis manquant sur l'hôte · 3 la VM (tart) n'a pas suivi
  · 4 une étape a échoué dans la VM

Variables : ASH_VM_NAME, ASH_VM_IMAGE, ASH_VM_USER, ASH_VM_PASSWORD
Documentation : .claude/docs/qa-vm.md
USAGE
}

main() {
    local command="${1:-}"
    [ "$#" -gt 0 ] && shift || true
    check_configuration
    case "$command" in
    doctor) cmd_doctor "$@" ;;
    up) cmd_up "$@" ;;
    install) cmd_install "$@" ;;
    fixture) cmd_fixture "$@" ;;
    run) cmd_run "$@" ;;
    shot) cmd_shot "$@" ;;
    state) cmd_state "$@" ;;
    ssh) cmd_ssh "$@" ;;
    down) cmd_down "$@" ;;
    console) cmd_console "$@" ;;
    -h | --help | help | "") usage ;;
    *)
        printf '\033[31mqa-vm: commande inconnue : %s\033[0m\n' "$command" >&2
        usage >&2
        exit "$EXIT_USAGE"
        ;;
    esac
}

main "$@"
