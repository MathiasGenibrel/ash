//! `applicationShouldTerminate:` — le seul endroit où `⌘Q` se laisse encore répondre.
//!
//! **C'est le troisième module `unsafe` du crate**, après la sonde (`features/probe/`,
//! ADR-0005) et les bannières (`features/notifications/macos.rs`), et il est posé comme
//! eux : les appels bruts sont ici, sous une fonction sûre, et rien de ce qui sort d'ici
//! n'est un objet Objective-C.
//!
//! # Pourquoi pas `RunEvent::ExitRequested`
//!
//! C'est la première chose qu'on essaie, et elle ne marche pas — il faut le dire avant que
//! quelqu'un ne la réessaie. Dans Tauri 2.11 / tao 0.35, `RunEvent::ExitRequested` n'a que
//! **deux** émetteurs (`tauri-runtime-wry/src/lib.rs`) : la destruction de la **dernière
//! fenêtre**, et [`tauri::AppHandle::exit`]. `⌘Q`, l'entrée `Quitter` du menu applicatif et
//! le menu du Dock ne passent par ni l'un ni l'autre : l'entrée prédéfinie de muda envoie
//! `terminate:` à `NSApplication` (`muda/src/platform_impl/macos/mod.rs`), et `⌘Q` en est
//! l'équivalent clavier. macOS appelle alors `applicationShouldTerminate:` sur le délégué,
//! puis `applicationWillTerminate:` — que tao implémente, et qui arrive **après** la
//! décision : à ce moment-là, il n'y a plus rien à empêcher.
//!
//! Le délégué de tao (`TaoAppDelegateParent`) n'implémente pas
//! `applicationShouldTerminate:`, donc macOS applique son défaut, qui est « oui ». Ce
//! module ajoute la méthode manquante à la classe du délégué **vivant** — pas à un nom de
//! classe écrit en dur : c'est ce qui le laisse survivre à un renommage chez tao. Si elle
//! existait déjà, `class_addMethod` refuse et [`intercept_terminate`] rend `false` plutôt
//! que d'écraser en silence l'implémentation de quelqu'un d'autre.
//!
//! # Ce que ça couvre, et ce que ça ne couvre pas
//!
//! Couvert : `⌘Q`, `Ash ▸ Quitter`, le menu du Dock, et toute autre route vers
//! `-[NSApplication terminate:]`. Non couvert ici : la fermeture de la **dernière fenêtre**,
//! qui ne termine pas l'application sur macOS et arrive par un autre chemin — c'est le
//! composition root qui la branche sur la même question.
//!
//! **Une extinction ou une déconnexion de session passe aussi par ici**, et Ash y répondra
//! « pas tout de suite » comme n'importe quel éditeur avec un document non enregistré :
//! macOS dira alors qu'Ash a interrompu la déconnexion. Le ticket met la sortie provoquée
//! par le système hors périmètre ; distinguer les deux demanderait de lire l'Apple Event
//! courant, et ce n'est pas fait.
#![allow(unsafe_code)]

use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::OnceLock;

use objc2::ffi::{class_addMethod, object_getClass};
use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{class, msg_send, sel};

/// `NSApplicationTerminateReply`, mot pour mot.
const TERMINATE_CANCEL: usize = 0;
const TERMINATE_NOW: usize = 1;

/// `@encode` de `-(NSUInteger)applicationShouldTerminate:(NSApplication *)sender`.
///
/// `NSUInteger` est un `unsigned long` sur 64 bits, donc `L` ; puis le `self` et le
/// sélecteur que toute méthode reçoit, puis l'argument.
const SIGNATURE: &[u8] = b"L@:@\0";

/// La question, telle que le composition root la pose.
///
/// Un état global, et il n'y a pas d'alternative : `applicationShouldTerminate:` est une
/// fonction C que macOS appelle avec le délégué de tao pour seul contexte — un objet dont
/// nous ne possédons ni les ivars ni la construction. Il est écrit **une** fois, avant que
/// l'application ne tourne, et lu depuis le fil principal uniquement.
static MAY_LEAVE: OnceLock<Box<dyn Fn() -> bool + Send + Sync>> = OnceLock::new();

/// Branche `may_leave` sur `-[NSApplication terminate:]`. Rend `false` si rien n'a été posé.
///
/// **À appeler depuis le fil principal, avant `app.run`** : le délégué existe dès que la
/// boucle d'événements est construite, et une question posée après le premier `⌘Q` serait
/// arrivée trop tard.
///
/// `false` n'empêche pas Ash de démarrer, et c'est le composition root qui en décide : un
/// `⌘Q` qui ne demande rien est le comportement d'avant cette tranche, pas une panne. Le
/// message sur la sortie d'erreur est ce qui rend le cas trouvable.
pub fn intercept_terminate(may_leave: impl Fn() -> bool + Send + Sync + 'static) -> bool {
    if MAY_LEAVE.set(Box::new(may_leave)).is_err() {
        return false;
    }

    // SAFETY: `sharedApplication` et `delegate` sont deux getters sans convention de
    // possession — on ne retient ni l'un ni l'autre au-delà de cet appel, qui est sur le fil
    // principal. `object_getClass` rend la classe du délégué que tao a posé, et
    // `class_addMethod` refuse d'elle-même si la méthode y existe déjà. `should_terminate` a
    // exactement la signature déclarée par `SIGNATURE`.
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return false;
        }
        let delegate: *mut AnyObject = msg_send![app, delegate];
        if delegate.is_null() {
            return false;
        }
        let class = object_getClass(delegate) as *mut AnyClass;
        if class.is_null() {
            return false;
        }

        let method: extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize =
            should_terminate;
        class_addMethod(
            class,
            sel!(applicationShouldTerminate:),
            std::mem::transmute::<
                extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
                Imp,
            >(method),
            SIGNATURE.as_ptr().cast::<c_char>(),
        )
        .as_bool()
    }
}

/// Ce que macOS appelle quand quelque chose demande la fin de l'application.
///
/// La panique est rattrapée plutôt que laissée traverser : au-dessus de cette trame, il n'y
/// a que du code Objective-C, où un déroulement de pile Rust n'a aucun sens. Et ce qu'on
/// répond alors est **oui** — un terminal qu'on ne peut plus quitter est un piège pire que
/// la question qu'on vient de rater.
extern "C-unwind" fn should_terminate(
    _this: *mut AnyObject,
    _cmd: Sel,
    _sender: *mut AnyObject,
) -> usize {
    let Some(may_leave) = MAY_LEAVE.get() else {
        return TERMINATE_NOW;
    };
    match catch_unwind(AssertUnwindSafe(may_leave)) {
        Ok(false) => TERMINATE_CANCEL,
        Ok(true) | Err(_) => TERMINATE_NOW,
    }
}
