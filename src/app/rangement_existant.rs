//! Suite du rangement des telechargements (`app/rangement.rs`) :
//!   - **doublons** : a la fin d'un telechargement, un fichier identique deja present est
//!     cherche hors du fil principal (`crate::doublons`) ; la page propose de garder les deux ou
//!     de mettre le nouveau a la Corbeille ;
//!   - **ranger l'existant** : un apercu de ce que deviendrait la racine du dossier, puis le
//!     rangement sur « Valider », note dans `rangement-journal.json` pour etre defait pendant
//!     30 jours.
//!
//! L'existant n'est jamais renomme (seul un telechargement neuf l'est, par une regle) et il est
//! range au mois de sa date de modification, pas a celui d'aujourd'hui.

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::*;

const JOURNAL: &str = "rangement-journal.json";
/// Un rangement se defait pendant 30 jours ; au-dela, il sort du journal.
const GARDE_MS: u64 = 30 * 24 * 3600 * 1000;
/// Au-dela, l'apercu s'arrete (une racine de 20 000 fichiers se range en plusieurs fois).
const MAX_FICHIERS: usize = 5000;

/// Un rangement fait, pour pouvoir le defaire : chaque deplacement `(de, vers)`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Operation {
    date: u64,
    mouvements: Vec<(String, String)>,
}

thread_local! {
    /// Le plan montre par l'apercu : (fichier a la racine, sous-dossier ou il ira).
    static PLAN: RefCell<Vec<(PathBuf, PathBuf)>> = const { RefCell::new(Vec::new()) };
    /// L'apercu tel que la rubrique l'affiche.
    static APERCU: RefCell<Value> = const { RefCell::new(Value::Null) };
    /// Le journal, lu une fois puis tenu en memoire.
    static JOURNAL_LU: RefCell<Option<Vec<Operation>>> = const { RefCell::new(None) };
}

/// Fichiers a ne jamais toucher : caches, systeme, temporaires, telechargements en cours.
fn a_laisser(chemin: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    let nom = chemin.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if nom.starts_with('.') || nom.starts_with("~$") || nom == "desktop.ini" || nom == "thumbs.db" {
        return true;
    }
    let ext = crate::rangement::extension(&nom);
    if matches!(ext.as_str(), "crdownload" | "part" | "partial" | "tmp" | "download" | "opdownload") {
        return true;
    }
    // FILE_ATTRIBUTE_HIDDEN (2) | FILE_ATTRIBUTE_SYSTEM (4)
    std::fs::metadata(chemin).map(|m| m.file_attributes() & 0x6 != 0).unwrap_or(true)
}

impl App {
    fn chemin_journal(&self) -> PathBuf {
        self.profil.racine().join(JOURNAL)
    }

    fn journal(&self) -> Vec<Operation> {
        let chemin = self.chemin_journal();
        JOURNAL_LU.with(|j| {
            j.borrow_mut()
                .get_or_insert_with(|| {
                    std::fs::read_to_string(&chemin)
                        .ok()
                        .and_then(|t| serde_json::from_str::<Vec<Operation>>(t.trim_start_matches('\u{feff}')).ok())
                        .unwrap_or_default()
                })
                .clone()
        })
    }

    fn ecrire_journal(&self, mut ops: Vec<Operation>) {
        let limite = maintenant_ms().saturating_sub(GARDE_MS);
        ops.retain(|o| o.date >= limite);
        let debut = ops.len().saturating_sub(10);
        ops.drain(..debut);
        let texte = serde_json::to_string_pretty(&ops).unwrap_or_else(|_| "[]".into());
        if let Err(e) = std::fs::write(self.chemin_journal(), texte) {
            alerte!("rangement : journal non ecrit ({e})");
        }
        JOURNAL_LU.with(|j| *j.borrow_mut() = Some(ops));
    }

    /// Apercu en cours et dernier rangement qu'on peut encore defaire.
    pub(super) fn etat_existant(&self) -> Value {
        let limite = maintenant_ms().saturating_sub(GARDE_MS);
        let dernier = self.journal().into_iter().rev().find(|o| o.date >= limite);
        json!({
            "apercu": APERCU.with(|a| a.borrow().clone()),
            "dernier": dernier.map(|o| json!({ "date": o.date, "n": o.mouvements.len() })),
        })
    }

    /// Lance, hors du fil principal, la recherche d'un jumeau du telechargement `id` qui vient
    /// de finir. Le resultat revient par `UserEvent::Doublon`.
    pub(super) fn chercher_doublon(&self, id: u64) {
        if !self.reglages.rangement.doublons {
            return;
        }
        let Some(t) = self.telechargements.iter().find(|t| t.id == id) else { return };
        let fichier = PathBuf::from(&t.chemin);
        let connus: Vec<PathBuf> = self
            .telechargements
            .iter()
            .filter(|x| x.id != id && x.etat == "termine")
            .map(|x| PathBuf::from(&x.chemin))
            .collect();
        let proxy = self.proxy.clone();
        let lance = std::thread::Builder::new().name("doublons".into()).spawn(move || {
            if let Some(de) = crate::doublons::chercher(&fichier, &connus) {
                let _ = proxy.send_event(UserEvent::Doublon { id, de: de.to_string_lossy().to_string() });
            }
        });
        if let Err(e) = lance {
            alerte!("doublons : fil non lance ({e})");
        }
    }

    pub(super) fn doublon_trouve(&mut self, id: u64, de: String) {
        let Some(t) = self.telechargements.iter_mut().find(|t| t.id == id) else { return };
        info!("doublons : {} identique a {de}", t.fichier);
        t.doublon = de;
        self.profil.sauver_telechargements(&self.telechargements);
        self.pousser_etat();
    }

    /// Le plan de rangement de la racine, sans rien deplacer.
    fn apercu_existant(&mut self) {
        let racine = self.dossier_dl.borrow().clone();
        // Ce que ferait le reglage ALLUME : on regarde justement avant de l'allumer.
        let mut reglage = self.reglages.rangement.clone();
        reglage.actif = true;
        let en_cours: Vec<&str> = self.telechargements.iter().filter(|t| t.actif()).map(|t| t.chemin.as_str()).collect();
        let mut plan: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut restent = 0usize;
        let mut coupe = false;
        let entrees = match std::fs::read_dir(&racine) {
            Ok(e) => e,
            Err(e) => {
                self.signaler(&format!("Dossier des téléchargements illisible : {e}"));
                return;
            }
        };
        for entree in entrees.filter_map(Result::ok) {
            let chemin = entree.path();
            let Ok(meta) = entree.metadata() else { continue };
            if !meta.is_file() || a_laisser(&chemin) || en_cours.contains(&chemin.to_string_lossy().as_ref()) {
                continue;
            }
            if plan.len() >= MAX_FICHIERS {
                coupe = true;
                break;
            }
            let nom = entree.file_name().to_string_lossy().to_string();
            let chemin_txt = chemin.to_string_lossy();
            let site = self.telechargements.iter().find(|t| t.chemin == chemin_txt).map(|t| t.site.clone()).unwrap_or_default();
            let jour = meta.modified().map(crate::telechargements::jour_de).unwrap_or_else(|_| crate::telechargements::jour_local());
            let choix = reglage.choisir(&nom, &site, "", jour);
            if choix.sous_dossier.as_os_str().is_empty() {
                restent += 1;
            } else {
                plan.push((chemin, choix.sous_dossier));
            }
        }
        let mut groupes: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (de, sous) in &plan {
            groupes
                .entry(sous.to_string_lossy().to_string())
                .or_default()
                .push(de.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        }
        let mut groupes: Vec<Value> = groupes
            .into_iter()
            .map(|(dossier, noms)| json!({ "dossier": dossier, "n": noms.len(), "exemples": noms.iter().take(4).collect::<Vec<_>>() }))
            .collect();
        groupes.sort_by(|a, b| b["n"].as_u64().cmp(&a["n"].as_u64()));
        info!("rangement : apercu de la racine, {} a ranger, {restent} restent", plan.len());
        APERCU.with(|a| *a.borrow_mut() = json!({ "total": plan.len(), "restent": restent, "coupe": coupe, "groupes": groupes }));
        PLAN.with(|p| *p.borrow_mut() = plan);
        self.pousser_etat();
    }

    /// Applique le plan de l'apercu, et le note pour pouvoir le defaire.
    fn valider_existant(&mut self) {
        let plan = PLAN.with(|p| std::mem::take(&mut *p.borrow_mut()));
        APERCU.with(|a| *a.borrow_mut() = Value::Null);
        if plan.is_empty() {
            self.pousser_etat();
            return;
        }
        let racine = self.dossier_dl.borrow().clone();
        let mut mouvements: Vec<(String, String)> = Vec::new();
        let mut rates = 0usize;
        for (de, sous) in plan {
            // Un fichier parti ou deplace depuis l'apercu : on le laisse.
            if !de.is_file() || de.parent() != Some(racine.as_path()) {
                continue;
            }
            let dossier = racine.join(&sous);
            let nom = de.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if std::fs::create_dir_all(&dossier).is_err() {
                rates += 1;
                continue;
            }
            let vers = crate::telechargements::chemin_libre(&dossier, &nom);
            match std::fs::rename(&de, &vers) {
                Ok(()) => {
                    let (de_txt, vers_txt) = (de.to_string_lossy().to_string(), vers.to_string_lossy().to_string());
                    for t in self.telechargements.iter_mut().filter(|t| t.chemin == de_txt) {
                        t.chemin = vers_txt.clone();
                        t.fichier = vers.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        t.range = sous.to_string_lossy().to_string();
                        t.range_raison = "rangé après coup".into();
                    }
                    mouvements.push((de_txt, vers_txt));
                }
                Err(e) => {
                    rates += 1;
                    alerte!("rangement : {} non deplace ({e})", de.display());
                }
            }
        }
        let n = mouvements.len();
        info!("rangement : racine rangee, {n} deplace(s), {rates} rate(s)");
        if n > 0 {
            let mut ops = self.journal();
            ops.push(Operation { date: maintenant_ms(), mouvements });
            self.ecrire_journal(ops);
            self.profil.sauver_telechargements(&self.telechargements);
        }
        let mut message = format!("{n} fichier{} rangé{}. « Défaire » les remet en place pendant 30 jours.", pluriel(n), pluriel(n));
        if rates > 0 {
            message.push_str(&format!(" {rates} n'ont pas pu bouger (ouverts ailleurs ?)."));
        }
        self.signaler(&message);
        self.pousser_etat();
    }

    /// Remet chaque fichier du dernier rangement la ou il etait.
    fn defaire_existant(&mut self) {
        let mut ops = self.journal();
        let Some(op) = ops.pop() else {
            self.signaler("Rien à défaire.");
            return;
        };
        let mut remis = 0usize;
        for (de, vers) in op.mouvements.iter().rev() {
            let (de, vers) = (PathBuf::from(de), PathBuf::from(vers));
            if !vers.is_file() {
                continue;
            }
            let Some(origine) = de.parent().map(Path::to_path_buf) else { continue };
            let cible = if de.exists() {
                crate::telechargements::chemin_libre(&origine, &de.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
            } else {
                de.clone()
            };
            if std::fs::rename(&vers, &cible).is_ok() {
                remis += 1;
                crate::telechargements::retirer_dossiers_vides(vers.parent(), &origine);
                let (vers_txt, cible_txt) = (vers.to_string_lossy().to_string(), cible.to_string_lossy().to_string());
                for t in self.telechargements.iter_mut().filter(|t| t.chemin == vers_txt) {
                    t.chemin = cible_txt.clone();
                    t.fichier = cible.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    t.range.clear();
                    t.range_raison.clear();
                }
            }
        }
        info!("rangement : defait, {remis}/{} remis", op.mouvements.len());
        self.ecrire_journal(ops);
        self.profil.sauver_telechargements(&self.telechargements);
        self.signaler(&format!("{remis} fichier{} remis à la racine.", pluriel(remis)));
        self.pousser_etat();
    }

    /// Doublons et existant. `true` = traite ici.
    pub(super) fn traiter_ipc_existant(&mut self, m: &str, v: &Value) -> bool {
        match m {
            "rangementDoublons" => {
                self.reglages.rangement.doublons = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                self.pousser_etat();
            }
            "telechargementDoublonGarder" => {
                let id = v["id"].as_u64().unwrap_or(0);
                if let Some(t) = self.telechargements.iter_mut().find(|t| t.id == id) {
                    t.doublon.clear();
                }
                self.profil.sauver_telechargements(&self.telechargements);
                self.pousser_etat();
            }
            "telechargementDoublonJeter" => {
                let id = v["id"].as_u64().unwrap_or(0);
                let Some(t) = self.telechargements.iter().find(|t| t.id == id).cloned() else { return true };
                if t.actif() || t.doublon.is_empty() {
                    return true;
                }
                match crate::telechargements::mettre_a_la_corbeille(Path::new(&t.chemin)) {
                    Ok(()) => {
                        info!("doublons : {} mis a la Corbeille", t.fichier);
                        self.telechargements.retain(|x| x.id != id);
                        self.profil.sauver_telechargements(&self.telechargements);
                        self.signaler(&format!("{} est dans la Corbeille (récupérable). L'autre exemplaire reste.", t.fichier));
                        self.pousser_etat();
                    }
                    Err(e) => self.signaler(&e),
                }
            }
            "rangementApercu" => self.apercu_existant(),
            "rangementApercuFermer" => {
                PLAN.with(|p| p.borrow_mut().clear());
                APERCU.with(|a| *a.borrow_mut() = Value::Null);
                self.pousser_etat();
            }
            "rangementValider" => self.valider_existant(),
            "rangementDefaire" => self.defaire_existant(),
            _ => return false,
        }
        true
    }
}

fn pluriel(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fichiers_jamais_touches() {
        let d = std::env::temp_dir().join(format!("trame-existant-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        for nom in ["a.crdownload", "desktop.ini", "~$devis.docx", ".cache", "photo.png"] {
            std::fs::write(d.join(nom), b"x").unwrap();
        }
        let laisses: Vec<bool> = ["a.crdownload", "desktop.ini", "~$devis.docx", ".cache", "photo.png"]
            .iter()
            .map(|n| a_laisser(&d.join(n)))
            .collect();
        assert_eq!(laisses, vec![true, true, true, true, false]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn le_journal_se_relit() {
        let op = Operation { date: 5, mouvements: vec![("C:\\a.png".into(), "C:\\Images\\a.png".into())] };
        let texte = serde_json::to_string(&vec![op.clone()]).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Operation>>(&texte).unwrap(), vec![op]);
    }
}
