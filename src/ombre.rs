//! Ombre portee des fenetres posees sur la page (menu des extensions, Couture, fenetre
//! d'extension).
//!
//! Une fenetre decoupee par une region ne peut rien dessiner hors de sa region : l'ombre ne peut
//! donc pas venir du menu lui-meme. Elle vient d'une **fenetre a part** : une fenetre popup
//! *layered* (transparence par pixel), *transparent* aux clics, sans activation, possedee par la
//! fenetre de Trame (elle suit donc son ordre d'empilement et sa reduction). Son bitmap est un
//! anneau d'ombre autour de la forme du menu, **transparent a l'interieur** : posee au-dessus du
//! menu, elle ne le cache pas. C'est la technique des ombres de menus de Windows.

use std::cell::Cell;

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
    ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassW, ShowWindow, UpdateLayeredWindow, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP,
};

/// Une forme a ombrer, en pixels physiques **clients** de la fenetre de Trame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Forme {
    pub x: i32,
    pub y: i32,
    pub largeur: i32,
    pub hauteur: i32,
    /// Rayon des coins (0 : coins droits).
    pub rayon: i32,
}

/// Opacite maximale de l'ombre, au ras de la forme (0-255).
const OPACITE: f64 = 78.0;

/// Opacite de l'ombre en un pixel (coordonnees du bitmap, dont l'origine est decalee de
/// `marge` par rapport a la forme). Nulle a l'interieur de la forme, au-dessus de son bord haut
/// (le menu est colle sous la barre : pas d'ombre sur elle), et au-dela de `marge`. L'ombre
/// descend de `decalage` pixels, comme une lumiere venue d'en haut.
pub fn opacite(px: i32, py: i32, forme: &Forme, marge: i32, decalage: i32) -> u8 {
    let (l, h) = (forme.largeur as f64, forme.hauteur as f64);
    let rayon = (forme.rayon as f64).min(l / 2.0).min(h / 2.0).max(0.0);
    // Distance signee a la forme : seuls ses coins BAS sont arrondis (le menu est colle sous la
    // barre, ses coins hauts sont droits — comme sa region de fenetre).
    let interieur = |dy: f64| -> f64 {
        let cx = marge as f64 + l / 2.0;
        let cy = marge as f64 + h / 2.0 + dy;
        let r = if (py as f64 + 0.5) < cy { 0.0 } else { rayon };
        let qx = ((px as f64 + 0.5) - cx).abs() - (l / 2.0 - r);
        let qy = ((py as f64 + 0.5) - cy).abs() - (h / 2.0 - r);
        let dehors = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        dehors + qx.max(qy).min(0.0) - r
    };
    // Dans la forme elle-meme : rien (le menu doit rester parfaitement net).
    if interieur(0.0) <= 0.0 {
        return 0;
    }
    // Au-dessus du bord haut de la forme : rien.
    if py < marge {
        return 0;
    }
    // Distance a la forme decalee vers le bas.
    let d = interieur(decalage as f64);
    if d <= 0.0 {
        return OPACITE as u8;
    }
    let t = d / marge.max(1) as f64;
    if t >= 1.0 {
        return 0;
    }
    (OPACITE * (1.0 - t).powi(2)).round() as u8
}

/// Une fenetre d'ombre. Creee cachee ; `montrer` la dessine et la place, `cacher` la retire.
pub struct Ombre {
    hwnd: HWND,
    proprietaire: HWND,
    /// Derniere forme dessinee (et sa position a l'ecran) : on ne redessine que si elle change.
    derniere: Cell<Option<(Forme, i32, i32)>>,
}

unsafe extern "system" fn procedure(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(h, m, w, l) }
}

impl Ombre {
    pub fn nouvelle(proprietaire: isize) -> Option<Ombre> {
        let proprietaire = HWND(proprietaire as *mut core::ffi::c_void);
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let classe = w!("TrameOmbre");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: classe,
                ..Default::default()
            };
            // Deja enregistree pour la seconde ombre : l'echec est attendu et sans consequence.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                classe,
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                Some(proprietaire),
                None,
                Some(instance.into()),
                None,
            )
            .ok()?;
            Some(Ombre {
                hwnd,
                proprietaire,
                derniere: Cell::new(None),
            })
        }
    }

    pub fn cacher(&self) {
        if self.derniere.take().is_some() {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    /// Dessine l'ombre autour de `forme` (pixels clients de la fenetre de Trame).
    pub fn montrer(&self, forme: Forme, echelle: f64) {
        let marge = (18.0 * echelle).round() as i32;
        let decalage = (4.0 * echelle).round() as i32;
        // Position a l'ecran de l'origine de la zone cliente.
        let mut origine = POINT { x: 0, y: 0 };
        unsafe {
            let _ = ClientToScreen(self.proprietaire, &mut origine);
        }
        let (ex, ey) = (origine.x + forme.x - marge, origine.y + forme.y - marge);
        if self.derniere.get() == Some((forme, ex, ey)) {
            return;
        }
        let (l, h) = (forme.largeur + 2 * marge, forme.hauteur + 2 * marge);
        if l <= 0 || h <= 0 || l > 16_384 || h > 16_384 {
            return;
        }
        unsafe {
            let ecran = GetDC(None);
            let memoire = CreateCompatibleDC(Some(ecran));
            let mut info = BITMAPINFO::default();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: l,
                biHeight: -h, // de haut en bas
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let Ok(bitmap) = CreateDIBSection(Some(memoire), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(memoire);
                ReleaseDC(None, ecran);
                return;
            };
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (l * h) as usize);
            for py in 0..h {
                for px in 0..l {
                    // Noir premultiplie : seul l'alpha compte.
                    let a = opacite(px, py, &forme, marge, decalage) as u32;
                    pixels[(py * l + px) as usize] = a << 24;
                }
            }
            let ancien = SelectObject(memoire, HGDIOBJ(bitmap.0));
            let position = POINT { x: ex, y: ey };
            let taille = SIZE { cx: l, cy: h };
            let source = POINT { x: 0, y: 0 };
            let melange = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                Some(ecran),
                Some(&position),
                Some(&taille),
                Some(memoire),
                Some(&source),
                COLORREF(0),
                Some(&melange),
                ULW_ALPHA,
            );
            SelectObject(memoire, ancien);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memoire);
            ReleaseDC(None, ecran);
            let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        self.derniere.set(Some((forme, ex, ey)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENU: Forme = Forme { x: 0, y: 0, largeur: 100, hauteur: 60, rayon: 12 };

    #[test]
    fn l_ombre_ne_touche_jamais_le_menu_ni_la_barre_au_dessus() {
        let (marge, dec) = (18, 4);
        // Interieur du menu : parfaitement net.
        for (x, y) in [(18 + 50, 18 + 30), (18 + 1, 18 + 1), (18 + 99, 18 + 40)] {
            assert_eq!(opacite(x, y, &MENU, marge, dec), 0, "({x},{y}) dans le menu");
        }
        // Au-dessus du bord haut : la barre, jamais ombree.
        assert_eq!(opacite(18 + 50, 5, &MENU, marge, dec), 0);
    }

    #[test]
    fn l_ombre_s_estompe_en_s_eloignant_et_tombe_vers_le_bas() {
        let (marge, dec) = (18, 4);
        let sous = |d: i32| opacite(18 + 50, 18 + 60 + d, &MENU, marge, dec);
        assert!(sous(1) > sous(8) && sous(8) > sous(16), "decroissante : {} {} {}", sous(1), sous(8), sous(16));
        assert_eq!(opacite(18 + 50, 18 + 60 + 30, &MENU, marge, dec), 0, "au-dela de la marge");
        // Plus marquee dessous que sur le cote, a la meme distance (lumiere d'en haut).
        let cote = opacite(18 + 100 + 4, 18 + 30, &MENU, marge, dec);
        assert!(sous(4) > cote, "dessous {} > cote {}", sous(4), cote);
    }

    #[test]
    fn un_coin_arrondi_n_est_pas_ombre_comme_un_coin_droit() {
        let droit = Forme { rayon: 0, ..MENU };
        // Juste a l'exterieur du coin bas-droit : dans un coin arrondi, ce point est plus loin
        // de la forme, donc moins ombre.
        let (x, y) = (18 + 100 + 2, 18 + 60 + 2);
        assert!(opacite(x, y, &MENU, 18, 0) < opacite(x, y, &droit, 18, 0));
    }
}
