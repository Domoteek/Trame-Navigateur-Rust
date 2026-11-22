//! La Couture : une seule saisie qui cherche partout et classe par attention.
//!
//! Le classement melange trois choses et c'est deliberé :
//!   - la pertinence textuelle (correspondance exacte, puis floue) ;
//!   - la vitalite de l'onglet (ce a quoi vous pensez maintenant) ;
//!   - la fraicheur (ce que vous venez de faire).
//! Une liste triee par simple pertinence textuelle renvoie souvent un onglet mort.

use crate::model::{EntreeHistorique, Fil, Onglet};

#[derive(Clone, Debug)]
pub struct Resultat {
    pub genre: String,
    pub id: String,
    pub titre: String,
    pub sous_titre: String,
    pub score: f32,
    /// Charge utile a renvoyer au coeur pour executer l'action.
    pub action: serde_json::Value,
}

/// Correspondance floue : 1.0 exact, puis prefixe, puis sous-chaine, puis sous-sequence.
pub fn pertinence(requete: &str, cible: &str) -> f32 {
    let q = requete.to_lowercase();
    let c = cible.to_lowercase();
    if q.is_empty() {
        return 0.0;
    }
    if c == q {
        return 1.0;
    }
    if c.starts_with(&q) {
        return 0.94;
    }
    if let Some(pos) = c.find(&q) {
        // Une correspondance en debut de mot vaut mieux qu'au milieu.
        let debut_mot = pos == 0 || !c.as_bytes()[pos - 1].is_ascii_alphanumeric();
        return if debut_mot { 0.86 } else { 0.74 };
    }
    // Sous-sequence : les caracteres apparaissent dans l'ordre.
    let carq: Vec<char> = q.chars().collect();
    let carc: Vec<char> = c.chars().collect();
    let mut i = 0usize;
    let mut premier = None;
    let mut dernier = 0usize;
    for (j, ch) in carc.iter().enumerate() {
        if i < carq.len() && *ch == carq[i] {
            if premier.is_none() {
                premier = Some(j);
            }
            dernier = j;
            i += 1;
        }
    }
    if i == carq.len() && !carq.is_empty() {
        let etendue = (dernier - premier.unwrap_or(0) + 1) as f32;
        let densite = carq.len() as f32 / etendue;
        0.30 + 0.34 * densite
    } else {
        0.0
    }
}

fn recence(ts: u64, now: u64) -> f32 {
    let age_min = now.saturating_sub(ts) as f32 / 60_000.0;
    (-age_min / 240.0).exp()
}

/// Classe onglets, fils et historique pour une requete.
pub fn classer(
    requete: &str,
    onglets: &[Onglet],
    fils: &[Fil],
    historique: &[EntreeHistorique],
    now: u64,
) -> Vec<Resultat> {
    let mut out: Vec<Resultat> = Vec::new();

    for o in onglets {
        let mut p = pertinence(requete, &o.titre).max(pertinence(requete, &o.url) * 0.92);
        if o.titre.is_empty() {
            p = p.max(pertinence(requete, &crate::urls::affichage_court(&o.url)) * 0.9);
        }
        if p <= 0.05 {
            continue;
        }
        let score = 0.62 * p + 0.26 * o.vitalite + 0.12 * recence(o.dernier_focus.max(o.cree_le), now);
        out.push(Resultat {
            genre: if o.eveille { "onglet".into() } else { "onglet-endormi".into() },
            id: o.id.to_string(),
            titre: o.libelle(),
            sous_titre: if o.eveille {
                format!("onglet · {}", o.domaine())
            } else {
                format!("en veille · {}", o.domaine())
            },
            score,
            action: serde_json::json!({"type": "onglet", "id": o.id}),
        });
    }

    for f in fils {
        let membres = onglets.iter().filter(|o| o.fil == f.id).count();
        let p = pertinence(requete, &f.nom);
        if p <= 0.05 {
            continue;
        }
        let vitalite_max = onglets
            .iter()
            .filter(|o| o.fil == f.id)
            .map(|o| o.vitalite)
            .fold(0.0f32, f32::max);
        let score = 0.66 * p + 0.24 * vitalite_max + 0.10 * recence(f.actif_le, now);
        out.push(Resultat {
            genre: "fil".into(),
            id: f.id.to_string(),
            titre: f.nom.clone(),
            sous_titre: format!("fil · {membres} onglet(s)"),
            score,
            action: serde_json::json!({"type": "fil", "id": f.id}),
        });
    }

    // Une adresse visitee cinq fois n'apparait qu'une fois (la visite la plus recente) : la
    // Couture montrait cinq lignes « Lithium — Wikipedia » identiques (constate le 2026-11-21).
    let mut vues: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for h in historique.iter().rev() {
        if !vues.insert(h.url.as_str()) {
            continue;
        }
        let p = pertinence(requete, &h.titre).max(pertinence(requete, &h.url) * 0.9);
        if p <= 0.35 {
            continue;
        }
        let score = 0.70 * p + 0.30 * recence(h.ts, now);
        out.push(Resultat {
            genre: "historique".into(),
            id: h.url.clone(),
            titre: if h.titre.is_empty() { h.url.clone() } else { h.titre.clone() },
            sous_titre: crate::urls::affichage_court(&h.url),
            score,
            action: serde_json::json!({"type": "historique", "url": h.url}),
        });
    }

    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(12);
    out
}

/// Actions proposees quand la requete commence par `>` (et au clic droit sur le logo du rail).
pub fn commandes(requete: &str) -> Vec<Resultat> {
    const TABLE: [(&str, &str, &str); 15] = [
        ("nouvel-onglet", "Nouvel onglet", "Ctrl+T"),
        ("fermer-onglet", "Fermer l'onglet", "Ctrl+W"),
        ("rouvrir", "Rouvrir le dernier onglet ferme", "Ctrl+Maj+T"),
        ("reglages", "Reglages : moteur de recherche, extensions, veille", "Ctrl+,"),
        ("zap", "Masquer des elements (pub, Shorts…)", ""),
        ("trame", "Voir la trame (vue tissee)", "Ctrl+Esp"),
        ("veille", "Endormir les onglets froids", ""),
        ("capsule", "Sceller le fil actif en capsule", ""),
        ("capsules", "Ouvrir le dossier des capsules", ""),
        ("extensions", "Recharger les extensions", ""),
        ("rail", "Replier / deplier le rail", "Ctrl+B"),
        ("devtools", "Inspecter l'onglet actif", "F12"),
        ("anchrer", "Ancrer / detacher l'onglet actif", ""),
        ("alire", "Marquer l'onglet actif a lire plus tard", ""),
        ("vider-historique", "Vider l'historique", ""),
    ];
    let nu = requete.trim_start_matches('>').trim();
    let mut out = Vec::new();
    for (id, nom, raccourci) in TABLE {
        let p = if nu.is_empty() { 0.5 } else { pertinence(nu, nom) };
        if p <= 0.05 {
            continue;
        }
        out.push(Resultat {
            genre: "commande".into(),
            id: id.to_string(),
            titre: nom.to_string(),
            sous_titre: raccourci.to_string(),
            score: p * 0.9,
            action: serde_json::json!({"type": "commande", "id": id}),
        });
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Declencheur;

    #[test]
    fn la_pertinence_ordonne_bien() {
        assert_eq!(pertinence("git", "git"), 1.0);
        assert!(pertinence("git", "github") > pertinence("git", "logiciel gitlab"));
        assert!(pertinence("gtb", "github") > 0.0);
        assert_eq!(pertinence("xyz", "github"), 0.0);
    }

    #[test]
    fn un_onglet_vif_passe_devant_un_onglet_mort() {
        let now = 10_000_000;
        let mut vif = Onglet::neuf(1, "Rust async".into(), 1, Declencheur::Saisie, now);
        vif.titre = "Rust async".into();
        vif.vitalite = 0.95;
        vif.dernier_focus = now;
        let mut mort = Onglet::neuf(2, "Rust async".into(), 1, Declencheur::Saisie, 0);
        mort.titre = "Rust async".into();
        mort.vitalite = 0.05;
        mort.dernier_focus = 0;
        let r = classer("rust async", &[mort, vif], &[], &[], now);
        assert_eq!(r[0].id, "1");
    }

    #[test]
    fn une_adresse_visitee_plusieurs_fois_n_apparait_qu_une_fois() {
        let now = 10_000_000;
        let visite = |ts| EntreeHistorique {
            url: "https://fr.wikipedia.org/wiki/Lithium".into(),
            titre: "Lithium — Wikipédia".into(),
            ts,
        };
        let autre = EntreeHistorique {
            url: "https://fr.wikipedia.org/wiki/Accumulateur_lithium-ion".into(),
            titre: "Accumulateur lithium-ion — Wikipédia".into(),
            ts: now - 50,
        };
        let historique = vec![visite(now - 500), visite(now - 300), autre, visite(now - 100)];
        let r = classer("lithium", &[], &[], &historique, now);
        let lithium: Vec<_> = r.iter().filter(|x| x.id.ends_with("/Lithium")).collect();
        assert_eq!(lithium.len(), 1, "une seule ligne par adresse");
        assert_eq!(r.iter().filter(|x| x.genre == "historique").count(), 2);
    }

    #[test]
    fn les_commandes_repondent_au_chevron() {
        let c = commandes(">trame");
        assert!(c.iter().any(|r| r.id == "trame"));
    }
}
