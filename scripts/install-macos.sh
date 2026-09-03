#!/usr/bin/env bash
#
# Installe la dernière version d'Ash sur un Mac Apple Silicon.
#
#   curl -fsSL https://raw.githubusercontent.com/MathiasGenibrel/ash/main/scripts/install-macos.sh | bash
#
# Ce script est servi tel quel par raw.githubusercontent.com depuis `main` : il n'est ni
# publié, ni versionné dans une URL, et la ligne ci-dessus ne bouge donc jamais d'un jalon à
# l'autre. Il ne sert qu'à la **première installation** — les mises à jour seront le travail
# de `tauri-plugin-updater`, qui fait sa propre pose.
#
# Il ne demande jamais rien : aucun `sudo`, aucune question, aucune saisie. Ce qui n'est pas
# inscriptible sans privilèges est refusé plutôt que réclamé.
#
# Codes de retour — la même table que dans le README :
#
#   0  installé
#   1  échec inattendu
#   2  usage (option inconnue, destination inexistante ou non inscriptible)
#   4  serveur injoignable
#   5  aucune release, ou pas d'archive pour cette architecture
#   6  système non supporté
#
# Le `3` de RYNO — jeton absent ou invalide — n'existe pas ici : le dépôt est public et
# aucune requête n'est authentifiée, donc ce code n'a aucune cause.
#
# ─────────────────────────────────────────────────────────────────────────────────────────
# La duplication assumée
#
# Le nom de l'archive (`Ash-<version>-macos-arm64.zip`) est décidé dans
# `scripts/release/artifact.ts`, et nulle part ailleurs. Ce script, servi seul à une machine
# qui n'a ni le dépôt ni bun, ne peut pas l'importer : il **redit** la règle, avec
# `PRODUCT_NAME` et `ARCH_LABEL` ci-dessous.
#
# C'est la seule duplication de ce fichier, et ce qui la rend tenable est
# `scripts/install-macos.test.ts` : il lance `--artifact-name <version>` et confronte la
# sortie à `artifactName()`. Le jour où l'une des deux formes dérive, ce test casse.
#
# ─────────────────────────────────────────────────────────────────────────────────────────
# Ce que ce script accepte du dehors
#
# Il télécharge sur le réseau et écrit dans `/Applications` : c'est une surface exposée, et
# elle se tient comme les trois autres du dépôt — `git_cli.rs`, `usage/token.rs` et
# `links/target.rs`. **Une fonction décide, toutes les autres demandent.**
#
#   `asset_url` décide ce qui sera téléchargé. Un corps JSON n'est pas une source de
#   confiance : c'est lui qui désigne l'URL que `curl` ira lire, puis le bundle qu'on posera
#   à la place d'une application. Elle n'accepte donc qu'une **égalité** avec l'unique URL
#   attendue, sous les téléchargements de release de ce dépôt — liste blanche, jamais motif.
#
#   `resolve_install_dir` décide où l'on écrit, et c'est le seul endroit qui le décide.
#
# `curl` est bridé sur `https` à l'aller **et sur ses redirections** : sans quoi une réponse
# de l'API renverrait la pose vers `http://` ou `file://`.
# ─────────────────────────────────────────────────────────────────────────────────────────

set -euo pipefail

# Le dépôt, son API, ses téléchargements et le nom du bundle : écrits une seule fois, ici.
readonly REPO="MathiasGenibrel/ash"
readonly LATEST_URL="https://api.github.com/repos/${REPO}/releases/latest"
readonly DOWNLOAD_PREFIX="https://github.com/${REPO}/releases/download/"
readonly PRODUCT_NAME="Ash"
readonly ARCH_LABEL="macos-arm64"
readonly DEFAULT_INSTALL_DIR="/Applications"

# Renseignés par l'installation, relus par le nettoyage de sortie.
STAGING=""
SET_ASIDE=""
TARGET_APP=""

usage() {
    cat <<USAGE
usage :
  install-macos.sh [--dir <chemin>]        installe la dernière version d'Ash
  install-macos.sh --artifact-name <ver>   imprime le nom de l'archive de cette version
  install-macos.sh --help

La destination est \`--dir\`, sinon \$ASH_INSTALL_DIR, sinon ${DEFAULT_INSTALL_DIR}.
USAGE
}

# Tout ce que le script raconte part sur la sortie d'erreur : la sortie standard n'imprime
# qu'un nom d'archive, et reste exploitable par un appelant.
say() {
    printf '%s\n' "$*" >&2
}

die() {
    local code="$1"
    shift
    printf 'ash: %s\n' "$*" >&2
    exit "$code"
}

# `Ash-1.2.0-macos-arm64.zip`. Le `v` d'un tag ne survit jamais dans le nom du fichier.
artifact_name() {
    local asked="$1"
    local version="${asked#v}"
    printf '%s-%s-%s.zip\n' "$PRODUCT_NAME" "$version" "$ARCH_LABEL"
}

# `uname -m` ne suffit pas : un Mac Apple Silicon dont le shell tourne sous Rosetta répond
# `x86_64`, et s'en tenir là ferait chercher une archive Intel — ou, si elle existait un
# jour, l'installerait sur une machine ARM sans que rien ne le signale. `proc_translated`
# vaut 1 dans ce cas exactement.
detect_arch() {
    local machine translated
    machine="$(uname -m)"
    translated="$(sysctl -n sysctl.proc_translated 2>/dev/null || printf '0')"
    if [ "$machine" = "arm64" ] || [ "$translated" = "1" ]; then
        printf 'arm64\n'
    else
        printf '%s\n' "$machine"
    fi
}

require_supported_system() {
    [ "$(uname -s)" = "Darwin" ] || die 6 "Ash n'existe que sur macOS."
    [ -x /usr/bin/ditto ] || die 6 "/usr/bin/ditto est introuvable : ce système n'est pas un macOS complet."

    local arch
    arch="$(detect_arch)"
    [ "$arch" = "arm64" ] || die 5 "aucune archive pour l'architecture ${arch} : Ash n'est publié que pour Apple Silicon."
}

# Le corps JSON de la dernière release. Un 404 dit « aucune release » (5) ; tout le reste
# dit « serveur injoignable » (4) — deux causes qu'un seul code confondrait.
fetch_latest_release() {
    local body status=0
    body="$(curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 10 --max-time 60 "$LATEST_URL" 2>/dev/null)" || status=$?
    if [ "$status" -ne 0 ]; then
        if [ "$status" -eq 22 ]; then
            die 5 "aucune release publiée sur ${REPO}."
        fi
        die 4 "impossible de joindre api.github.com (curl ${status})."
    fi
    printf '%s\n' "$body"
}

json_string_field() {
    local body="$1" field="$2"
    printf '%s\n' "$body" |
        sed -n "s/.*\"${field}\"[[:space:]]*:[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" |
        sed -n '1p'
}

# **C'est la fonction qui décide ce que ce script téléchargera**, donc ce qu'il posera dans
# `/Applications`, et c'est la seule.
#
# Elle ne retient qu'une URL que la release annonce vraiment : si l'asset manque, la liste ne
# le contient pas et l'échec est franc (5) au lieu d'un 404 au téléchargement. Mais annoncée
# ne veut pas dire acceptable — un corps JSON désigne ici le bundle qui remplacera une
# application, donc l'URL doit **en plus** être exactement celle des téléchargements de
# release de ce dépôt. La confrontation est une **égalité de chaînes** (`grep -F -x`) : ni
# motif, ni sous-chaîne, si bien qu'un `evil.example.com/…/Ash-1.2.0-macos-arm64.zip`, un
# `http://` et un tag portant `.` ou `*` sont hors sujet plutôt que subtils.
asset_url() {
    local body="$1" tag="$2" name="$3"
    local expected="${DOWNLOAD_PREFIX}${tag}/${name}"
    printf '%s\n' "$body" |
        sed -n 's/.*"browser_download_url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
        grep -F -x -m 1 -e "$expected" || true
}

# La destination doit exister et être inscriptible **sans privilèges** : sur un Mac où
# `/Applications` ne l'est pas, on le dit plutôt que de réclamer un mot de passe.
resolve_install_dir() {
    local dir="$1"
    [ -d "$dir" ] || die 2 "${dir} n'existe pas."
    [ -w "$dir" ] || die 2 "${dir} n'est pas inscriptible sans privilèges."
    printf '%s\n' "$dir"
}

# Remet l'application écartée à sa place, et ne le dit que si ça a réellement eu lieu.
restore_set_aside() {
    if [ -n "$SET_ASIDE" ] && [ -e "$SET_ASIDE" ] && [ ! -e "$TARGET_APP" ]; then
        if mv -- "$SET_ASIDE" "$TARGET_APP" 2>/dev/null; then
            SET_ASIDE=""
            say "l'application déjà installée a été remise en place."
        else
            say "l'application déjà installée n'a PAS pu être remise en place : ${SET_ASIDE}"
            # Le dossier de bascule contient la seule copie de l'application : le laisser
            # derrière soi vaut mieux que l'effacer avec elle.
            STAGING=""
        fi
    fi
}

cleanup() {
    local status=$?
    restore_set_aside
    if [ -n "$STAGING" ] && [ -d "$STAGING" ]; then
        rm -rf -- "$STAGING" || true
    fi
    exit "$status"
}

# La pose : copie à côté, écartement par renommage, renommage de la nouvelle. Le dossier de
# bascule est **dans la destination**, donc sur le même volume : les deux renommages sont
# instantanés, et la partie longue — téléchargement et décompression — ne touche jamais à
# l'application en place. C'est le cas nominal : Ash s'installe par-dessus lui-même pendant
# qu'il tourne.
install_release() {
    local install_dir="$1" url="$2" name="$3"

    TARGET_APP="${install_dir}/${PRODUCT_NAME}.app"

    # Un `mktemp` qui échoue doit être un échec : sous `set -u` une chaîne vide ferait viser
    # la racine, donc `rm -rf` sur un dossier qu'on n'a pas fabriqué.
    STAGING="$(mktemp -d "${install_dir}/.ash-install.XXXXXX")" || STAGING=""
    [ -n "$STAGING" ] && [ -d "$STAGING" ] ||
        die 1 "impossible de créer un dossier de bascule dans ${install_dir}."
    trap cleanup EXIT

    say "téléchargement de ${name}…"
    curl -fsSL --proto '=https' --proto-redir '=https' --connect-timeout 10 --max-time 600 \
        -o "${STAGING}/archive.zip" "$url" ||
        die 4 "le téléchargement de ${name} a échoué."

    # `ditto` est le dépaqueteur de macOS : il rend un bundle avec ses attributs étendus et
    # ses liens symboliques intacts.
    /usr/bin/ditto -x -k "${STAGING}/archive.zip" "${STAGING}/unpacked" ||
        die 1 "l'archive ${name} n'a pas pu être ouverte."

    local fresh="${STAGING}/unpacked/${PRODUCT_NAME}.app"
    [ -d "$fresh" ] || die 1 "l'archive ${name} ne contient pas ${PRODUCT_NAME}.app."

    if [ -e "$TARGET_APP" ]; then
        SET_ASIDE="${STAGING}/precedent-${PRODUCT_NAME}.app"
        mv -- "$TARGET_APP" "$SET_ASIDE" || die 1 "impossible d'écarter ${TARGET_APP}."
    fi

    mv -- "$fresh" "$TARGET_APP" || die 1 "impossible de poser ${TARGET_APP}."

    say "${PRODUCT_NAME} est installé dans ${install_dir}."
    if [ -n "$SET_ASIDE" ]; then
        say "une version précédente a été remplacée : si elle tourne encore, quitte-la et relance-la."
    fi
}

main() {
    local install_dir="${ASH_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

    while [ $# -gt 0 ]; do
        case "$1" in
        --artifact-name)
            [ $# -ge 2 ] || {
                usage
                exit 2
            }
            artifact_name "$2"
            return 0
            ;;
        --dir)
            [ $# -ge 2 ] || {
                usage
                exit 2
            }
            install_dir="$2"
            shift 2
            ;;
        --help | -h)
            usage
            return 0
            ;;
        *)
            printf 'ash: option inconnue : %s\n' "$1" >&2
            usage
            exit 2
            ;;
        esac
    done

    require_supported_system
    install_dir="$(resolve_install_dir "$install_dir")"

    local body tag name url
    body="$(fetch_latest_release)"
    tag="$(json_string_field "$body" "tag_name")"
    [ -n "$tag" ] || die 5 "la dernière release de ${REPO} ne porte pas de tag exploitable."

    name="$(artifact_name "$tag")"
    url="$(asset_url "$body" "$tag" "$name")"
    [ -n "$url" ] || die 5 "la release ${tag} n'offre pas ${name} au téléchargement."

    install_release "$install_dir" "$url" "$name"
}

# Servi par `curl … | bash`, le script est lu sur l'entrée standard et `BASH_SOURCE` est
# vide : c'est l'usage nominal, pas un `source`. Un `source` explicite, lui, donne un
# `BASH_SOURCE[0]` différent de `$0` — et n'exécute alors rien.
if [ -z "${BASH_SOURCE[0]:-}" ] || [ "${BASH_SOURCE[0]}" = "${0}" ]; then
    main "$@"
fi
