//! Liens ouverts depuis l'exterieur : Trame navigateur par defaut de Windows.
//!
//! Windows lance `trame.exe "<adresse>"` pour chaque lien clique ailleurs (mail, document,
//! autre application). Deux cas :
//! - Trame n'est pas ouvert : l'adresse s'ouvre dans un onglet au demarrage ;
//! - Trame est deja ouvert (le verrou du profil est pris) : la seconde instance depose l'adresse
//!   dans une **boite aux lettres** du profil (`a-ouvrir\`), autorise l'instance ouverte a passer
//!   au premier plan, et s'arrete ; l'instance ouverte releve la boite a chaque battement.
//! Un fichier par demande, ecrit puis renomme : jamais lu a moitie.

use std::path::{Path, PathBuf};

/// Ce que Windows (ou l'utilisateur) a passe en argument, rendu en adresse ouvrable ; `None`
/// s'il n'y a rien a ouvrir.
pub fn adresse_des_arguments<I: IntoIterator<Item = String>>(arguments: I) -> Option<String> {
    for a in arguments {
        let a = a.trim().trim_matches('"').to_string();
        if a.is_empty() || a.starts_with("--") {
            continue;
        }
        let bas = a.to_ascii_lowercase();
        if bas.starts_with("http://") || bas.starts_with("https://") || bas.starts_with("file:") {
            return Some(a);
        }
        // Un fichier local (.html, .pdf, .svg...) ouvert « avec Trame ».
        let chemin = Path::new(&a);
        if chemin.is_absolute() && chemin.exists() {
            if let Ok(u) = url::Url::from_file_path(chemin) {
                return Some(u.to_string());
            }
        }
    }
    None
}

pub fn boite(racine_profil: &Path) -> PathBuf {
    racine_profil.join("a-ouvrir")
}

/// Depose une demande dans la boite : une adresse, ou vide pour « reviens au premier plan ».
pub fn deposer(racine_profil: &Path, adresse: Option<&str>) -> std::io::Result<()> {
    let dossier = boite(racine_profil);
    std::fs::create_dir_all(&dossier)?;
    let nom = format!("{}-{}", crate::log::maintenant_ms(), std::process::id());
    let provisoire = dossier.join(format!("{nom}.tmp"));
    std::fs::write(&provisoire, adresse.unwrap_or(""))?;
    std::fs::rename(&provisoire, dossier.join(format!("{nom}.url")))
}

/// Releve les demandes deposees (les plus anciennes d'abord) et les retire de la boite.
pub fn relever(racine_profil: &Path) -> Vec<String> {
    let dossier = boite(racine_profil);
    let Ok(lecture) = std::fs::read_dir(&dossier) else {
        return Vec::new();
    };
    let mut fichiers: Vec<PathBuf> = lecture
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("url"))
        .collect();
    fichiers.sort();
    let mut demandes = Vec::new();
    for f in fichiers {
        if let Ok(texte) = std::fs::read_to_string(&f) {
            demandes.push(texte.trim().to_string());
        }
        let _ = std::fs::remove_file(&f);
    }
    demandes
}

/// Autorise l'instance deja ouverte (`pid`) a passer au premier plan : Windows ne le permet qu'a
/// la demande du processus qui a le premier plan — la seconde instance, lancee par le clic.
pub fn autoriser_premier_plan(pid: u32) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
    }
}

/// Ouvre une adresse comme un clic sur un lien (`ShellExecuteW`, verbe « open »).
///
/// Passer `ms-settings:defaultapps?registeredAppUser=Trame` a `explorer` ouvrait l'Explorateur
/// de fichiers au lieu des Parametres (vu le 2026-11-22) : la partie `?…` n'est pas comprise
/// par `explorer`. Rend `false` si Windows refuse (code <= 32).
pub fn ouvrir_uri(uri: &str) -> bool {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let cible = HSTRING::from(uri);
    let code = unsafe { ShellExecuteW(None, w!("open"), &cible, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    code.0 as isize > 32
}

/// Le dossier « Telechargements » de Windows (deplace eventuellement par l'utilisateur).
pub fn dossier_telechargements_windows() -> Option<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let brut = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let chemin = brut.to_string().ok();
        CoTaskMemFree(Some(brut.0 as *const _));
        chemin.map(std::path::PathBuf::from)
    }
}

/// Les deux arguments donnes a `explorer` pour montrer un fichier.
///
/// **Deux**, jamais un seul. Un `/select,` colle au chemin (`/select,C:\…\mon dossier\f.pdf`)
/// est mal compris des que le chemin contient une espace : Windows cite alors l'argument
/// entier, `explorer` ne reconnait plus son prefixe et ouvre son dossier par defaut
/// (« Documents »). Constate le 2026-11-22 sur le PC du fils de Clement : les fichiers
/// arrivaient bien dans « Telechargements », mais « Afficher dans le dossier » ouvrait
/// Documents. Mesure : en deux arguments, Explorer ouvre le bon dossier **et** selectionne le
/// fichier, espace ou pas.
pub fn arguments_select(chemin: &std::path::Path) -> [std::ffi::OsString; 2] {
    [std::ffi::OsString::from("/select,"), chemin.as_os_str().to_os_string()]
}

/// Montre un fichier selectionne dans l'Explorateur (« Afficher dans le dossier »).
/// Rend `false` si Explorer n'a pas pu etre lance.
pub fn montrer_dans_dossier(chemin: &std::path::Path) -> bool {
    let [prefixe, cible] = arguments_select(chemin);
    std::process::Command::new("explorer")
        .arg(prefixe)
        .arg(cible)
        .spawn()
        .is_ok()
}

/// Ouvre un dossier dans l'Explorateur (`ShellExecuteW`, verbe « open »).
///
/// Rend `false` quand Windows refuse — dossier absent, par exemple. C'est la difference avec
/// `explorer.exe <dossier>` : lui, quand le chemin est invalide, n'echoue pas, il ouvre son
/// dossier par defaut, et on croit avoir ouvert autre chose.
pub fn ouvrir_dossier(dossier: &std::path::Path) -> bool {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let cible = HSTRING::from(dossier.as_os_str());
    let code = unsafe { ShellExecuteW(None, w!("open"), &cible, PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    code.0 as isize > 32
}

/// Est-ce que Trame est le navigateur par defaut de l'utilisateur (liens https) ?
pub fn est_par_defaut() -> bool {
    use std::os::windows::process::CommandExt;
    let sortie = std::process::Command::new("reg")
        // Sans console : Trame est une application fenetree, et chaque `reg` ouvrait une fenetre
        // noire qui volait le focus — le retour du focus relancait la verification, d'ou une
        // serie de fenetres a l'ouverture des reglages (2026-11-22).
        .creation_flags(crate::systeme::SANS_CONSOLE)
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
            "/v",
            "ProgId",
        ])
        .output();
    match sortie {
        Ok(s) => String::from_utf8_lossy(&s.stdout).contains("TrameURL"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn montrer_dans_le_dossier_donne_deux_arguments() {
        let chemin = std::path::Path::new(r"C:\Users\Jean Paul\Telechargements\mon document avec espaces.pdf");
        let [prefixe, cible] = arguments_select(chemin);
        assert_eq!(prefixe, std::ffi::OsString::from("/select,"));
        // Le chemin reste UN SEUL argument, espaces comprises : c'est tout l'objet du correctif.
        assert_eq!(std::path::Path::new(&cible), chemin);
        // Un chemin sans espace doit donner exactement la meme forme.
        let simple = std::path::Path::new(r"C:\Telechargements\preuve.pdf");
        let [p2, c2] = arguments_select(simple);
        assert_eq!(p2, prefixe);
        assert_eq!(std::path::Path::new(&c2), simple);
    }

    #[test]
    fn l_adresse_se_lit_dans_les_arguments() {
        let a = |v: &[&str]| adresse_des_arguments(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&["https://exemple.fr/x"]).as_deref(), Some("https://exemple.fr/x"));
        assert_eq!(a(&["\"https://exemple.fr/x\""]).as_deref(), Some("https://exemple.fr/x"));
        assert_eq!(a(&["--un-drapeau", "http://a.b/"]).as_deref(), Some("http://a.b/"));
        assert_eq!(a(&[]), None);
        assert_eq!(a(&["pas-une-adresse"]), None);
        // Un fichier local existant devient une adresse file:.
        let f = std::env::temp_dir().join(format!("trame-externe-{}.html", std::process::id()));
        std::fs::write(&f, "<p>x</p>").unwrap();
        let u = a(&[f.to_string_lossy().as_ref()]).unwrap();
        assert!(u.starts_with("file:///"), "{u}");
        let _ = std::fs::remove_file(&f);
    }

    #[test]
    fn la_boite_aux_lettres_rend_les_demandes_dans_l_ordre_et_se_vide() {
        let racine = std::env::temp_dir().join(format!("trame-boite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        deposer(&racine, Some("https://un.fr/")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        deposer(&racine, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        deposer(&racine, Some("https://deux.fr/")).unwrap();
        assert_eq!(relever(&racine), vec!["https://un.fr/", "", "https://deux.fr/"]);
        assert!(relever(&racine).is_empty(), "la boite est videe");
        let _ = std::fs::remove_dir_all(&racine);
    }
}
