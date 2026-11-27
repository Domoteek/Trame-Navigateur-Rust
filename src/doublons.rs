//! Doublons : un fichier qui vient d'arriver est-il deja la, a l'identique ?
//!
//! On ne compare que des fichiers de MEME taille (une lecture de metadonnees, gratuite), puis
//! octet par octet, par blocs, en s'arretant a la premiere difference. Pas d'empreinte gardee :
//! rien a tenir a jour, et un fichier modifie depuis n'est jamais pris pour un doublon.
//!
//! Candidats : les fichiers voisins (meme dossier — c'est la que tombe « photo (2).png ») et ceux
//! que la liste des telechargements connait ailleurs (une meme image rangee hier dans Images\).
//! Appele hors du fil principal (`app/rangement.rs`) : un gros fichier ne fige jamais Trame.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// Taille au-dela de laquelle on ne compare plus (une image disque de 8 Go relue deux fois pour
/// un avertissement n'en vaut pas la peine).
pub const TAILLE_MAX: u64 = 2 * 1024 * 1024 * 1024;

/// Les deux fichiers ont-ils exactement le meme contenu ?
pub fn identiques(a: &Path, b: &Path) -> bool {
    let (Ok(fa), Ok(fb)) = (File::open(a), File::open(b)) else { return false };
    let (Ok(ma), Ok(mb)) = (fa.metadata(), fb.metadata()) else { return false };
    if ma.len() != mb.len() {
        return false;
    }
    let mut ra = BufReader::with_capacity(1 << 20, fa);
    let mut rb = BufReader::with_capacity(1 << 20, fb);
    let mut ba = vec![0u8; 1 << 16];
    let mut bb = vec![0u8; 1 << 16];
    loop {
        let n = match lire_plein(&mut ra, &mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let m = match lire_plein(&mut rb, &mut bb) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if n != m || ba[..n] != bb[..m] {
            return false;
        }
        if n == 0 {
            return true;
        }
    }
}

/// Remplit le tampon autant que possible (un `read` peut rendre moins que demande).
fn lire_plein(r: &mut impl Read, tampon: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < tampon.len() {
        match r.read(&mut tampon[total..])? {
            0 => break,
            n => total += n,
        }
    }
    Ok(total)
}

/// Un fichier deja present, identique a `fichier` : d'abord ses voisins de dossier, puis `connus`.
/// `None` si aucun (ou si le fichier est vide, ou trop gros pour valoir une relecture).
pub fn chercher(fichier: &Path, connus: &[PathBuf]) -> Option<PathBuf> {
    let taille = std::fs::metadata(fichier).ok().filter(|m| m.is_file())?.len();
    if taille == 0 || taille > TAILLE_MAX {
        return None;
    }
    let voisins = fichier
        .parent()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()));
    let mut vus: Vec<PathBuf> = Vec::new();
    for candidat in voisins.chain(connus.iter().cloned()) {
        if candidat == fichier || vus.contains(&candidat) {
            continue;
        }
        vus.push(candidat.clone());
        let meme_taille = std::fs::metadata(&candidat).map(|m| m.is_file() && m.len() == taille).unwrap_or(false);
        if meme_taille && identiques(fichier, &candidat) {
            return Some(candidat);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trame-doublons-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn un_voisin_identique_est_trouve() {
        let d = dossier("voisin");
        std::fs::write(d.join("photo.png"), b"abc").unwrap();
        std::fs::write(d.join("photo (2).png"), b"abc").unwrap();
        std::fs::write(d.join("autre.png"), b"abd").unwrap();
        assert_eq!(chercher(&d.join("photo (2).png"), &[]), Some(d.join("photo.png")));
        assert_eq!(chercher(&d.join("autre.png"), &[]), None, "meme taille, contenu different");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_fichier_connu_ailleurs_est_trouve() {
        let d = dossier("connu");
        std::fs::create_dir_all(d.join("Images")).unwrap();
        std::fs::write(d.join("Images").join("a.png"), vec![7u8; 200_000]).unwrap();
        std::fs::write(d.join("b.png"), vec![7u8; 200_000]).unwrap();
        assert_eq!(chercher(&d.join("b.png"), &[d.join("Images").join("a.png")]), Some(d.join("Images").join("a.png")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn vide_ou_seul_n_a_pas_de_doublon() {
        let d = dossier("vide");
        std::fs::write(d.join("vide.txt"), b"").unwrap();
        std::fs::write(d.join("vide2.txt"), b"").unwrap();
        std::fs::write(d.join("seul.txt"), b"x").unwrap();
        assert_eq!(chercher(&d.join("vide.txt"), &[]), None);
        assert_eq!(chercher(&d.join("seul.txt"), &[]), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn difference_tardive() {
        let d = dossier("tard");
        let mut a = vec![1u8; 300_000];
        std::fs::write(d.join("a"), &a).unwrap();
        a[299_999] = 2;
        std::fs::write(d.join("b"), &a).unwrap();
        assert!(!identiques(&d.join("a"), &d.join("b")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
