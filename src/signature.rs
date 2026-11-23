//! Signature Authenticode d'un executable : **avant de le lancer**.
//!
//! La mise a jour de Trame telecharge un setup depuis l'adresse publiee dans la fiche
//! `products/trame`, puis l'execute. Jusqu'a la 0.4.54, rien ne verifiait ce fichier : la
//! confiance reposait entierement sur « qui peut ecrire cette fiche ». Ce module ferme ce trou
//! avec l'API Windows elle-meme (`WinVerifyTrust`, celle que le SmartScreen et l'Explorateur
//! utilisent) : un installeur non signe, ou dont la signature ne tient pas, ne part pas.
//!
//! **Ce que ce module ne fait pas, et qu'il faut savoir** : il verifie que la signature est
//! valide et de confiance (chaine reconnue par Windows, horodatage compris). Il ne verifie pas
//! *qui* a signe. Un installeur signe par un autre editeur de confiance passerait donc — c'est
//! ecrit dans le rapport de la 0.4.55 plutot que sous-entendu.
use std::path::Path;

/// Le verdict, avec de quoi le dire dans le journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// La signature est valide et la chaine de confiance est reconnue par Windows.
    pub valide: bool,
    /// Code rendu par `WinVerifyTrust` (0 = tout va bien).
    pub code: i32,
}

impl Verdict {
    /// Un refus, pour un fichier qu'on n'a pas pu ouvrir.
    fn absent(motif: &str) -> Verdict {
        let _ = motif;
        Verdict {
            valide: false,
            code: -1,
        }
    }
}

/// Verifie la signature d'un fichier.
#[cfg(windows)]
pub fn verifier(chemin: &Path) -> Verdict {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::GUID;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0,
        WINTRUST_FILE_INFO, WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_NONE, WTD_REVOKE_NONE,
        WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UICONTEXT_EXECUTE, WTD_UI_NONE,
    };

    if !chemin.is_file() {
        return Verdict::absent("fichier absent");
    }
    // Chemins Windows en UTF-16, termines par zero.
    let mut large: Vec<u16> = chemin.as_os_str().encode_wide().collect();
    large.push(0);

    unsafe {
        let mut fichier = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: windows::core::PCWSTR(large.as_ptr()),
            hFile: Default::default(),
            pgKnownSubject: std::ptr::null_mut(),
        };
        let mut donnees = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            // Aucune fenetre : la decision se prend ici, pas devant Clement.
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: WINTRUST_DATA_0 { pFile: &mut fichier },
            dwStateAction: WTD_STATEACTION_VERIFY,
            // `WTD_REVOCATION_CHECK_NONE` : on ne veut pas dependre du reseau (un PC hors ligne
            // refuserait une mise a jour parfaitement signee). L'horodatage reste verifie.
            dwProvFlags: WTD_REVOCATION_CHECK_NONE,
            dwUIContext: WTD_UICONTEXT_EXECUTE,
            ..Default::default()
        };
        let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        let code = WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut donnees as *mut _ as *mut _);
        // L'etat doit toujours etre rendu, meme en cas d'echec.
        donnees.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(HWND(std::ptr::null_mut()), &mut action, &mut donnees as *mut _ as *mut _);
        Verdict {
            valide: code == 0,
            code,
        }
    }
}

#[cfg(not(windows))]
pub fn verifier(_chemin: &Path) -> Verdict {
    Verdict::absent("plateforme sans Authenticode")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporaire(nom: &str) -> std::path::PathBuf {
        let chemin = std::env::temp_dir().join(format!("trame-signature-{}-{nom}", std::process::id()));
        std::fs::write(&chemin, b"ceci n'est pas un programme signe").unwrap();
        chemin
    }

    #[test]
    fn un_fichier_quelconque_n_est_pas_signe() {
        let chemin = temporaire("quelconque.exe");
        let verdict = verifier(&chemin);
        assert!(!verdict.valide, "un fichier ecrit a la main ne peut pas etre signe");
        let _ = std::fs::remove_file(&chemin);
    }

    #[test]
    fn un_fichier_absent_ne_part_pas() {
        let verdict = verifier(&std::env::temp_dir().join("trame-signature-absent-xyz.exe"));
        assert!(!verdict.valide);
    }

    /// Le binaire de ce projet, s'il est signe, doit passer : c'est le controle qui compte, et
    /// il ne s'execute que si `dist\trame.exe` existe (poste de fabrication).
    #[test]
    fn le_binaire_signe_du_projet_passe_quand_il_existe() {
        let chemin = std::path::Path::new("dist").join("trame.exe");
        if !chemin.is_file() {
            return; // pas de livrable sous la main : rien a prouver ici
        }
        let verdict = verifier(&chemin);
        assert!(verdict.valide, "le trame.exe signe doit passer : {verdict:?}");
    }
}
