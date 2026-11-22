//! Mise a jour : la fiche Firestore du produit, exactement comme celle de Cadre.
//!
//! Trame n'est pas diffusee (usage interne) : pas de page de vente, pas de Stripe, pas
//! d'entree dans la boutique. La fiche `products/trame` du projet Firebase
//! `desktopiconviewer` sert donc de manifeste, et elle est lisible **sans compte** — les
//! regles Firestore l'autorisent, et c'est deja ainsi que Cadre verifie sa version
//! (`firestore.googleapis.com/v1/projects/desktopiconviewer/databases/(default)/documents/products/<id>`).
//!
//! Ce module ne fait que **lire et comparer**. Il n'ecrit rien et ne telecharge rien lui-meme :
//! le nouveau setup passe par le gestionnaire de telechargements de Trame (dossier choisi,
//! journal, reprise), pas par un second chemin qui aurait ses propres regles.

use std::process::Command;

/// La fiche produit de Trame, lue sans authentification.
pub const FICHE: &str = "https://firestore.googleapis.com/v1/projects/desktopiconviewer/databases/(default)/documents/products/trame";

/// La version de ce binaire.
pub const ACTUELLE: &str = env!("CARGO_PKG_VERSION");

/// Une version plus recente annoncee par la fiche.
#[derive(Clone, Debug, PartialEq)]
pub struct Nouveaute {
    pub version: String,
    pub url: String,
}

/// Vrai si `a` est une version plus recente que `b` (« 0.4.12 » > « 0.4.9 »).
///
/// Comparaison **numerique**, champ par champ : une comparaison de chaines dirait que
/// « 0.4.9 » est plus recente que « 0.4.12 ». Ce qui ne se lit pas comme un nombre compte
/// pour zero.
pub fn plus_recente(a: &str, b: &str) -> bool {
    let champs = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split('.')
            .map(|c| c.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (gauche, droite) = (champs(a), champs(b));
    for i in 0..gauche.len().max(droite.len()) {
        let g = gauche.get(i).copied().unwrap_or(0);
        let d = droite.get(i).copied().unwrap_or(0);
        if g != d {
            return g > d;
        }
    }
    false
}

/// Lit la fiche et rend la mise a jour a proposer, s'il y en a une plus recente que `actuelle`.
///
/// `curl.exe` (present depuis Windows 10) evite d'ajouter un client HTTP au binaire pour un
/// seul GET. Sans console : Trame est une application fenetree.
pub fn verifier(actuelle: &str) -> Option<Nouveaute> {
    use std::os::windows::process::CommandExt;
    let sortie = Command::new("curl")
        .args(["-s", "--max-time", "10", FICHE])
        .creation_flags(crate::systeme::SANS_CONSOLE)
        .output()
        .ok()?;
    if !sortie.status.success() {
        return None;
    }
    lire(&String::from_utf8_lossy(&sortie.stdout), actuelle)
}

/// Extrait la nouveaute d'une reponse Firestore (texte JSON), si elle est plus recente.
pub fn lire(json: &str, actuelle: &str) -> Option<Nouveaute> {
    let valeur: serde_json::Value = serde_json::from_str(json).ok()?;
    let champs = valeur.get("fields")?;
    let texte = |nom: &str| {
        champs
            .get(nom)
            .and_then(|c| c.get("stringValue"))
            .and_then(|c| c.as_str())
            .map(str::to_string)
    };
    // `isActive` n'est ecrit que lorsqu'il est faux : absent vaut actif.
    if champs
        .get("isActive")
        .and_then(|c| c.get("booleanValue"))
        .and_then(|c| c.as_bool())
        == Some(false)
    {
        return None;
    }
    let version = texte("version")?;
    if version.trim().is_empty() || !plus_recente(&version, actuelle) {
        return None;
    }
    let url = texte("download_url")?;
    if !url.starts_with("http") {
        return None;
    }
    Some(Nouveaute { version, url })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_versions_se_comparent_en_nombres() {
        assert!(plus_recente("0.4.12", "0.4.9"), "12 vient apres 9");
        assert!(plus_recente("0.5.0", "0.4.99"));
        assert!(plus_recente("1.0", "0.9.9"));
        assert!(plus_recente("v0.4.13", "0.4.12"), "le v de devant ne compte pas");
        assert!(!plus_recente("0.4.12", "0.4.12"), "la meme version n'est pas plus recente");
        assert!(!plus_recente("0.4.9", "0.4.12"));
        assert!(!plus_recente("", "0.4.12"));
    }

    #[test]
    fn une_fiche_plus_recente_est_proposee() {
        let fiche = |version: &str, url: &str| {
            format!(
                r#"{{"fields":{{"version":{{"stringValue":"{version}"}},"download_url":{{"stringValue":"{url}"}}}}}}"#
            )
        };
        let nouvelle = lire(&fiche("0.9.0", "https://storage.googleapis.com/b/Trame-0.9.0-setup.exe"), "0.4.12");
        let nouvelle = nouvelle.expect("une version plus recente doit etre proposee");
        assert_eq!(nouvelle.version, "0.9.0");
        assert!(lire(&fiche("0.4.12", "https://x/y.exe"), "0.4.12").is_none(), "la meme version ne se propose pas");
        assert!(lire(&fiche("0.4.11", "https://x/y.exe"), "0.4.12").is_none(), "on ne revient pas en arriere");
        assert!(lire(&fiche("0.9.0", "pas-une-adresse"), "0.4.12").is_none(), "une adresse invalide est refusee");
        assert!(lire("{}", "0.4.12").is_none());
        assert!(lire("pas du json", "0.4.12").is_none());
    }

    #[test]
    fn une_fiche_desactivee_ne_propose_rien() {
        let json = r#"{"fields":{"version":{"stringValue":"0.9.0"},"download_url":{"stringValue":"https://x/y.exe"},"isActive":{"booleanValue":false}}}"#;
        assert!(lire(json, "0.4.12").is_none());
    }
}
