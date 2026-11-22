//! Superposition des webviews : ordre d'empilement et regions de fenetre.
//!
//! Chaque webview de wry est une fenetre fille Win32 (`WS_CHILD | WS_CLIPCHILDREN`), creee
//! **sans** `WS_CLIPSIBLINGS` : deux webviews qui se chevauchent peuvent alors se dessiner l'une
//! sur l'autre dans un ordre quelconque. C'est pour cela que Trame avait renonce a toute
//! superposition (le menu des extensions poussait la page vers le bas).
//!
//! Avec `WS_CLIPSIBLINGS` sur toutes les webviews, c'est l'**ordre d'empilement** qui decide de
//! ce qui se voit : il suffit alors de mettre le menu ou la fenetre d'extension au premier plan.
//! Et une **region de fenetre** decoupe la barre agrandie : seuls la ligne du haut et la colonne
//! du menu restent visibles ; autour, la page se voit et recoit les clics.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, HRGN,
    RGN_OR,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE, HWND_TOP, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, WS_CLIPSIBLINGS,
};
use wry::{WebView, WebViewExtWindows};

/// Un rectangle en pixels physiques, relatif a la webview : (x, y, largeur, hauteur).
pub type Rectangle = (i32, i32, i32, i32);

/// Donne `WS_CLIPSIBLINGS` a une webview : elle ne dessine plus sur ses soeurs placees
/// au-dessus d'elle.
pub fn clip_soeurs(webview: &WebView) {
    // `TRAME_SANS_CLIP` (diagnostic) : ne rien toucher, pour comparer.
    if std::env::var("TRAME_SANS_CLIP").is_ok() {
        return;
    }
    let hwnd: HWND = webview.hwnd();
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let voulu = style | WS_CLIPSIBLINGS.0 as isize;
        if voulu != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, voulu);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Met une webview au-dessus de ses soeurs, sans l'activer ni lui prendre le focus.
pub fn au_premier_plan(webview: &WebView) {
    unsafe {
        let _ = SetWindowPos(
            webview.hwnd(),
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

/// Decoupe une webview : seuls `rectangles` (et `arrondis`, a coins ronds de `rayon`) restent
/// visibles et cliquables. `None` rend la fenetre entiere.
pub fn region(webview: &WebView, decoupe: Option<(&[Rectangle], &[Rectangle], i32)>) {
    let hwnd = webview.hwnd();
    unsafe {
        let Some((rectangles, arrondis, rayon)) = decoupe else {
            SetWindowRgn(hwnd, None, true);
            return;
        };
        let total: HRGN = CreateRectRgn(0, 0, 0, 0);
        let ajouter = |r: HRGN| {
            CombineRgn(Some(total), Some(total), Some(r), RGN_OR);
            let _ = DeleteObject(HGDIOBJ(r.0));
        };
        for (x, y, l, h) in rectangles {
            ajouter(CreateRectRgn(*x, *y, x + l, y + h));
        }
        for (x, y, l, h) in arrondis {
            // +1 : les bornes droite et basse d'une region arrondie sont exclusives.
            ajouter(CreateRoundRectRgn(*x, *y, x + l + 1, y + h + 1, rayon * 2, rayon * 2));
        }
        // Le systeme prend possession de la region : on ne la libere pas.
        SetWindowRgn(hwnd, Some(total), true);
    }
}
