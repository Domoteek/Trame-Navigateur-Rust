//! Geometrie de la coquille : trois zones rectangulaires, calculees en **physique**.
//!
//! Les tailles de conception sont exprimees en pixels logiques (292 de rail, 46 de barre)
//! puis multipliees par le facteur d'echelle. Passer du logique a `set_bounds` directement
//! decale toute l'interface a 125 % ou 150 % — c'est le piege documente de wry.

use tao::dpi::{PhysicalPosition, PhysicalSize};
use wry::Rect;

/// Largeur du rail deplie (px logiques).
pub const RAIL_DEPLIE: f64 = 292.0;
/// Largeur du rail replie.
pub const RAIL_PLIE: f64 = 64.0;
/// Hauteur de la barre haute.
pub const BARRE_H: f64 = 46.0;
/// Hauteur de la barre d'extensions, sous la barre haute.
pub const EXT_H: f64 = 30.0;
/// Largeur du panneau de droite (regles de masquage), quand il est ouvert.
pub const ZAP_LARGEUR: f64 = 330.0;
/// En dessous de cette largeur, le rail se replie de lui-meme pour laisser la place au site.
///
/// Le seuil est calibre sur la place restante, pas sur la fenetre : un rail deplie coute
/// 292 px au contenu. Sous 1200 px logiques, il ne resterait plus assez de page pour un site
/// a `min-width` (beaucoup de sites de commerce en ont une) — d'ou un repli automatique.
pub const LARGEUR_MINIMALE_RAIL: f64 = 1200.0;

#[derive(Clone, Copy, Debug)]
pub struct Geometrie {
    pub rail: Rect,
    pub barre: Rect,
    pub contenu: Rect,
    /// Panneau de droite (regles de masquage) ; largeur nulle quand il est ferme.
    pub zap: Rect,
}

fn zone(x: f64, y: f64, largeur: f64, hauteur: f64) -> Rect {
    Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(largeur.max(1.0), hauteur.max(1.0)).into(),
    }
}

/// Calcule les trois zones pour une taille de fenetre donnee.
///
/// `barre_logique` permet d'agrandir la barre haute : c'est ainsi que s'affiche le panneau
/// de la Couture. Un WebView2 ne pouvant pas se superposer, l'alternative serait de
/// dessiner le panneau sous la barre — impossible sans melanger deux HWND.
pub fn geometrie(
    largeur_phys: u32,
    hauteur_phys: u32,
    echelle: f64,
    rail_replie: bool,
    barre_logique: f64,
    barre_base_logique: f64,
    zap_logique: f64,
) -> Geometrie {
    let echelle = if echelle > 0.1 { echelle } else { 1.0 };
    let l = largeur_phys as f64;
    let h = hauteur_phys as f64;

    let trop_etroit = l / echelle < LARGEUR_MINIMALE_RAIL;
    let rail_logique = if rail_replie || trop_etroit { RAIL_PLIE } else { RAIL_DEPLIE };
    // Tout est arrondi **avant** la conversion en pixels physiques : `set_bounds` arrondit de
    // son cote, et une position arrondie au-dessus avec une taille arrondie au-dessus fait
    // depasser la fenetre d'un pixel (contenu rogne en bas). Les complements sont donc
    // calcules en entiers, pas arrondis separement.
    let rail = (rail_logique * echelle).min(l * 0.5).round();
    // Le plafond de la barre est une proportion de la fenetre (72 %, voir plus bas). Sur une
    // fenetre minuscule, il tombe **sous** la hauteur minimale : `f64::clamp` panique alors
    // (« min > max »). Mesure du 2026-11-22 : reduite dans la barre des taches, la fenetre de
    // Trame ne fait plus 19 px de haut — plafond 13,68 px pour un minimum de 46 px — et
    // l'application mourait la, sans un mot dans le journal. Le minimum gagne : la barre garde
    // sa hauteur, la page se contente de ce qui reste.
    let barre_mini = BARRE_H * echelle;
    let barre_plafond = (h * 0.72).max(barre_mini);
    let barre = (barre_logique * echelle).clamp(barre_mini, barre_plafond).round();
    // Le panneau de droite ne doit jamais prendre plus de 40 % de la largeur : sinon le site
    // n'a plus de place et l'outil devient plus genant que la publicite. Ferme, il garde
    // 1 px (une webview de largeur nulle est refusee par WebView2).
    let zap = ((zap_logique * echelle).clamp(0.0, l * 0.40)).round().max(1.0);
    // La page commence sous la barre **de base** (ligne du haut + etagere d'extensions), pas
    // sous la barre agrandie : le menu des extensions et la Couture se posent PAR-DESSUS la page
    // (voir `fenetres.rs`), ils ne la poussent plus. Meme chose pour le panneau de droite.
    let base = (barre_base_logique * echelle).clamp(barre_mini, barre.max(barre_mini)).round();
    let contenu_l = (l - rail).max(1.0);
    let contenu_h = (h - base).max(1.0);

    Geometrie {
        rail: zone(0.0, 0.0, rail, h),
        barre: zone(rail, 0.0, l - rail, barre),
        contenu: zone(rail, base, contenu_l, contenu_h),
        zap: zone((l - zap).max(0.0), base, zap, contenu_h),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taille(r: Rect) -> (String, f64, f64) {
        match (r.position, r.size) {
            (tao::dpi::Position::Physical(p), tao::dpi::Size::Physical(s)) => {
                (format!("{},{}", p.x, p.y), s.width as f64, s.height as f64)
            }
            _ => ("logique".to_string(), 0.0, 0.0),
        }
    }

    /// Position et taille physiques, pour les invariants geometriques.
    fn rectangle(r: Rect) -> (f64, f64, f64, f64) {
        match (r.position, r.size) {
            (tao::dpi::Position::Physical(p), tao::dpi::Size::Physical(s)) => {
                (p.x as f64, p.y as f64, s.width as f64, s.height as f64)
            }
            _ => (0.0, 0.0, 0.0, 0.0),
        }
    }

    #[test]
    fn les_trois_zones_ne_se_chevauchent_pas() {
        let g = geometrie(1600, 1000, 1.0, false, BARRE_H, BARRE_H, 0.0);

        let (pos_rail, l_rail, h_rail) = taille(g.rail);
        assert_eq!(pos_rail, "0,0");
        assert_eq!((l_rail, h_rail), (292.0, 1000.0), "rail pleine hauteur, a gauche");

        let (pos_barre, l_barre, h_barre) = taille(g.barre);
        assert_eq!(pos_barre, "292,0", "la barre commence a droite du rail");
        assert_eq!((l_barre, h_barre), (1308.0, 46.0));

        let (pos_contenu, l_contenu, h_contenu) = taille(g.contenu);
        assert_eq!(pos_contenu, "292,46", "le contenu commence sous la barre");
        // 1600 − 292 de rail : le panneau de droite, quand il s'ouvre, se pose PAR-DESSUS.
        assert_eq!((l_contenu, h_contenu), (1308.0, 954.0));

        // Rail, barre et contenu forment une partition exacte de la fenetre.
        assert_eq!(l_rail + l_contenu, 1600.0);
        assert_eq!(h_barre + h_contenu, 1000.0);
    }

    #[test]
    fn l_echelle_est_appliquee() {
        let g = geometrie(2400, 1500, 1.5, false, BARRE_H, BARRE_H, 0.0);
        let (_, rl, _) = taille(g.rail);
        assert_eq!(rl, 438.0, "292 * 1.5");
    }

    #[test]
    fn une_fenetre_etroite_replie_le_rail() {
        let g = geometrie(800, 600, 1.0, false, BARRE_H, BARRE_H, 0.0);
        let (_, rl, _) = taille(g.rail);
        assert_eq!(rl, RAIL_PLIE);
    }

    #[test]
    fn le_rail_se_replie_avant_de_trop_roguer_le_site() {
        // 1100 px logiques : rail deplie = 292, il ne resterait que ~808 px au site.
        let etroit = geometrie(1100, 800, 1.0, false, BARRE_H, BARRE_H, 0.0);
        let (_, l_rail_etroit, _) = taille(etroit.rail);
        assert_eq!(l_rail_etroit, RAIL_PLIE);

        // 1300 px : le rail deplie laisse encore 1008 px au site, il peut rester ouvert.
        let large = geometrie(1300, 800, 1.0, false, BARRE_H, BARRE_H, 0.0);
        let (_, l_rail_large, _) = taille(large.rail);
        assert_eq!(l_rail_large, RAIL_DEPLIE);
    }

    #[test]
    fn le_menu_agrandit_la_barre_par_dessus_la_page_sans_la_pousser() {
        // Menu ouvert : la barre monte a 400 px, mais la page reste ou elle est (sous la barre
        // de base) ; c'est la region de la barre qui decide de ce qui la recouvre.
        let g = geometrie(1600, 1000, 1.0, false, 400.0, BARRE_H + EXT_H, 0.0);
        let (_, _, hauteur) = taille(g.barre);
        assert_eq!(hauteur, 400.0);
        let (pos_contenu, _, h_contenu) = taille(g.contenu);
        assert_eq!(pos_contenu, format!("292,{}", BARRE_H + EXT_H), "la page ne bouge pas");
        assert_eq!(h_contenu, 1000.0 - BARRE_H - EXT_H);
    }

    #[test]
    fn le_panneau_droit_se_pose_sur_la_page_sans_la_retrecir() {
        let g = geometrie(1600, 1000, 1.0, false, BARRE_H, BARRE_H, ZAP_LARGEUR);
        let (pos_zap, l_zap, h_zap) = taille(g.zap);
        assert_eq!(l_zap, 330.0);
        assert_eq!(pos_zap, format!("{},46", 1600.0 - 330.0), "le panneau est colle a droite");
        assert_eq!(h_zap, 954.0, "il occupe la hauteur du contenu");

        let (_, l_contenu, _) = taille(g.contenu);
        assert_eq!(l_contenu, 1600.0 - 292.0, "la page garde toute sa largeur, dessous");
    }

    #[test]
    fn le_panneau_droit_ne_mange_jamais_plus_de_40_pour_cent() {
        // 700 px : 40 % = 280, donc la largeur demandee (330) est ramenee a 280.
        let g = geometrie(700, 600, 1.0, false, BARRE_H, BARRE_H, ZAP_LARGEUR);
        let (_, l_zap, _) = taille(g.zap);
        assert_eq!(l_zap, 280.0, "40 % de 700");
        let (_, l_contenu, _) = taille(g.contenu);
        assert!(l_contenu > 100.0, "il reste de la place pour le site");
    }

    /// Invariant fort : quelles que soient la taille, l'echelle et les panneaux, les quatre
    /// zones ne se recouvrent jamais, ne sortent pas de la fenetre et restent positives.
    #[test]
    fn aucun_recouvrement_pour_une_grille_de_tailles() {
        let tailles = [
            (320u32, 240u32),
            (880, 560),   // taille minimale de la fenetre
            (1024, 768),
            (1440, 900),
            (1920, 1080),
            (2560, 1440),
            (3840, 1600),
            (1200, 700),  // juste au seuil de repli du rail
            (1199, 700),
        ];
        let echelles = [1.0, 1.25, 1.5, 2.0];
        for (l, h) in tailles {
            for e in echelles {
                for rail in [false, true] {
                    for (barre, zap) in [(BARRE_H, 0.0), (BARRE_H + EXT_H, 0.0), (420.0, ZAP_LARGEUR)] {
                        let g = geometrie(l, h, e, rail, barre, BARRE_H + EXT_H, zap);
                        let (rx, ry, rl, rh) = rectangle(g.rail);
                        let (bx, by, bl, bh) = rectangle(g.barre);
                        let (cx, cy, cl, ch) = rectangle(g.contenu);
                        let (zx, zy, zl, zh) = rectangle(g.zap);
                        let cas = format!("{l}x{h} @{e} rail={rail} barre={barre} zap={zap}");

                        for (nom, w, hh) in [("rail", rl, rh), ("barre", bl, bh), ("contenu", cl, ch), ("zap", zl, zh)] {
                            assert!(w >= 1.0 && hh >= 1.0, "{nom} degenere : {cas}");
                        }
                        // Chaque zone reste dans la fenetre.
                        for (nom, x, y, w, hh) in [
                            ("rail", rx, ry, rl, rh),
                            ("barre", bx, by, bl, bh),
                            ("contenu", cx, cy, cl, ch),
                            ("zap", zx, zy, zl, zh),
                        ] {
                            let boite = format!("{nom}=({x},{y},{w}x{hh}) fenetre={l}x{h}");
                            assert!(x >= -0.5 && y >= -0.5, "{boite} sort a gauche/haut : {cas}");
                            assert!(
                                x + w <= l as f64 + 0.5,
                                "{boite} sort a droite : {cas}"
                            );
                            assert!(
                                y + hh <= h as f64 + 0.5,
                                "{boite} sort en bas : {cas}"
                            );
                        }
                        // Le rail et la barre ne se recouvrent jamais.
                        assert!(rx + rl <= bx + 0.5, "rail/barre se recouvrent : {cas}");
                        // Le contenu ne remonte jamais dans la ligne du haut ; le panneau de droite
                        // se pose sur lui (superposition voulue), sans en deborder.
                        assert!(cy + 0.5 >= (BARRE_H * e).round(), "contenu dans la ligne du haut : {cas}");
                        assert!(cy <= by + bh + 0.5, "trou entre la barre et la page : {cas}");
                        assert!(zy + 0.5 >= cy && zx + zl <= cx + cl + 0.5, "panneau hors du contenu : {cas}");
                        let _ = bx;
                    }
                }
            }
        }
    }

    #[test]
    fn une_echelle_absurde_ne_desorganise_rien() {
        // Un facteur a 0 (ou negatif) vient d'une API, pas d'un reglage : on retombe sur 1.
        let g = geometrie(1440, 900, 0.0, false, BARRE_H, BARRE_H, 0.0);
        let (_, l_rail, _) = taille(g.rail);
        assert_eq!(l_rail, RAIL_DEPLIE);
    }

    #[test]
    fn le_plafond_de_barre_laisse_toujours_du_contenu() {
        // Une barre demandee plus haute que la fenetre est ramenee a 72 % : le menu ne couvre
        // jamais toute la page, et la page, dessous, garde sa hauteur.
        let g = geometrie(1440, 600, 1.0, false, 5000.0, BARRE_H, 0.0);
        let (_, _, h_barre) = taille(g.barre);
        let (_, _, h_contenu) = taille(g.contenu);
        assert_eq!(h_barre, 600.0 * 0.72);
        assert_eq!(h_contenu, 600.0 - BARRE_H, "la page ne retrecit pas sous le menu");
    }

    #[test]
    fn une_fenetre_reduite_ne_fait_pas_paniquer_la_geometrie() {
        // Mesure du 2026-11-22 : reduite dans la barre des taches, la fenetre ne fait plus
        // 19 px de haut. Le plafond de la barre (72 % = 13,68 px) tombait alors sous son
        // minimum (46 px) et `f64::clamp` paniquait — l'application mourait au reduire.
        for (l, h) in [(1000, 19), (1000, 0), (0, 0), (1, 1), (19, 1000)] {
            for echelle in [1.0, 1.25, 1.5, 2.0] {
                let g = geometrie(l, h, echelle, false, BARRE_H, BARRE_H, ZAP_LARGEUR);
                let (_, _, h_barre) = taille(g.barre);
                let (_, _, h_contenu) = taille(g.contenu);
                assert!(
                    h_barre + 0.5 >= (BARRE_H * echelle).round(),
                    "la barre perd sa hauteur minimale en {l}x{h} a {echelle}"
                );
                assert!(h_contenu >= 1.0, "le contenu disparait en {l}x{h} a {echelle}");
            }
        }
    }
}