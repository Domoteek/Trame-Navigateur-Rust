//! L'aide de Trame vue de l'hote : la notice (`ui/notice.js`), relue a la compilation.
//!
//! Une seule source pour toute l'aide (demande de Clement, 2026-11-24, « les cinq » de
//! `docs/maquettes/aide.html`) :
//!  - la page Notice (`ui/aide.html`, F1) et son PDF la lisent directement ;
//!  - ici, l'hote s'en sert pour l'aide de la Couture (« écran en deux » → la fiche Côte à côte)
//!    et pour le texte des astuces.
//!
//! Le fichier est du JavaScript (`window.NOTICE = {…};`) pour que la page s'ouvre aussi en
//! `file://` (PDF fabrique par `outils/faire-notice.mjs`) ; ce qui suit le `=` est du JSON strict.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::search::Resultat;

const SOURCE: &str = include_str!("../ui/notice.js");

#[derive(Debug, Deserialize)]
pub struct Notice {
    pub chapitres: Vec<Chapitre>,
    pub astuces: Vec<Astuce>,
}

#[derive(Debug, Deserialize)]
pub struct Chapitre {
    pub id: String,
    pub titre: String,
    pub chapeau: String,
    /// Reponse courte, montree dans la Couture ; a defaut, le chapeau.
    #[serde(default)]
    pub resume: String,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub mots: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Action {
    pub libelle: String,
    #[serde(default)]
    pub commande: String,
    /// Ouvre une fiche de la notice au lieu d'une commande.
    #[serde(default)]
    pub chapitre: String,
}

#[derive(Debug, Deserialize)]
pub struct Astuce {
    pub id: String,
    pub texte: String,
    pub faire: Action,
    /// Fiche de la Notice qui va avec l'astuce (lue par la page, verifiee par le test).
    #[allow(dead_code)]
    pub chapitre: String,
}

fn lire(source: &str) -> Result<Notice, String> {
    let debut = source.find('{').ok_or("pas d'objet dans notice.js")?;
    let fin = source.rfind('}').ok_or("objet non ferme dans notice.js")?;
    serde_json::from_str(&source[debut..=fin]).map_err(|e| format!("notice.js : {e}"))
}

/// La notice embarquee. Illisible = notice vide (et une alerte) : l'aide ne doit jamais
/// empecher Trame de demarrer. Le test `la_notice_se_lit` garde ce cas hors des versions.
pub fn notice() -> &'static Notice {
    static NOTICE: OnceLock<Notice> = OnceLock::new();
    NOTICE.get_or_init(|| {
        lire(SOURCE).unwrap_or_else(|e| {
            crate::alerte!("aide : {e}");
            Notice { chapitres: Vec::new(), astuces: Vec::new() }
        })
    })
}

pub fn chapitre(id: &str) -> Option<&'static Chapitre> {
    notice().chapitres.iter().find(|c| c.id == id)
}

pub fn astuce(id: &str) -> Option<&'static Astuce> {
    notice().astuces.iter().find(|a| a.id == id)
}

/// Minuscules sans accents : « Écran » et « ecran » se valent.
pub fn plat(texte: &str) -> String {
    texte
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            '’' => '\'',
            autre => autre,
        })
        .collect()
}

/// Mots qui ne disent rien du sujet : « comment je fais pour… », et le nom du logiciel.
const VIDES: [&str; 30] = [
    "comment", "faire", "fait", "fais", "pour", "les", "des", "une", "un", "le", "la", "de", "du",
    "je", "on", "en", "et", "ou", "mon", "ma", "mes", "est", "que", "qui", "avec", "dans", "sur",
    "peut", "trame", "navigateur",
];

/// La requete ressemble-t-elle a une question ? Alors l'aide passe en tete de la Couture.
pub fn est_question(requete: &str) -> bool {
    let q = plat(requete.trim());
    q.ends_with('?')
        || ["comment", "aide", "ou est", "c'est quoi", "a quoi", "pourquoi", "je veux", "how"]
            .iter()
            .any(|d| q.starts_with(d))
        || q.split_whitespace().count() >= 3
}

/// Pertinence d'une fiche pour une requete (0 = rien a voir).
///
/// Un « mot » de la fiche peut etre une expression (« écran en deux ») : elle compte si elle
/// figure dans la requete ; un mot de la requete compte s'il commence un mot de la fiche
/// (« compar » trouve « comparer »).
fn pertinence(requete: &str, c: &Chapitre) -> f32 {
    let q = plat(requete);
    let q = q.trim().trim_end_matches('?').trim();
    if q.chars().count() < 3 {
        return 0.0;
    }
    let mut score = 0.0f32;
    let mots: Vec<String> = c.mots.iter().map(|m| plat(m)).collect();
    // Une expression entiere de la fiche, presente dans la requete.
    for m in &mots {
        if m.contains(' ') && q.contains(m.as_str()) {
            score = score.max(1.0);
        }
    }
    // Mot a mot.
    let utiles: Vec<&str> = q
        .split(|ch: char| !ch.is_alphanumeric() && ch != '\'')
        .filter(|w| w.chars().count() >= 3 && !VIDES.contains(w))
        .collect();
    if utiles.is_empty() {
        return score;
    }
    let titre = plat(&c.titre);
    // Un mot trouve tel quel vaut 1 ; un mot qui ne fait que commencer un mot de la fiche
    // (« compar » → « comparer ») vaut 0,75 : « bloqué » doit aller au Dépannage (« bloqué »)
    // plutot qu'au Zappeur (« bloquer »).
    let mut trouves = 0.0f32;
    for w in &utiles {
        let pieces = || mots.iter().flat_map(|m| m.split(' ')).chain(titre.split(|ch: char| !ch.is_alphanumeric()));
        if pieces().any(|piece| piece == *w) {
            trouves += 1.0;
        } else if w.len() >= 4 && mots.iter().flat_map(|m| m.split(' ')).any(|piece| piece.starts_with(w)) {
            trouves += 0.75;
        }
    }
    score.max(0.85 * trouves / utiles.len() as f32)
}

/// Les fiches qui repondent a une saisie de la Couture, la meilleure d'abord (deux au plus).
pub fn resultats_couture(requete: &str) -> Vec<Resultat> {
    let mut trouves: Vec<(&Chapitre, f32)> = notice()
        .chapitres
        .iter()
        .map(|c| (c, pertinence(requete, c)))
        .filter(|(_, p)| *p >= 0.5)
        .collect();
    trouves.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    trouves.truncate(2);
    trouves
        .into_iter()
        .map(|(c, p)| {
            let faire = c.actions.iter().find(|a| !a.commande.is_empty());
            Resultat {
                genre: "aide".into(),
                id: format!("aide:{}", c.id),
                titre: c.titre.clone(),
                sous_titre: if c.resume.is_empty() { c.chapeau.clone() } else { c.resume.clone() },
                score: p,
                action: serde_json::json!({
                    "type": "aide",
                    "chapitre": c.id,
                    "commande": faire.map(|a| a.commande.clone()).unwrap_or_default(),
                    "faire": faire.map(|a| a.libelle.clone()).unwrap_or_default(),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_notice_se_lit() {
        let n = lire(SOURCE).expect("ui/notice.js doit etre du JSON strict apres le =");
        assert!(n.chapitres.len() >= 10);
        for c in &n.chapitres {
            assert!(!c.titre.is_empty() && !c.chapeau.is_empty(), "fiche {} incomplete", c.id);
        }
        let ids: Vec<&str> = n.chapitres.iter().map(|c| c.id.as_str()).collect();
        for a in &n.astuces {
            assert!(ids.contains(&a.chapitre.as_str()), "astuce {} : fiche {} absente", a.id, a.chapitre);
        }
    }

    #[test]
    fn la_couture_comprend_les_questions_de_clement() {
        for q in ["split écran", "comment split les ecrans", "écran en deux", "comparer deux pages", "diviser l'écran", "revenir à une seule page", "comment quitter le côte à côte"] {
            let r = resultats_couture(q);
            assert_eq!(r.first().map(|x| x.id.as_str()), Some("aide:cote-a-cote"), "{q}");
        }
        for q in ["sortir la vidéo", "vidéo dans une fenêtre", "picture in picture"] {
            let r = resultats_couture(q);
            assert_eq!(r.first().map(|x| x.id.as_str()), Some("aide:video"), "{q}");
        }
        assert_eq!(resultats_couture("raccourcis clavier")[0].id, "aide:raccourcis");
        for q in ["trame ne marche pas", "réparer trame", "trame est bloqué"] {
            assert_eq!(resultats_couture(q).first().map(|x| x.id.as_str()), Some("aide:depannage"), "{q}");
        }
    }

    #[test]
    fn une_adresse_ne_declenche_pas_l_aide() {
        for q in ["leboncoin.fr", "amazon", "meteo lyon", "yt"] {
            assert!(resultats_couture(q).is_empty(), "{q}");
        }
    }

    #[test]
    fn les_questions_sont_reconnues() {
        assert!(est_question("comment sortir la vidéo"));
        assert!(est_question("écran partagé ?"));
        assert!(!est_question("youtube"));
    }
}
