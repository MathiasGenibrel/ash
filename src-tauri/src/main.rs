// Pas de console derrière la fenêtre en release. Sans cet attribut, Windows en
// ouvrirait une ; macOS l'ignore, et le garder évite d'avoir à y repenser si la
// question du portage se repose.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Le composition root est le seul endroit où un échec n'a personne à qui remonter :
    // si l'application ne démarre pas, il n'y a pas d'application. On l'affiche et on
    // sort avec un code non nul plutôt que de paniquer avec une trace illisible.
    if let Err(error) = ash_lib::run() {
        eprintln!("ash: démarrage impossible : {error}");
        std::process::exit(1);
    }
}
