//! Installation d'une extension Chrome **hors du fil de l'interface**.
//!
//! Mesure du 2026-11-21 : installer uBlock Origin Lite depuis le magasin prenait 24 s
//! (telechargement de 9 Mo, puis `Expand-Archive`), et pendant ces 24 s la fenetre entiere
//! etait figee — la boucle d'evenements attendait le telechargement. Tout ce qui touche au
//! reseau, au disque ou au selecteur de fichiers de Windows part donc ici, dans un fil annexe ;
//! le resultat revient dans la boucle sous forme de `UserEvent::Installation`.
//!
//! Une seule installation a la fois : elles partagent le dossier de travail
//! `extensions-chrome-installation`, et deux decompressions simultanees s'y melangeraient.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::extensions_chrome;

/// Ce qu'on demande d'installer.
#[derive(Debug, Clone)]
pub enum Demande {
    /// Par identifiant, depuis le point d'entree du magasin.
    Magasin { id: String, source: String },
    /// Un paquet deja sur le disque (`.crx` / `.zip`). `None` : demander au selecteur de Windows.
    Fichier { chemin: Option<PathBuf>, source: String },
    /// Un paquet telecharge par la fiche du magasin (« Ajouter a Chrome »). En cas d'echec, on
    /// reprend par l'identifiant trouve dans l'URL ou le nom du fichier.
    PaquetDuMagasin { chemin: PathBuf, url: String },
}

impl Demande {
    /// Ce que l'interface affiche pendant l'installation.
    pub fn libelle(&self) -> String {
        match self {
            Demande::Magasin { id, .. } => id.clone(),
            Demande::Fichier { chemin: Some(c), .. } | Demande::PaquetDuMagasin { chemin: c, .. } => c
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "paquet".into()),
            Demande::Fichier { chemin: None, .. } => "fichier".into(),
        }
    }
}

/// Une extension mise en place sur le disque.
#[derive(Debug, Clone)]
pub struct Installee {
    pub id: String,
    pub nom: String,
    pub dossier: PathBuf,
    pub octets: u64,
    pub version_crx: u32,
    pub source: String,
    /// Le paquet vient d'une fiche du magasin ouverte dans Trame.
    pub depuis_fiche: bool,
}

/// Issue d'une demande.
#[derive(Debug, Clone)]
pub enum Issue {
    Installee(Installee),
    /// L'utilisateur a ferme le selecteur sans choisir.
    Annulee,
    Echec(String),
}

static UNE_A_LA_FOIS: Mutex<()> = Mutex::new(());

/// Execute une demande jusqu'au bout. Bloquant : a appeler depuis un fil annexe.
pub fn executer(actives: &Path, demande: Demande) -> Issue {
    let _garde = UNE_A_LA_FOIS.lock().unwrap_or_else(|e| e.into_inner());
    match demande {
        Demande::Magasin { id, source } => match depuis_magasin(actives, &id, &source) {
            Ok(i) => Issue::Installee(i),
            Err(e) => Issue::Echec(e),
        },
        Demande::Fichier { chemin, source } => {
            let chemin = match chemin {
                Some(c) => c,
                None => match crate::systeme::choisir_fichier(
                    "Choisir une extension Chrome (.crx ou .zip)",
                    "Extensions Chrome (*.crx;*.zip)|*.crx;*.zip|Tous les fichiers (*.*)|*.*",
                ) {
                    Ok(Some(c)) => PathBuf::from(c),
                    Ok(None) => return Issue::Annulee,
                    Err(e) => return Issue::Echec(format!("sélecteur de fichier indisponible : {e}")),
                },
            };
            match depuis_fichier(actives, &chemin, &source) {
                Ok(i) => Issue::Installee(i),
                Err(e) => Issue::Echec(e),
            }
        }
        Demande::PaquetDuMagasin { chemin, url } => match depuis_fichier(actives, &chemin, "magasin") {
            Ok(mut i) => {
                i.depuis_fiche = true;
                Issue::Installee(i)
            }
            Err(premier) => {
                // Paquet inexploitable (tronque, en-tete abime) : l'identifiant peut encore etre
                // dans l'URL ou dans le nom du fichier, et notre point d'entree sait le reprendre.
                match crate::crx::identifiant(&url).or_else(|| crate::crx::identifiant_du_nom(&chemin)) {
                    Some(id) => {
                        crate::alerte!("paquet du magasin illisible ({premier}), reprise par {id}");
                        match depuis_magasin(actives, &id, "magasin") {
                            Ok(i) => Issue::Installee(i),
                            Err(e) => Issue::Echec(e),
                        }
                    }
                    None => Issue::Echec(format!("paquet illisible et sans identifiant : {premier}")),
                }
            }
        },
    }
}

fn profil_de(actives: &Path) -> PathBuf {
    actives
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn nom_installe(actives: &Path, id: &str) -> String {
    let dossier = actives.join(id);
    std::fs::read_to_string(dossier.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|m| extensions_chrome::nom_extension_chrome(&m, &dossier))
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| id.to_string())
}

fn depuis_magasin(actives: &Path, id: &str, source: &str) -> Result<Installee, String> {
    let profil = profil_de(actives);
    let travail = profil.join("extensions-chrome-telechargement");
    let fichier = travail.join(format!("{id}.crx"));
    crate::info!("installation d'extension : {id} (source {source})");
    let resultat = (|| {
        let octets = crate::systeme::telecharger(&crate::crx::url_magasin(id), &fichier)
            .map_err(|e| format!("téléchargement impossible : {e}"))?;
        let brut = std::fs::read(&fichier).map_err(|e| format!("fichier illisible : {e}"))?;
        let archive = crate::crx::extraire(&brut).map_err(|e| format!("paquet invalide : {e}"))?;
        // La cle publique du paquet fixe l'identifiant que Chromium donnera a l'extension :
        // sans elle, le dossier s'appelle bien `<id>` mais le moteur en attribue un autre
        // (empreinte du chemin), et la fenetre d'action ne s'ouvre pas.
        let cle = crate::crx::identite(&brut)
            .filter(|identite| identite.identifiant == id)
            .and_then(|identite| identite.cle_publique);
        let dossiers = extensions_chrome::Dossiers::depuis(actives.to_path_buf());
        let dossier = extensions_chrome::installer(&dossiers, id, &archive.zip, cle.as_deref())?;
        extensions_chrome::noter_source(&profil, id, source);
        Ok(Installee {
            id: id.to_string(),
            nom: nom_installe(actives, id),
            dossier,
            octets,
            version_crx: archive.version_crx,
            source: source.to_string(),
            depuis_fiche: false,
        })
    })();
    let _ = std::fs::remove_dir_all(&travail);
    resultat
}

fn depuis_fichier(actives: &Path, fichier: &Path, source: &str) -> Result<Installee, String> {
    let brut = std::fs::read(fichier).map_err(|e| format!("fichier illisible : {e}"))?;
    let archive = crate::crx::extraire(&brut)?;
    let dossiers = extensions_chrome::Dossiers::depuis(actives.to_path_buf());
    // L'en-tete du CRX d'abord : c'est l'identifiant que Chromium attribue, donc le seul qui
    // rende `chrome-extension://<id>/...` utile. Le manifeste ne sert de repli que pour un
    // paquet fabrique hors du magasin.
    let identite = crate::crx::identite(&brut);
    let id = match &identite {
        Some(identite) => identite.identifiant.clone(),
        None => extensions_chrome::identifiant_du_paquet(&dossiers, &archive.zip)?,
    };
    let cle = identite.as_ref().and_then(|i| i.cle_publique.clone());
    let dossier = extensions_chrome::installer(&dossiers, &id, &archive.zip, cle.as_deref())?;
    extensions_chrome::noter_source(&profil_de(actives), &id, source);
    crate::info!(
        "extension installee depuis {source} : {id} (CRX v{}) -> {}",
        archive.version_crx,
        dossier.display()
    );
    Ok(Installee {
        nom: nom_installe(actives, &id),
        id,
        dossier,
        octets: brut.len() as u64,
        version_crx: archive.version_crx,
        source: source.to_string(),
        depuis_fiche: false,
    })
}
