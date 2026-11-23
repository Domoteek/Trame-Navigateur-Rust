//! La teinte de Trame vue du cote Rust : pour ce qui ne lit pas les variables CSS de la
//! coquille — les scripts poses DANS les pages (popup de mise a jour, barre du coup d'oeil,
//! recherche dans la page, selecteur de Zap) et la couleur de fond des webviews.
//!
//! Une seule source : `ui/teintes.js`, embarque a la compilation et relu ici. Ecrire une seconde
//! table en Rust, c'etait s'assurer qu'un jour les deux divergent — ce qui etait deja arrive
//! entre la coquille et les pastilles des reglages (2026-11-23).

const TEINTES_JS: &str = include_str!("../ui/teintes.js");

#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub fond: String,
    pub surface: String,
    pub surface2: String,
    pub accent: String,
}

/// Une valeur `cle: "#xxxxxx"` dans le bloc de la teinte `id` de `teintes.js`.
fn lire(id: &str, cle: &str) -> Option<String> {
    let debut = TEINTES_JS.find(&format!("\n    {id}: {{"))?;
    let bloc = &TEINTES_JS[debut..];
    let bloc = &bloc[..bloc.find("\n    },")?];
    let motif = format!("{cle}: \"");
    let i = bloc.find(&motif)? + motif.len();
    let valeur = &bloc[i..i + bloc[i..].find('"')?];
    rgb(valeur).map(|_| valeur.to_string())
}

/// La palette de la teinte `id` ; « dynamique » prend l'accent du site s'il est en hexadecimal.
pub fn palette(id: &str, site: Option<&str>) -> Palette {
    let base = |cle: &str| lire(id, cle).or_else(|| lire("defaut", cle)).unwrap_or_else(|| "#60a5fa".into());
    let mut p = Palette {
        fond: base("fond"),
        surface: base("surface"),
        surface2: base("surface2"),
        accent: base("accent"),
    };
    if id == "dynamique" {
        if let Some(c) = site.filter(|c| rgb(c).is_some()) {
            p.accent = c.to_string();
        }
    }
    p
}

/// `#abc` ou `#aabbcc` en (r, g, b).
pub fn rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim().strip_prefix('#')?;
    let h: String = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        6 => h.to_string(),
        _ => return None,
    };
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((v(0)?, v(2)?, v(4)?))
}

/// La couleur de fond d'une webview avant qu'elle ne peigne : celle de la teinte.
pub fn couleur_fond(p: &Palette) -> (u8, u8, u8, u8) {
    let (r, g, b) = rgb(&p.fond).unwrap_or((8, 11, 17));
    (r, g, b, 255)
}

/// L'objet JS que recoivent les scripts poses dans les pages, a la place de `__TRAME_TEINTE__`.
pub fn js(p: &Palette) -> String {
    let (r, g, b) = rgb(&p.accent).unwrap_or((96, 165, 250));
    // Un accent eclairci pour le texte sur fond sombre : 40 % d'accent, 60 % de blanc.
    let m = |c: u8| ((c as u16 * 2 + 255 * 3) / 5) as u8;
    format!(
        "{{\"fond\":\"{}\",\"surface\":\"{}\",\"surface2\":\"{}\",\"accent\":\"{}\",\"rgb\":\"{r}, {g}, {b}\",\"clair\":\"#{:02x}{:02x}{:02x}\"}}",
        p.fond, p.surface, p.surface2, p.accent, m(r), m(g), m(b)
    )
}

/// Pose la palette dans un script : `__TRAME_TEINTE__` devient l'objet de couleurs.
pub fn injecter(script: &str, p: &Palette) -> String {
    script.replace("__TRAME_TEINTE__", &js(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_table_js_est_lue_pour_chaque_teinte() {
        for id in ["defaut", "menthe", "ciel", "violet", "ambre", "ardoise"] {
            assert!(lire(id, "fond").is_some(), "{id} : fond");
            assert!(lire(id, "surface2").is_some(), "{id} : surface2");
            assert!(lire(id, "accent").is_some(), "{id} : accent");
        }
        assert_eq!(palette("ambre", None).accent, "#fbbf24");
        assert_eq!(palette("defaut", None).fond, "#080b11");
        assert_ne!(palette("ciel", None), palette("defaut", None), "Ciel n'est plus Défaut");
    }

    #[test]
    fn dynamique_prend_l_accent_du_site_sur_les_surfaces_d_origine() {
        let p = palette("dynamique", Some("#e11d48"));
        assert_eq!(p.accent, "#e11d48");
        assert_eq!(p.fond, palette("defaut", None).fond);
        assert_eq!(palette("dynamique", Some("rgb(1,2,3)")).accent, "#60a5fa", "non hexadecimal : accent d'origine");
        assert_eq!(palette("inconnue", None), palette("defaut", None));
    }

    #[test]
    fn l_objet_js_et_le_fond() {
        let p = palette("menthe", None);
        let js = js(&p);
        assert!(js.contains("\"rgb\":\"52, 211, 153\""), "{js}");
        assert!(js.contains("\"accent\":\"#34d399\""));
        assert_eq!(couleur_fond(&p), (7, 16, 14, 255));
        assert_eq!(injecter("var T = __TRAME_TEINTE__;", &p), format!("var T = {js};"));
    }
}
