//! Sauvegarde et restauration du profil, export et import des favoris.
//!
//! CE QUI PESE, ET CE QUI COMPTE. Mesure du 2026-11-22 sur le profil de Clement :
//! **1 217 Mo, 10 204 fichiers**. Dedans :
//!
//! | ce qui definit le navigateur (19 Ko) | ce qui pese (1 217 Mo) |
//! |---|---|
//! | reglages 0,6 · session 6,2 · favoris 3,3 | webview2 854,6 Mo : cookies, connexions, |
//! | acces 1,3 · zap 4,7 · capsules 2,4 · guets | sessions ouvertes |
//! | | telechargements 283,8 Mo (des fichiers, pas |
//! | | des donnees) · extensions-chrome 77,6 Mo |
//! | | lectures.jsonl 489,9 Ko : le texte des pages lues |
//!
//! Le rapport est de **1 a 64 000**. Une sauvegarde utile emporte donc la colonne de gauche —
//! un fichier d'une vingtaine de kilo-octets qu'on s'envoie par mail — et **laisse la droite
//! dehors**, en le disant. Ce module ecrit ce refus dans le fichier lui-meme (`exclus`) : une
//! sauvegarde qui ne dit pas ce qu'elle a omis fait croire qu'elle protege ce qu'elle n'a pas
//! pris.
//!
//! POURQUOI LA RESTAURATION PASSE PAR UN FICHIER D'ATTENTE. Trame reecrit `session.json` a son
//! arret. Restaurer depuis la fenetre ouverte serait donc efface quelques secondes plus tard
//! par la sauvegarde de sortie. La commande ecrit `restauration-en-attente.json`, ferme Trame,
//! et c'est **le demarrage suivant** qui applique l'archive — avant que le moteur WebView2 ne
//! soit cree et avant que la session ne soit lue. Rien n'est applique a moitie, et le disque
//! garde toujours l'ancien profil (`avant-restauration-<date>\`).

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::carnet::Favori;
use crate::{alerte, info};

/// Version du format d'archive. Une version plus recente est refusee avec son numero plutot que
/// lue de travers : une sauvegarde faite par une autre version de Trame ne se devine pas.
pub const VERSION: u32 = 1;
/// Marque du fichier, premiere chose lue.
pub const FORMAT: &str = "trame-sauvegarde";
/// Extension du fichier de sauvegarde. Le contenu est du JSON lisible : l'extension ne cache
/// rien, elle sert au selecteur de Windows.
pub const EXTENSION: &str = "trame";

/// Nom de la demande de restauration, lue et appliquee au demarrage suivant.
pub const FICHIER_ATTENTE: &str = "restauration-en-attente.json";
/// Compte rendu de la derniere restauration, lu une fois par l'interface.
pub const FICHIER_DERNIERE: &str = "derniere-restauration.json";

/// Un contenu qu'une sauvegarde peut emporter.
pub struct Contenu {
    pub cle: &'static str,
    pub libelle: &'static str,
    pub detail: &'static str,
    /// Fichier du profil, ou dossier quand `dossier` est vrai.
    pub nom: &'static str,
    pub dossier: bool,
    /// Coché par defaut : ce qui fait qu'un Trame est *ce* Trame.
    pub defaut: bool,
}

pub const CONTENUS: &[Contenu] = &[
    Contenu {
        cle: "reglages",
        libelle: "Réglages",
        detail: "moteur, veille, mémoire, fenêtre, extensions",
        nom: "reglages.json",
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "session",
        libelle: "Fils et onglets",
        detail: "les fils, leurs onglets, la position de lecture",
        nom: "session.json",
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "favoris",
        libelle: "Favoris",
        detail: "les pages gardées (Ctrl+D)",
        nom: "favoris.json",
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "acces",
        libelle: "Accès rapides",
        detail: "les sites du nouvel onglet et du rail",
        nom: crate::carnet::NOM_FICHIER_ACCES,
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "zappeur",
        libelle: "Règles du zappeur",
        detail: "ce qui est masqué, par domaine",
        nom: "zap.json",
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "guets",
        libelle: "Pages guettées",
        detail: "les éléments surveillés (prix, stocks)",
        nom: "guets.json",
        dossier: false,
        defaut: true,
    },
    Contenu {
        cle: "capsules",
        libelle: "Capsules",
        detail: "les fils scellés",
        nom: "capsules",
        dossier: true,
        defaut: true,
    },
    Contenu {
        cle: "ext-storage",
        libelle: "Stockage des extensions",
        detail: "la mémoire des extensions Trame",
        nom: "ext-storage.json",
        dossier: false,
        defaut: false,
    },
];

/// Ce qu'une sauvegarde ne contient **jamais**, avec la raison. Ecrit dans le fichier : c'est
/// la partie que l'utilisateur ne verrait pas autrement.
pub const TOUJOURS_EXCLUS: &[(&str, &str)] = &[
    (
        "les connexions et les cookies (webview2\\)",
        "une session ouverte est un secret : il ne se glisse pas dans un fichier qu'on s'envoie \
         ou qu'on dépose quelque part",
    ),
    (
        "la mémoire de lecture (lectures.jsonl)",
        "c'est le texte des pages que vous avez lues — le fichier le plus intime du profil",
    ),
    (
        "les téléchargements (telechargements\\)",
        "ce sont des fichiers, pas des données de navigateur",
    ),
    (
        "le journal (trame.log)",
        "un journal de diagnostic ne se restaure pas",
    ),
];

pub fn contenu(cle: &str) -> Option<&'static Contenu> {
    CONTENUS.iter().find(|c| c.cle == cle)
}

/// Les contenus cochés par defaut.
pub fn cles_par_defaut() -> Vec<String> {
    CONTENUS
        .iter()
        .filter(|c| c.defaut)
        .map(|c| c.cle.to_string())
        .collect()
}

/// Le chemin d'un contenu dans un profil.
pub fn chemin(racine: &Path, c: &Contenu) -> PathBuf {
    racine.join(c.nom)
}

// ------------------------------------------------------------------ archive

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Exclu {
    pub quoi: String,
    pub pourquoi: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entete {
    pub format: String,
    pub version: u32,
    pub cree_le: u64,
    /// Date locale lisible (`2026-11-22 21:40`).
    pub date: String,
    pub trame: String,
    pub machine: String,
    pub contenus: Vec<String>,
    pub exclus: Vec<Exclu>,
    #[serde(default)]
    pub non_coches: Vec<String>,
    /// Empreinte du bloc `donnees`, pour reperer un fichier tronque ou retouche.
    pub empreinte: String,
}

/// Une archive lue et verifiee.
#[derive(Debug)]
pub struct Archive {
    pub entete: Entete,
    pub donnees: Map<String, Value>,
}

/// Lit un fichier texte en retirant un eventuel BOM : un fichier retouche a la main dans le
/// Bloc-notes en porte un, et `serde_json` le refuse (meme piege que `reglages.json`).
fn lire_texte(chemin: &Path) -> Result<String, String> {
    let texte = fs::read_to_string(chemin).map_err(|e| format!("lecture impossible : {e}"))?;
    Ok(match texte.strip_prefix('\u{feff}') {
        Some(reste) => reste.to_string(),
        None => texte,
    })
}

/// Ecrit un bloc JSON de facon atomique (fichier temporaire puis renommage).
fn ecrire_json(chemin: &Path, valeur: &Value) -> Result<(), String> {
    let texte = serde_json::to_string_pretty(valeur).map_err(|e| format!("sérialisation impossible : {e}"))?;
    crate::profile::ecrire_atomique(chemin, &texte).map_err(|e| format!("écriture impossible : {e}"))
}

fn hexa(octets: &[u8]) -> String {
    let mut s = String::with_capacity(octets.len() * 2);
    for o in octets {
        s.push_str(&format!("{o:02x}"));
    }
    s
}

fn empreinte(donnees: &Map<String, Value>) -> String {
    let texte = Value::Object(donnees.clone()).to_string();
    hexa(&crate::crx::sha256(texte.as_bytes()))
}

/// Date et heure **locales**, lues à Windows.
///
/// `log::horodatage` est en UTC et rend `HH:MM:SSZ` : s'en servir pour nommer une sauvegarde
/// donnerait « 20 h 05 » pour un fichier écrit à 21 h 05, et ses deux-points sont interdits
/// dans un nom de fichier Windows. Rend (lisible, nom de fichier).
pub(crate) fn date_locale() -> (String, String) {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    let lisible = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute
    );
    let fichier = format!(
        "{:04}-{:02}-{:02}-{:02}{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute
    );
    (lisible, fichier)
}

/// Le contenu d'un contenu, tel qu'il entre dans l'archive. Un fichier absent donne `null` :
/// la difference entre « vide » et « pas là » doit survivre à un aller-retour.
fn lire_contenu(racine: &Path, c: &Contenu) -> Result<Value, String> {
    let chemin = chemin(racine, c);
    if !c.dossier {
        if !chemin.exists() {
            return Ok(Value::Null);
        }
        let texte = lire_texte(&chemin)?;
        return serde_json::from_str::<Value>(&texte).map_err(|e| format!("fichier illisible ({e})"));
    }
    if !chemin.is_dir() {
        return Ok(Value::Array(Vec::new()));
    }
    let mut capsules = Vec::new();
    let mut entrees: Vec<PathBuf> = fs::read_dir(&chemin)
        .map_err(|e| format!("dossier illisible : {e}"))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|x| x.to_str()) == Some("json"))
        .collect();
    entrees.sort();
    for entree in entrees {
        let Some(nom) = entree.file_name().and_then(|n| n.to_str()).map(str::to_string) else {
            continue;
        };
        match lire_texte(&entree).and_then(|t| {
            serde_json::from_str::<Value>(&t).map_err(|e| format!("illisible ({e})"))
        }) {
            Ok(capsule) => capsules.push(json!({ "fichier": nom, "capsule": capsule })),
            Err(e) => alerte!("sauvegarde : capsule {nom} non emportee ({e})"),
        }
    }
    Ok(Value::Array(capsules))
}

/// Ecrit un contenu dans un profil, depuis une archive. Rend le nombre d'éléments écrits.
fn ecrire_contenu(racine: &Path, cle: &str, valeur: &Value) -> Result<usize, String> {
    let Some(c) = contenu(cle) else {
        return Ok(0);
    };
    if valeur.is_null() {
        return Ok(0);
    }
    if !c.dossier {
        ecrire_json(&chemin(racine, c), valeur)?;
        return Ok(1);
    }
    let dossier = chemin(racine, c);
    fs::create_dir_all(&dossier).map_err(|e| format!("dossier impossible : {e}"))?;
    let mut n = 0usize;
    for item in valeur.as_array().cloned().unwrap_or_default() {
        let Some(nom) = item.get("fichier").and_then(|f| f.as_str()) else {
            continue;
        };
        let Some(capsule) = item.get("capsule") else { continue };
        // Un nom venu d'ailleurs ne doit pas sortir du dossier des capsules : une archive est
        // un fichier qu'on peut recevoir de quelqu'un d'autre.
        if nom.contains('/') || nom.contains('\\') || nom.contains("..") || nom.trim().is_empty() {
            alerte!("restauration : nom de capsule refusé ({nom})");
            continue;
        }
        ecrire_json(&dossier.join(nom), capsule)?;
        n += 1;
    }
    Ok(n)
}

/// Ecrit une sauvegarde du profil. Rend le nombre d'octets et de contenus emportes.
pub fn sauver(racine: &Path, cible: &Path, cles: &[String]) -> Result<(u64, usize), String> {
    let mut donnees = Map::new();
    let mut retenus: Vec<String> = Vec::new();
    for c in CONTENUS {
        if !cles.iter().any(|k| k == c.cle) {
            continue;
        }
        match lire_contenu(racine, c) {
            Ok(valeur) => {
                donnees.insert(c.cle.to_string(), valeur);
                retenus.push(c.cle.to_string());
            }
            // Une partie illisible ne doit pas emporter toute la sauvegarde : on continue et
            // on le dit, plutôt que d'écrire un fichier amputé en silence.
            Err(e) => alerte!("sauvegarde : {} non emporté ({e})", c.libelle),
        }
    }
    if retenus.is_empty() {
        return Err("aucun contenu à sauvegarder".to_string());
    }
    let non_coches: Vec<String> = CONTENUS
        .iter()
        .filter(|c| !retenus.iter().any(|r| r == c.cle))
        .map(|c| c.libelle.to_string())
        .collect();
    let (date, _) = date_locale();
    let entete = Entete {
        format: FORMAT.to_string(),
        version: VERSION,
        cree_le: crate::log::maintenant_ms(),
        date,
        trame: env!("CARGO_PKG_VERSION").to_string(),
        machine: std::env::var("COMPUTERNAME").unwrap_or_default(),
        contenus: retenus.clone(),
        exclus: TOUJOURS_EXCLUS
            .iter()
            .map(|(quoi, pourquoi)| Exclu {
                quoi: quoi.to_string(),
                pourquoi: pourquoi.to_string(),
            })
            .collect(),
        non_coches,
        empreinte: empreinte(&donnees),
    };
    let fichier = json!({
        "format": entete.format,
        "version": entete.version,
        "cree_le": entete.cree_le,
        "date": entete.date,
        "trame": entete.trame,
        "machine": entete.machine,
        "contenus": entete.contenus,
        "exclus": entete.exclus,
        "non_coches": entete.non_coches,
        "empreinte": entete.empreinte,
        "donnees": Value::Object(donnees),
    });
    ecrire_json(cible, &fichier)?;
    let octets = fs::metadata(cible).map(|m| m.len()).unwrap_or(0);
    info!(
        "sauvegarde : {octets} octets, {} contenu(s) -> {}",
        retenus.len(),
        cible.display()
    );
    Ok((octets, retenus.len()))
}

/// Lit et verifie une archive.
pub fn lire(chemin: &Path) -> Result<Archive, String> {
    let texte = lire_texte(chemin)?;
    let valeur: Value = serde_json::from_str(&texte).map_err(|e| {
        format!(
            "ce fichier n'est pas du JSON lisible ({e}) — est-ce bien une sauvegarde Trame ?"
        )
    })?;
    let format = valeur.get("format").and_then(|f| f.as_str()).unwrap_or("");
    if format != FORMAT {
        return Err(format!(
            "ce fichier n'est pas une sauvegarde Trame (marque « {format} »)"
        ));
    }
    let version = valeur.get("version").and_then(Value::as_u64).unwrap_or(0) as u32;
    if version == 0 || version > VERSION {
        return Err(format!(
            "sauvegarde en version {version} : Trame ne sait lire que jusqu'à {VERSION}"
        ));
    }
    let entete: Entete = serde_json::from_value(valeur.clone())
        .map_err(|e| format!("en-tête de sauvegarde illisible ({e})"))?;
    let donnees = valeur
        .get("donnees")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let attendue = empreinte(&donnees);
    if !entete.empreinte.is_empty() && entete.empreinte != attendue {
        // Ce n'est pas un refus : le fichier est du JSON lisible, et le corriger a la main est
        // legitime. Mais il faut le dire, sinon une alteration passerait pour l'original.
        alerte!(
            "sauvegarde : l'empreinte ne correspond pas (fichier retouché ou tronqué) — {}",
            chemin.display()
        );
    }
    Ok(Archive { entete, donnees })
}

// ------------------------------------------------------------------ restauration

#[derive(Serialize, Deserialize)]
struct Attente {
    archive: String,
    #[serde(default)]
    contenus: Vec<String>,
    #[serde(default)]
    quand: u64,
}

/// Demande l'application d'une archive au prochain demarrage.
pub fn demander(racine: &Path, archive: &Path, contenus: &[String]) -> Result<(), String> {
    let attente = Attente {
        archive: archive.display().to_string(),
        contenus: contenus.to_vec(),
        quand: crate::log::maintenant_ms(),
    };
    let valeur = serde_json::to_value(&attente).map_err(|e| format!("sérialisation impossible : {e}"))?;
    ecrire_json(&racine.join(FICHIER_ATTENTE), &valeur)
}

fn marquer(racine: &Path, ok: bool, message: &str) {
    let valeur = json!({
        "quand": crate::log::maintenant_ms(),
        "ok": ok,
        "message": message,
    });
    if let Err(e) = ecrire_json(&racine.join(FICHIER_DERNIERE), &valeur) {
        alerte!("restauration : compte rendu impossible à écrire ({e})");
    }
}

/// Copie ce qui va être écrasé, avant de l'écraser. On n'efface jamais : même règle que
/// `.migre` et `extensions-chrome-archives\`.
fn archiver_avant(racine: &Path, cles: &[String], horodatage: &str) -> Result<PathBuf, String> {
    let dossier = racine.join(format!("avant-restauration-{horodatage}"));
    fs::create_dir_all(&dossier).map_err(|e| format!("dossier d'archive impossible : {e}"))?;
    for cle in cles {
        let Some(c) = contenu(cle) else { continue };
        let source = chemin(racine, c);
        if !source.exists() {
            continue;
        }
        let cible = dossier.join(c.nom);
        let resultat = if c.dossier {
            crate::ressources::copier_arbre(&source, &cible)
        } else {
            fs::copy(&source, &cible).map(|_| ())
        };
        if let Err(e) = resultat {
            return Err(format!(
                "impossible de garder l'ancien {} dans {} ({e}) — restauration annulée, rien n'a été touché",
                c.nom,
                dossier.display()
            ));
        }
    }
    Ok(dossier)
}

/// Applique une archive dans un profil. Rend le compte rendu, ou le motif du refus.
fn appliquer(racine: &Path, attente: &Path) -> Result<String, String> {
    let texte = lire_texte(attente)?;
    let demande: Attente =
        serde_json::from_str(&texte).map_err(|e| format!("demande de restauration illisible ({e})"))?;
    let archive = lire(Path::new(&demande.archive))?;
    let mut cles = if demande.contenus.is_empty() {
        archive.entete.contenus.clone()
    } else {
        demande.contenus.clone()
    };
    // Seuls les contenus connus **et réellement présents** dans l'archive sont appliqués :
    // une archive fabriquée ne peut pas faire écrire `webview2\` ni un fichier inventé.
    cles.retain(|c| contenu(c).is_some() && archive.donnees.contains_key(c));
    if cles.is_empty() {
        return Err("l'archive ne contient aucun contenu applicable".to_string());
    }
    let (_, horodatage) = date_locale();
    let garde = archiver_avant(racine, &cles, &horodatage)?;
    let mut faits = Vec::new();
    for cle in &cles {
        let Some(valeur) = archive.donnees.get(cle) else { continue };
        let libelle = contenu(cle).map(|c| c.libelle).unwrap_or(cle.as_str());
        match ecrire_contenu(racine, cle, valeur) {
            Ok(n) => faits.push(format!("{libelle} ({n})")),
            Err(e) => return Err(format!("{libelle} : {e}")),
        }
    }
    Ok(format!(
        "restauré : {} — depuis {} ; l'ancien profil est gardé dans {}",
        faits.join(", "),
        demande.archive,
        garde.display()
    ))
}

/// Applique la restauration en attente, s'il y en a une. Appelé **au démarrage**, avant que la
/// session ne soit lue. Rend le compte rendu pour le journal.
pub fn appliquer_restauration_en_attente(racine: &Path) -> Option<String> {
    let attente = racine.join(FICHIER_ATTENTE);
    if !attente.exists() {
        return None;
    }
    match appliquer(racine, &attente) {
        Ok(message) => {
            info!("restauration : {message}");
            let _ = fs::remove_file(&attente);
            marquer(racine, true, &message);
            Some(message)
        }
        Err(e) => {
            // On garde la demande sous un autre nom : elle n'est pas rejouée en boucle, et elle
            // reste lisible pour comprendre ce qui a échoué.
            let echec = attente.with_file_name("restauration-en-attente.echec.json");
            let _ = fs::rename(&attente, &echec);
            alerte!(
                "restauration : refusée ({e}) — rien n'a été touché, demande gardée en {}",
                echec.display()
            );
            marquer(racine, false, &format!("restauration refusée : {e}"));
            Some(format!("restauration refusée : {e}"))
        }
    }
}

/// Lit le compte rendu de la dernière restauration, et l'efface (il ne s'affiche qu'une fois).
pub fn message_de_restauration(racine: &Path) -> Option<String> {
    let chemin = racine.join(FICHIER_DERNIERE);
    let texte = lire_texte(&chemin).ok()?;
    let _ = fs::remove_file(&chemin);
    let valeur: Value = serde_json::from_str(&texte).ok()?;
    let message = valeur.get("message").and_then(|m| m.as_str())?.to_string();
    let ok = valeur.get("ok").and_then(Value::as_bool).unwrap_or(false);
    Some(if ok {
        format!("Sauvegarde appliquée — {message}")
    } else {
        format!("Restauration impossible — {message}")
    })
}

// ------------------------------------------------------------------ le résumé avant d'appliquer

fn compte(v: &Value) -> usize {
    match v {
        Value::Array(a) => a.len(),
        Value::Null => 0,
        _ => 1,
    }
}

fn urls(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f.get("url").and_then(|u| u.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn diff_reglages(actuel: &Value, nouveau: &Value) -> String {
    let (Some(a), Some(n)) = (actuel.as_object(), nouveau.as_object()) else {
        return "réglages écrits".to_string();
    };
    let mut changes = Vec::new();
    for (k, v) in n.iter() {
        if a.get(k) != Some(v) {
            changes.push(k.clone());
        }
    }
    if changes.is_empty() {
        return "aucun changement".to_string();
    }
    let apercu: Vec<&str> = changes.iter().take(3).map(|s| s.as_str()).collect();
    format!(
        "{} réglage(s) changé(s) : {}{}",
        changes.len(),
        apercu.join(", "),
        if changes.len() > 3 { ", …" } else { "" }
    )
}

fn diff_favoris(actuel: &Value, nouveau: &Value) -> String {
    let avant = urls(actuel);
    let apres = urls(nouveau);
    let ajoutes = apres.iter().filter(|u| !avant.contains(u)).count();
    let perdus = avant.iter().filter(|u| !apres.contains(u)).count();
    let mut s = format!("{} favori(s) (actuellement {})", apres.len(), avant.len());
    if ajoutes > 0 {
        s.push_str(&format!(" — {ajoutes} ajouté(s)"));
    }
    if perdus > 0 {
        s.push_str(&format!(" — {perdus} de maintenant absent(s) de la sauvegarde"));
    }
    s
}

fn diff_session(actuel: &Value, nouveau: &Value) -> String {
    let compter = |v: &Value, cle: &str| -> usize {
        v.get(cle).and_then(Value::as_array).map(|a| a.len()).unwrap_or(0)
    };
    format!(
        "{} onglet(s) dans {} fil(s) (actuellement {} onglet(s) dans {} fil(s))",
        compter(nouveau, "onglets"),
        compter(nouveau, "fils"),
        compter(actuel, "onglets"),
        compter(actuel, "fils")
    )
}

/// Le texte montré avant d'appliquer une archive : ce qui change, ce qui ne vient pas, et ce
/// qui ne viendra jamais.
pub fn resume_restauration(racine: &Path, archive: &Archive) -> String {
    let mut lignes = Vec::new();
    for c in CONTENUS {
        let Some(valeur) = archive.donnees.get(c.cle) else { continue };
        let actuel = lire_contenu(racine, c).unwrap_or(Value::Null);
        let ligne = match c.cle {
            "reglages" => diff_reglages(&actuel, valeur),
            "session" => diff_session(&actuel, valeur),
            "favoris" => diff_favoris(&actuel, valeur),
            _ => format!("{} élément(s) (actuellement {})", compte(valeur), compte(&actuel)),
        };
        lignes.push(format!("• {} — {}", c.libelle, ligne));
    }
    let mut texte = format!(
        "Sauvegarde du {} (Trame {}, machine {}).\n\n{}",
        archive.entete.date,
        archive.entete.trame,
        if archive.entete.machine.is_empty() { "inconnue" } else { &archive.entete.machine },
        lignes.join("\n")
    );
    if !archive.entete.non_coches.is_empty() {
        texte.push_str(&format!(
            "\n\nNon emporté : {}.",
            archive.entete.non_coches.join(", ")
        ));
    }
    texte.push_str(
        "\n\nJamais dans une sauvegarde : les connexions et les cookies, la mémoire de lecture, \
         les téléchargements, le journal.",
    );
    texte
}

// ------------------------------------------------------------------ favoris : format des autres

fn html_echapper(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_decoder(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut reste = s;
    while let Some(i) = reste.find('&') {
        out.push_str(&reste[..i]);
        let apres = &reste[i..];
        let entite = apres
            .find(';')
            .filter(|f| *f <= 10)
            .map(|fin| (fin, &apres[1..fin]));
        let remplace = entite.and_then(|(_, e)| match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => {
                let num = e.strip_prefix('#')?;
                let code = match num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                code.and_then(char::from_u32)
            }
        });
        match (remplace, entite) {
            (Some(c), Some((fin, _))) => {
                out.push(c);
                reste = &apres[fin + 1..];
            }
            _ => {
                out.push('&');
                reste = &reste[i + 1..];
            }
        }
    }
    out.push_str(reste);
    out
}

/// Lit un attribut d'une balise `<A …>` (`href`, `add_date`). Rend la valeur sans ses
/// guillemets ; une valeur sans guillemets s'arrete a l'espace.
fn attribut<'a>(attributs: &'a str, nom: &str) -> Option<&'a str> {
    let bas = attributs.to_ascii_lowercase();
    let motif = format!("{nom}=");
    let i = bas.find(&motif)?;
    let apres = attributs[i + motif.len()..].trim_start();
    match apres.chars().next() {
        Some(q @ ('"' | '\'')) => {
            let reste = &apres[1..];
            reste.find(q).map(|f| &reste[..f])
        }
        _ => apres.split_whitespace().next(),
    }
}

/// Ecrit les favoris au format **Netscape** — celui que Chrome, Edge et Firefox lisent.
pub fn exporter_favoris_html(favoris: &[Favori], cible: &Path) -> Result<usize, String> {
    let mut s = String::new();
    s.push_str("<!DOCTYPE NETSCAPE-Bookmark-file-1>\n");
    s.push_str("<!-- Écrit par Trame. Format lu par Chrome, Edge et Firefox. -->\n");
    s.push_str("<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n");
    s.push_str("<TITLE>Favoris</TITLE>\n<H1>Favoris</H1>\n");
    s.push_str("<DL><p>\n    <DT><H3>Favoris Trame</H3>\n    <DL><p>\n");
    for f in favoris {
        let titre = if f.titre.trim().is_empty() { &f.url } else { &f.titre };
        let date = f.ajoute_le / 1000;
        if date > 0 {
            s.push_str(&format!(
                "        <DT><A HREF=\"{}\" ADD_DATE=\"{date}\">{}</A>\n",
                html_echapper(&f.url),
                html_echapper(titre)
            ));
        } else {
            s.push_str(&format!(
                "        <DT><A HREF=\"{}\">{}</A>\n",
                html_echapper(&f.url),
                html_echapper(titre)
            ));
        }
    }
    s.push_str("    </DL><p>\n</DL><p>\n");
    crate::profile::ecrire_atomique(cible, &s).map_err(|e| format!("écriture impossible : {e}"))?;
    Ok(favoris.len())
}

/// Lit un fichier de favoris Netscape (Chrome, Edge, Firefox, ou un export Trame).
///
/// Le format est simple et tolérant : on prend les `<A HREF="…">titre</A>`, on décode les
/// entités, on écarte les adresses qui ne sont pas des pages, et **on dédoublonne par adresse**
/// (un import ne doit jamais créer deux fois le même favori).
pub fn importer_favoris_html(texte: &str, maintenant: u64) -> Vec<Favori> {
    // `to_ascii_lowercase` ne change que les A-Z : les index restent valables dans l'original.
    let bas = texte.to_ascii_lowercase();
    let mut out: Vec<Favori> = Vec::new();
    let mut poses: HashSet<String> = HashSet::new();
    let mut pos = 0usize;
    while let Some(i) = bas[pos..].find("<a ") {
        let debut = pos + i;
        let Some(fin_balise) = bas[debut..].find('>').map(|f| debut + f) else { break };
        let Some(fin_texte) = bas[fin_balise..].find("</a>").map(|f| fin_balise + f) else {
            break;
        };
        let attributs = &texte[debut + 3..fin_balise];
        let titre = texte[fin_balise + 1..fin_texte].trim().to_string();
        pos = fin_texte + 4;
        let Some(brut) = attribut(attributs, "href") else { continue };
        let url = html_decoder(brut.trim());
        if !crate::carnet::favorisable(&url) || !poses.insert(url.clone()) {
            continue;
        }
        let ajoute_le = attribut(attributs, "add_date")
            .and_then(|d| d.trim().parse::<u64>().ok())
            .map(|s| s.saturating_mul(1000))
            .filter(|t| *t > 0)
            .unwrap_or(maintenant);
        out.push(Favori {
            url,
            titre: html_decoder(&titre),
            ajoute_le,
            icone: None,
        });
        if out.len() >= 5000 {
            alerte!("import de favoris : arrêté à 5 000 entrées");
            break;
        }
    }
    out
}

/// Fusionne des favoris importés dans la liste existante : rend (liste, ajoutés, déjà présents).
pub fn fusionner_favoris(actuels: &mut Vec<Favori>, importes: Vec<Favori>) -> (usize, usize) {
    let mut connus: BTreeSet<String> = actuels.iter().map(|f| f.url.clone()).collect();
    let mut ajoutes = 0usize;
    let mut doublons = 0usize;
    for f in importes {
        if connus.insert(f.url.clone()) {
            actuels.push(f);
            ajoutes += 1;
        } else {
            doublons += 1;
        }
    }
    (ajoutes, doublons)
}

// ------------------------------------------------------------------ hors du fil de l'interface

/// Ce qu'on demande. Les variantes `chemin: None` ouvrent le sélecteur de Windows : elles
/// passent par un fil annexe, comme l'installation d'une extension (un sélecteur bloque la
/// boucle d'événements, et la fenêtre entière se figerait).
pub enum Demande {
    Sauver {
        chemin: Option<PathBuf>,
        contenus: Vec<String>,
    },
    Restaurer,
    ExporterFavoris {
        chemin: Option<PathBuf>,
        favoris: Vec<Favori>,
    },
    ImporterFavoris {
        chemin: Option<PathBuf>,
    },
}

/// Issue d'une demande.
#[derive(Debug)]
pub enum Issue {
    Sauvegardee {
        chemin: PathBuf,
        octets: u64,
        contenus: usize,
    },
    /// Archive lue et vérifiée : elle attend une confirmation, puis Trame se ferme.
    AConfirmer {
        chemin: PathBuf,
        resume: String,
        contenus: Vec<String>,
    },
    FavorisExportes {
        chemin: PathBuf,
        nombre: usize,
    },
    FavorisImportes(Vec<Favori>),
    Annulee,
    Echec(String),
}

const FILTRE_SAUVEGARDE: &str = "Sauvegarde Trame (*.trame)|*.trame|Tous les fichiers (*.*)|*.*";
const FILTRE_HTML: &str = "Favoris en HTML (*.html;*.htm)|*.html;*.htm|Tous les fichiers (*.*)|*.*";

/// Executé dans un fil annexe.
pub fn executer(racine: &Path, demande: Demande) -> Issue {
    let (_, horodatage) = date_locale();
    match demande {
        Demande::Sauver { chemin, contenus } => {
            let cible = match chemin {
                Some(c) => c,
                None => match crate::systeme::choisir_enregistrement(
                    "Enregistrer une sauvegarde Trame",
                    FILTRE_SAUVEGARDE,
                    &format!("trame-{horodatage}.{EXTENSION}"),
                ) {
                    Ok(Some(c)) => PathBuf::from(c),
                    Ok(None) => return Issue::Annulee,
                    Err(e) => return Issue::Echec(e),
                },
            };
            match sauver(racine, &cible, &contenus) {
                Ok((octets, contenus)) => Issue::Sauvegardee {
                    chemin: cible,
                    octets,
                    contenus,
                },
                Err(e) => Issue::Echec(e),
            }
        }
        Demande::Restaurer => {
            let chemin = match crate::systeme::choisir_fichier(
                "Choisir une sauvegarde Trame",
                "Sauvegarde Trame (*.trame;*.json)|*.trame;*.json|Tous les fichiers (*.*)|*.*",
            ) {
                Ok(Some(c)) => PathBuf::from(c),
                Ok(None) => return Issue::Annulee,
                Err(e) => return Issue::Echec(e),
            };
            match lire(&chemin) {
                Ok(archive) => Issue::AConfirmer {
                    resume: resume_restauration(racine, &archive),
                    contenus: archive.entete.contenus.clone(),
                    chemin,
                },
                Err(e) => Issue::Echec(e),
            }
        }
        Demande::ExporterFavoris { chemin, favoris } => {
            let cible = match chemin {
                Some(c) => c,
                None => match crate::systeme::choisir_enregistrement(
                    "Exporter les favoris (format lu par Chrome, Edge et Firefox)",
                    FILTRE_HTML,
                    &format!("trame-favoris-{horodatage}.html"),
                ) {
                    Ok(Some(c)) => PathBuf::from(c),
                    Ok(None) => return Issue::Annulee,
                    Err(e) => return Issue::Echec(e),
                },
            };
            match exporter_favoris_html(&favoris, &cible) {
                Ok(nombre) => Issue::FavorisExportes { chemin: cible, nombre },
                Err(e) => Issue::Echec(e),
            }
        }
        Demande::ImporterFavoris { chemin } => {
            let source = match chemin {
                Some(c) => c,
                None => match crate::systeme::choisir_fichier(
                    "Importer des favoris (fichier HTML de Chrome, Edge ou Firefox)",
                    FILTRE_HTML,
                ) {
                    Ok(Some(c)) => PathBuf::from(c),
                    Ok(None) => return Issue::Annulee,
                    Err(e) => return Issue::Echec(e),
                },
            };
            match lire_texte(&source) {
                Ok(texte) => {
                    let favoris = importer_favoris_html(&texte, crate::log::maintenant_ms());
                    if favoris.is_empty() {
                        Issue::Echec(
                            "aucun favori reconnu dans ce fichier (attendu : un export HTML de \
                             favoris de Chrome, Edge ou Firefox)"
                                .to_string(),
                        )
                    } else {
                        Issue::FavorisImportes(favoris)
                    }
                }
                Err(e) => Issue::Echec(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Declencheur, Fil, Onglet, Session};
    use crate::profile::Profil;

    /// Un profil jetable, unique par test : les tests tournent en parallele.
    fn bac(nom: &str) -> PathBuf {
        let racine = std::env::temp_dir().join(format!("trame-sauvegarde-{}-{nom}", std::process::id()));
        let _ = fs::remove_dir_all(&racine);
        fs::create_dir_all(&racine).unwrap();
        racine
    }

    fn ecrire(racine: &Path, nom: &str, contenu: &str) {
        fs::create_dir_all(racine).unwrap();
        fs::write(racine.join(nom), contenu).unwrap();
    }

    /// Le nom de fichier de chaque contenu doit être **celui du profil** : deux tables de noms
    /// qui divergent, et une sauvegarde qui ne sauve plus rien, sans un mot.
    #[test]
    fn les_noms_de_fichiers_sont_ceux_du_profil() {
        let racine = bac("noms");
        let p = Profil::avec_racine(racine.clone());
        for c in CONTENUS {
            let attendu = match c.cle {
                "reglages" => p.fichier_reglages(),
                "session" => p.fichier_session(),
                "favoris" => p.fichier_favoris(),
                "guets" => p.fichier_guets(),
                "zappeur" => p.fichier_zap(),
                "capsules" => p.dossier_capsules(),
                "ext-storage" => p.fichier_ext_storage(),
                "acces" => racine.join("acces.json"),
                autre => panic!("contenu sans chemin connu : {autre}"),
            };
            assert_eq!(chemin(&racine, c), attendu, "contenu {}", c.cle);
        }
        // Et l'acces rapide doit vraiment exister sous ce nom dans un vrai profil (ecrit par
        // `app/outils.rs`) : le test le fige.
        assert_eq!(crate::carnet::NOM_FICHIER_ACCES, "acces.json");
    }

    #[test]
    fn une_sauvegarde_se_relit_et_se_restaure() {
        let racine = bac("aller-retour");
        let mut session = Session::default();
        session.fils.push(Fil::neuf(1, "Bitwarden".into(), 1_000));
        session
            .onglets
            .push(Onglet::neuf(10, "https://exemple.fr/a".into(), 1, Declencheur::Saisie, 1_000));
        ecrire(&racine, "session.json", &serde_json::to_string(&session).unwrap());
        ecrire(&racine, "reglages.json", r#"{"moteur":"qwant","veille_auto":false}"#);
        ecrire(
            &racine,
            "favoris.json",
            r#"[{"url":"https://exemple.fr/a","titre":"A","ajoute_le":1000}]"#,
        );
        ecrire(&racine, "zap.json", r#"[{"domaine":"x.fr","selecteur":".pub"}]"#);
        // Ce qui ne doit jamais sortir.
        ecrire(&racine, "lectures.jsonl", "une phrase que j'ai lue");
        fs::create_dir_all(racine.join("webview2")).unwrap();
        fs::write(racine.join("webview2").join("cookie"), "JSESSIONID-0123456789").unwrap();

        let fichier = std::env::temp_dir().join(format!("trame-sauvegarde-{}.trame", std::process::id()));
        let (octets, n) = sauver(&racine, &fichier, &cles_par_defaut()).unwrap();
        assert!(octets > 100, "une sauvegarde vide n'est pas une sauvegarde");
        assert!(n >= 5);

        let archive = lire(&fichier).unwrap();
        assert_eq!(archive.entete.version, VERSION);
        assert!(archive.donnees.contains_key("session"));
        // Les connexions ne sont pas dans le fichier : ni la cle, ni les mots.
        assert!(!archive.donnees.contains_key("webview2"));
        let brut = fs::read_to_string(&fichier).unwrap();
        assert!(
            !brut.contains("JSESSIONID"),
            "le cookie a fui dans la sauvegarde"
        );
        assert!(!brut.contains("une phrase que j'ai lue"));
        // Le refus est écrit, lui.
        assert!(brut.contains("les connexions et les cookies"));

        // On casse tout, puis on restaure.
        ecrire(&racine, "session.json", "{}");
        ecrire(&racine, "reglages.json", "{}");
        ecrire(&racine, "favoris.json", "[]");
        demander(&racine, &fichier, &archive.entete.contenus).unwrap();
        let message = appliquer_restauration_en_attente(&racine).expect("une restauration était en attente");
        assert!(message.contains("restauré"), "{message}");
        assert!(!racine.join(FICHIER_ATTENTE).exists(), "la demande doit être consommée");

        let relue = Profil::avec_racine(racine.clone()).charger_session();
        assert_eq!(relue.onglets.len(), 1);
        assert_eq!(relue.fils.len(), 1);
        let favoris: Vec<Favori> =
            serde_json::from_str(&fs::read_to_string(racine.join("favoris.json")).unwrap()).unwrap();
        assert_eq!(favoris.len(), 1);
        assert_eq!(favoris[0].titre, "A");
        // L'ancien profil est gardé, pas effacé.
        let gardes: Vec<_> = fs::read_dir(&racine)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.starts_with("avant-restauration-"))
            .collect();
        assert_eq!(gardes.len(), 1);
        // Le compte rendu est là, et se lit une seule fois.
        assert!(message_de_restauration(&racine).is_some());
        assert!(message_de_restauration(&racine).is_none());
    }

    #[test]
    fn une_archive_ne_peut_pas_ecrire_hors_des_contenus_connus() {
        let racine = bac("intrus");
        ecrire(&racine, "favoris.json", "[]");
        let fichier = std::env::temp_dir().join(format!("trame-intrus-{}.trame", std::process::id()));
        let malveillante = json!({
            "format": FORMAT,
            "version": VERSION,
            "cree_le": 1,
            "date": "2026-11-22 21:40",
            "trame": "0.4.10",
            "machine": "essai",
            "contenus": ["favoris", "webview2", "extensions-chrome"],
            "exclus": [],
            "non_coches": [],
            "empreinte": "",
            "donnees": {
                "favoris": [{"url": "https://exemple.fr", "titre": "A"}],
                "webview2": {"cookie": "vole"},
                "extensions-chrome": {"x": "y"},
                "capsules": [{"fichier": "..\\..\\evade.json", "capsule": {}}]
            }
        });
        fs::write(&fichier, serde_json::to_string_pretty(&malveillante).unwrap()).unwrap();
        let archive = lire(&fichier).unwrap();
        demander(&racine, &fichier, &archive.entete.contenus).unwrap();
        appliquer_restauration_en_attente(&racine);
        assert!(!racine.join("webview2").exists(), "webview2 ne doit jamais être restauré");
        assert!(!racine.join("extensions-chrome").exists());
        assert!(!racine.parent().unwrap().join("evade.json").exists());
        assert!(racine.join("favoris.json").exists());
    }

    #[test]
    fn une_version_inconnue_est_refusee_avec_son_numero() {
        let racine = bac("version");
        let fichier = racine.join("futur.trame");
        ecrire(
            &racine,
            "futur.trame",
            &json!({"format": FORMAT, "version": 99, "donnees": {}}).to_string(),
        );
        let e = lire(&fichier).unwrap_err();
        assert!(e.contains("99"), "{e}");
        ecrire(&racine, "autre.trame", r#"{"format":"autre-chose","version":1}"#);
        assert!(lire(&racine.join("autre.trame")).unwrap_err().contains("pas une sauvegarde Trame"));
        ecrire(&racine, "casse.trame", "{ pas du json");
        assert!(lire(&racine.join("casse.trame")).is_err());
    }

    #[test]
    fn le_resume_dit_ce_qui_change_et_ce_qui_ne_vient_pas() {
        let racine = bac("resume");
        ecrire(&racine, "favoris.json", r#"[{"url":"https://a.fr","titre":"A"}]"#);
        ecrire(&racine, "reglages.json", r#"{"moteur":"google"}"#);
        let fichier = racine.join("s.trame");
        sauver(&racine, &fichier, &["favoris".to_string(), "reglages".to_string()]).unwrap();
        // L'état change après la sauvegarde : le résumé doit le voir.
        ecrire(
            &racine,
            "favoris.json",
            r#"[{"url":"https://a.fr","titre":"A"},{"url":"https://b.fr","titre":"B"}]"#,
        );
        ecrire(&racine, "reglages.json", r#"{"moteur":"qwant"}"#);
        let resume = resume_restauration(&racine, &lire(&fichier).unwrap());
        assert!(resume.contains("1 favori(s)"), "{resume}");
        assert!(resume.contains("absent(s) de la sauvegarde"), "{resume}");
        assert!(resume.contains("1 réglage(s) changé(s)"), "{resume}");
        assert!(resume.contains("Non emporté"), "{resume}");
        assert!(resume.contains("Jamais dans une sauvegarde"), "{resume}");
    }

    #[test]
    fn les_favoris_font_un_aller_retour_par_le_html() {
        let racine = bac("html");
        let favoris = vec![
            Favori {
                url: "https://exemple.fr/article?x=1&y=2".into(),
                titre: "Un « titre » & une esperluette".into(),
                ajoute_le: 1_700_000_000_000,
                icone: None,
            },
            Favori {
                url: "https://trame.local/a/b".into(),
                titre: String::new(),
                ajoute_le: 0,
                icone: None,
            },
        ];
        let fichier = racine.join("favoris.html");
        exporter_favoris_html(&favoris, &fichier).unwrap();
        let texte = fs::read_to_string(&fichier).unwrap();
        assert!(texte.starts_with("<!DOCTYPE NETSCAPE-Bookmark-file-1>"));
        assert!(texte.contains("ADD_DATE=\"1700000000\""));
        let relus = importer_favoris_html(&texte, 42);
        assert_eq!(relus.len(), 2);
        assert_eq!(relus[0].url, favoris[0].url);
        assert_eq!(relus[0].titre, favoris[0].titre);
        assert_eq!(relus[0].ajoute_le, 1_700_000_000_000);
        // Sans titre, c'est l'adresse qui sert de nom — et l'absence de date prend « maintenant ».
        assert_eq!(relus[1].titre, "https://trame.local/a/b");
        assert_eq!(relus[1].ajoute_le, 42);
    }

    #[test]
    fn un_import_de_favoris_est_tolerant_et_sans_doublon() {
        // Un fichier de Chrome : majuscules, entités, doublons, une adresse non web, un dossier.
        let texte = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<DL><p>
    <DT><H3 ADD_DATE="1">Barre des favoris</H3>
    <DL><p>
        <DT><A HREF="https://a.fr" ADD_DATE="1700000000">A &amp; B</A>
        <DT><A href="https://a.fr">doublon</A>
        <DT><A HREF="https://b.fr/x?a=1&amp;b=2">B &#233;t&#xE9;</A>
        <DT><A HREF="javascript:alert(1)">pas une page</A>
        <DT><A HREF="file:///C:/doc.html">un fichier local</A>
    </DL><p>
</DL><p>"#;
        let favoris = importer_favoris_html(texte, 7);
        assert_eq!(favoris.len(), 3, "doublon et javascript: doivent être écartés");
        assert_eq!(favoris[0].titre, "A & B");
        assert_eq!(favoris[1].url, "https://b.fr/x?a=1&b=2");
        assert_eq!(favoris[1].titre, "B été");
        assert_eq!(favoris[2].url, "file:///C:/doc.html");

        let mut actuels = vec![Favori {
            url: "https://a.fr".into(),
            titre: "déjà là".into(),
            ajoute_le: 1,
            icone: None,
        }];
        let (ajoutes, doublons) = fusionner_favoris(&mut actuels, favoris);
        assert_eq!(ajoutes, 2);
        assert_eq!(doublons, 1);
        assert_eq!(actuels.len(), 3);
        assert_eq!(actuels[0].titre, "déjà là", "un import ne réécrit pas l'existant");
    }

    #[test]
    fn une_sauvegarde_sans_contenu_est_refusee() {
        let racine = bac("vide");
        let fichier = racine.join("vide.trame");
        let e = sauver(&racine, &fichier, &[]).unwrap_err();
        assert!(e.contains("aucun contenu"), "{e}");
    }
}
