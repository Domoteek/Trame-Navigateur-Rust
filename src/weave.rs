//! Tissage : comment un onglet rejoint un fil.
//!
//! Trois signaux, aucun n'est suffisant seul :
//!   - la provenance (onglet parent) : c'est ce que fait Opera avec ses « ilots » ;
//!   - le domaine enregistrable : le signal le plus grossier, mais il evite les fils
//!     incoherents quand le parent est inconnu ;
//!   - la proximite lexicale des titres : ce qui distingue une *tache* d'un *site*,
//!     conformement a ce que montre Tabs.do (UIST 2021) — regrouper par tache, pas par domaine.
//!
//! On n'effectue jamais de fusion autoritaire : on emet une *suggestion*. Le
//! rearrangement automatique entretient le desordre (Ma & al., CHI 2023).

use std::collections::{HashMap, HashSet};

use crate::model::{Fil, Onglet, ThreadId};

/// Seuil au-dessus duquel un onglet rejoint un fil existant.
pub const SEUIL_TISSAGE: f32 = 0.42;
/// Seuil au-dessus duquel on propose de tisser deux fils ensemble.
pub const SEUIL_FUSION: f32 = 0.52;

const MOTS_VIDES: [&str; 34] = [
    "les", "des", "une", "and", "the", "for", "avec", "pour", "dans", "sur", "pas", "que",
    "qui", "quoi", "est", "son", "ses", "leur", "vous", "nous", "elle", "ils", "elles", "aux",
    "par", "plus", "sans", "sous", "chez", "tout", "tous", "toute", "com", "www",
];

/// Mots significatifs d'un titre.
pub fn mots(texte: &str) -> Vec<String> {
    let mut vu = HashSet::new();
    let mut sortie = Vec::new();
    let mut courant = String::new();
    let pousser = |mot: &mut String, sortie: &mut Vec<String>, vu: &mut HashSet<String>| {
        if mot.chars().count() >= 3 && !MOTS_VIDES.contains(&mot.as_str()) && vu.insert(mot.clone()) {
            sortie.push(mot.clone());
        }
        mot.clear();
    };
    for c in texte.chars() {
        if c.is_alphanumeric() {
            for b in c.to_lowercase() {
                courant.push(b);
            }
        } else {
            pousser(&mut courant, &mut sortie, &mut vu);
        }
    }
    pousser(&mut courant, &mut sortie, &mut vu);
    sortie
}

/// Indice de Jaccard entre deux ensembles de mots.
pub fn jaccard(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: HashSet<&String> = a.iter().collect();
    let sb: HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Mots portes par un fil (union des titres de ses onglets, plafonnee).
pub fn mots_du_fil(fil: ThreadId, onglets: &[Onglet]) -> Vec<String> {
    let mut vu = HashSet::new();
    let mut sortie = Vec::new();
    for o in onglets.iter().filter(|o| o.fil == fil) {
        for m in mots(&o.titre) {
            if vu.insert(m.clone()) {
                sortie.push(m);
            }
        }
    }
    sortie
}

/// Domaine dominant d'un fil.
pub fn domaine_du_fil(fil: ThreadId, onglets: &[Onglet]) -> String {
    let mut compte: HashMap<String, usize> = HashMap::new();
    for o in onglets.iter().filter(|o| o.fil == fil) {
        let d = o.domaine();
        if !d.is_empty() {
            *compte.entry(d).or_insert(0) += 1;
        }
    }
    compte
        .into_iter()
        .max_by_key(|(_, n)| *n)
        .map(|(d, _)| d)
        .unwrap_or_default()
}

/// Evaluation d'un rattachement : (fil, score).
pub fn evaluer(onglet: &Onglet, onglets: &[Onglet], fils: &[Fil], now: u64) -> Option<(ThreadId, f32)> {
    let mots_onglet = mots(&onglet.titre);
    let domaine_onglet = onglet.domaine();
    let mut meilleur: Option<(ThreadId, f32)> = None;

    for fil in fils {
        let membres: Vec<&Onglet> = onglets.iter().filter(|o| o.fil == fil.id).collect();
        if membres.is_empty() {
            continue;
        }

        // 1. Provenance : un onglet ouvert depuis un parent rejoit son fil.
        let lien_direct = onglet
            .parent
            .map(|p| membres.iter().any(|o| o.id == p))
            .unwrap_or(false);

        // 2. Domaine : proportion de membres partageant le domaine.
        let part_domaine = if domaine_onglet.is_empty() || membres.is_empty() {
            0.0
        } else {
            let n = membres.iter().filter(|o| o.domaine() == domaine_onglet).count();
            n as f32 / membres.len() as f32
        };

        // 3. Lexical : proximite des titres.
        let lexical = jaccard(&mots_onglet, &mots_du_fil(fil.id, onglets));

        // 4. Fraicheur du fil : on rejoint plus volontiers ce qui vit.
        let age_ms = now.saturating_sub(fil.actif_le) as f32;
        let fraicheur = (-age_ms / (30.0 * 60_000.0)).exp();

        let score = if lien_direct {
            // La provenance est le signal le plus fiable (cf. ilots d'Opera) : il domine
            // mais ne suffit pas a lui seul a un score parfait.
            0.78 + 0.12 * part_domaine + 0.10 * fraicheur
        } else {
            0.22 * part_domaine + 0.34 * lexical + 0.14 * fraicheur + 0.10
        };

        if meilleur.map(|(_, s)| score > s).unwrap_or(true) {
            meilleur = Some((fil.id, score));
        }
    }
    meilleur
}

/// Mots significatifs avec leur casse d'origine : `(mot, commence par une majuscule)`.
///
/// La casse est le seul indice de « nom propre » disponible sans dictionnaire, et sur un
/// titre de page il designe presque toujours la marque ou le sujet (AliExpress, France)
/// plutot qu'un mot de liaison (« achat », « livraison »).
fn mots_cases(texte: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut mot = String::new();
    let mut majuscule = false;
    let mut premier = true;

    let pousser = |mot: &mut String, majuscule: bool, out: &mut Vec<(String, bool)>| {
        if mot.chars().count() >= 3 && !MOTS_VIDES.contains(&mot.as_str()) {
            out.push((mot.clone(), majuscule));
        }
        mot.clear();
    };

    for c in texte.chars() {
        if c.is_alphanumeric() {
            if premier {
                majuscule = c.is_uppercase();
                premier = false;
            }
            for b in c.to_lowercase() {
                mot.push(b);
            }
        } else if !mot.is_empty() {
            pousser(&mut mot, majuscule, &mut out);
            premier = true;
        }
    }
    if !mot.is_empty() {
        pousser(&mut mot, majuscule, &mut out);
    }
    out
}

/// Nom automatique d'un fil a partir de ses onglets.
///
/// Les pages internes (`trame:`) sont ignorees : leur « domaine » est `localhost` et elles
/// nommeraient le fil « Localhost » alors que le contenu reel est ailleurs.
pub fn nommer(onglets_du_fil: &[&Onglet], onglets: &[Onglet]) -> String {
    // mot -> (occurrences, occurrences capitalisees)
    let mut compte: HashMap<String, (usize, usize)> = HashMap::new();
    for o in onglets_du_fil.iter().filter(|o| !o.interne()) {
        for (mot, majuscule) in mots_cases(&o.titre) {
            let entree = compte.entry(mot).or_insert((0, 0));
            entree.0 += 1;
            if majuscule {
                entree.1 += 1;
            }
        }
    }
    let mut mots: Vec<(String, usize, usize)> = compte
        .into_iter()
        .map(|(mot, (total, maj))| (mot, total, maj))
        .collect();
    // Priorite aux noms propres, puis a la frequence, puis a l'ordre alphabetique.
    mots.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then_with(|| a.0.cmp(&b.0)));
    if mots.is_empty() {
        // Repli sur le domaine, sinon un intitule neutre.
        for o in onglets_du_fil.iter().filter(|o| !o.interne()) {
            let d = o.domaine();
            if !d.is_empty() {
                return titre_propre(&d);
            }
        }
        // Fil ne contenant que des pages internes : on le nomme d'apres la page
        // (« Reglages », « La trame »), sinon « Fil ».
        for o in onglets_du_fil {
            if o.interne() {
                let nom = o.libelle();
                if !nom.trim().is_empty() && nom != "Page interne" {
                    return nom;
                }
            }
        }
        return "Fil".to_string();
    }
    let mut choisis: Vec<String> = Vec::new();
    for (m, _, _) in mots.iter().take(3) {
        choisis.push(titre_propre(m));
        if choisis.len() >= 2 {
            break;
        }
    }
    let _ = onglets;
    choisis.join(" ")
}

fn titre_propre(mot: &str) -> String {
    let mut c = mot.chars();
    match c.next() {
        Some(premier) => premier.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Deux fils qui se ressemblent, a suggerer a l'utilisateur (jamais appliques d'office).
pub fn suggestion_fusion(fils: &[Fil], onglets: &[Onglet]) -> Option<(ThreadId, ThreadId, f32)> {
    let mut meilleur: Option<(ThreadId, ThreadId, f32)> = None;
    for (i, a) in fils.iter().enumerate() {
        for b in fils.iter().skip(i + 1) {
            let ma = mots_du_fil(a.id, onglets);
            let mb = mots_du_fil(b.id, onglets);
            if ma.is_empty() || mb.is_empty() {
                continue;
            }
            let lexical = jaccard(&ma, &mb);
            let meme_domaine = {
                let da = domaine_du_fil(a.id, onglets);
                let db = domaine_du_fil(b.id, onglets);
                !da.is_empty() && da == db
            };
            let score = lexical + if meme_domaine { 0.18 } else { 0.0 };
            if score >= SEUIL_FUSION && meilleur.map(|(_, _, s)| score > s).unwrap_or(true) {
                meilleur = Some((a.id, b.id, score));
            }
        }
    }
    meilleur
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Declencheur;

    fn onglet(id: u32, titre: &str, url: &str, fil: u32, now: u64) -> Onglet {
        let mut o = Onglet::neuf(id, url.to_string(), fil, Declencheur::Saisie, now);
        o.titre = titre.to_string();
        o
    }

    #[test]
    fn les_mots_ecartent_le_bruit() {
        let m = mots("Le guide des batteries au lithium pour les velos");
        assert!(m.contains(&"batteries".to_string()));
        assert!(!m.contains(&"les".to_string()));
    }

    #[test]
    fn un_lien_herite_du_fil_de_son_parent() {
        let now = 1_000_000;
        let parent = onglet(1, "Batteries lithium", "https://a.fr/x", 1, now);
        let mut enfant = onglet(2, "Anode silicium", "https://b.fr/y", 2, now);
        enfant.parent = Some(1);
        let onglets = vec![parent.clone()];
        let fils = vec![Fil::neuf(1, "Batteries".into(), now), Fil::neuf(2, "Autre".into(), now)];
        let (fil, score) = evaluer(&enfant, &onglets, &fils, now).unwrap();
        assert_eq!(fil, 1);
        assert!(score > SEUIL_TISSAGE);
    }

    #[test]
    fn la_proximite_lexicale_suffit_sans_parent() {
        let now = 1_000_000;
        let onglets = vec![onglet(1, "Batteries lithium anode", "https://a.fr/x", 1, now)];
        let fils = vec![Fil::neuf(1, "Batteries".into(), now)];
        let candidat = onglet(2, "Anode et batteries lithium", "https://z.org/q", 2, now);
        let (fil, score) = evaluer(&candidat, &onglets, &fils, now).unwrap();
        assert_eq!(fil, 1);
        assert!(score > SEUIL_TISSAGE, "score = {score}");
    }

    #[test]
    fn une_page_interne_ne_nomme_pas_le_fil() {
        let now = 1_000_000;
        let interne = onglet(1, "Nouvel onglet", "trame://localhost/newtab.html", 1, now);
        let vrai = onglet(2, "AliExpress France Livraison", "https://fr.aliexpress.com/", 1, now);
        let nom = nommer(&[&interne, &vrai], &[interne.clone(), vrai.clone()]);
        assert!(!nom.to_lowercase().contains("localhost"), "nom = {nom}");
    }

    #[test]
    fn un_nom_propre_passe_devant_un_mot_commun() {
        let now = 1_000_000;
        let a = onglet(
            1,
            "AliExpress France - Livraison gratuite, achat en ligne",
            "https://fr.aliexpress.com/",
            1,
            now,
        );
        let nom = nommer(&[&a], &[a.clone()]);
        assert!(
            nom.starts_with("Aliexpress") || nom.starts_with("AliExpress"),
            "nom = {nom}"
        );
        assert!(!nom.to_lowercase().starts_with("achat"), "nom = {nom}");
    }

    #[test]
    fn un_domaine_vraiment_different_ne_tisse_pas() {
        let now = 1_000_000;
        let onglets = vec![onglet(1, "Recette de pain", "https://cuisine.fr/x", 1, now)];
        let fils = vec![Fil::neuf(1, "Pain".into(), now)];
        let candidat = onglet(2, "Rapport financier annuel", "https://finance.com/q", 2, now);
        let r = evaluer(&candidat, &onglets, &fils, now).unwrap();
        assert!(r.1 < SEUIL_TISSAGE, "score = {}", r.1);
    }
}
