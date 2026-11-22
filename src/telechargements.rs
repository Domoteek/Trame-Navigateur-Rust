//! Telechargements : suivi complet, branche directement sur le moteur WebView2.
//!
//! wry n'exposait que « commence » et « fini ». Le moteur, lui, donne tout pour chaque
//! telechargement (`ICoreWebView2DownloadOperation`) : octets recus, taille totale, etat, et
//! les commandes pause / reprise / annulation. Ce module :
//!   - branche `DownloadStarting` sur chaque onglet (et cache le panneau de WebView2, invisible
//!     ici puisque la webview est enfant d'une fenetre Trame) ;
//!   - choisit le fichier de destination (jamais d'ecrasement : `nom (2).ext`) ;
//!   - garde les operations vivantes (objets COM, fil principal) pour la progression et les
//!     commandes ;
//!   - remonte debut et changements d'etat dans la boucle d'evenements.
//!
//! La liste elle-meme (persistante, `telechargements.json`) vit dans `app.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2DownloadOperation, ICoreWebView2_4, COREWEBVIEW2_DOWNLOAD_STATE,
    COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS,
    COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON,
};
use webview2_com::{take_pwstr, DownloadStartingEventHandler, StateChangedEventHandler};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use wry::{WebView, WebViewExtWindows};

use crate::app::UserEvent;

/// Un telechargement tel que la liste le garde.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Telechargement {
    pub id: u64,
    pub url: String,
    /// Nom du fichier (sans dossier).
    pub fichier: String,
    pub chemin: String,
    /// Site d'origine (domaine de la page qui a lance le telechargement).
    #[serde(default)]
    pub site: String,
    /// Taille annoncee par le serveur, si elle l'est.
    #[serde(default)]
    pub total: Option<u64>,
    #[serde(default)]
    pub recu: u64,
    /// `en_cours`, `en_pause`, `termine`, `echoue`, `annule`, `interrompu`.
    pub etat: String,
    /// Motif d'un echec, lisible.
    #[serde(default)]
    pub raison: String,
    pub debut: u64,
    #[serde(default)]
    pub fin: u64,
}

impl Telechargement {
    pub fn actif(&self) -> bool {
        matches!(self.etat.as_str(), "en_cours" | "en_pause")
    }
}

/// Identifiants : l'horodatage du debut, puis +1 — uniques d'une session a l'autre.
static PROCHAIN: AtomicU64 = AtomicU64::new(0);

fn nouvel_id() -> u64 {
    let maintenant = crate::log::maintenant_ms();
    let mut actuel = PROCHAIN.load(Ordering::Relaxed);
    loop {
        let suivant = actuel.max(maintenant) + 1;
        match PROCHAIN.compare_exchange(actuel, suivant, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return suivant,
            Err(v) => actuel = v,
        }
    }
}

thread_local! {
    /// Operations en cours, par identifiant. Objets COM : fil principal seulement, comme les
    /// gestionnaires du moteur qui les fournissent.
    static OPERATIONS: RefCell<HashMap<u64, ICoreWebView2DownloadOperation>> = RefCell::new(HashMap::new());
    /// Destinations deja attribuees a un telechargement en cours : deux fichiers de meme nom
    /// lances ensemble ne doivent pas viser le meme chemin (le fichier n'existe qu'a la fin).
    static RESERVES: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
}

/// Chemin libre, en tenant compte des destinations reservees par les telechargements en cours.
fn destination(dossier: &Path, nom: &str) -> PathBuf {
    let _ = std::fs::create_dir_all(dossier);
    let base = Path::new(nom);
    let tronc = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "telechargement".to_string());
    let extension = base
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    RESERVES.with(|r| {
        let mut reserves = r.borrow_mut();
        let pris = |c: &Path| c.exists() || reserves.iter().any(|x| x == c);
        let mut candidat = dossier.join(format!("{tronc}{extension}"));
        let mut numero = 2;
        while pris(&candidat) && numero < 1000 {
            candidat = dossier.join(format!("{tronc} ({numero}){extension}"));
            numero += 1;
        }
        reserves.push(candidat.clone());
        candidat
    })
}

fn liberer(chemin: &str) {
    RESERVES.with(|r| r.borrow_mut().retain(|x| x.to_string_lossy() != chemin));
}

/// Le motif d'interruption, en francais.
pub fn raison_lisible(code: i32) -> String {
    match code {
        0 => String::new(),
        1 | 2 | 3 | 4 | 5 | 6 | 7 => match code {
            3 => "plus de place sur le disque".into(),
            4 => "nom de fichier trop long".into(),
            5 => "fichier trop gros pour ce disque".into(),
            6 => "bloqué par l'antivirus".into(),
            _ => "erreur d'écriture du fichier".into(),
        },
        10..=15 => "réseau coupé ou trop lent".into(),
        16..=26 => "le serveur a refusé ou mal répondu".into(),
        40 => "annulé".into(),
        41 => "Trame a été fermé pendant le téléchargement".into(),
        50 => "le système a interrompu le téléchargement".into(),
        _ => format!("interrompu (code {code})"),
    }
}

/// Branche le suivi des telechargements sur une webview d'onglet. `dossier` est lu a chaque
/// telechargement (le reglage peut changer en cours de session).
pub fn brancher(
    webview: &WebView,
    proxy: EventLoopProxy<UserEvent>,
    site: String,
    dossier: std::rc::Rc<RefCell<PathBuf>>,
) -> Result<(), String> {
    let coeur: ICoreWebView2_4 = webview.webview().cast().map_err(|e| e.message().to_string())?;
    let gestionnaire = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        unsafe {
            let operation = args.DownloadOperation()?;
            let mut brut = PWSTR::null();
            operation.Uri(&mut brut)?;
            let url = take_pwstr(brut);
            let mut brut = PWSTR::null();
            args.ResultFilePath(&mut brut)?;
            let propose = PathBuf::from(take_pwstr(brut));
            let nom = propose
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "telechargement".to_string());
            let chemin = destination(&dossier.borrow(), &nom);
            args.SetResultFilePath(&HSTRING::from(chemin.as_os_str()))?;
            // Le panneau de WebView2 serait invisible (webview enfant) : Trame montre le sien.
            args.SetHandled(true)?;

            let mut total = 0i64;
            let _ = operation.TotalBytesToReceive(&mut total);
            let id = nouvel_id();
            OPERATIONS.with(|o| o.borrow_mut().insert(id, operation.clone()));

            let p_etat = proxy.clone();
            let chemin_txt = chemin.to_string_lossy().to_string();
            let chemin_fin = chemin_txt.clone();
            let changement = StateChangedEventHandler::create(Box::new(move |op, _| {
                let Some(op) = op else { return Ok(()) };
                let mut etat = COREWEBVIEW2_DOWNLOAD_STATE::default();
                op.State(&mut etat)?;
                let mut raison = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                let _ = op.InterruptReason(&mut raison);
                let mut recu = 0i64;
                let _ = op.BytesReceived(&mut recu);
                let fini = etat != COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS;
                let mut reprenable = BOOL(0);
                let _ = op.CanResume(&mut reprenable);
                if fini && !reprenable.as_bool() {
                    liberer(&chemin_fin);
                }
                let _ = p_etat.send_event(UserEvent::TelechargementEtat {
                    id,
                    termine: etat == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
                    interrompu: fini && etat != COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED,
                    reprenable: reprenable.as_bool(),
                    raison: raison.0,
                    recu: recu.max(0) as u64,
                });
                Ok(())
            }));
            let mut jeton = 0i64;
            operation.add_StateChanged(&changement, &mut jeton)?;

            let _ = proxy.send_event(UserEvent::TelechargementDebut(Telechargement {
                id,
                url,
                fichier: chemin
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or(nom),
                chemin: chemin_txt,
                site: site.clone(),
                total: (total > 0).then_some(total as u64),
                recu: 0,
                etat: "en_cours".into(),
                raison: String::new(),
                debut: crate::log::maintenant_ms(),
                fin: 0,
            }));
        }
        Ok(())
    }));
    unsafe {
        let mut jeton = 0i64;
        coeur
            .add_DownloadStarting(&gestionnaire, &mut jeton)
            .map_err(|e| e.message().to_string())
    }
}

/// Octets recus et taille totale d'un telechargement en cours.
pub fn progression(id: u64) -> Option<(u64, Option<u64>)> {
    OPERATIONS.with(|o| {
        let ops = o.borrow();
        let op = ops.get(&id)?;
        let mut recu = 0i64;
        let mut total = 0i64;
        unsafe {
            let _ = op.BytesReceived(&mut recu);
            let _ = op.TotalBytesToReceive(&mut total);
        }
        Some((recu.max(0) as u64, (total > 0).then_some(total as u64)))
    })
}

/// Pause, reprise, annulation. `false` si l'operation n'existe plus (fini, ou Trame relance).
pub fn commander(id: u64, ordre: &str) -> bool {
    OPERATIONS.with(|o| {
        let ops = o.borrow();
        let Some(op) = ops.get(&id) else { return false };
        unsafe {
            match ordre {
                "pause" => op.Pause().is_ok(),
                "reprendre" => op.Resume().is_ok(),
                "annuler" => op.Cancel().is_ok(),
                _ => false,
            }
        }
    })
}

/// Oublie l'operation d'un telechargement termine (plus rien a lui demander).
pub fn oublier(id: u64) {
    OPERATIONS.with(|o| {
        o.borrow_mut().remove(&id);
    });
}

/// Taille lisible : « 3,4 Mo ».
pub fn taille_lisible(octets: u64) -> String {
    let o = octets as f64;
    let (valeur, unite) = if o >= 1_073_741_824.0 {
        (o / 1_073_741_824.0, "Go")
    } else if o >= 1_048_576.0 {
        (o / 1_048_576.0, "Mo")
    } else if o >= 1024.0 {
        (o / 1024.0, "Ko")
    } else {
        return format!("{octets} o");
    };
    format!("{valeur:.1} {unite}").replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deux_telechargements_du_meme_nom_ne_visent_pas_le_meme_fichier() {
        let d = std::env::temp_dir().join(format!("trame-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let a = destination(&d, "rapport.pdf");
        let b = destination(&d, "rapport.pdf");
        assert_ne!(a, b);
        assert!(b.to_string_lossy().ends_with("rapport (2).pdf"));
        liberer(&a.to_string_lossy());
        let c = destination(&d, "rapport.pdf");
        assert_eq!(c, a, "une destination liberee redevient disponible");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn les_identifiants_ne_se_repetent_pas() {
        let a = nouvel_id();
        let b = nouvel_id();
        assert!(b > a);
    }

    #[test]
    fn tailles_et_raisons_lisibles() {
        assert_eq!(taille_lisible(512), "512 o");
        assert_eq!(taille_lisible(3_565_158), "3,4 Mo");
        assert_eq!(raison_lisible(3), "plus de place sur le disque");
        assert_eq!(raison_lisible(12), "réseau coupé ou trop lent");
        assert!(raison_lisible(0).is_empty());
    }
}
