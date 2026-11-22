//! Journal : ecrit a la fois sur stderr (mode console) et dans `profile/trame.log`.
//!
//! En release la fenetre console est masquee (cf. `main.rs`) : le fichier est alors
//! la seule trace. Le pere de ce module est une session ou une application qui ne
//! dit rien ne peut pas etre diagnostiquee.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

/// Horodatage lisible `AAAAMMJJ-HHMMSS` en heure locale approchee (UTC+offset systeme).
pub fn horodatage() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let secs = ms / 1000;
    // Decalage local via la crate standard : on se contente d'un calcul UTC + decalage
    // fourni par `chrono`-free : on lit TZ via l'API Windows n'est pas disponible ici,
    // donc on affiche l'heure UTC en clair quand le decalage est inconnu.
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}Z")
}

/// Millisecondes depuis l'epoque Unix.
pub fn maintenant_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Fixe le fichier de journal (appele une fois au demarrage).
pub fn init(chemin: PathBuf) {
    let _ = LOG_PATH.set(chemin);
}

fn ecrire(ligne: &str) {
    eprintln!("{ligne}");
    let Some(chemin) = LOG_PATH.get() else { return };
    let _garde = LOCK.lock();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(chemin) {
        let _ = writeln!(f, "{ligne}");
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::ecrire_public(&format!("[{}] {}", $crate::log::horodatage(), format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! alerte {
    ($($arg:tt)*) => {
        $crate::log::ecrire_public(&format!("[{}] !! {}", $crate::log::horodatage(), format!($($arg)*)))
    };
}

/// Point d'entree public des macros (l'implementation reste privee).
pub fn ecrire_public(ligne: &str) {
    ecrire(ligne);
}
