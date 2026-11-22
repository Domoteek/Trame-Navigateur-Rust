//! Protocole `trame:` : sert l'interface locale depuis le dossier `ui/`.
//!
//! POURQUOI PAS `file://` — mesure du 2026-11-20 sur ce poste :
//! une page chargee en `file://` recoit bien `window.ipc` et `postMessage` ne leve aucune
//! erreur, mais le message n'arrive JAMAIS au gestionnaire IPC de wry (origine opaque,
//! envoi ignore silencieusement). La meme page servie par une origine http fonctionne.
//! Toute l'interface est donc servie par ce protocole, jamais par `file://`.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use http::{header, Response};

/// Dossier des extensions Chrome actives du profil (les desactivees sont a cote).
static EXTENSIONS_CHROME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Declare ou vivent les extensions Chrome, pour servir leurs icones a la barre.
pub fn definir_extensions_chrome(dossier: PathBuf) {
    let _ = EXTENSIONS_CHROME.set(dossier);
}

/// `/ext-icone/<actives|desactivees>/<id>/<chemin>` : l'icone d'une extension Chrome.
///
/// La barre est servie par `trame:` et ne peut pas lire `chrome-extension://` (autre origine,
/// et rien n'existe pour une extension eteinte). On sert donc le fichier depuis le disque — mais
/// seulement une image, et seulement dans le dossier de l'extension nommee.
fn icone_extension(chemin: &str) -> Option<PathBuf> {
    let reste = chemin.strip_prefix("/ext-icone/")?;
    let (genre, reste) = reste.split_once('/')?;
    let (id, relatif) = reste.split_once('/')?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let actives = EXTENSIONS_CHROME.get()?;
    let base = match genre {
        "actives" => actives.clone(),
        "desactivees" => actives.parent()?.join("extensions-chrome-desactivees"),
        _ => return None,
    };
    let relatif = percent_encoding::percent_decode_str(relatif).decode_utf8().ok()?;
    let fichier = resoudre(&base.join(id), &relatif)?;
    let image = matches!(
        fichier.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("png" | "svg" | "jpg" | "jpeg" | "gif" | "webp" | "ico" | "bmp")
    );
    image.then_some(fichier)
}

/// Construit la reponse HTTP pour une requete du protocole `trame:`.
pub fn repondre(racine_ui: &Path, uri: &str) -> Response<Cow<'static, [u8]>> {
    let chemin = chemin_demande(uri);
    let trouve = if chemin.starts_with("/ext-icone/") {
        icone_extension(&chemin)
    } else {
        resoudre(racine_ui, &chemin)
    };
    let Some(fichier) = trouve else {
        crate::alerte!("protocole : 404 pour {uri} (chemin {chemin})");
        return texte(404, "text/plain; charset=utf-8", format!("introuvable : {chemin}"));
    };
    match std::fs::read(&fichier) {
        Ok(octets) => {
            let mime = mime_pour(&fichier);
            Response::builder()
                .status(200)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "no-store")
                // Une origine unique pour tout le protocole : indispensable pour que les
                // modules et les fetch relatifs de l'interface fonctionnent.
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                .body(Cow::Owned(octets))
                .unwrap_or_else(|_| texte(500, "text/plain", "erreur de construction".into()))
        }
        Err(e) => texte(500, "text/plain; charset=utf-8", format!("lecture impossible : {e}")),
    }
}

fn texte(code: u16, mime: &str, corps: String) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(code)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Cow::Owned(corps.into_bytes()))
        .expect("reponse valide")
}

/// Extrait le chemin utile de l'URI, quelle que soit la reecriture de wry sous Windows
/// (`trame://localhost/x` devient `http://trame.localhost/x`).
fn chemin_demande(uri: &str) -> String {
    let brut = match url::Url::parse(uri) {
        Ok(u) => u.path().to_string(),
        Err(_) => uri.to_string(),
    };
    let mut chemin = brut;
    if chemin.is_empty() || chemin == "/" {
        chemin = "/newtab.html".to_string();
    }
    chemin
}

/// Resout un chemin demande dans la racine, en refusant toute remontee (`..`).
fn resoudre(racine: &Path, chemin: &str) -> Option<PathBuf> {
    let relatif = chemin.trim_start_matches('/');
    if relatif.contains("..") {
        return None;
    }
    let candidat = racine.join(relatif);
    let canonique = candidat.canonicalize().ok()?;
    let racine_canonique = racine.canonicalize().ok()?;
    if canonique.starts_with(&racine_canonique) && canonique.is_file() {
        Some(canonique)
    } else {
        None
    }
}

fn mime_pour(chemin: &Path) -> &'static str {
    match chemin.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_chemin_vient_de_l_uri() {
        assert_eq!(chemin_demande("trame://localhost/chrome.html"), "/chrome.html");
        assert_eq!(chemin_demande("http://trame.localhost/chrome.html"), "/chrome.html");
        assert_eq!(chemin_demande("trame://localhost/"), "/newtab.html");
    }

    #[test]
    fn la_remontee_est_refusee() {
        let racine = std::env::temp_dir();
        assert!(resoudre(&racine, "/../windows/win.ini").is_none());
    }

    #[test]
    fn une_remontee_encodee_est_refusee() {
        let racine = std::env::temp_dir();
        // Une URI garde ses octets encodes : `%2e%2e` n'est pas `..` pour le disque, donc la
        // resolution echoue d'elle-meme. Le test fige ce comportement.
        assert!(resoudre(&racine, "/%2e%2e/%2e%2e/windows/win.ini").is_none());
        assert!(resoudre(&racine, "/..%5cwindows%5cwin.ini").is_none());
        assert!(resoudre(&racine, "/sous/../../secret.txt").is_none());
    }

    #[test]
    fn un_chemin_absolu_ne_sort_pas_de_la_racine() {
        let racine = std::env::temp_dir();
        // Sous Windows, `join` avec un chemin absolu remplace la base : le controle de
        // racine est donc le seul rempart.
        assert!(resoudre(&racine, "/C:/Windows/win.ini").is_none());
        assert!(resoudre(&racine, "/C:\\Windows\\win.ini").is_none());
    }

    #[test]
    fn un_fichier_reel_est_servi_avec_son_type() {
        let racine = std::env::temp_dir().join(format!("trame-protocole-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        std::fs::create_dir_all(&racine).unwrap();
        std::fs::write(racine.join("page.html"), "<h1>ok</h1>").unwrap();
        std::fs::write(racine.join("style.css"), "body{}").unwrap();

        let reponse = repondre(&racine, "trame://localhost/page.html");
        assert_eq!(reponse.status(), 200);
        assert_eq!(
            reponse.headers()[header::CONTENT_TYPE],
            "text/html; charset=utf-8"
        );
        assert_eq!(reponse.body().as_ref(), b"<h1>ok</h1>");
        // L'interface se recharge a chaque demarrage : aucun cache ne doit s'interposer.
        assert_eq!(reponse.headers()[header::CACHE_CONTROL], "no-store");

        let css = repondre(&racine, "http://trame.localhost/style.css");
        assert_eq!(css.headers()[header::CONTENT_TYPE], "text/css; charset=utf-8");

        // Un dossier n'est pas un fichier : 404, jamais un listing.
        std::fs::create_dir_all(racine.join("sous")).unwrap();
        let dossier = repondre(&racine, "trame://localhost/sous");
        assert_eq!(dossier.status(), 404);

        let absent = repondre(&racine, "trame://localhost/absent.js");
        assert_eq!(absent.status(), 404);
        let _ = std::fs::remove_dir_all(&racine);
    }

    #[test]
    fn l_icone_d_une_extension_est_servie_et_seulement_elle() {
        let profil = std::env::temp_dir().join(format!("trame-icone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profil);
        let actives = profil.join("extensions-chrome");
        let ext = actives.join("abcdefghijklmnop");
        std::fs::create_dir_all(ext.join("images")).unwrap();
        std::fs::write(ext.join("images").join("icone 32.png"), b"\x89PNG").unwrap();
        std::fs::write(ext.join("background.js"), b"secret").unwrap();
        std::fs::create_dir_all(profil.join("ui")).unwrap();
        definir_extensions_chrome(actives.clone());
        // Une autre execution de test a pu fixer la valeur avant : on ne verifie que si c'est la notre.
        if EXTENSIONS_CHROME.get() == Some(&actives) {
            let ui = profil.join("ui");
            let ok = repondre(&ui, "trame://localhost/ext-icone/actives/abcdefghijklmnop/images/icone%2032.png");
            assert_eq!(ok.status(), 200);
            assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/png");
            // Un script de l'extension n'est pas une image : refuse.
            let js = repondre(&ui, "trame://localhost/ext-icone/actives/abcdefghijklmnop/background.js");
            assert_eq!(js.status(), 404);
            // Remontee hors du dossier de l'extension : refusee.
            let sortie = repondre(&ui, "trame://localhost/ext-icone/actives/abcdefghijklmnop/../../ui/x.png");
            assert_eq!(sortie.status(), 404);
            // Identifiant forge : refuse.
            let forge = repondre(&ui, "trame://localhost/ext-icone/actives/..%5c..%5cui/x.png");
            assert_eq!(forge.status(), 404);
        }
        let _ = std::fs::remove_dir_all(&profil);
    }

    #[test]
    fn un_type_inconnu_est_servi_en_binaire() {
        assert_eq!(
            mime_pour(Path::new("x.bin")),
            "application/octet-stream"
        );
        assert_eq!(mime_pour(Path::new("x.woff2")), "font/woff2");
        assert_eq!(mime_pour(Path::new("x")), "application/octet-stream");
    }
}
