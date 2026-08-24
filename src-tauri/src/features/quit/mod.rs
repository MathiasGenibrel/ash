//! Quitter Ash : la question posée quand un agent est reconnu dans un onglet.
//!
//! Sur un terminal ordinaire, `⌘Q` n'a rien à demander — il n'y a rien à perdre. Ash, lui,
//! supervise des agents, et un `⌘Q` frappé de travers coupe un travail que le prochain
//! lancement ne rendra pas. La feature ne fait donc qu'une chose : décider, **au moment du
//! geste**, s'il faut poser une question avant de partir.
//!
//! # Le critère est l'agent, pas ce qui tourne
//!
//! Un onglet compte quand sa fiche porte un outil reconnu
//! ([`TabInfo::agent`], [ADR-0006](../../../../docs/adr/0006-decouverte-automatique-des-agents.md)),
//! quel que soit son état : un Claude Code `idle` à son invite compte, parce que le quitter
//! perd sa session. Un `vim` ou un `tail -f` ne comptent pas — ce sont des choses qu'on
//! ferme tous les jours en quittant un terminal, et poser la question à chaque fois userait
//! la question jusqu'à ce qu'on la réponde sans la lire.
//!
//! C'est **le backend** qui décide, parce que c'est lui qui détient les onglets
//! ([ADR-0009](../../../../docs/adr/0009-cycle-de-vie-des-agents.md)) : le frontend rend la
//! modale, il ne calcule pas le critère. Et il décide en **relisant** les onglets, jamais en
//! consultant un souvenir — un agent apparu depuis le dernier rendu figure dans la liste.
//!
//! # Les quatre demandes de sortie, et les deux endroits où elles arrivent
//!
//! `⌘Q`, l'entrée `Quitter` du menu applicatif et le menu du Dock sont **la même chose** :
//! macOS envoie `terminate:` à `NSApplication`, et l'entrée prédéfinie de muda ne fait rien
//! d'autre. Ce chemin n'a **aucun** équivalent dans Tauri — `RunEvent::ExitRequested` n'est
//! émis que par [`tauri::AppHandle::exit`] et par la destruction de la dernière fenêtre —,
//! et c'est pour ça que [`macos`] existe : `applicationShouldTerminate:` est le seul endroit
//! où l'on peut encore répondre « pas tout de suite ».
//!
//! La quatrième demande — fermer la dernière fenêtre — n'arrive pas par là, et c'est le
//! composition root qui la branche sur la même question.
//!
//! Les deux chemins passent donc par [`QuitQuestion::may_leave`], qui rend `true` quand Ash
//! peut partir et `false` quand la question vient d'être posée.
//!
//! # Ce qu'Ash ne fait pas
//!
//! Il ne négocie rien : pas de `SIGTERM`, pas d'attente, pas de reprise de session au
//! prochain lancement. Quitter reste quitter — la question existe pour que le geste reste
//! celui de l'utilisateur
//! ([ADR-0015](../../../../docs/adr/0015-ash-compose-l-utilisateur-envoie.md)), pas pour
//! qu'Ash décide à sa place.

pub mod commands;
mod macos;
mod question;

pub use macos::intercept_terminate;
pub use question::{ObservedTabs, QuitGate, QuitQuestion};
