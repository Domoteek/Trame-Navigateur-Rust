//! Sauvegarde automatique du profil : une archive `.trame` deposee toute seule dans le dossier
//! choisi, a intervalle regulier, et une rotation qui ne garde que les dernieres.
//!
//! Trois regles, apprises ailleurs dans ce depot :
//!
//! 1. **Rien ne part tant qu'aucun dossier n'est choisi.** Une sauvegarde qu'on ne sait pas ou
//!    mettre ne doit pas atterrir dans le dossier de l'application (une desinstallation
//!    l'effacerait) ni dans le dossier courant.
//! 2. **On n'efface qu'apres avoir ecrit, et seulement ce que Trame a ecrit.** La rotation ne
//!    touche qu'un fichier dont le nom commence par `Trame-` et finit par `.trame` : un dossier
//!    de sauvegardes peut contenir autre chose, et un fichier de l'utilisateur n'est pas a nous.
//! 3. **L'etat est ecrit apres coup, dans le profil.** Si Trame s'arrete au milieu, la prochaine
//!    session ne croit pas avoir sauvegarde : elle le refait.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Le fichier d'etat, dans la racine du profil.
pub const FICHIER_ETAT: &str = "sauvegarde-auto.json";
/// Prefixe des archives ecrites par cette automatisation.
pub const PREFIXE: &str = "Trame-";
/// Extension des archives.
pub const EXTENSION: &str = "trame";

/// Ce qui a ete fait la derniere fois. Persiste : un redemarrage ne doit pas relancer une
/// sauvegarde qui vient d'avoir lieu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Etat {
    /// Derniere sauvegarde **reussie**, en ms depuis l'epoque. 0 = jamais.
    #[serde(default)]
    pub derniere_ms: u64,
    /// Nom du fichier ecrit (le nom seul : le dossier peut changer sans invalider l'etat).
    #[serde(default)]
    pub fichier: String,
    /// Dernier echec, en clair ; efface au premier succes.
    #[serde(default)]
    pub erreur: String,
    /// Nombre d'archives effacees par la rotation a la derniere sauvegarde.
    #[serde(default)]
    pub effaces: usize,
}

/// Le chemin du fichier d'etat d'un profil.
pub fn chemin_etat(racine: &Path) -> PathBuf {
    racine.join(FICHIER_ETAT)
}

/// Lit l'etat ; un fichier absent ou illisible rend l'etat par defaut (jamais d'erreur : une
/// automatisation muette qui ne demarre plus serait pire que pas d'automatisation).
pub fn lire(racine: &Path) -> Etat {
    let chemin = chemin_etat(racine);
    let Ok(texte) = std::fs::read_to_string(&chemin) else {
        return Etat::default();
    };
    serde_json::from_str(texte.trim_start_matches('\u{feff}')).unwrap_or_default()
}

/// Ecrit l'etat de facon atomique.
pub fn ecrire(racine: &Path, etat: &Etat) -> Result<(), String> {
    let texte = serde_json::to_string_pretty(etat).map_err(|e| format!("sérialisation impossible : {e}"))?;
    crate::profile::ecrire_atomique(&chemin_etat(racine), &texte)
        .map_err(|e| format!("écriture impossible : {e}"))
}

/// L'echeance, en ms depuis l'epoque.
pub fn echeance_ms(derniere_ms: u64, heures: u32) -> u64 {
    derniere_ms.saturating_add(u64::from(heures).saturating_mul(3_600_000))
}

/// Vrai si une sauvegarde est due. Une date de derniere sauvegarde **dans le futur** (horloge
/// reglee a l'envers, etat recopie d'une autre machine) ne bloque pas : on sauvegarde.
pub fn due(derniere_ms: u64, heures: u32, maintenant_ms: u64) -> bool {
    if derniere_ms == 0 {
        return true;
    }
    if derniere_ms > maintenant_ms {
        return true;
    }
    maintenant_ms >= echeance_ms(derniere_ms, heures)
}

/// Nom d'archive pour un instant donne, **a l'heure locale** et a la seconde.
///
/// `log::horodatage` est en UTC : s'en servir nommerait « 9 h » une archive ecrite a 10 h. Les
/// deux-points d'une heure sont interdits dans un nom de fichier Windows, d'ou `h` et `m`.
pub fn nom(t: &windows::Win32::Foundation::SYSTEMTIME) -> String {
    format!(
        "{PREFIXE}{:04}-{:02}-{:02}-{:02}h{:02}m{:02}.{EXTENSION}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// Le nom d'archive pour l'instant present.
pub fn nom_maintenant() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    nom(&unsafe { GetLocalTime() })
}

/// Vrai si ce chemin est une archive **ecrite par Trame**.
///
/// Le nom doit avoir exactement la forme que Trame ecrit — `Trame-AAAA-MM-JJ-HHhMMmSS.trame` —
/// et non pas seulement le bon prefixe : la rotation efface des fichiers, et un `Trame-a-moi.trame`
/// fabrique a la main n'a rien a faire dans une liste d'effacement. Un dossier, meme bien nomme,
/// n'est jamais concerne.
fn est_notre(chemin: &Path) -> bool {
    let Some(nom) = chemin.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !chemin.is_file() {
        return false;
    }
    let Some(tige) = nom
        .strip_prefix(PREFIXE)
        .and_then(|r| r.strip_suffix(&format!(".{EXTENSION}")))
    else {
        return false;
    };
    let o = tige.as_bytes();
    // AAAA-MM-JJ-HHhMMmSS : 19 octets, chiffres aux bons endroits, separateurs fixes.
    let forme = o.len() == 19
        && o[4] == b'-'
        && o[7] == b'-'
        && o[10] == b'-'
        && o[13] == b'h'
        && o[16] == b'm'
        && [0..4, 5..7, 8..10, 11..13, 14..16, 17..19]
            .iter()
            .all(|plage| o[plage.clone()].iter().all(u8::is_ascii_digit));
    if !forme || !tige.is_ascii() {
        return false;
    }
    // La forme ne suffit pas : les valeurs doivent exister. `Trame-2026-13-99-99h99m99.trame`
    // a la bonne longueur, mais Trame n'ecrira jamais ca — et on ne supprime que ce qu'on a
    // forcement ecrit.
    let nombre = |a: usize, b: usize| tige[a..b].parse::<u32>().ok();
    let (Some(annee), Some(mois), Some(jour), Some(heure), Some(minute), Some(seconde)) =
        (nombre(0, 4), nombre(5, 7), nombre(8, 10), nombre(11, 13), nombre(14, 16), nombre(17, 19))
    else {
        return false;
    };
    (1900..=9999).contains(&annee)
        && (1..=12).contains(&mois)
        && (1..=31).contains(&jour)
        && heure <= 23
        && minute <= 59
        && seconde <= 59
}

/// Les archives de Trame presentes dans un dossier, de la plus ancienne a la plus recente.
///
/// Le tri se fait sur le **nom** : il porte la date et l'heure, donc l'ordre alphabetique est
/// l'ordre chronologique — et il ne depend pas de la date de modification, qu'une copie ou une
/// restauration reecrit.
pub fn archives(dossier: &Path) -> Vec<PathBuf> {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return Vec::new();
    };
    let mut noms: Vec<PathBuf> = entrees
        .flatten()
        .map(|e| e.path())
        .filter(|p| est_notre(p))
        .collect();
    noms.sort();
    noms
}

/// Rotation : ne garde que les `garder` archives les plus recentes, efface les autres.
/// Rend (effacees, echecs). **A n'appeler qu'apres une ecriture reussie** : sans cela, un echec
/// de sauvegarde ferait quand meme disparaitre les anciennes.
pub fn epurer(dossier: &Path, garder: u32) -> (usize, usize) {
    let garder = garder.max(1) as usize;
    let mut liste = archives(dossier);
    if liste.len() <= garder {
        return (0, 0);
    }
    let a_effacer = liste.len() - garder;
    let (mut faites, mut echecs) = (0, 0);
    for chemin in liste.drain(..a_effacer) {
        match std::fs::remove_file(&chemin) {
            Ok(()) => faites += 1,
            Err(e) => {
                echecs += 1;
                crate::alerte!("sauvegarde automatique : {} non effacee ({e})", chemin.display());
            }
        }
    }
    (faites, echecs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trame-auto-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn une_sauvegarde_est_due_apres_l_intervalle_pas_avant() {
        let h = 24u32;
        let jour = 86_400_000u64;
        assert!(due(0, h, 1_000), "jamais sauvegarde : elle est due");
        assert!(!due(1_000, h, 1_000 + jour - 1), "pas encore l'heure");
        assert!(due(1_000, h, 1_000 + jour), "l'heure est passee");
        assert!(due(2 * jour, h, jour), "une date dans le futur ne bloque pas");
    }

    #[test]
    fn le_nom_porte_la_date_locale_a_la_seconde() {
        use windows::Win32::Foundation::SYSTEMTIME;
        let t = SYSTEMTIME { wYear: 2026, wMonth: 11, wDay: 23, wHour: 10, wMinute: 5, wSecond: 7, wMilliseconds: 0, wDayOfWeek: 1 };
        assert_eq!(nom(&t), "Trame-2026-11-23-10h05m07.trame");
        // Deux sauvegardes dans la meme minute ne s'ecrasent pas.
        let u = SYSTEMTIME { wSecond: 8, ..t };
        assert_ne!(nom(&t), nom(&u));
    }

    #[test]
    fn un_nom_qui_ressemble_au_notre_mais_n_en_a_pas_la_forme_est_epargne() {
        let d = dossier("forme");
        // Tous ces fichiers portent le prefixe et l'extension : aucun ne doit partir.
        for intrus in [
            "Trame-a-moi.trame",
            "Trame-2026-11-23.trame",
            "Trame-2026-11-23-10h05.trame",
            "Trame-2026-13-99-99h99m99.trame",
            "mon-Trame-2026-11-23-10h05m07.trame",
        ] {
            std::fs::write(d.join(intrus), b"x").unwrap();
        }
        // Le vrai nom, lui, est reconnu.
        std::fs::write(d.join("Trame-2026-11-23-10h05m07.trame"), b"archive").unwrap();
        let liste = archives(&d);
        assert_eq!(
            liste.len(),
            1,
            "seule l'archive dont le nom a la forme exacte est reconnue : {liste:?}"
        );
        assert_eq!(epurer(&d, 1), (0, 0), "rien a effacer : un seul fichier reconnu");
        for intrus in ["Trame-a-moi.trame", "Trame-2026-11-23-10h05.trame"] {
            assert!(d.join(intrus).exists(), "{intrus} doit survivre");
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn la_rotation_ne_garde_que_les_dernieres_et_ne_touche_qu_a_nos_archives() {
        let d = dossier("rotation");
        // Un fichier de l'utilisateur, qui ne doit JAMAIS partir.
        std::fs::write(d.join("notes-a-moi.txt"), b"x").unwrap();
        std::fs::write(d.join("Trame-a-moi.trame"), b"pas notre nom").unwrap();
        for i in 1..=5 {
            std::fs::write(d.join(format!("Trame-2026-11-0{i}-10h00m00.trame")), b"archive").unwrap();
        }
        std::fs::create_dir_all(d.join("Trame-2026-11-09-10h00m00.trame")).unwrap(); // un DOSSIER au bon nom
        assert_eq!(archives(&d).len(), 5, "le dossier et les intrus ne comptent pas");

        let (effacees, echecs) = epurer(&d, 2);
        assert_eq!((effacees, echecs), (3, 0));
        let restants: Vec<String> = archives(&d)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            restants,
            vec!["Trame-2026-11-04-10h00m00.trame", "Trame-2026-11-05-10h00m00.trame"],
            "les deux plus recentes restent"
        );
        assert!(d.join("notes-a-moi.txt").exists(), "le fichier de l'utilisateur est intact");
        assert!(d.join("Trame-a-moi.trame").exists(), "un .trame qui n'est pas de nous est intact");
        assert!(d.join("Trame-2026-11-09-10h00m00.trame").is_dir(), "le dossier est intact");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn garder_zero_ne_vide_pas_tout() {
        let d = dossier("zero");
        for i in 1..=3 {
            std::fs::write(d.join(format!("Trame-2026-11-0{i}-10h00m00.trame")), b"a").unwrap();
        }
        // `garder` est borne a 1 par `epurer` : effacer la sauvegarde qu'on vient d'ecrire
        // serait le contraire du but.
        let (effacees, _) = epurer(&d, 0);
        assert_eq!(effacees, 2);
        assert_eq!(archives(&d).len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_etat_se_relit_et_un_etat_casse_ne_bloque_rien() {
        let d = dossier("etat");
        assert_eq!(lire(&d), Etat::default(), "absent : etat par defaut, pas d'erreur");
        let etat = Etat { derniere_ms: 1_795_000_000_000, fichier: "Trame-x.trame".into(), erreur: String::new(), effaces: 2 };
        ecrire(&d, &etat).unwrap();
        assert_eq!(lire(&d), etat);
        std::fs::write(chemin_etat(&d), b"{ pas du json").unwrap();
        assert_eq!(lire(&d), Etat::default(), "illisible : on repart de zero, on ne bloque pas");
        let _ = std::fs::remove_dir_all(&d);
    }
}
