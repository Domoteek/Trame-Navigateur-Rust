//! Rangement des telechargements : choisit le sous-dossier (et parfois le nom) d'un fichier AU
//! MOMENT ou son telechargement commence.
//!
//! Inspire de FolderFresh (idees seulement : aucun code repris, il est en C# et sous GPL), mais a
//! l'envers : FolderFresh surveille un dossier et deplace les fichiers apres coup ; Trame choisit
//! la destination avant d'ecrire le premier octet. Rien ne tourne en fond, rien n'est deplace, et
//! Trame sait ce qu'un outil de dossier ignore : le site d'origine et le type annonce par le
//! serveur.
//!
//! Ordre de decision :
//!   1. les regles, de haut en bas (la premiere qui convient gagne) ;
//!   2. les categories par type (extension du nom, sinon type MIME annonce) ;
//!   3. rien : le fichier reste a la racine du dossier des telechargements.
//!
//! Module pur : ni fichier, ni horloge (la date est passee en argument), pour etre teste tel quel.
//! Le branchement sur le moteur vit dans `telechargements.rs`, les messages dans `app/rangement.rs`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Reglage complet, garde dans `reglages.json` (`Reglages::rangement`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Rangement {
    /// Interrupteur general. Coupe par defaut : Trame range tout a la racine, comme avant.
    #[serde(default)]
    pub actif: bool,
    #[serde(default = "categories_defaut")]
    pub categories: Vec<Categorie>,
    #[serde(default)]
    pub regles: Vec<Regle>,
    /// Un sous-dossier `AAAA-MM` sous chaque destination.
    #[serde(default)]
    pub par_mois: bool,
    /// Prevenir quand un fichier arrive alors qu'un identique est deja la (`doublons.rs`).
    /// Independant de `actif` : utile meme sans rangement.
    #[serde(default = "vrai")]
    pub doublons: bool,
}

impl Default for Rangement {
    fn default() -> Self {
        Rangement { actif: false, categories: categories_defaut(), regles: Vec::new(), par_mois: false, doublons: true }
    }
}

/// Une famille de fichiers reconnue par son extension.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Categorie {
    /// Identifiant stable (`images`, `videos`…), cite par les regles.
    pub id: String,
    pub nom: String,
    /// Sous-dossier, relatif au dossier des telechargements.
    pub dossier: String,
    /// Extensions, sans point, en minuscules.
    pub extensions: Vec<String>,
    /// Debut du type MIME qui designe aussi cette famille (`image/`), pour un nom sans extension.
    #[serde(default)]
    pub mime: Vec<String>,
    #[serde(default = "vrai")]
    pub actif: bool,
}

/// « Si ça vient de … et que c'est un … et que le nom contient … → dossier ».
/// Une condition vide ne filtre rien ; une regle sans aucune condition ne prend rien (elle
/// avalerait tout, ce qui n'est jamais voulu).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Regle {
    pub id: u64,
    #[serde(default = "vrai")]
    pub actif: bool,
    /// Domaines : `pronote.fr` prend lui-meme et ses sous-domaines (`demo.pronote.fr`), jamais
    /// `faux-pronote.fr`.
    #[serde(default)]
    pub sites: Vec<String>,
    /// Identifiants de categorie (`images`) ou extensions (`pdf`).
    #[serde(default)]
    pub types: Vec<String>,
    /// Morceau du nom de fichier, sans tenir compte des majuscules ni des accents.
    #[serde(default)]
    pub nom_contient: String,
    /// Sous-dossier ; accepte `{annee}`, `{mois}` (et `{année}`).
    pub dossier: String,
    /// Nouveau nom ; vide = garder le nom. Accepte `{date}`, `{nom}`, `{annee}`, `{mois}`.
    /// L'extension d'origine est toujours gardee.
    #[serde(default)]
    pub renommer: String,
}

/// Ce que le rangement decide pour un fichier.
#[derive(Clone, Debug, PartialEq)]
pub struct Choix {
    /// Relatif au dossier des telechargements ; vide = la racine.
    pub sous_dossier: PathBuf,
    /// Nom de fichier final (renomme ou non).
    pub nom: String,
    /// Pourquoi, en francais, pour la page des telechargements et l'essai des reglages.
    pub raison: String,
}

/// Date locale, passee par l'appelant (le module ne lit pas l'horloge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jour {
    pub annee: u16,
    pub mois: u8,
    pub jour: u8,
}

fn vrai() -> bool {
    true
}

fn cat(id: &str, nom: &str, ext: &[&str], mime: &[&str]) -> Categorie {
    Categorie {
        id: id.into(),
        nom: nom.into(),
        dossier: nom.into(),
        extensions: ext.iter().map(|s| s.to_string()).collect(),
        mime: mime.iter().map(|s| s.to_string()).collect(),
        actif: true,
    }
}

pub fn categories_defaut() -> Vec<Categorie> {
    vec![
        cat("images", "Images", &["jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "avif", "bmp", "svg", "ico", "tif", "tiff"], &["image/"]),
        cat("videos", "Vidéos", &["mp4", "mkv", "webm", "mov", "avi", "m4v", "wmv"], &["video/"]),
        cat("musique", "Musique", &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"], &["audio/"]),
        cat(
            "documents",
            "Documents",
            &["pdf", "doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ods", "csv", "ppt", "pptx", "odp", "epub"],
            &["application/pdf", "text/", "application/msword", "application/vnd.openxmlformats", "application/vnd.oasis"],
        ),
        cat("archives", "Archives", &["zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso"], &["application/zip", "application/x-7z", "application/x-rar"]),
        cat("installeurs", "Installeurs", &["exe", "msi", "msix", "msixbundle", "appx", "apk"], &["application/x-msdownload", "application/x-msi"]),
    ]
}

/// Extension d'un nom, sans point, en minuscules (`archive.tar.gz` -> `gz`).
pub fn extension(nom: &str) -> String {
    match nom.rsplit_once('.') {
        Some((tronc, ext)) if !tronc.is_empty() && !ext.is_empty() && ext.len() <= 12 && ext.chars().all(|c| c.is_ascii_alphanumeric()) => {
            ext.to_ascii_lowercase()
        }
        _ => String::new(),
    }
}

/// Minuscules et sans accents, pour comparer des noms comme on les lit.
fn simple(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// `www.` retire, minuscules, sans port.
fn domaine(site: &str) -> String {
    let s = site.trim().to_ascii_lowercase();
    let s = s.split("://").last().unwrap_or("").split(['/', ':']).next().unwrap_or("");
    s.strip_prefix("www.").unwrap_or(s).to_string()
}

fn site_correspond(site: &str, motif: &str) -> bool {
    let motif = domaine(motif);
    !motif.is_empty() && (site == motif || site.ends_with(&format!(".{motif}")))
}

impl Rangement {
    fn categorie_de(&self, ext: &str, mime: &str, toutes: bool) -> Option<&Categorie> {
        let mime = mime.trim().to_ascii_lowercase();
        let par_ext = self.categories.iter().find(|c| (toutes || c.actif) && !ext.is_empty() && c.extensions.iter().any(|e| e == ext));
        par_ext.or_else(|| {
            // Le type annonce ne sert que pour un nom sans extension : `application/octet-stream`
            // sur un `.exe` ne doit pas l'emporter sur le nom.
            if !ext.is_empty() || mime.is_empty() {
                return None;
            }
            self.categories.iter().find(|c| (toutes || c.actif) && c.mime.iter().any(|m| mime.starts_with(m.as_str())))
        })
    }

    fn regle_prend(&self, r: &Regle, nom: &str, site: &str, ext: &str, mime: &str) -> bool {
        if !r.actif || (r.sites.is_empty() && r.types.is_empty() && r.nom_contient.trim().is_empty()) {
            return false;
        }
        if !r.sites.is_empty() && !r.sites.iter().any(|m| site_correspond(site, m)) {
            return false;
        }
        if !r.types.is_empty() {
            // Une regle cite une categorie meme coupee : couper « Images » dans les types ne doit
            // pas casser « images de Discord → Images\Discord ».
            let famille = self.categorie_de(ext, mime, true).map(|c| c.id.as_str());
            let ok = r.types.iter().any(|t| {
                let t = t.trim().trim_start_matches('.').to_ascii_lowercase();
                t == ext || Some(t.as_str()) == famille
            });
            if !ok {
                return false;
            }
        }
        let morceau = simple(r.nom_contient.trim());
        morceau.is_empty() || simple(nom).contains(&morceau)
    }

    /// Ou ranger `nom`, telecharge depuis `site`, annonce en `mime`.
    pub fn choisir(&self, nom: &str, site: &str, mime: &str, jour: Jour) -> Choix {
        let racine = Choix { sous_dossier: PathBuf::new(), nom: nom.to_string(), raison: String::new() };
        if !self.actif {
            return racine;
        }
        let site = domaine(site);
        let ext = extension(nom);
        for r in &self.regles {
            if self.regle_prend(r, nom, &site, &ext, mime) {
                let mut sous = chemin_sur(&remplir(&r.dossier, nom, jour));
                if self.par_mois && !r.dossier.contains('{') && !sous.as_os_str().is_empty() {
                    sous.push(format!("{:04}-{:02}", jour.annee, jour.mois));
                }
                let nom_final = renommer(&r.renommer, nom, &ext, jour);
                return Choix { sous_dossier: sous, nom: nom_final, raison: format!("règle « {} »", decrire(r)) };
            }
        }
        if let Some(c) = self.categorie_de(&ext, mime, false) {
            let mut sous = chemin_sur(&remplir(&c.dossier, nom, jour));
            if !sous.as_os_str().is_empty() {
                if self.par_mois {
                    sous.push(format!("{:04}-{:02}", jour.annee, jour.mois));
                }
                let pourquoi = if ext.is_empty() { format!("type {} (annoncé par le site)", c.nom) } else { format!("type {} (.{ext})", c.nom) };
                return Choix { sous_dossier: sous, nom: nom.to_string(), raison: pourquoi };
            }
        }
        Choix {
            raison: if ext.is_empty() { "aucun type reconnu : à la racine".into() } else { format!(".{ext} n'est dans aucun type allumé : à la racine") },
            ..racine
        }
    }
}

/// Jour civil d'un nombre de jours depuis le 1970-01-01 (algorithme de H. Hinnant).
pub fn jour_depuis_epoque(jours: i64) -> Jour {
    let z = jours + 719_468;
    let ere = z.div_euclid(146_097);
    let doe = z - ere * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let jour = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
    let annee = (yoe + ere * 400 + i64::from(mois <= 2)) as u16;
    Jour { annee, mois, jour }
}

/// Nombre de jours depuis le 1970-01-01 d'un jour civil (inverse de `jour_depuis_epoque`).
pub fn jours_depuis_epoque(j: Jour) -> i64 {
    let (a, m, d) = (i64::from(j.annee) - i64::from(j.mois <= 2), i64::from(j.mois), i64::from(j.jour));
    let ere = a.div_euclid(400);
    let yoe = a - ere * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    ere * 146_097 + doe - 719_468
}

/// La regle en une phrase courte (pour la raison affichee).
pub fn decrire(r: &Regle) -> String {
    let mut morceaux = Vec::new();
    if !r.sites.is_empty() {
        morceaux.push(r.sites.join(" ou "));
    }
    if !r.types.is_empty() {
        morceaux.push(r.types.join(" ou "));
    }
    if !r.nom_contient.trim().is_empty() {
        morceaux.push(format!("nom contient {}", r.nom_contient.trim()));
    }
    morceaux.join(", ")
}

fn remplir(modele: &str, nom: &str, jour: Jour) -> String {
    let tronc = match nom.rsplit_once('.') {
        Some((t, _)) if !extension(nom).is_empty() => t,
        _ => nom,
    };
    modele
        .replace("{année}", &format!("{:04}", jour.annee))
        .replace("{annee}", &format!("{:04}", jour.annee))
        .replace("{mois}", &format!("{:02}", jour.mois))
        .replace("{date}", &format!("{:04}-{:02}-{:02}", jour.annee, jour.mois, jour.jour))
        .replace("{nom}", tronc)
}

fn renommer(modele: &str, nom: &str, ext: &str, jour: Jour) -> String {
    if modele.trim().is_empty() {
        return nom.to_string();
    }
    let tronc = nettoyer_morceau(&remplir(modele.trim(), nom, jour));
    if tronc.is_empty() {
        return nom.to_string();
    }
    if ext.is_empty() { tronc } else { format!("{tronc}.{ext}") }
}

/// Un morceau de chemin sur pour Windows : sans caracteres interdits, sans point ni espace final,
/// jamais `.`/`..`, jamais un nom reserve (`CON`, `NUL`…).
fn nettoyer_morceau(s: &str) -> String {
    let propre: String = s
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    let propre = propre.trim().trim_end_matches(['.', ' ']).trim().to_string();
    if propre.is_empty() || propre == "." || propre == ".." {
        return String::new();
    }
    let base = propre.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserve = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((base.starts_with("COM") || base.starts_with("LPT")) && base.len() == 4 && base.as_bytes()[3].is_ascii_digit());
    if reserve { format!("_{propre}") } else { propre }
}

/// Sous-dossier relatif et sur : jamais absolu, jamais au-dessus du dossier des telechargements.
pub fn chemin_sur(s: &str) -> PathBuf {
    s.split(['/', '\\']).map(nettoyer_morceau).filter(|m| !m.is_empty()).take(6).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOUR: Jour = Jour { annee: 2026, mois: 11, jour: 27 };

    fn allume() -> Rangement {
        Rangement { actif: true, ..Rangement::default() }
    }

    fn regle(id: u64, sites: &[&str], types: &[&str], nom: &str, dossier: &str) -> Regle {
        Regle {
            id,
            actif: true,
            sites: sites.iter().map(|s| s.to_string()).collect(),
            types: types.iter().map(|s| s.to_string()).collect(),
            nom_contient: nom.into(),
            dossier: dossier.into(),
            renommer: String::new(),
        }
    }

    #[test]
    fn coupe_tout_reste_a_la_racine() {
        let c = Rangement::default().choisir("photo.png", "x.fr", "", JOUR);
        assert_eq!(c.sous_dossier, PathBuf::new());
        assert_eq!(c.nom, "photo.png");
    }

    #[test]
    fn par_type_puis_racine() {
        let r = allume();
        assert_eq!(r.choisir("Photo.JPG", "", "", JOUR).sous_dossier, PathBuf::from("Images"));
        assert_eq!(r.choisir("SteamSetup.exe", "", "", JOUR).sous_dossier, PathBuf::from("Installeurs"));
        assert_eq!(r.choisir("plan.sh3d", "", "", JOUR).sous_dossier, PathBuf::new());
        // Sans extension : le type annonce par le site decide.
        assert_eq!(r.choisir("telechargement", "", "image/webp", JOUR).sous_dossier, PathBuf::from("Images"));
        // Avec extension, le type annonce ne l'emporte pas.
        assert_eq!(r.choisir("jeu.exe", "", "application/octet-stream", JOUR).sous_dossier, PathBuf::from("Installeurs"));
    }

    #[test]
    fn une_categorie_coupee_laisse_a_la_racine() {
        let mut r = allume();
        r.categories.iter_mut().find(|c| c.id == "images").unwrap().actif = false;
        assert_eq!(r.choisir("photo.png", "", "", JOUR).sous_dossier, PathBuf::new());
    }

    #[test]
    fn les_regles_passent_avant_les_types_dans_l_ordre() {
        let mut r = allume();
        r.regles.push(regle(1, &["pronote.fr"], &[], "", "École"));
        r.regles.push(regle(2, &["impots.gouv.fr", "ameli.fr"], &["pdf"], "", "Papiers\\{année}"));
        r.regles.push(regle(3, &[], &[], "facture", "Papiers/Factures"));
        assert_eq!(r.choisir("bulletin.pdf", "https://demo.pronote.fr/eleve", "", JOUR).sous_dossier, PathBuf::from("École"));
        assert_eq!(r.choisir("avis.pdf", "www.impots.gouv.fr", "", JOUR).sous_dossier, PathBuf::from("Papiers").join("2026"));
        // Pas un PDF : la regle 2 ne prend pas, les types prennent.
        assert_eq!(r.choisir("avis.png", "impots.gouv.fr", "", JOUR).sous_dossier, PathBuf::from("Images"));
        assert_eq!(r.choisir("Facture-EDF.pdf", "edf.fr", "", JOUR).sous_dossier, PathBuf::from("Papiers").join("Factures"));
        // « pronote.fr » ne prend pas « faux-pronote.fr ».
        assert_eq!(r.choisir("x.pdf", "faux-pronote.fr", "", JOUR).sous_dossier, PathBuf::from("Documents"));
    }

    #[test]
    fn une_regle_cite_une_categorie_meme_coupee() {
        let mut r = allume();
        r.categories.iter_mut().find(|c| c.id == "images").unwrap().actif = false;
        r.regles.push(regle(1, &["discord.com"], &["images"], "", "Images/Discord"));
        assert_eq!(r.choisir("capture.png", "cdn.discord.com", "", JOUR).sous_dossier, PathBuf::from("Images").join("Discord"));
    }

    #[test]
    fn regle_sans_condition_ou_coupee_ne_prend_rien() {
        let mut r = allume();
        r.regles.push(regle(1, &[], &[], "  ", "Tout"));
        let mut coupee = regle(2, &["x.fr"], &[], "", "X");
        coupee.actif = false;
        r.regles.push(coupee);
        assert_eq!(r.choisir("a.png", "x.fr", "", JOUR).sous_dossier, PathBuf::from("Images"));
    }

    #[test]
    fn nom_sans_accents_ni_majuscules() {
        let mut r = allume();
        r.regles.push(regle(1, &[], &[], "Écolé", "Ecole"));
        assert_eq!(r.choisir("devoir-ECOLE.odt", "", "", JOUR).sous_dossier, PathBuf::from("Ecole"));
    }

    #[test]
    fn renommer_garde_l_extension() {
        let mut r = allume();
        let mut f = regle(1, &[], &[], "facture", "Factures");
        f.renommer = "{date} {nom}".into();
        r.regles.push(f);
        let c = r.choisir("facture-edf.pdf", "", "", JOUR);
        assert_eq!(c.nom, "2026-11-27 facture-edf.pdf");
        assert!(c.raison.contains("facture"));
    }

    #[test]
    fn par_mois() {
        let mut r = allume();
        r.par_mois = true;
        assert_eq!(r.choisir("a.mp3", "", "", JOUR).sous_dossier, PathBuf::from("Musique").join("2026-11"));
    }

    #[test]
    fn jamais_hors_du_dossier() {
        assert_eq!(chemin_sur("..\\..\\Windows"), PathBuf::from("Windows"));
        assert_eq!(chemin_sur("C:\\Temp"), PathBuf::from("C_").join("Temp"));
        assert_eq!(chemin_sur("/etc/./"), PathBuf::from("etc"));
        assert_eq!(chemin_sur("Papiers/CON/a?b. "), PathBuf::from("Papiers").join("_CON").join("a_b"));
        let mut r = allume();
        r.regles.push(regle(1, &["x.fr"], &[], "", "../.."));
        // Destination vide apres nettoyage : la racine, jamais au-dessus.
        assert_eq!(r.choisir("a.png", "x.fr", "", JOUR).sous_dossier, PathBuf::new());
    }

    #[test]
    fn calendrier() {
        assert_eq!(jour_depuis_epoque(0), Jour { annee: 1970, mois: 1, jour: 1 });
        assert_eq!(jour_depuis_epoque(20_419), Jour { annee: 2025, mois: 11, jour: 27 });
        assert_eq!(jour_depuis_epoque(19_782), Jour { annee: 2024, mois: 2, jour: 29 });
        for j in [0, 59, 60, 11_016, 19_782, 20_784, 40_000] {
            assert_eq!(jours_depuis_epoque(jour_depuis_epoque(j)), j);
        }
    }

    #[test]
    fn extensions() {
        assert_eq!(extension("archive.tar.gz"), "gz");
        assert_eq!(extension(".bashrc"), "");
        assert_eq!(extension("sans"), "");
        assert_eq!(extension("Photo.JPEG"), "jpeg");
    }

    #[test]
    fn un_reglage_ancien_se_lit() {
        let r: Rangement = serde_json::from_str(r#"{"actif":true}"#).unwrap();
        assert_eq!(r.categories.len(), 6);
        assert!(r.regles.is_empty());
        assert!(r.doublons, "les doublons sont signales par defaut");
    }
}
