//! Chien de garde de la boucle principale.
//!
//! Le 2026-11-22, apres l'enregistrement d'une passkey Bitwarden, la boucle s'est figee : en
//! reveillant un onglet, wry attendait que WebView2 confirme l'installation des scripts de la
//! page (`AddScriptToExecuteOnDocumentCreated`), et cette confirmation n'est jamais venue (vidage
//! lu dans WinDbg). La fenetre restait « vivante » pour Windows, mais Trame ne traitait plus rien,
//! pas meme sa fermeture. La cause n'a pas ete reproduite (4 essais).
//!
//! Le battement (`Tick`, chaque seconde) passe par la boucle : s'il n'y est plus traite depuis
//! `SEUIL`, le chien note dans le journal ce que la boucle faisait, relance Trame (la session
//! est sauvee a chaque changement) et termine ce processus. Le nouveau Trame attend la fin de
//! l'ancien avant de prendre le verrou du profil.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use crate::log::maintenant_ms;
use crate::{alerte, info};

/// Au-dela, la boucle est jugee bloquee. Large : aucune operation normale du fil principal
/// ne dure autant, et un faux positif couterait une relance.
const SEUIL_MS: u64 = 30_000;
/// Variable posee sur le Trame relance : le pid de celui qu'il remplace.
const VAR_RELANCE: &str = "TRAME_RELANCE_APRES";

static DERNIER_BATTEMENT: AtomicU64 = AtomicU64::new(0);
static EN_COURS: Mutex<String> = Mutex::new(String::new());

/// A appeler a chaque evenement traite par la boucle.
pub fn battement() {
    DERNIER_BATTEMENT.store(maintenant_ms(), Ordering::Relaxed);
}

/// Note ce que la boucle commence a traiter (ecrit dans le journal en cas de blocage).
pub fn noter(description: impl FnOnce() -> String) {
    if let Ok(mut e) = EN_COURS.try_lock() {
        *e = description();
    }
}

/// Lance la surveillance. `TRAME_SANS_CHIEN_DE_GARDE` (diagnostic) la coupe : utile sous un
/// debogueur, qui arrete la boucle a dessein.
pub fn lancer() {
    if std::env::var("TRAME_SANS_CHIEN_DE_GARDE").is_ok() {
        info!("chien de garde coupe (TRAME_SANS_CHIEN_DE_GARDE)");
        return;
    }
    battement();
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(2));
        let ecart = maintenant_ms().saturating_sub(DERNIER_BATTEMENT.load(Ordering::Relaxed));
        if ecart < SEUIL_MS {
            continue;
        }
        let en_cours = EN_COURS.lock().map(|e| e.clone()).unwrap_or_default();
        alerte!(
            "chien de garde : boucle principale bloquee depuis {} s, pendant : {}",
            ecart / 1000,
            if en_cours.is_empty() { "(inconnu)" } else { en_cours.as_str() }
        );
        relancer();
    });
}

/// Relance Trame puis termine ce processus. Les webviews (processus msedgewebview2) suivent
/// leur hote.
fn relancer() {
    let pid = std::process::id();
    match std::env::current_exe() {
        Ok(exe) => {
            let dossier = exe.parent().map(|p| p.to_path_buf());
            let mut commande = std::process::Command::new(&exe);
            // La sonde de blocage ne doit pas suivre : le relance se figerait a son tour.
            commande.env(VAR_RELANCE, pid.to_string()).env_remove("TRAME_SONDE_BLOCAGE");
            if let Some(d) = dossier {
                commande.current_dir(d);
            }
            match commande.spawn() {
                Ok(_) => alerte!("chien de garde : Trame relance, ce processus ({pid}) s'arrete"),
                Err(e) => alerte!("chien de garde : relance impossible ({e}), arret quand meme"),
            }
        }
        Err(e) => alerte!("chien de garde : executable introuvable ({e}), arret sans relance"),
    }
    std::process::exit(3);
}

/// Au demarrage d'un Trame relance par le chien : attendre que l'ancien soit parti (15 s au
/// plus), sinon il tiendrait encore le verrou du profil.
pub fn attendre_l_ancien() {
    let Some(pid) = std::env::var(VAR_RELANCE).ok().and_then(|v| v.parse::<u32>().ok()) else {
        return;
    };
    std::env::remove_var(VAR_RELANCE);
    info!("relance par le chien de garde : attente de la fin du processus {pid}");
    for _ in 0..150 {
        if !crate::profile::processus_vivant(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    alerte!("le processus {pid} vit encore apres 15 s");
}
