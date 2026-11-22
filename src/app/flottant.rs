//! Onglet en mini-fenetre : la video flottante « avec ses commandes » (etape 3).
//!
//! L'etape 1 (`ui/incrustation.js`) sort la video dans la fenetre d'incrustation de Chromium,
//! qui n'a que lecture/pause. Ici, c'est l'**onglet entier** qui part dans une petite fenetre
//! de Trame, toujours au premier plan, sans barre de titre : rien ne se recharge, le lecteur
//! garde toutes ses commandes (qualite, sous-titres, saute-pub). La page recoit l'ordre de
//! faire remplir la fenetre par son lecteur (`__trame_flottant(true)` dans `incrustation.js`).
//!
//! La webview est **retiree** de `webviews` pendant le vol : toutes les boucles de la fenetre
//! principale (placement, visibilite, veille, empilement) l'ignorent ainsi sans exception a
//! ecrire partout. Activer l'onglet (rail, Ctrl+Tab…) ou fermer la mini-fenetre la ramene.
//!
//! Une fenetre ne se cree qu'avec la cible de la boucle d'evenements : `traiter_ipc` note la
//! demande (`flottant_demande`) et `main.rs` appelle `ouvrir_flottant` juste apres.

use super::*;
use tao::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{WindowBuilder, WindowId};
use wry::WebViewExtWindows;

/// Taille de depart de la mini-fenetre (px logiques, 16:9).
const LARGEUR: f64 = 480.0;
const HAUTEUR: f64 = 270.0;
/// Ecart au bord de l'ecran et hauteur reservee a la barre des taches.
const MARGE: f64 = 24.0;
const BARRE_TACHES: f64 = 48.0;

pub(super) struct Flottant {
    pub onglet: TabId,
    // Ordre des champs = ordre de destruction : la page avant sa fenetre.
    webview: WebView,
    fenetre: Window,
}

impl App {
    /// Messages `flottant` d'une page : `ouvrir`, `ramener`, `montrer`, `deplacer`.
    pub(super) fn traiter_ipc_flottant(&mut self, source: Source, v: &Value) {
        let Source::Onglet(id) = source else { return };
        let ici = self.flottant.as_ref().map(|f| f.onglet) == Some(id);
        match v["action"].as_str().unwrap_or("") {
            "ouvrir" if !ici => self.flottant_demande = Some(id),
            // « Ramener » (bouton, Alt+Maj+P) remet la page dans son onglet et l'affiche,
            // comme le retour a l'onglet d'Opera. Fermer la fenetre (Alt+F4) la range sans plus.
            "ramener" if ici => self.montrer_onglet(id),
            // « Retour a l'onglet » de la fenetre d'incrustation de Chromium : la video revient
            // dans sa page, mais Chromium ne sait pas afficher un onglet de Trame. Mesure du
            // 2026-11-22 : sans ce message, la page restait `hidden` — la video revenait dans un
            // onglet que Clement ne voyait pas.
            "montrer" if !ici => self.montrer_onglet(id),
            "deplacer" if ici => {
                if let Some(f) = &self.flottant {
                    let _ = f.fenetre.drag_window();
                }
            }
            _ => {}
        }
    }

    /// Met l'onglet de la page au premier plan dans la fenetre principale.
    fn montrer_onglet(&mut self, id: TabId) {
        self.activer(id);
        self.window.set_focus();
    }

    /// Demande en attente, prise par la boucle d'evenements (qui seule peut creer la fenetre).
    pub fn prendre_demande_flottant(&mut self) -> Option<TabId> {
        self.flottant_demande.take()
    }

    pub fn est_flottant(&self, fenetre: WindowId) -> bool {
        self.flottant.as_ref().is_some_and(|f| f.fenetre.id() == fenetre)
    }

    pub fn onglet_flottant(&self) -> Option<TabId> {
        self.flottant.as_ref().map(|f| f.onglet)
    }

    /// Sort l'onglet `id` dans sa mini-fenetre.
    pub fn ouvrir_flottant(&mut self, id: TabId, cible: &EventLoopWindowTarget<UserEvent>) {
        if self.flottant.is_some() {
            self.ramener_flottant();
        }
        if self.plein_ecran == Some(id) || !self.webviews.contains_key(&id) {
            return;
        }
        let titre = self.trouver(id).map(|o| o.libelle()).unwrap_or_default();
        let mut constructeur = WindowBuilder::new()
            .with_title(format!("{titre} — Trame"))
            .with_decorations(false)
            .with_undecorated_shadow(true)
            .with_always_on_top(true)
            .with_skip_taskbar(true)
            .with_background_color((0, 0, 0, 255))
            .with_window_icon(Some(crate::icone_fenetre()))
            .with_min_inner_size(LogicalSize::new(240.0, 135.0))
            .with_focused(false);
        match self.flottant_place {
            Some((position, taille)) => {
                constructeur = constructeur.with_position(position).with_inner_size(taille);
            }
            None => {
                constructeur = constructeur.with_inner_size(LogicalSize::new(LARGEUR, HAUTEUR));
                // En bas a droite de l'ecran ou se trouve Trame.
                if let Some(ecran) = self.window.current_monitor() {
                    let echelle = ecran.scale_factor();
                    let o = ecran.position();
                    let t = ecran.size();
                    let x = o.x + t.width as i32 - ((LARGEUR + MARGE) * echelle) as i32;
                    let y = o.y + t.height as i32 - ((HAUTEUR + MARGE + BARRE_TACHES) * echelle) as i32;
                    constructeur = constructeur.with_position(PhysicalPosition::new(x, y));
                }
            }
        }
        let fenetre = match constructeur.build(cible) {
            Ok(f) => f,
            Err(e) => {
                alerte!("mini-fenetre impossible : {e}");
                return;
            }
        };
        let Some(webview) = self.webviews.remove(&id) else { return };
        if let Err(e) = webview.reparent(fenetre.hwnd() as isize) {
            alerte!("mini-fenetre : la page ne suit pas ({e})");
            let _ = webview.reparent(self.window.hwnd() as isize);
            self.webviews.insert(id, webview);
            return;
        }
        let _ = webview.set_bounds(plein(&fenetre));
        let _ = webview.set_visible(true);
        let _ = webview.evaluate_script("window.__trame_flottant && window.__trame_flottant(true)");
        info!("onglet {id} : en mini-fenetre");

        if matches!(self.session.cote, Some((g, d)) if g == id || d == id) {
            self.session.cote = None;
        }
        self.flottant = Some(Flottant { onglet: id, webview, fenetre });
        if self.actif == Some(id) {
            match self.voisin_hors(id) {
                Some(v) => {
                    self.actif = None;
                    self.activer(v);
                }
                None => {
                    self.actif = None;
                    self.ouvrir_onglet(urls::NOUVEL_ONGLET.to_string(), Declencheur::NouvelOnglet, None);
                }
            }
        }
        self.appliquer_geometrie();
        self.pousser_etat();
    }

    /// Ramene l'onglet de la mini-fenetre dans la fenetre principale (sans l'activer).
    pub fn ramener_flottant(&mut self) {
        let Some(Flottant { onglet, webview, fenetre }) = self.flottant.take() else { return };
        if let Ok(position) = fenetre.outer_position() {
            self.flottant_place = Some((position, fenetre.inner_size()));
        }
        let _ = webview.evaluate_script("window.__trame_flottant && window.__trame_flottant(false)");
        let _ = webview.set_visible(false);
        if let Err(e) = webview.reparent(self.window.hwnd() as isize) {
            alerte!("mini-fenetre : retour impossible ({e}), onglet {onglet} rendormi");
            if let Some(o) = self.onglet_mut(onglet) {
                o.eveille = false;
            }
            drop(webview);
        } else {
            self.webviews.insert(onglet, webview);
        }
        drop(fenetre);
        info!("onglet {onglet} : revenu de la mini-fenetre");
        self.appliquer_geometrie();
        self.pousser_etat();
    }

    /// La mini-fenetre change de taille : la page la suit.
    pub fn flottant_redimensionne(&mut self) {
        if let Some(f) = &self.flottant {
            let _ = f.webview.set_bounds(plein(&f.fenetre));
        }
    }

    /// Onglet a montrer quand l'onglet actif part en mini-fenetre : le plus proche du meme fil,
    /// sinon le plus proche tout court.
    fn voisin_hors(&self, id: TabId) -> Option<TabId> {
        let onglets = &self.session.onglets;
        let i = onglets.iter().position(|o| o.id == id)?;
        let fil = onglets[i].fil;
        let candidat = |o: &&Onglet| o.id != id && o.rendez_vous.is_none();
        let proches = onglets[..i].iter().rev().chain(onglets[i + 1..].iter());
        proches
            .clone()
            .filter(candidat)
            .find(|o| o.fil == fil)
            .or_else(|| proches.filter(candidat).next())
            .map(|o| o.id)
    }
}

/// Toute la surface de la fenetre, en pixels physiques.
fn plein(fenetre: &Window) -> wry::Rect {
    let taille: PhysicalSize<u32> = fenetre.inner_size();
    wry::Rect {
        position: PhysicalPosition::new(0, 0).into(),
        size: taille.into(),
    }
}
