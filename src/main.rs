//! Trame — navigateur a onglets tisses.
//!
//! Point d'entree : une boucle d'evenements tao, une fenetre, et trois surfaces d'interface
//! plus une webview par onglet reveille. Tout l'etat vit dans `app::App`, sur le fil
//! principal : les webviews ne sont pas `Send` et wry impose de les piloter depuis la
//! boucle. Les fils annexes ne font que poster des evenements.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// L'etat pousse a l'interface (`App::instantane`) est un seul `json!` d'une quarantaine de
// cles : il depasse la limite par defaut du macro.
#![recursion_limit = "256"]

mod aide;
mod app;
mod carnet;
mod chien_de_garde;
mod crx;
mod extensions;
mod extensions_chrome;
mod externe;
mod fenetres;
mod installation;
mod log;
mod maj;
mod mandataire;
mod model;
mod ombre;
mod moteur_ext;
mod probe;
mod profile;
mod protocol;
mod reseau;
mod ressources;
mod sauvegarde;
mod sauvegarde_auto;
mod search;
mod signature;
mod shell;
mod systeme;
mod teinte;
mod telechargements;
mod tunnel;
mod vpn;
mod urls;
mod vitality;
mod weave;

use std::time::Duration;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;

use app::{App, UserEvent};

/// Icone de fenetre : 64x64 RGBA brut, embarquee dans le binaire.
///
/// Le meme dessin est integre a l'executable sous forme de `.ico` multi-tailles par `build.rs`
/// (barre des taches, explorateur, alt-tab). Ici, c'est la fenetre elle-meme : sans cela, la
/// barre de titre et le bouton de barre des taches prennent l'icone par defaut du runtime.
pub(crate) fn icone_fenetre() -> tao::window::Icon {
    const COTES: u32 = 64;
    let pixels = include_bytes!("../ressources/trame-64.rgba").to_vec();
    tao::window::Icon::from_rgba(pixels, COTES, COTES).expect("icone 64x64 RGBA embarquee")
}

/// Identite d'application Windows (AppUserModelID). La meme que celle du raccourci du menu
/// Demarrer et des fiches `TrameURL` / `TrameHTML` posees par le setup (`outils/trame.iss`) :
/// sans elle, les Parametres de Windows ne proposaient pas Trame comme navigateur (2026-11-22).
const APP_USER_MODEL_ID: &str = "Trame.Navigateur";

fn main() {
    // Avant toute fenetre : la fenetre et le raccourci sont alors une seule application.
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            &windows::core::HSTRING::from(APP_USER_MODEL_ID),
        );
    }
    let profil = profile::Profil::ouvrir();
    log::init(profil.fichier_journal());
    info!("Trame demarre (profil {})", profil.racine().display());

    // Sonde de mise au point du tunnel (jamais en usage normal) : `TRAME_TUNNEL_ESSAI=1` monte le
    // tunnel configure, tente la poignee de main avec le pair, ecrit le resultat et sort. Elle
    // existe pour pouvoir eprouver le moteur sans toucher au profil de tous les jours.
    if let Ok(mode) = std::env::var("TRAME_TUNNEL_ESSAI") {
        let resultat = if mode == "2" {
            mandataire::essai(profil.racine())
        } else {
            vpn::essai(profil.racine())
        };
        let ligne = match resultat {
            Ok(detail) => format!("tunnel : {detail}"),
            Err(e) => format!("tunnel : ECHEC ({e})"),
        };
        info!("{ligne}");
        println!("{ligne}");
        return;
    }

    // Un lien recu de Windows (Trame navigateur par defaut) ou de la ligne de commande.
    let adresse = externe::adresse_des_arguments(std::env::args().skip(1));

    // Un seul Trame par profil : deux instances ecraseraient mutuellement leur session. La
    // seconde transmet son lien (ou simplement « reviens au premier plan ») a la premiere.
    chien_de_garde::attendre_l_ancien();
    let verrou = match profil.prendre_verrou() {
        Ok(v) => v,
        Err(pid) if pid == 0 => {
            alerte!("profil deja ouvert : impossible de prendre le verrou, sortie");
            return;
        }
        Err(pid) => {
            info!("profil deja ouvert par le processus {pid} : demande transmise");
            if let Err(e) = externe::deposer(profil.racine(), adresse.as_deref()) {
                alerte!("demande impossible a transmettre ({e})");
            }
            externe::autoriser_premier_plan(pid);
            return;
        }
    };
    if let Some(a) = &adresse {
        // Au demarrage, l'adresse passe par la meme boite : elle s'ouvrira une fois la session
        // restauree, dans un onglet a elle.
        let _ = externe::deposer(profil.racine(), Some(a));
    }

    // Une restauration demandee depuis la session precedente s'applique ICI : avant que la
    // session ne soit lue, avant que le moteur WebView2 ne soit cree, et une fois le verrou
    // pris (donc sans autre Trame en train d'ecrire le meme profil). L'ancien profil part dans
    // `avant-restauration-<date>\` : on archive, on ne detruit pas.
    sauvegarde::appliquer_restauration_en_attente(profil.racine());

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 1440 x 900 au plus, mais jamais plus grand que l'ecran : sur un ecran de 1080 px, la
    // fenetre de 900 px plus sa barre de titre passait sous la barre des taches (constate le
    // 2026-11-21, le pied du rail etait cache).
    let ecran = event_loop.primary_monitor();
    let (largeur, hauteur) = ecran
        .as_ref()
        .map(|m| {
            let taille = m.size().to_logical::<f64>(m.scale_factor());
            (
                (taille.width * 0.92).min(1440.0).max(880.0),
                // 0,82 : la barre des taches (48 px) et la barre de titre doivent tenir aussi.
                (taille.height * 0.82).min(900.0).max(560.0),
            )
        })
        .unwrap_or((1440.0, 900.0));
    // `TRAME_TAILLE=LxH` (diagnostic) : taille imposee, pour eprouver un petit ecran de portable
    // sur ce PC (menu ⋯ muet chez le fils de Clement, 2026-11-24).
    let (largeur, hauteur) = std::env::var("TRAME_TAILLE")
        .ok()
        .and_then(|t| {
            let (l, h) = t.split_once('x')?;
            Some((l.trim().parse::<f64>().ok()?.max(880.0), h.trim().parse::<f64>().ok()?.max(560.0)))
        })
        .unwrap_or((largeur, hauteur));
    // Centree dans la hauteur utile (ecran moins barre des taches) : Windows la posait en
    // cascade, et son bas tombait sous la barre des taches.
    let position = ecran.as_ref().map(|m| {
        let echelle = m.scale_factor();
        let origine = m.position().to_logical::<f64>(echelle);
        let taille = m.size().to_logical::<f64>(echelle);
        tao::dpi::LogicalPosition::new(
            origine.x + ((taille.width - largeur) / 2.0).max(0.0),
            origine.y + ((taille.height - 48.0 - hauteur - 32.0) / 2.0).max(0.0),
        )
    });
    let mut constructeur = WindowBuilder::new();
    if let Some(p) = position {
        constructeur = constructeur.with_position(p);
    }
    // `TRAME_CACHEE` (diagnostic) : fenetre jamais affichee, pour mesurer sans occuper l'ecran
    // ni prendre le focus (les pages tournent quand meme).
    if std::env::var("TRAME_CACHEE").is_ok() {
        constructeur = constructeur.with_visible(false).with_focused(false);
    }
    // Barre de la fenetre : sauf le style « windows », Trame dessine ses propres boutons
    // (reduire, agrandir, fermer) et retire la barre de titre de Windows. L'ombre de tao garde
    // les bords invisibles de Windows : on redimensionne par la gauche, la droite et le bas
    // comme avant ; le bord du haut passe par l'interface (`fenetre` dans app.rs).
    let sans_barre = profil.charger_reglages().barre_fenetre() != "windows";
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        constructeur = constructeur.with_decorations(!sans_barre).with_undecorated_shadow(true);
    }
    let window = constructeur
        .with_title("Trame")
        // Le fond de la fenetre ne se voit qu'entre deux pages du cote a cote (la separation
        // qu'on fait glisser) : la couleur des surfaces de Trame, pas le blanc par defaut.
        .with_background_color((13, 16, 22, 255))
        .with_window_icon(Some(icone_fenetre()))
        .with_inner_size(LogicalSize::new(largeur, hauteur))
        .with_min_inner_size(LogicalSize::new(880.0, 560.0))
        .build(&event_loop)
        .expect("fenetre principale");

    let mut application = App::nouveau(window, profil, proxy.clone());

    // Le verrou vit aussi longtemps que la boucle : le deplacer dans la closure la plus
    // longue est ce qui garantit qu'il n'est pas relache avant la fin.
    let _verrou = verrou;

    // Battement : vitalite, veille, sauvegarde differee.
    let proxy_tick = proxy.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1000));
        if proxy_tick.send_event(UserEvent::Tick).is_err() {
            break;
        }
    });

    // Mesure du budget memoire (reel, jamais simule).
    let proxy_memoire = proxy.clone();
    let pid = std::process::id();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(4000));
        let mesure = app::mesurer_memoire(pid);
        if proxy_memoire
            .send_event(UserEvent::Memoire(mesure))
            .is_err()
        {
            break;
        }
    });

    chien_de_garde::lancer();
    // `TRAME_SONDE_BLOCAGE` (diagnostic) : fige la boucle 60 s au bout de 10 s, pour eprouver
    // le chien de garde.
    let sonde_blocage = std::env::var("TRAME_SONDE_BLOCAGE").is_ok();
    let debut = std::time::Instant::now();
    let mut sonde_faite = false;

    // Etat en attente d'envoi : la boucle se reveille a l'image suivante (App::vider_etat).
    let mut echeance_etat: Option<std::time::Instant> = None;
    event_loop.run(move |event, _cible, controle| {
        // Garde le verrou du profil vivant jusqu'a la fermeture de la boucle.
        let _ = &_verrou;
        *controle = match echeance_etat {
            Some(t) => ControlFlow::WaitUntil(t),
            None => ControlFlow::Wait,
        };
        chien_de_garde::battement();
        match event {
            Event::UserEvent(evenement) => {
                if !matches!(evenement, UserEvent::Tick | UserEvent::Memoire(_)) {
                    chien_de_garde::noter(|| {
                        format!("{evenement:?}").chars().take(300).collect()
                    });
                } else if sonde_blocage && !sonde_faite && debut.elapsed() > Duration::from_secs(10) {
                    sonde_faite = true;
                    chien_de_garde::noter(|| "sonde de blocage (TRAME_SONDE_BLOCAGE)".into());
                    info!("sonde : boucle figee pendant 60 s");
                    std::thread::sleep(Duration::from_secs(60));
                }
                application.traiter(evenement);
                // Une restauration a ete confirmee : elle s'applique au demarrage suivant, donc
                // on ferme proprement (la session est ecrite a la sortie, comme d'habitude).
                if application.a_quitter() {
                    info!("arret demande : la restauration s'appliquera au prochain demarrage");
                    *controle = ControlFlow::Exit;
                    return;
                }
                // Une page a demande sa mini-fenetre : seule la boucle peut la creer.
                if let Some(onglet) = application.prendre_demande_flottant() {
                    application.ouvrir_flottant(onglet, _cible);
                }
            }
            // La mini-fenetre d'un onglet (video flottante) : la page suit sa taille, et la
            // fermer ramene l'onglet dans la fenetre principale (sans quitter Trame).
            Event::WindowEvent { event, window_id, .. } if application.est_flottant(window_id) => match event {
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                    application.flottant_redimensionne()
                }
                WindowEvent::CloseRequested => application.ramener_flottant(),
                _ => {}
            },
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::Resized(_) => application.redimensionner(),
                WindowEvent::Moved(_) => application.deplacee(),
                WindowEvent::ScaleFactorChanged { .. } => application.redimensionner(),
                WindowEvent::Focused(true) => application.refocaliser(),
                // La souris n'arrive a la fenetre principale que hors des pages : c'est-a-dire
                // sur la separation du cote a cote, qu'on peut faire glisser.
                WindowEvent::CursorMoved { position, .. } => application.souris_bouge(position.x, position.y),
                WindowEvent::CursorLeft { .. } => application.souris_sortie(),
                WindowEvent::MouseInput { state, button: tao::event::MouseButton::Left, .. } => {
                    application.souris_bouton(state == tao::event::ElementState::Pressed)
                }
                WindowEvent::CloseRequested => {
                    info!("fermeture demandee, sauvegarde de la session");
                    application.sauver();
                    *controle = ControlFlow::Exit;
                }
                _ => {}
            },
            // Fin du tour : l'etat demande pendant ce tour part, une seule fois.
            Event::MainEventsCleared => {
                echeance_etat = application.vider_etat();
                if let Some(t) = echeance_etat {
                    *controle = ControlFlow::WaitUntil(t);
                }
            }
            Event::LoopDestroyed => {
                application.sauver();
            }
            _ => {}
        }
    });
}
