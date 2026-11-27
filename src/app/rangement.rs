//! Messages du rangement des telechargements : rubrique « Téléchargements » des reglages et
//! bouton « Annuler » de la page des telechargements. La decision elle-meme vit dans
//! `crate::rangement` (module pur) ; elle est appliquee par `crate::telechargements`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use super::*;
use crate::rangement::Regle;

thread_local! {
    /// Dernier essai de la rubrique (« où irait ce fichier ? »), rendu dans l'etat.
    static ESSAI: RefCell<Value> = const { RefCell::new(Value::Null) };
}

/// Une liste de mots tapee par l'utilisateur (`pronote.fr, ameli.fr`) ou deja en tableau.
fn liste(v: &Value) -> Vec<String> {
    let brut: Vec<String> = match v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Value::String(s) => s.split([',', ';', ' ']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let mut vus = Vec::new();
    for m in brut.into_iter().map(|m| m.trim().to_string()).filter(|m| !m.is_empty()) {
        if !vus.contains(&m) {
            vus.push(m);
        }
    }
    vus.truncate(20);
    vus
}

fn texte(v: &Value, max: usize) -> String {
    v.as_str().unwrap_or("").trim().chars().take(max).collect()
}

impl App {
    /// Le reglage, ce que Trame a deja range (par sous-dossier) et le dernier essai.
    pub(super) fn etat_rangement(&self) -> Value {
        let mut compteurs: BTreeMap<String, u64> = BTreeMap::new();
        for t in self.telechargements.iter().filter(|t| t.etat == "termine" && !t.range.is_empty()) {
            *compteurs.entry(t.range.clone()).or_default() += 1;
        }
        let r = &self.reglages.rangement;
        json!({
            "actif": r.actif,
            "parMois": r.par_mois,
            "categories": r.categories.iter().map(|c| json!({
                "id": c.id, "nom": c.nom, "dossier": c.dossier, "extensions": c.extensions, "actif": c.actif,
            })).collect::<Vec<_>>(),
            "regles": r.regles.iter().map(|g| json!({
                "id": g.id, "actif": g.actif, "sites": g.sites, "types": g.types,
                "nomContient": g.nom_contient, "dossier": g.dossier, "renommer": g.renommer,
            })).collect::<Vec<_>>(),
            "compteurs": compteurs,
            "racine": self.dossier_dl.borrow().display().to_string(),
            "essai": ESSAI.with(|e| e.borrow().clone()),
        })
    }

    fn rangement_change(&mut self, message: Option<&str>) {
        self.profil.sauver_reglages(&self.reglages);
        crate::telechargements::regler_rangement(&self.reglages.rangement);
        if let Some(m) = message {
            self.signaler(m);
        }
        self.pousser_etat();
    }

    /// `true` = message traite ici.
    pub(super) fn traiter_ipc_rangement(&mut self, m: &str, v: &Value) -> bool {
        match m {
            "rangementActif" => {
                let actif = v["actif"].as_bool().unwrap_or(false);
                self.reglages.rangement.actif = actif;
                info!("rangement : {}", if actif { "allume" } else { "coupe" });
                self.rangement_change(Some(if actif {
                    "Les prochains téléchargements seront rangés."
                } else {
                    "Rangement coupé : tout arrive à la racine du dossier."
                }));
            }
            "rangementParMois" => {
                self.reglages.rangement.par_mois = v["actif"].as_bool().unwrap_or(false);
                self.rangement_change(None);
            }
            "rangementCategorie" => {
                let id = v["id"].as_str().unwrap_or("");
                let Some(c) = self.reglages.rangement.categories.iter_mut().find(|c| c.id == id) else { return true };
                if let Some(a) = v["actif"].as_bool() {
                    c.actif = a;
                }
                if v.get("dossier").is_some() {
                    let d = texte(&v["dossier"], 120);
                    let sur = crate::rangement::chemin_sur(&d);
                    c.dossier = if sur.as_os_str().is_empty() { c.nom.clone() } else { sur.to_string_lossy().to_string() };
                }
                if v.get("extensions").is_some() {
                    c.extensions = liste(&v["extensions"])
                        .into_iter()
                        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
                        .filter(|e| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()))
                        .collect();
                }
                self.rangement_change(None);
            }
            "rangementCategoriesDefaut" => {
                self.reglages.rangement.categories = crate::rangement::categories_defaut();
                self.rangement_change(Some("Types de fichiers remis comme au départ."));
            }
            "rangementRegle" => {
                let g = &v["regle"];
                let id = g["id"].as_u64().filter(|i| *i > 0).unwrap_or_else(maintenant_ms);
                let regle = Regle {
                    id,
                    actif: g["actif"].as_bool().unwrap_or(true),
                    sites: liste(&g["sites"]),
                    types: liste(&g["types"]),
                    nom_contient: texte(&g["nomContient"], 80),
                    dossier: texte(&g["dossier"], 160),
                    renommer: texte(&g["renommer"], 120),
                };
                if regle.sites.is_empty() && regle.types.is_empty() && regle.nom_contient.is_empty() {
                    self.signaler("Une règle a besoin d'au moins une condition : un site, un type ou un morceau du nom.");
                    return true;
                }
                if crate::rangement::chemin_sur(&regle.dossier).as_os_str().is_empty() {
                    self.signaler("Indique le dossier où ranger (par exemple « École »).");
                    return true;
                }
                let regles = &mut self.reglages.rangement.regles;
                match regles.iter_mut().find(|r| r.id == id) {
                    Some(r) => *r = regle,
                    None => regles.push(regle),
                }
                self.rangement_change(None);
            }
            "rangementRegleActif" => {
                let id = v["id"].as_u64().unwrap_or(0);
                if let Some(r) = self.reglages.rangement.regles.iter_mut().find(|r| r.id == id) {
                    r.actif = v["actif"].as_bool().unwrap_or(true);
                }
                self.rangement_change(None);
            }
            "rangementRegleRetirer" => {
                let id = v["id"].as_u64().unwrap_or(0);
                self.reglages.rangement.regles.retain(|r| r.id != id);
                self.rangement_change(None);
            }
            "rangementReglesOrdre" => {
                // L'ordre complet, tel que la page le montre apres un glisser : les identifiants
                // inconnus sont ignores, les regles oubliees gardent leur place relative a la fin.
                let ids: Vec<u64> = v["ids"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
                let mut reste = std::mem::take(&mut self.reglages.rangement.regles);
                let mut ordre = Vec::new();
                for id in ids {
                    if let Some(i) = reste.iter().position(|r| r.id == id) {
                        ordre.push(reste.remove(i));
                    }
                }
                ordre.extend(reste);
                self.reglages.rangement.regles = ordre;
                self.rangement_change(None);
            }
            "rangementEssai" => {
                let nom = texte(&v["nom"], 200);
                let site = texte(&v["site"], 200);
                // L'essai montre ce que ferait le reglage ALLUME, meme s'il est coupe : c'est
                // justement avant de l'allumer qu'on veut voir.
                let mut r = self.reglages.rangement.clone();
                r.actif = true;
                let c = r.choisir(&nom, &site, "", crate::telechargements::jour_local());
                let mut chemin = c.sous_dossier.clone();
                chemin.push(&c.nom);
                ESSAI.with(|e| {
                    *e.borrow_mut() = json!({
                        "nom": nom, "site": site,
                        "chemin": chemin.to_string_lossy(),
                        "raison": c.raison,
                    })
                });
                self.pousser_etat();
            }
            "rangementOuvrir" => {
                let racine = self.dossier_dl.borrow().clone();
                let sous = crate::rangement::chemin_sur(&texte(&v["dossier"], 200));
                let dossier = racine.join(sous);
                let _ = std::fs::create_dir_all(&dossier);
                if !crate::externe::ouvrir_dossier(&dossier) {
                    self.signaler(&format!("Windows n'a pas pu ouvrir ce dossier : {}", dossier.display()));
                }
            }
            "telechargementRacine" => {
                let id = v["id"].as_u64().unwrap_or(0);
                let Some(t) = self.telechargements.iter().find(|t| t.id == id).cloned() else { return true };
                if t.actif() || t.range.is_empty() {
                    return true;
                }
                // La racine du rangement, c'est le dossier ou le fichier a ete range, pas le
                // dossier choisi aujourd'hui (il a pu changer depuis).
                let chemin = PathBuf::from(&t.chemin);
                let profondeur = crate::rangement::chemin_sur(&t.range).components().count();
                let racine = chemin.ancestors().nth(profondeur + 1).map(Path::to_path_buf);
                let Some(racine) = racine else { return true };
                match crate::telechargements::remettre_a_la_racine(&chemin, &racine) {
                    Ok(nouveau) => {
                        info!("rangement : {} remis a la racine", t.fichier);
                        if let Some(e) = self.telechargements.iter_mut().find(|e| e.id == id) {
                            e.chemin = nouveau.to_string_lossy().to_string();
                            if let Some(nom) = nouveau.file_name() {
                                e.fichier = nom.to_string_lossy().to_string();
                            }
                            e.range.clear();
                            e.range_raison.clear();
                        }
                        self.profil.sauver_telechargements(&self.telechargements);
                        self.signaler("Remis à la racine du dossier des téléchargements.");
                        self.pousser_etat();
                    }
                    Err(e) => self.signaler(&e),
                }
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listes_tapees() {
        assert_eq!(liste(&json!("pronote.fr, ameli.fr  ameli.fr")), vec!["pronote.fr", "ameli.fr"]);
        assert_eq!(liste(&json!(["pdf", " ", "pdf"])), vec!["pdf"]);
        assert!(liste(&Value::Null).is_empty());
    }
}
