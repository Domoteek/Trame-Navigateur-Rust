//! Installation des ressources de l'application dans le dossier du profil.
//!
//! **Pourquoi.** Jusqu'ici Trame lisait son interface (`ui/`) et ses extensions livrees à côté
//! de l'exécutable, ou dans le dossier du projet en développement. Un binaire déplacé, un
//! `cargo clean`, et l'application n'avait plus d'interface ; un profil correctement rempli ne
//! suffisait pas. Et le profil de WebView2 — cookies, connexions aux extensions, service
//! workers — était créé **à côté de l'exécutable** (`trame.exe.WebView2`), donc dans
//! `target\release`, effaçable par un nettoyage de build.
//!
//! Désormais tout ce dont le navigateur a besoin vit sous `%APPDATA%\Trame\` :
//!
//! ```text
//! ui\                     interface, miroir de la version livree (mise a jour a chaque demarrage)
//! extensions\             extensions Trame, amorcees une seule fois (l'utilisateur peut les modifier)
//! extensions-chrome\      extensions Chrome actives
//! webview2\EBWebView\     profil WebView2 : cookies, connexions, stockage des extensions
//! ```
//!
//! L'interface est un **miroir** : un fichier retiré de la livraison disparaît du profil, sinon
//! une vieille copie pourrait masquer un correctif. Les extensions Trame, elles, ne sont copiées
//! qu'une fois : l'utilisateur a le droit de les modifier, on ne les écrase pas.

use std::path::{Path, PathBuf};

use crate::{alerte, info};

/// Ce qui a été fait lors de la synchronisation.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Bilan {
    pub copies: usize,
    pub mises_a_jour: usize,
    pub supprimes: usize,
    pub identiques: usize,
}

impl Bilan {
    pub fn a_change(&self) -> bool {
        self.copies + self.mises_a_jour + self.supprimes > 0
    }

    pub fn resume(&self) -> String {
        format!(
            "{} copié(s), {} mis à jour, {} retiré(s), {} identique(s)",
            self.copies, self.mises_a_jour, self.supprimes, self.identiques
        )
    }
}

/// Dossier qui contient les ressources livrées (`ui/`, `extensions/`).
///
/// À côté de l'exécutable pour un binaire publié, dans le dossier du projet en développement.
/// `None` : rien à installer — l'application utilise alors ce qui est déjà dans le profil.
pub fn source() -> Option<PathBuf> {
    let a_cote = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()));
    if let Some(dossier) = a_cote {
        if dossier.join("ui").join("chrome.html").is_file() {
            return Some(dossier);
        }
    }
    let projet = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if projet.join("ui").join("chrome.html").is_file() {
        return Some(projet);
    }
    None
}

/// Installe ou met à jour l'interface dans le profil.
pub fn installer_interface(source: &Path, profil: &Path) -> Result<Bilan, String> {
    let depuis = source.join("ui");
    if !depuis.join("chrome.html").is_file() {
        return Err(format!(
            "aucune interface à installer dans {}",
            depuis.display()
        ));
    }
    miroir(&depuis, &profil.join("ui"))
}

/// Copie récursivement `source` vers `destination`, en mettant à jour et en retirant.
///
/// Comparaison **octet par octet** : un fichier identique n'est pas réécrit, donc l'horodatage
/// ne change pas et un démarrage normal ne touche à rien. L'interface pèse quelques dizaines de
/// kilo-octets : c'est plus sûr qu'une comparaison de dates, qui se trompe après une copie.
pub fn miroir(source: &Path, destination: &Path) -> Result<Bilan, String> {
    let mut bilan = Bilan::default();
    fs_creer(destination)?;
    let mut attendus: Vec<String> = Vec::new();
    let entrees = std::fs::read_dir(source).map_err(|e| format!("lecture de {} : {e}", source.display()))?;
    for entree in entrees.flatten() {
        let chemin = entree.path();
        let nom = chemin
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if nom.is_empty() {
            continue;
        }
        if chemin.is_dir() {
            // Un sous-dossier est traite en entier. Son NOM doit compter comme attendu : sans
            // cela la passe de suppression du parent effacerait le dossier qu'on vient de
            // remplir (constate par le test des sous-dossiers).
            attendus.push(nom.clone());
            let sous = miroir(&chemin, &destination.join(&nom))?;
            bilan.copies += sous.copies;
            bilan.mises_a_jour += sous.mises_a_jour;
            bilan.supprimes += sous.supprimes;
            bilan.identiques += sous.identiques;
            continue;
        }
        attendus.push(nom.clone());
        let cible = destination.join(&nom);
        let octets = std::fs::read(&chemin).map_err(|e| format!("lecture de {} : {e}", chemin.display()))?;
        let deja_bon = std::fs::read(&cible)
            .map(|existants| existants == octets)
            .unwrap_or(false);
        if deja_bon {
            bilan.identiques += 1;
            continue;
        }
        let existait = cible.exists();
        std::fs::write(&cible, &octets)
            .map_err(|e| format!("ecriture de {} : {e}", cible.display()))?;
        if existait {
            bilan.mises_a_jour += 1;
        } else {
            bilan.copies += 1;
        }
    }
    // Ce que la livraison ne contient plus disparait : un vieux fichier pourrait masquer un
    // correctif (c'est arrive avec un `chrome.js` perime).
    if let Ok(existants) = std::fs::read_dir(destination) {
        for entree in existants.flatten() {
            let chemin = entree.path();
            let nom = chemin
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if attendus.contains(&nom) {
                continue;
            }
            if chemin.is_dir() {
                // Un dossier disparu de la livraison est retire en entier.
                if std::fs::remove_dir_all(&chemin).is_ok() {
                    bilan.supprimes += 1;
                }
            } else if std::fs::remove_file(&chemin).is_ok() {
                bilan.supprimes += 1;
            }
        }
    }
    Ok(bilan)
}

fn fs_creer(dossier: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dossier).map_err(|e| format!("dossier {} : {e}", dossier.display()))
}

/// Copie recursive, utilisee si un simple `rename` ne suffit pas (volumes differents).
pub fn copier_arbre(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entree in std::fs::read_dir(source)?.flatten() {
        let chemin = entree.path();
        let nom = chemin.file_name().unwrap_or_default().to_os_string();
        let cible = destination.join(nom);
        if chemin.is_dir() {
            copier_arbre(&chemin, &cible)?;
        } else {
            std::fs::copy(&chemin, &cible)?;
        }
    }
    Ok(())
}

/// Taille d'un dossier en octets (pour dire ce qui a ete deplace).
pub fn taille(dossier: &Path) -> u64 {
    let mut total = 0;
    let Ok(lecture) = std::fs::read_dir(dossier) else {
        return 0;
    };
    for entree in lecture.flatten() {
        let chemin = entree.path();
        if chemin.is_dir() {
            total += taille(&chemin);
        } else if let Ok(meta) = chemin.metadata() {
            total += meta.len();
        }
    }
    total
}

/// Installe l'interface au démarrage et raconte ce qu'elle a fait.
pub fn au_demarrage(profil: &Path) {
    let Some(source) = source() else {
        info!("ressources : aucune source livrée, le profil fait foi");
        return;
    };
    match installer_interface(&source, profil) {
        Ok(bilan) if bilan.a_change() => {
            info!("interface installee dans le profil : {}", bilan.resume());
        }
        Ok(bilan) => {
            info!("interface deja a jour ({})", bilan.resume());
        }
        Err(e) => {
            alerte!("installation de l'interface impossible : {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trame-res-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn ecrire(racine: &Path, relatif: &str, contenu: &str) {
        let chemin = racine.join(relatif);
        std::fs::create_dir_all(chemin.parent().unwrap()).unwrap();
        std::fs::write(chemin, contenu).unwrap();
    }

    #[test]
    fn un_premier_install_copie_tout() {
        let atelier = dossier("premier");
        let source = atelier.join("source");
        let profil = atelier.join("profil");
        ecrire(&source, "ui/chrome.html", "<html>a</html>");
        ecrire(&source, "ui/chrome.js", "// js");
        let bilan = installer_interface(&source, &profil).unwrap();
        assert_eq!(bilan.copies, 2);
        assert_eq!(bilan.mises_a_jour, 0);
        assert_eq!(
            std::fs::read_to_string(profil.join("ui").join("chrome.html")).unwrap(),
            "<html>a</html>"
        );
        let _ = std::fs::remove_dir_all(&atelier);
    }

    #[test]
    fn un_second_passage_ne_reecrit_rien() {
        let atelier = dossier("stable");
        let source = atelier.join("source");
        let profil = atelier.join("profil");
        ecrire(&source, "ui/chrome.html", "<html>a</html>");
        installer_interface(&source, &profil).unwrap();
        let second = installer_interface(&source, &profil).unwrap();
        assert!(!second.a_change(), "rien ne doit bouger : {}", second.resume());
        assert_eq!(second.identiques, 1);
        let _ = std::fs::remove_dir_all(&atelier);
    }

    #[test]
    fn un_fichier_modifie_est_mis_a_jour() {
        let atelier = dossier("maj");
        let source = atelier.join("source");
        let profil = atelier.join("profil");
        ecrire(&source, "ui/chrome.js", "v1");
        miroir(&source.join("ui"), &profil.join("ui")).unwrap();
        ecrire(&source, "ui/chrome.js", "v2");
        let bilan = miroir(&source.join("ui"), &profil.join("ui")).unwrap();
        assert_eq!(bilan.mises_a_jour, 1);
        assert_eq!(
            std::fs::read_to_string(profil.join("ui").join("chrome.js")).unwrap(),
            "v2"
        );
        let _ = std::fs::remove_dir_all(&atelier);
    }

    #[test]
    fn un_fichier_retire_de_la_livraison_disparait_du_profil() {
        let atelier = dossier("retire");
        let source = atelier.join("source");
        let profil = atelier.join("profil");
        ecrire(&source, "ui/chrome.html", "a");
        ecrire(&source, "ui/vieux.js", "obsolete");
        installer_interface(&source, &profil).unwrap();
        std::fs::remove_file(source.join("ui").join("vieux.js")).unwrap();
        let bilan = installer_interface(&source, &profil).unwrap();
        assert_eq!(bilan.supprimes, 1);
        assert!(!profil.join("ui").join("vieux.js").exists());
        let _ = std::fs::remove_dir_all(&atelier);
    }

    #[test]
    fn les_sous_dossiers_sont_miroirs_aussi() {
        let atelier = dossier("sous-dossier");
        let source = atelier.join("source");
        let profil = atelier.join("profil");
        ecrire(&source, "ui/img/logo.svg", "<svg/>");
        ecrire(&source, "ui/theme/sombre.css", "body{}");
        miroir(&source.join("ui"), &profil.join("ui")).unwrap();
        assert!(profil.join("ui").join("img").join("logo.svg").is_file());
        assert!(profil.join("ui").join("theme").join("sombre.css").is_file());
        std::fs::remove_dir_all(source.join("ui").join("theme")).unwrap();
        miroir(&source.join("ui"), &profil.join("ui")).unwrap();
        assert!(
            !profil.join("ui").join("theme").exists(),
            "un dossier retire de la livraison doit disparaitre du profil"
        );
        let _ = std::fs::remove_dir_all(&atelier);
    }

    #[test]
    fn une_source_sans_interface_est_refusee() {
        let atelier = dossier("sans-ui");
        let source = atelier.join("source");
        std::fs::create_dir_all(source.join("ui")).unwrap();
        let message = installer_interface(&source, &atelier.join("profil")).unwrap_err();
        assert!(message.contains("aucune interface"), "{message}");
        let _ = std::fs::remove_dir_all(&atelier);
    }
}
