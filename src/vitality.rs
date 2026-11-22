//! Vitalite d'un onglet : un score d'attention continu, qui decroit et se recharge.
//!
//! Ce score est la piece centrale du projet : il remplace le minuteur des autres
//! navigateurs (Edge endort apres 2 h, Chrome defausse sous pression memoire) par une
//! estimation de *votre* attention restante sur l'onglet. Il ne depl ace jamais un onglet
//! et ne ferme jamais rien : il pilote seulement la mise en veille, qui est reversible.
//!
//! Fondement : le cout de reprise est reel (Monk & al. 2008 ; Mark & al. 2005), donc un
//! onglet qu'on n'a jamais regarde apres cinq minutes a tres peu de chances d'etre rouvert
//! spontanement. On le refroidit au lieu de le fermer.

use crate::model::Onglet;

/// Demi-vie normale, en millisecondes.
pub const DEMI_VIE_MS: f64 = 8.0 * 60_000.0;
/// Demi-vie d'un onglet mis de cote pour lecture ulterieure.
pub const DEMI_VIE_LECTURE_MS: f64 = 40.0 * 60_000.0;
/// Seuil sous lequel on endort (si le budget le permet).
pub const SEUIL_VEILLE: f32 = 0.16;
/// Un onglet jamais regarde devient plus froid plus vite apres ce delai.
pub const DELAI_ABANDON_MS: u64 = 5 * 60_000;

/// Applique la decroissance de `dt_ms` a un onglet.
pub fn decroitre(o: &mut Onglet, dt_ms: f64, now: u64) {
    if o.ancre {
        o.vitalite = 1.0;
        return;
    }
    let mut demi_vie = if o.a_lire {
        DEMI_VIE_LECTURE_MS
    } else {
        DEMI_VIE_MS
    };
    // Jamais regarde et deja ancien : on quitte le "patch" (analogie fourrage d'information).
    if o.nb_focus == 0 && now.saturating_sub(o.cree_le) > DELAI_ABANDON_MS {
        demi_vie /= 3.0;
    }
    if o.temps_actif_ms > 0 && now.saturating_sub(o.dernier_focus) < 2_000 {
        // Onglet sous les yeux : il ne se refroidit pas, il se recharge.
        o.vitalite = (o.vitalite + 0.004 * (dt_ms / 1000.0) as f32).min(1.0);
        return;
    }
    let facteur = 0.5f64.powf(dt_ms / demi_vie) as f32;
    o.vitalite = (o.vitalite * facteur).clamp(0.0, 1.0);
}

/// Recharge liee a un retour sur l'onglet.
pub fn sur_focus(o: &mut Onglet, now: u64) {
    if o.dernier_focus > 0 {
        o.temps_actif_ms = o.temps_actif_ms.saturating_add(now.saturating_sub(o.dernier_focus));
    }
    o.dernier_focus = now;
    o.nb_focus = o.nb_focus.saturating_add(1);
    let gain = 0.22 + 0.04 * ((1 + o.nb_focus) as f32).ln();
    o.vitalite = (o.vitalite + gain).min(1.0);
    if o.ancre {
        o.vitalite = 1.0;
    }
}

/// Classe lisible pour l'interface.
pub fn palier(v: f32) -> &'static str {
    if v >= 0.62 {
        "vif"
    } else if v >= 0.34 {
        "tiede"
    } else if v >= SEUIL_VEILLE {
        "froid"
    } else {
        "dormant"
    }
}

/// Un onglet doit-il etre endormi a cet instant ?
pub fn doit_dormir(o: &Onglet) -> bool {
    !o.ancre && o.eveille && o.vitalite < SEUIL_VEILLE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Declencheur;

    #[test]
    fn la_vitalite_decroit_et_se_recharge() {
        let mut o = Onglet::neuf(1, "https://a.fr".into(), 1, Declencheur::Saisie, 0);
        let depart = o.vitalite;
        decroitre(&mut o, DEMI_VIE_MS, 60_000);
        assert!(o.vitalite < depart, "doit decroitre");
        let refroidi = o.vitalite;
        sur_focus(&mut o, 90_000);
        assert!(o.vitalite > refroidi, "doit se recharger au retour");
    }

    #[test]
    fn un_onglet_ancre_ne_decroit_pas() {
        let mut o = Onglet::neuf(1, "https://a.fr".into(), 1, Declencheur::Saisie, 0);
        o.ancre = true;
        decroitre(&mut o, 10.0 * DEMI_VIE_MS, 900_000);
        assert_eq!(o.vitalite, 1.0);
    }

    #[test]
    fn un_onglet_a_lire_se_refroidit_plus_lentement() {
        let a = Onglet::neuf(1, "https://a.fr".into(), 1, Declencheur::Saisie, 0);
        let mut b = a.clone();
        b.a_lire = true;
        let mut a2 = a.clone();
        decroitre(&mut a2, 600_000.0, 600_000);
        decroitre(&mut b, 600_000.0, 600_000);
        assert!(b.vitalite > a2.vitalite);
    }
}
