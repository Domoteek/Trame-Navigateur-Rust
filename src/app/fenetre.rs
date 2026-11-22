//! Barre de la fenetre dessinee par Trame : deplacer, reduire, agrandir, fermer.
//!
//! Sauf au style « windows », la fenetre n'a pas de barre de titre (`with_decorations(false)`
//! dans main.rs). tao garde alors les bords invisibles de Windows a gauche, a droite et en bas
//! (ombre « undecorated ») : on y redimensionne comme avant. Le haut de la fenetre, lui, est
//! couvert par les surfaces de Trame : c'est l'interface qui demande le deplacement
//! (`drag_window`, comme une barre de titre) ou le redimensionnement par le bord du haut.
//!
//! Les dispositions d'ancrage de Windows 11 (le survol du bouton agrandir) ne s'ouvrent
//! d'elles-memes que sur un vrai bouton de barre de titre (`HTMAXBUTTON`), que les pages de
//! WebView2 ne peuvent pas presenter. Trame les ouvre comme le clavier : Win+Z.

use super::*;
use tao::window::ResizeDirection;

/// Hauteur du bandeau du style « bandeau » (px logiques).
pub const TITRE_H: f64 = 28.0;

impl App {
    /// Les messages de la barre de fenetre (surfaces rail et barre seulement).
    pub(super) fn traiter_ipc_fenetre(&mut self, source: Source, m: &str, v: &Value) -> bool {
        match m {
            "fenetre" => {
                if !matches!(source, Source::Rail | Source::Barre) {
                    return true;
                }
                match v["action"].as_str().unwrap_or("") {
                    "deplacer" => {
                        let _ = self.window.drag_window();
                    }
                    "agrandir" => {
                        let agrandie = self.window.is_maximized();
                        self.window.set_maximized(!agrandie);
                    }
                    "reduire" => self.window.set_minimized(true),
                    "fermer" => self.demander_fermeture(),
                    "bord" => {
                        let direction = match v["bord"].as_str().unwrap_or("") {
                            "haut" => Some(ResizeDirection::North),
                            "haut-gauche" => Some(ResizeDirection::NorthWest),
                            "haut-droite" => Some(ResizeDirection::NorthEast),
                            _ => None,
                        };
                        if let Some(d) = direction {
                            if !self.window.is_maximized() {
                                let _ = self.window.drag_resize_window(d);
                            }
                        }
                    }
                    // Win+Z vise la fenetre au premier plan : jamais une autre que Trame.
                    "dispositions" if self.au_premier_plan() => dispositions_ancrage(),
                    _ => {}
                }
                true
            }
            "barreFenetre" => {
                if let Some(mode) = v["mode"].as_str().filter(|s| crate::model::BARRES_FENETRE.contains(s)) {
                    if mode != self.reglages.barre_fenetre {
                        self.reglages.barre_fenetre = mode.to_string();
                        self.profil.sauver_reglages(&self.reglages);
                        self.appliquer_barre_fenetre();
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Met la fenetre au style choisi : barre de Windows ou barre de Trame.
    pub(super) fn appliquer_barre_fenetre(&mut self) {
        self.window.set_decorations(self.reglages.barre_fenetre() == "windows");
        self.appliquer_geometrie();
        self.pousser_etat();
    }

    /// Hauteur du bandeau en haut de la fenetre (0 hors du style « bandeau »).
    pub(super) fn hauteur_titre(&self) -> f64 {
        if self.reglages.barre_fenetre() == "bandeau" && self.window.fullscreen().is_none() {
            TITRE_H
        } else {
            0.0
        }
    }

    /// La fenetre est-elle agrandie ? Change l'icone du bouton (agrandir / restaurer).
    pub(super) fn suivre_agrandissement(&mut self) {
        let agrandie = self.window.is_maximized();
        if agrandie != self.agrandie {
            self.agrandie = agrandie;
            self.pousser_etat();
        }
    }

    fn au_premier_plan(&self) -> bool {
        use tao::platform::windows::WindowExtWindows;
        let ici = self.window.hwnd() as *mut core::ffi::c_void;
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 == ici }
    }

    /// Fermer comme par la croix de Windows : la boucle recoit `CloseRequested` et sauve la
    /// session avant de sortir.
    fn demander_fermeture(&self) {
        use tao::platform::windows::WindowExtWindows;
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        let hwnd = HWND(self.window.hwnd() as *mut core::ffi::c_void);
        unsafe {
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Ouvre les dispositions d'ancrage de Windows 11 pour la fenetre au premier plan (Win+Z).
fn dispositions_ancrage() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_LWIN,
    };
    let touche = |vk: VIRTUAL_KEY, relache: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if relache { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let z = VIRTUAL_KEY(b'Z' as u16);
    let suite = [touche(VK_LWIN, false), touche(z, false), touche(z, true), touche(VK_LWIN, true)];
    unsafe {
        SendInput(&suite, std::mem::size_of::<INPUT>() as i32);
    }
}
