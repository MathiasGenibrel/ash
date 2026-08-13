//! Le centre de notifications de macOS, et le délégué par lequel le clic revient.
//!
//! **C'est le second module `unsafe` du crate**, et il est posé comme le premier
//! (`features/probe/macos.rs`, ADR-0005) : les appels bruts sont ici, chacun sous une
//! fonction sûre, et personne d'autre dans Ash n'a à connaître leurs conventions. Ce qui
//! sort d'ici est [`Banner`], [`Authorization`] et une chaîne — jamais un objet
//! Objective-C.
//!
//! ## Pourquoi `UNUserNotificationCenter`, et pas la pile précédente
//!
//! `tauri-plugin-notification` → `notify-rust` → `mac-notification-sys` pose des
//! `NSUserNotification`, dépréciés depuis macOS 11. Deux critères de la spec §8 y sont
//! **inatteignables**, et ils tombent tous les deux ici :
//!
//! - **l'autorisation réelle.** `NSUserNotificationCenter` n'a aucune API de permission,
//!   et le `permission_state()` du plugin rend `Granted` en dur sur le bureau. Il n'y a
//!   donc rien à brancher : la seule source qui existe est
//!   `UNNotificationSettings.authorizationStatus`, lu par [`authorization`] ;
//! - **le clic.** `mac-notification-sys` pose son propre délégué sur le centre partagé,
//!   dans un `dispatch_once` (`objc/notify.m`), et le délégué de `NSUserNotificationCenter`
//!   est **global au processus**. Deux couches qui se le disputent est une panne
//!   silencieuse : celle qui perd ne reçoit plus rien, et rien ne le dit. C'est pourquoi
//!   Ash ne cohabite pas — la dépendance au plugin a été retirée avec ce module.
//!
//! ## Ce que ça coûte : la bannière n'existe pas en développement
//!
//! `+[UNUserNotificationCenter currentNotificationCenter]` exige que le processus soit une
//! application empaquetée. Hors d'un `.app`, il ne rend pas d'erreur : il **lève**
//! `NSInternalInconsistencyException` (« bundleProxyForCurrentProcess is nil »), que rien
//! côté Rust ne rattrape, et le processus meurt. Or `bun run tauri dev` et
//! `bun run smoke` lancent `target/debug/ash`, un binaire nu.
//!
//! D'où [`bundled`], qui est franchi **avant** toute mention du centre : sans identifiant de
//! bundle, Ash ne pose pas de bannière et dit [`Authorization::Undisclosed`]. La
//! fonctionnalité n'est donc vérifiable que sur `bun run tauri build`, et c'est un coût réel
//! plutôt qu'un détail — la pile précédente, elle, postait en développement sous l'identité
//! `com.apple.Terminal`, ce qui n'était pas davantage la vraie.
//!
//! Le test de fin de fichier est ce garde-fou : il tourne dans un binaire de test, donc sans
//! bundle, et retirer le garde ne le fait pas échouer — il fait **mourir** `cargo test`.
#![allow(unsafe_code)]

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_foundation::{NSBundle, NSObject, NSObjectProtocol, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNNotificationResponse, UNUserNotificationCenter, UNUserNotificationCenterDelegate,
};

use super::port::{Authorization, Banner, Banners, Clicked};

/// Combien de temps attendre la réponse de macOS sur l'autorisation.
///
/// `getNotificationSettings` répond par un bloc, sur une file à lui ; la fenêtre de
/// réglages, elle, pose une question synchrone. C'est donc **le système** qu'on attend, pas
/// l'utilisateur — la distinction est tout l'objet de cette tranche — et l'attente est
/// bornée pour qu'une réponse qui ne vient pas coûte une ligne prudente plutôt qu'une
/// fenêtre figée. Mesuré à moins de dix millisecondes sur une application empaquetée.
const ANSWER_WITHIN: Duration = Duration::from_secs(2);

/// Ce que `UNAuthorizationStatus` vaut, mot pour mot.
const NOT_DETERMINED: isize = 0;
const DENIED: isize = 1;
const AUTHORIZED: isize = 2;
const PROVISIONAL: isize = 3;

/// L'implémentation macOS de [`Banners`]. Choisie au composition root, nulle part ailleurs.
///
/// **Elle ne retient aucun objet Objective-C**, et c'est ce qui la rend `Send + Sync` sans
/// rien promettre de faux : le centre est un singleton qu'on redemande à chaque appel, et le
/// délégué vit aussi longtemps que le processus (voir [`SystemBanners::attach`]).
pub struct SystemBanners {
    /// Rend le type impossible à construire hors de [`SystemBanners::attach`], donc
    /// impossible à obtenir sans que le garde de [`bundled`] ait été franchi.
    _guarded: (),
}

impl SystemBanners {
    /// Branche Ash sur le centre de notifications, ou rend `None` s'il n'y en a pas.
    ///
    /// **À appeler depuis le fil principal, avant que l'application ne tourne** : c'est la
    /// consigne d'Apple pour le délégué, et c'est la seule façon qu'un clic reçu pendant le
    /// démarrage trouve quelqu'un à qui parler.
    ///
    /// `None` n'est pas une panne : c'est le développement, où Ash n'est pas empaqueté. Le
    /// composition root en fait un adaptateur muet plutôt qu'un refus de démarrer — les
    /// bannières valent moins que le terminal.
    pub fn attach(clicked: Clicked) -> Option<Self> {
        if !bundled() {
            return None;
        }

        let center = UNUserNotificationCenter::currentNotificationCenter();

        // Le délégué est une propriété **faible** : le relâcher le ferait désallouer, et le
        // centre garderait un `nil` — donc plus aucun clic, sans rien qui le dise. On le
        // fuit volontairement, parce que sa durée de vie est exactement celle du processus.
        let delegate = ClickDelegate::new(clicked);
        center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        std::mem::forget(delegate);

        // Sans cette demande, le statut reste `NotDetermined` pour toujours et aucune
        // bannière n'est jamais montrée : c'est le seul moment où macOS accepte de poser la
        // question à l'utilisateur. La réponse ne nous intéresse pas ici — c'est
        // [`Self::authorization`] qui la relit, et la fenêtre de réglages qui la dit.
        //
        // `Alert` seul : Ash demande le droit d'afficher une bannière, pas celui de faire du
        // bruit ni de coller une pastille sur son icône.
        let ignored = RcBlock::new(
            |_granted: objc2::runtime::Bool, _error: *mut objc2_foundation::NSError| {},
        );
        center.requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert,
            &ignored,
        );

        Some(Self { _guarded: () })
    }
}

impl Banners for SystemBanners {
    fn post(&self, banner: Banner) {
        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&banner.title));
        content.setBody(&NSString::from_str(&banner.body));

        // **Le `payload` voyage comme identifiant de la requête**, et c'est le chemin que
        // macOS garantit : la notification revient au délégué avec lui, sans dictionnaire à
        // construire ni à relire — donc sans `unsafe` de plus. C'est aussi ce que fait
        // `mac-notification-sys` avec son UUID.
        //
        // Conséquence assumée : deux bannières pour le **même** onglet se remplacent, parce
        // qu'un identifiant désigne une notification. C'est le comportement qu'on veut — un
        // agent qui passe de `waiting` à `error` laisse une seule ligne, la dernière, et
        // deux onglets restent deux bannières.
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &NSString::from_str(&banner.payload),
            &content,
            None,
        );
        center().addNotificationRequest_withCompletionHandler(&request, None);
    }

    fn authorization(&self) -> Authorization {
        authorization()
    }
}

/// Ce que macOS répond sur l'autorisation, attendu au plus [`ANSWER_WITHIN`].
fn authorization() -> Authorization {
    let answer = Arc::new((Mutex::new(None::<isize>), Condvar::new()));
    let filling = Arc::clone(&answer);

    let handler = RcBlock::new(
        move |settings: std::ptr::NonNull<objc2_user_notifications::UNNotificationSettings>| {
            // SAFETY: macOS passe au bloc de complétion un objet vivant pour la durée de
            // l'appel, et le pointeur est déclaré `NonNull` par la liaison. On ne le garde pas :
            // seule la valeur du statut, un entier, sort d'ici.
            let status = unsafe { settings.as_ref() }.authorizationStatus().0;
            let (slot, awake) = &*filling;
            if let Ok(mut slot) = slot.lock() {
                *slot = Some(status);
                awake.notify_one();
            }
        },
    );
    center().getNotificationSettingsWithCompletionHandler(&handler);

    let (slot, awake) = &*answer;
    let Ok(slot) = slot.lock() else {
        return Authorization::Undisclosed;
    };
    let Ok((slot, _)) = awake.wait_timeout_while(slot, ANSWER_WITHIN, |slot| slot.is_none()) else {
        return Authorization::Undisclosed;
    };
    slot.map_or(Authorization::Undisclosed, authorization_of)
}

/// Le centre partagé. **N'est appelé que derrière le garde de [`bundled`]** — voir le
/// mod-doc : ailleurs, cette ligne tue le processus.
fn center() -> Retained<UNUserNotificationCenter> {
    UNUserNotificationCenter::currentNotificationCenter()
}

/// Ce processus est-il une application empaquetée ?
///
/// L'identifiant de bundle est ce que macOS exige, et son absence est exactement ce qui fait
/// lever `currentNotificationCenter` : un binaire nu a un `mainBundle` — le dossier qui le
/// contient — mais pas d'`Info.plist`, donc pas d'identifiant.
fn bundled() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

/// Ce qu'un `UNAuthorizationStatus` veut dire pour la fenêtre de réglages.
///
/// Le regroupement porte deux décisions, et chacune se paie si on la prend à l'envers :
///
/// - **« pas encore demandé » n'est pas « refusé ».** Dire à quelqu'un qu'il a refusé une
///   question qu'on ne lui a jamais posée est le seul mensonge que cette ligne puisse
///   commettre — c'est précisément ce que le `Granted` constant du plugin faisait, dans
///   l'autre sens ;
/// - **`Provisional` compte comme accordée.** La bannière arrive, silencieusement et dans le
///   Centre de notifications ; du point de vue de l'utilisateur, Ash a le droit de parler.
fn authorization_of(status: isize) -> Authorization {
    match status {
        DENIED => Authorization::Denied,
        AUTHORIZED | PROVISIONAL => Authorization::Granted,
        NOT_DETERMINED => Authorization::Undisclosed,
        // Une valeur qu'une version future de macOS ajouterait : ne rien conclure est la
        // seule réponse qu'on ne regrettera pas.
        _ => Authorization::Undisclosed,
    }
}

/// Ce que le délégué garde : de quoi rejouer le `payload`, et rien d'autre.
struct ClickIvars {
    clicked: Clicked,
}

define_class!(
    /// Le délégué du centre de notifications — le rappel **asynchrone** de macOS.
    ///
    /// C'est tout l'objet de la tranche : `didReceiveNotificationResponse` ne bloque
    /// personne et rend la notification cliquée avec son identifiant. Le mode synchrone de
    /// `notify-rust` (`wait_for_response`) aurait exigé un fil garé par bannière, pour un
    /// cas nominal — l'utilisateur ne clique pas — où il ne serait jamais rendu.
    #[unsafe(super(NSObject))]
    #[name = "AshNotificationClickDelegate"]
    #[ivars = ClickIvars]
    struct ClickDelegate;

    unsafe impl NSObjectProtocol for ClickDelegate {}

    unsafe impl UNUserNotificationCenterDelegate for ClickDelegate {
        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &block2::DynBlock<dyn Fn()>,
        ) {
            let payload = response.notification().request().identifier().to_string();
            (self.ivars().clicked)(&payload);
            // macOS attend cet accusé pour libérer la notification. Il part tout de suite :
            // ce que le rappel a déclenché est une émission d'event, pas un travail.
            done.call(());
        }
    }
);

impl ClickDelegate {
    fn new(clicked: Clicked) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ClickIvars { clicked });
        // SAFETY: `init` de `NSObject` sur une allocation dont les ivars viennent d'être
        // posés — la séquence que `define_class!` impose, et la seule qui rende l'objet
        // utilisable.
        unsafe { msg_send![super(this), init] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_a_process_that_is_not_a_bundled_application_when_ash_tries_to_reach_the_notification_center_then_it_refuses_before_touching_it(
    ) {
        // Given — le binaire de `cargo test` n'est pas dans un `.app`, exactement comme
        // `target/debug/ash` que lancent `bun run tauri dev` et `bun run smoke`. Ce test est
        // le garde-fou lui-même : `currentNotificationCenter` lève là une exception
        // Objective-C que Rust ne rattrape pas, donc retirer le garde ne fait pas échouer
        // ce test — il fait **mourir** `cargo test`, et c'est la panne qu'on veut voir
        // avant l'utilisateur.
        let nothing_to_do = Arc::new(|_: &str| {}) as Clicked;

        // When
        let attached = SystemBanners::attach(nothing_to_do);

        // Then
        assert!(attached.is_none());
        assert!(!bundled());
    }

    #[test]
    fn given_a_permission_the_user_was_never_asked_for_when_the_settings_window_reads_it_then_ash_does_not_call_it_a_refusal(
    ) {
        // Given — les quatre valeurs que macOS peut rendre. Confondre « jamais demandé » et
        // « refusé » ferait accuser l'utilisateur d'un geste qu'il n'a pas fait, et
        // l'enverrait dans les Réglages Système corriger ce qui n'y est pas.
        let statuses = [NOT_DETERMINED, DENIED, AUTHORIZED, PROVISIONAL];

        // When
        let read: Vec<Authorization> = statuses.into_iter().map(authorization_of).collect();

        // Then
        assert_eq!(
            read,
            vec![
                Authorization::Undisclosed,
                Authorization::Denied,
                Authorization::Granted,
                Authorization::Granted,
            ]
        );
    }
}
