//! Gestion des extensions Chrome chargees par WebView2 : installation, activation, retrait.
//!
//! WebView2 ne charge qu'un **dossier decompresse** (ici sous
//! `%LOCALAPPDATA%\Trame\extensions-chrome\`). wry ne l'utilise qu'au demarrage ; le pilotage a
//! chaud (ajout, extinction, retrait) vit dans `moteur_ext.rs`. Ce qui doit survivre a un
//! redemarrage vit ici, sur le disque, avec trois dossiers :
//!
//! ```text
//! extensions-chrome\            active : chargee au demarrage
//! extensions-chrome-desactivees\ coupee : gardee, pas chargee
//! extensions-chrome-archives\    retiree ou remplacee : gardee, jamais detruite
//! ```
//!
//! Activer ou desactiver = **deplacer** le dossier d'un cote ou de l'autre. C'est le seul
//! mecanisme qui survit a un redemarrage, et un deplacement se defait. Le moteur, lui, est
//! aligne tout de suite par `moteur_ext.rs` : aucun changement n'attend le redemarrage.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::{alerte, info};

/// Les trois dossiers d'extensions du profil, derives du dossier actif.
pub struct Dossiers {
    pub active: PathBuf,
}

impl Dossiers {
    pub fn depuis(active: PathBuf) -> Self {
        Dossiers { active }
    }

    fn parent(&self) -> PathBuf {
        self.active
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn desactivees(&self) -> PathBuf {
        self.parent().join("extensions-chrome-desactivees")
    }

    pub fn archives(&self) -> PathBuf {
        self.parent().join("extensions-chrome-archives")
    }
}

/// Met l'extension a l'abri (archive) : jamais de suppression.
fn archiver(dossier: &Path, archives: &Path, suffixe: &str) -> Result<PathBuf, String> {
    let nom = dossier
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "extension".to_string());
    std::fs::create_dir_all(archives).map_err(|e| format!("archive impossible : {e}"))?;
    let mut cible = archives.join(format!("{nom}-{suffixe}"));
    let mut numero = 2;
    while cible.exists() {
        cible = archives.join(format!("{nom}-{suffixe}-{numero}"));
        numero += 1;
    }
    std::fs::rename(dossier, &cible).map_err(|e| format!("déplacement impossible : {e}"))?;
    info!("extension archivee : {}", cible.display());
    Ok(cible)
}

/// Deplace un dossier d'extension vers `destination` (un parent existant).
fn deplacer(dossier: &Path, destination: &Path) -> Result<(), String> {
    std::fs::create_dir_all(destination).map_err(|e| format!("dossier impossible : {e}"))?;
    let nom = dossier
        .file_name()
        .ok_or_else(|| "nom de dossier illisible".to_string())?;
    let cible = destination.join(nom);
    if cible.exists() {
        return Err(format!(
            "« {} » existe déjà : rien n'a été déplacé",
            cible.display()
        ));
    }
    std::fs::rename(dossier, &cible).map_err(|e| format!("déplacement impossible : {e}"))
}

// ------------------------------------------------------------------ lecture

/// Tout ce que l'interface doit savoir d'une extension installee.
pub fn lister(dossiers: &Dossiers) -> Vec<Value> {
    let sources = lire_sources(&dossiers.parent());
    let mut out = Vec::new();
    out.extend(lire_dossier(&dossiers.active, true, &sources));
    out.extend(lire_dossier(&dossiers.desactivees(), false, &sources));
    out.sort_by(|a, b| {
        let nom = |v: &Value| v["nom"].as_str().unwrap_or("").to_lowercase();
        nom(a).cmp(&nom(b))
    });
    out
}

fn lire_dossier(dossier: &Path, actif: bool, sources: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Ok(lecture) = std::fs::read_dir(dossier) else {
        return out;
    };
    let mut entrees: Vec<PathBuf> = lecture.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    entrees.sort();
    for chemin in entrees {
        let identifiant = chemin
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let manifeste = std::fs::read_to_string(chemin.join("manifest.json"))
            .ok()
            .and_then(|texte| serde_json::from_str::<Value>(&texte).ok());
        let (nom, version, description) = match &manifeste {
            Some(m) => (
                Some(nom_extension_chrome(m, &chemin))
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| identifiant.clone()),
                m["version"].as_str().unwrap_or("").to_string(),
                localiser_champ(m, &chemin, "description").unwrap_or_default(),
            ),
            None => (
                identifiant.clone(),
                String::new(),
                "manifest.json illisible — extension ignorée par WebView2".to_string(),
            ),
        };
        let popup = manifeste.as_ref().and_then(|m| {
            m["action"]["default_popup"]
                .as_str()
                .or_else(|| m["browser_action"]["default_popup"].as_str())
                .map(|s| s.to_string())
        });
        // Une icone d'action sans fenetre (Joko) : le clic est remis a l'extension
        // (`chrome.action.onClicked`), qui agit dans la page.
        let clic = popup.is_none()
            && manifeste
                .as_ref()
                .is_some_and(|m| m["action"].is_object() || m["browser_action"].is_object());
        let options = manifeste.as_ref().and_then(|m| {
            m["options_ui"]["page"]
                .as_str()
                .or_else(|| m["options_page"].as_str())
                .map(|s| s.to_string())
        });
        let icone = manifeste
            .as_ref()
            .and_then(|m| icone_relative(m, &chemin))
            .map(|relatif| {
                let dossier = if actif { "actives" } else { "desactivees" };
                format!("/ext-icone/{dossier}/{identifiant}/{relatif}")
            });
        out.push(json!({
            "id": identifiant,
            "nom": nom,
            "icone": icone,
            "version": version,
            "description": description,
            "chemin": chemin.display().to_string(),
            "valide": manifeste.is_some(),
            "actif": actif,
            "popup": popup,
            "clic": clic,
            "options": options,
            "permissions": manifeste.as_ref().map(permissions).unwrap_or_default(),
            "tailleKo": taille_ko(&chemin),
            "source": sources[&identifiant]["source"].as_str().unwrap_or("import"),
            "installeLe": sources[&identifiant]["quand"].as_u64().unwrap_or(0),
        }));
    }
    out
}

/// L'icone a montrer dans la barre, chemin relatif au dossier de l'extension.
///
/// Comme Chrome : l'icone d'action (`action.default_icon`, ou `browser_action` en MV2) d'abord,
/// puis les icones de l'extension. Parmi les tailles, la plus proche de 32 px par le haut (le
/// bouton fait 16 px logiques, soit 32 px sur un ecran a 200 %), sinon la plus grande.
pub fn icone_relative(manifeste: &Value, dossier: &Path) -> Option<String> {
    let candidats = [
        &manifeste["action"]["default_icon"],
        &manifeste["browser_action"]["default_icon"],
        &manifeste["icons"],
    ];
    for candidat in candidats {
        let choisie = match candidat {
            Value::String(s) => Some(s.clone()),
            Value::Object(tailles) => {
                let mut liste: Vec<(u32, String)> = tailles
                    .iter()
                    .filter_map(|(t, v)| Some((t.parse::<u32>().ok()?, v.as_str()?.to_string())))
                    .collect();
                liste.sort_by_key(|(t, _)| *t);
                liste
                    .iter()
                    .find(|(t, _)| *t >= 32)
                    .or_else(|| liste.last())
                    .map(|(_, v)| v.clone())
            }
            _ => None,
        };
        if let Some(relatif) = choisie {
            let relatif = relatif.trim_start_matches('/').replace('\\', "/");
            // Un chemin qui remonte ou qui n'existe pas ne sert a rien (et ne sera pas servi).
            if !relatif.contains("..") && dossier.join(&relatif).is_file() {
                return Some(relatif);
            }
        }
    }
    None
}

/// Poids du dossier d'une extension, en Ko (approximatif : c'est un ordre de grandeur).
pub fn taille_ko(dossier: &Path) -> u64 {
    let mut octets = 0u64;
    let Ok(lecture) = std::fs::read_dir(dossier) else {
        return 0;
    };
    for entree in lecture.flatten() {
        let chemin = entree.path();
        if chemin.is_dir() {
            octets += taille_ko(&chemin) * 1024;
        } else if let Ok(meta) = chemin.metadata() {
            octets += meta.len();
        }
    }
    octets / 1024
}

// ------------------------------------------------------------------ permissions

/// Traductions des permissions les plus courantes. Ce qui n'est pas ici s'affiche tel quel :
/// mieux vaut un nom technique qu'un mensonge.
fn libelle_permission(permission: &str) -> String {
    match permission {
        "storage" => "stockage local".into(),
        "tabs" => "accès aux onglets".into(),
        "activeTab" => "onglet actif".into(),
        "scripting" => "injection de scripts".into(),
        "alarms" => "minuteurs".into(),
        "notifications" => "notifications".into(),
        "contextMenus" => "menus contextuels".into(),
        "webRequest" => "lecture du trafic réseau".into(),
        "webRequestBlocking" => "blocage du trafic réseau".into(),
        "cookies" => "cookies".into(),
        "history" => "historique".into(),
        "bookmarks" => "favoris".into(),
        "downloads" => "téléchargements".into(),
        "identity" => "identité".into(),
        "nativeMessaging" => "programmes externes".into(),
        "unlimitedStorage" => "stockage illimité".into(),
        "clipboardWrite" => "écriture du presse-papiers".into(),
        "clipboardRead" => "lecture du presse-papiers".into(),
        "<all_urls>" => "toutes les pages".into(),
        "http://*/*" | "https://*/*" => "toutes les pages web".into(),
        autre => autre.to_string(),
    }
}

/// Permissions d'un manifeste (v2 et v3 confondues), en francais quand c'est possible.
pub fn permissions(manifeste: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let ajouter = |garder: &mut Vec<String>, valeur: &Value| {
        if let Some(liste) = valeur.as_array() {
            for element in liste {
                if let Some(texte) = element.as_str() {
                    let libelle = libelle_permission(texte);
                    if !garder.contains(&libelle) {
                        garder.push(libelle);
                    }
                }
            }
        }
    };
    for cle in ["permissions", "host_permissions", "optional_permissions"] {
        ajouter(&mut out, &manifeste[cle]);
    }
    out
}

// ------------------------------------------------------------------ installation
/// Identifiant d'une extension a partir de son paquet, quand on ne le connait pas.
///
/// Un `.crx` du magasin est nomme par son identifiant, mais un fichier telecharge a la main
/// ne dit rien. La regle de Chromium s'applique quand le manifeste porte une cle publique
/// (`key`) : l'empreinte de cette cle est l'identifiant, et c'est ce qui permet a
/// `chrome-extension://<id>/popup` de fonctionner. Sans cle, on fabrique un identifiant local
/// stable, prefixe `local-`, a partir du nom : WebView2 attribuera le sien, mais nos dossiers
/// resteront lisibles et une mise a jour par fichier retombera sur le meme dossier.
pub fn identifiant_du_paquet(dossiers: &Dossiers, zip: &[u8]) -> Result<String, String> {
    let atelier = dossiers.parent().join("extensions-chrome-inspection");
    let _ = std::fs::remove_dir_all(&atelier);
    std::fs::create_dir_all(&atelier).map_err(|e| format!("dossier d'inspection : {e}"))?;
    let archive = atelier.join("paquet.zip");
    std::fs::write(&archive, zip).map_err(|e| format!("écriture du paquet : {e}"))?;
    let etape = atelier.join("contenu");
    crate::systeme::dezipper(&archive, &etape)?;
    let racine = trouver_manifeste(&etape)
        .ok_or_else(|| "le paquet ne contient pas de manifest.json".to_string())?;
    let texte = std::fs::read_to_string(racine.join("manifest.json"))
        .map_err(|e| format!("manifeste illisible : {e}"))?;
    let manifeste: Value = serde_json::from_str(&texte)
        .map_err(|e| format!("manifeste invalide : {e}"))?;
    let identifiant = manifeste["key"]
        .as_str()
        .and_then(crate::crx::identifiant_depuis_cle)
        .or_else(|| {
            let nom = nom_extension_chrome(&manifeste, &racine);
            if nom.trim().is_empty() {
                None
            } else {
                Some(format!("local-{}", slug(&nom)))
            }
        });
    let _ = std::fs::remove_dir_all(&atelier);
    identifiant.ok_or_else(|| {
        "impossible de nommer cette extension : ni clé publique, ni nom dans le manifeste"
            .to_string()
    })
}

/// Nom de dossier sur : alphanumerique, tirets, 40 caracteres au plus.
pub fn slug(nom: &str) -> String {
    let mut s = String::new();
    for c in nom.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if (c == ' ' || c == '-' || c == '_') && !s.ends_with('-') {
            s.push('-');
        }
        if s.len() >= 40 {
            break;
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "extension".to_string()
    } else {
        s
    }
}

/// Installe (ou met a jour) une extension depuis le contenu d'un `.crx` / `.zip`.
///
/// L'ancienne version, si elle existe, part dans les archives : une mise a jour qui casse
/// l'extension doit pouvoir se defaire a la main. `cle_publique` (lue dans l'en-tete du CRX)
/// est inscrite dans le manifeste installe : sans elle, Chromium nomme l'extension d'apres le
/// chemin du dossier, et ni son nom de dossier ni `chrome-extension://<id>/` ne la designent.
pub fn installer(
    dossiers: &Dossiers,
    identifiant: &str,
    octets: &[u8],
    cle_publique: Option<&str>,
) -> Result<PathBuf, String> {
    let cible = dossiers.active.join(identifiant);
    let attente = dossiers.parent().join("extensions-chrome-installation");
    let _ = std::fs::remove_dir_all(&attente);
    std::fs::create_dir_all(&attente).map_err(|e| format!("dossier d'installation : {e}"))?;

    // 1. le ZIP dans un fichier temporaire (Expand-Archive veut une extension .zip)
    let archive = attente.join("paquet.zip");
    std::fs::write(&archive, octets).map_err(|e| format!("écriture du paquet : {e}"))?;

    // 2. decompression dans un dossier de travail
    let etape = attente.join("contenu");
    crate::systeme::dezipper(&archive, &etape)?;

    // 3. le manifeste doit etre la — sinon on ne remplace rien
    let racine = trouver_manifeste(&etape).ok_or_else(|| {
        "le paquet ne contient pas de manifest.json à sa racine : ce n'est pas une extension"
            .to_string()
    })?;

    // 4. mise a l'ecart de l'ancienne version, puis mise en place
    if cible.exists() {
        let horodatage = crate::log::maintenant_ms().to_string();
        archiver(&cible, &dossiers.archives(), &horodatage)?;
    }
    std::fs::create_dir_all(&dossiers.active).map_err(|e| format!("dossier des extensions : {e}"))?;
    match std::fs::rename(&racine, &cible) {
        Ok(()) => {}
        Err(premier) => {
            // `rename` echoue parfois entre volumes : on copie alors.
            copier_arbre(&racine, &cible).map_err(|e| {
                format!("mise en place impossible ({e}, après {premier})")
            })?;
        }
    }
    inscrire_cle(&cible, identifiant, cle_publique);
    adapter(&cible);
    let _ = std::fs::remove_dir_all(&attente);
    info!(
        "extension installee : {identifiant} -> {}",
        cible.display()
    );
    Ok(cible)
}

/// Inscrit la cle publique du magasin dans le manifeste installe, si elle en manque une.
///
/// C'est ce qui fixe l'identifiant que Chromium attribue a une extension chargee depuis un
/// dossier. Mesure du 2026-11-21, pour un dossier `mon-dossier` sans `key` :
/// `chrome.runtime.id` = `cfmnfkohahigcphgcoimlckammkneabk` (empreinte du **chemin**) ; avec la
/// cle du magasin dans le manifeste, l'identifiant est celui du magasin. Sans cette ligne, la
/// fenetre d'action d'une extension installee depuis un `.crx` ne s'ouvre pas : l'interface
/// demande `chrome-extension://<identifiant>/…`, que le moteur ne connait pas.
fn inscrire_cle(cible: &Path, identifiant: &str, cle_publique: Option<&str>) {
    let Some(cle) = cle_publique.filter(|c| !c.trim().is_empty()) else {
        return;
    };
    let chemin = cible.join("manifest.json");
    let Ok(texte) = std::fs::read_to_string(&chemin) else {
        return;
    };
    let Ok(mut manifeste) = serde_json::from_str::<Value>(&texte) else {
        alerte!("manifeste de {identifiant} : illisible, cle non inscrite");
        return;
    };
    if manifeste.get("key").and_then(Value::as_str).is_some() {
        return;
    }
    manifeste["key"] = Value::String(cle.to_string());
    match serde_json::to_string_pretty(&manifeste) {
        Ok(texte) => {
            if let Err(e) = std::fs::write(&chemin, texte) {
                alerte!("manifeste de {identifiant} : cle non ecrite ({e})");
                return;
            }
            info!("manifeste de {identifiant} : cle publique du magasin inscrite");
        }
        Err(e) => alerte!("manifeste de {identifiant} : serialisation impossible ({e})"),
    }
}

// ------------------------------------------------------------------ adaptateur WebView2

/// Marque de l'adaptateur en tete du service worker : sa presence suffit a ne pas l'ajouter deux
/// fois. Changer la version reapplique l'adaptateur (l'ancien bloc est remplace).
const MARQUE_ADAPTATEUR: &str = "/* trame-adaptateur v2 */";
const FIN_ADAPTATEUR: &str = "/* fin trame-adaptateur */";

/// WebView2 ouvre bien la fenetre demandee par `chrome.windows.create`, mais repond `undefined`
/// au lieu de la fenetre creee (mesure du 2026-11-22 par le port de debogage, sur le profil de
/// Clement). Bitwarden lit `.id` de cette reponse pour sa fenetre de passkey : il plantait
/// (« Cannot read properties of undefined (reading 'id') ») et abandonnait, Google affichait
/// « Impossible de créer votre clé d'accès ». L'adaptateur retrouve la fenetre nouvellement
/// apparue et la renvoie, comme Chrome. Il ne fait rien si le moteur repond correctement.
///
/// v2 (2026-11-28, Joko) : une extension sans fenetre d'action attend `chrome.action.onClicked`,
/// que WebView2 ne declenche jamais (il n'a pas de barre d'outils). L'adaptateur garde les
/// ecouteurs, et la page relais `trame-relais/action.html` (chargee par Trame au clic sur l'icone)
/// lui envoie `{__trame_action: {url}}` : il retrouve l'onglet de cette adresse et appelle les
/// ecouteurs avec lui, comme Chrome.
const ADAPTATEUR: &str = r#"/* trame-adaptateur v2 */
(() => {
  const a = globalThis.chrome && (chrome.action || chrome.browserAction);
  const r = globalThis.chrome && chrome.runtime;
  if (!a || !a.onClicked || !r || !r.onMessage || a.onClicked.__trame) return;
  const ev = a.onClicked;
  const ecouteurs = new Set();
  const ajouter = ev.addListener.bind(ev);
  const retirer = ev.removeListener.bind(ev);
  try {
    ev.addListener = (f) => { ecouteurs.add(f); return ajouter(f); };
    ev.removeListener = (f) => { ecouteurs.delete(f); return retirer(f); };
    ev.__trame = true;
  } catch (_) { return; }
  r.onMessage.addListener((m, envoyeur) => {
    if (!m || !m.__trame_action || !envoyeur || envoyeur.id !== r.id) return;
    (async () => {
      const onglets = await chrome.tabs.query({});
      const url = m.__trame_action.url || "";
      const pages = onglets.filter((t) => t.url && !/^(http:\/\/trame\.localhost|chrome-extension:)/.test(t.url));
      const onglet = pages.find((t) => t.url === url && t.active) || pages.find((t) => t.url === url)
        || pages.find((t) => t.active);
      if (!onglet) return;
      for (const f of ecouteurs) {
        try { f(onglet); } catch (e) { console.error("trame : onClicked", e); }
      }
    })();
  });
})();
(() => {
  const w = globalThis.chrome && chrome.windows;
  if (!w || typeof w.create !== "function" || w.create.__trame) return;
  const creer = w.create.bind(w);
  const attendre = (ms) => new Promise((r) => setTimeout(r, ms));
  const adapte = function (options, rappel) {
    const promesse = (async () => {
      const avant = new Set((await w.getAll()).map((f) => f.id));
      const reponse = await creer(options || {});
      if (reponse && reponse.id !== undefined) return reponse;
      for (let i = 0; i < 60; i++) {
        const nouvelle = (await w.getAll({ populate: true })).find((f) => !avant.has(f.id));
        if (nouvelle) return nouvelle;
        await attendre(50);
      }
      return reponse;
    })();
    if (typeof rappel === "function") {
      promesse.then((f) => rappel(f), () => rappel(undefined));
      return;
    }
    return promesse;
  };
  adapte.__trame = true;
  try { w.create = adapte; } catch (_) {}
})();
/* fin trame-adaptateur */
"#;

/// Ajoute l'adaptateur WebView2 en tete du service worker de l'extension (`background.
/// service_worker` du manifeste), s'il n'y est pas deja. Silencieux en cas d'echec : une
/// extension non adaptee marche comme avant. Le service worker est relu sur le disque a chaque
/// demarrage du moteur ; le manifeste n'est pas touche.
pub fn adapter(dossier: &Path) -> bool {
    let Some(chemin) = service_worker(dossier) else {
        return false;
    };
    poser_relais_action(dossier);
    let Ok(contenu) = std::fs::read_to_string(&chemin) else {
        return false;
    };
    let Some(nouveau) = avec_adaptateur(&contenu) else {
        return false; // deja a jour
    };
    match std::fs::write(&chemin, nouveau) {
        Ok(()) => {
            info!("adaptateur WebView2 ajoute a {}", chemin.display());
            true
        }
        Err(e) => {
            alerte!("adaptateur WebView2 : ecriture impossible dans {} ({e})", chemin.display());
            false
        }
    }
}

/// Page relais du clic sur l'icone (voir `ADAPTATEUR` v2), relative au dossier de l'extension.
pub const PAGE_RELAIS_ACTION: &str = "trame-relais/action.html";

/// Le script est dans un fichier a part : la CSP des pages d'extension MV3 refuse le script en
/// ligne. La page previent le service worker (ce qui le reveille s'il dort), puis se vide.
const RELAIS_ACTION_HTML: &str = "<!doctype html><meta charset=\"utf-8\"><script src=\"action.js\"></script>\n";
const RELAIS_ACTION_JS: &str = r#"const url = new URLSearchParams(location.search).get("url") || "";
chrome.runtime.sendMessage({ __trame_action: { url } }, () => void chrome.runtime.lastError);
"#;

/// Ecrit la page relais du clic dans le dossier de l'extension, si elle manque ou a change.
fn poser_relais_action(dossier: &Path) {
    let relais = dossier.join("trame-relais");
    for (nom, texte) in [("action.html", RELAIS_ACTION_HTML), ("action.js", RELAIS_ACTION_JS)] {
        let chemin = relais.join(nom);
        if std::fs::read_to_string(&chemin).ok().as_deref() == Some(texte) {
            continue;
        }
        let _ = std::fs::create_dir_all(&relais);
        if let Err(e) = std::fs::write(&chemin, texte) {
            alerte!("relais d'action : ecriture impossible dans {} ({e})", chemin.display());
        }
    }
}

/// Le fichier du service worker declare par le manifeste, s'il reste dans le dossier.
fn service_worker(dossier: &Path) -> Option<PathBuf> {
    let texte = std::fs::read_to_string(dossier.join("manifest.json")).ok()?;
    let manifeste = serde_json::from_str::<Value>(&texte).ok()?;
    let sw = manifeste["background"]["service_worker"].as_str()?;
    // Un chemin du manifeste ne doit pas sortir du dossier de l'extension.
    if sw.contains("..") || sw.starts_with('/') || sw.contains(':') {
        return None;
    }
    Some(dossier.join(sw))
}

/// Chromium garde en cache le service worker d'une extension deja chargee : ecrire
/// l'adaptateur dans le fichier ne suffit pas, il faut recharger l'extension UNE fois (mesure
/// du 2026-11-22 : fichier adapte, `chrome.windows.create.__trame` absent du worker vivant).
/// Vrai quand le fichier porte l'adaptateur de cette version mais que le moteur n'a pas encore
/// ete recharge avec (temoin dans `extensions-chrome-adaptees/<id>`).
pub fn a_recharger(dossiers: &Dossiers, identifiant: &str) -> bool {
    let Some(sw) = service_worker(&dossiers.active.join(identifiant)) else {
        return false;
    };
    let mut debut = [0u8; 64];
    let lu = std::fs::File::open(&sw)
        .and_then(|mut f| std::io::Read::read(&mut f, &mut debut))
        .unwrap_or(0);
    if !debut[..lu].starts_with(MARQUE_ADAPTATEUR.as_bytes()) {
        return false;
    }
    std::fs::read_to_string(temoin_rechargement(dossiers, identifiant)).ok().as_deref()
        != Some(MARQUE_ADAPTATEUR)
}

/// Note que le moteur a recharge l'extension avec l'adaptateur de cette version.
pub fn noter_rechargee(dossiers: &Dossiers, identifiant: &str) {
    let chemin = temoin_rechargement(dossiers, identifiant);
    if let Some(parent) = chemin.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(chemin, MARQUE_ADAPTATEUR);
}

/// Oublie que les extensions Chrome ont ete adaptees et rechargees (« Réparer Trame ») : au
/// demarrage suivant, chacune est re-verifiee et rechargee une fois. Rend vrai si oublie.
pub fn oublier_rechargements(dossiers: &Dossiers) -> bool {
    let dossier = dossiers.parent().join("extensions-chrome-adaptees");
    dossier.is_dir() && std::fs::remove_dir_all(&dossier).is_ok()
}

fn temoin_rechargement(dossiers: &Dossiers, identifiant: &str) -> PathBuf {
    dossiers.parent().join("extensions-chrome-adaptees").join(identifiant)
}

/// Le service worker avec l'adaptateur en tete, ou `None` s'il l'a deja dans cette version.
/// Une ancienne version de l'adaptateur est remplacee.
fn avec_adaptateur(contenu: &str) -> Option<String> {
    if contenu.starts_with(MARQUE_ADAPTATEUR) {
        return None;
    }
    let sans_ancien = if contenu.starts_with("/* trame-adaptateur") {
        match contenu.find(FIN_ADAPTATEUR) {
            Some(fin) => contenu[fin + FIN_ADAPTATEUR.len()..].trim_start_matches(['\r', '\n']),
            None => contenu,
        }
    } else {
        contenu
    };
    Some(format!("{ADAPTATEUR}{sans_ancien}"))
}

/// Adapte toutes les extensions installees (au demarrage, avant que le moteur ne lance leurs
/// service workers). Ne reinstalle rien : ecrit seulement les fichiers qui n'ont pas l'adaptateur.
pub fn adapter_toutes(dossiers: &Dossiers) {
    let Ok(lecture) = std::fs::read_dir(&dossiers.active) else {
        return;
    };
    for entree in lecture.flatten() {
        let chemin = entree.path();
        if chemin.is_dir() {
            adapter(&chemin);
        }
    }
}

/// Cherche `manifest.json` a la racine, sinon dans l'unique sous-dossier (certains paquets
/// enferment tout dans un dossier).
pub fn trouver_manifeste(dossier: &Path) -> Option<PathBuf> {    if dossier.join("manifest.json").is_file() {
        return Some(dossier.to_path_buf());
    }
    let lecture = std::fs::read_dir(dossier).ok()?;
    let sous_dossiers: Vec<PathBuf> = lecture
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    if sous_dossiers.len() == 1 && sous_dossiers[0].join("manifest.json").is_file() {
        return Some(sous_dossiers[0].clone());
    }
    None
}

/// Copie recursive (repli quand `rename` ne passe pas).
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

// ------------------------------------------------------------------ activation

/// Desactive une extension : son dossier part dans `extensions-chrome-desactivees`.
pub fn desactiver(dossiers: &Dossiers, identifiant: &str) -> Result<(), String> {
    let dossier = dossiers.active.join(identifiant);
    if !dossier.is_dir() {
        return Err("cette extension n'est pas installée".into());
    }
    deplacer(&dossier, &dossiers.desactivees())?;
    info!("extension desactivee : {identifiant}");
    Ok(())
}

/// Reactive une extension desactivee.
pub fn reactiver(dossiers: &Dossiers, identifiant: &str) -> Result<(), String> {
    let dossier = dossiers.desactivees().join(identifiant);
    if !dossier.is_dir() {
        return Err("cette extension n'est pas désactivée".into());
    }
    deplacer(&dossier, &dossiers.active)?;
    info!("extension reactivee : {identifiant}");
    Ok(())
}

/// Desinstalle : le dossier part dans les archives, avec la date.
pub fn desinstaller(dossiers: &Dossiers, identifiant: &str) -> Result<PathBuf, String> {
    for racine in [dossiers.active.clone(), dossiers.desactivees()] {
        let dossier = racine.join(identifiant);
        if dossier.is_dir() {
            let horodatage = crate::log::maintenant_ms().to_string();
            let ou = archiver(&dossier, &dossiers.archives(), &horodatage)?;
            oublier_source(&dossiers.parent(), identifiant);
            return Ok(ou);
        }
    }
    Err("cette extension n'est pas installée".into())
}

// ------------------------------------------------------------------ provenance

fn fichier_sources(racine: &Path) -> PathBuf {
    racine.join("extensions-chrome-sources.json")
}

/// D'ou vient chaque extension (magasin, fichier, import d'un navigateur).
pub fn lire_sources(racine: &Path) -> Value {
    std::fs::read_to_string(fichier_sources(racine))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
}

pub fn noter_source(racine: &Path, identifiant: &str, source: &str) {
    let mut valeurs = lire_sources(racine);
    valeurs[identifiant] = json!({
        "source": source,
        "quand": crate::log::maintenant_ms(),
    });
    if let Err(e) = std::fs::write(
        fichier_sources(racine),
        serde_json::to_string_pretty(&valeurs).unwrap_or_default(),
    ) {
        alerte!("sources d'extensions : écriture impossible ({e})");
    }
}

fn oublier_source(racine: &Path, identifiant: &str) {
    let mut valeurs = lire_sources(racine);
    if let Some(objet) = valeurs.as_object_mut() {
        objet.remove(identifiant);
    }
    let _ = std::fs::write(
        fichier_sources(racine),
        serde_json::to_string_pretty(&valeurs).unwrap_or_default(),
    );
}

// ------------------------------------------------------------------ noms localises

/// Nom lisible d'une extension : le manifeste contient souvent `__MSG_appName__`.
pub fn nom_extension_chrome(manifeste: &Value, dossier: &Path) -> String {
    localiser_champ(manifeste, dossier, "name").unwrap_or_default()
}

/// Resout une valeur de manifeste qui peut etre un jeton `__MSG_cle__`.
///
/// Un manifeste localise ne stocke ni « Bitwarden » ni sa description : il stocke
/// `__MSG_appName__` / `__MSG_extDesc__` et laisse le navigateur lire `_locales`.
pub fn localiser_champ(manifeste: &Value, dossier: &Path, champ: &str) -> Option<String> {
    let brut = manifeste[champ].as_str().unwrap_or("");
    if brut.is_empty() {
        return None;
    }
    let Some(cle) = brut.strip_prefix("__MSG_").and_then(|s| s.strip_suffix("__")) else {
        return Some(brut.to_string());
    };
    let locale = manifeste["default_locale"].as_str().unwrap_or("en");
    // L'interface est en francais : quand l'extension livre une traduction francaise, on la
    // prefere a la langue par defaut (souvent l'anglais). L'ordre est : fr, langue declaree,
    // puis anglais.
    let candidats = [
        dossier.join("_locales").join("fr").join("messages.json"),
        dossier.join("_locales").join(locale).join("messages.json"),
        dossier.join("_locales").join("en").join("messages.json"),
    ];
    for chemin in candidats {
        if let Ok(texte) = std::fs::read_to_string(&chemin) {
            if let Ok(valeur) = serde_json::from_str::<Value>(&texte) {
                let cherchee = cle.to_ascii_lowercase();
                if let Some(entrees) = valeur.as_object() {
                    for (nom, entree) in entrees {
                        if nom.to_ascii_lowercase() == cherchee {
                            if let Some(message) = entree["message"].as_str() {
                                return Some(message.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    Some(brut.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptateur_ajoute_une_fois_et_remplace_l_ancien() {
        let sw = "/*! licence */\nconsole.log(1);\n";
        let une = avec_adaptateur(sw).expect("ajoute");
        assert!(une.starts_with(MARQUE_ADAPTATEUR));
        assert!(une.ends_with(sw));
        assert!(avec_adaptateur(&une).is_none(), "pas deux fois");
        let ancien = format!("/* trame-adaptateur v0 */\nvieux();\n{FIN_ADAPTATEUR}\n{sw}");
        let remplace = avec_adaptateur(&ancien).expect("remplace");
        assert!(!remplace.contains("vieux()"));
        assert!(remplace.ends_with(sw));
    }

    #[test]
    fn adaptateur_ecrit_dans_le_service_worker_du_manifeste() {
        let d = std::env::temp_dir().join(format!("trame-adaptateur-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manifest.json"), r#"{"manifest_version":3,"background":{"service_worker":"bg.js"}}"#).unwrap();
        std::fs::write(d.join("bg.js"), "go();").unwrap();
        assert!(adapter(&d));
        assert!(!adapter(&d), "deuxieme passage : rien a faire");
        let js = std::fs::read_to_string(d.join("bg.js")).unwrap();
        assert!(js.starts_with(MARQUE_ADAPTATEUR) && js.ends_with("go();"));
        // un chemin qui sort du dossier est ignore
        std::fs::write(d.join("manifest.json"), r#"{"background":{"service_worker":"../x.js"}}"#).unwrap();
        assert!(!adapter(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rechargement_demande_une_seule_fois() {
        let racine = std::env::temp_dir().join(format!("trame-recharge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&racine);
        let dossiers = Dossiers::depuis(racine.join("extensions-chrome"));
        let ext = dossiers.active.join("abc");
        std::fs::create_dir_all(&ext).unwrap();
        std::fs::write(ext.join("manifest.json"), r#"{"background":{"service_worker":"bg.js"}}"#).unwrap();
        std::fs::write(ext.join("bg.js"), "go();").unwrap();
        assert!(!a_recharger(&dossiers, "abc"), "pas d'adaptateur : rien a recharger");
        adapter(&ext);
        assert!(a_recharger(&dossiers, "abc"));
        noter_rechargee(&dossiers, "abc");
        assert!(!a_recharger(&dossiers, "abc"), "une seule fois");
        let _ = std::fs::remove_dir_all(&racine);
    }

    fn racine(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trame-ext-{}-{nom}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("extensions-chrome")).unwrap();
        d
    }

    fn dossier(racine: &Path, id: &str, manifeste: &str) {
        let d = racine.join("extensions-chrome").join(id);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manifest.json"), manifeste).unwrap();
        std::fs::write(d.join("popup.html"), "<html></html>").unwrap();
    }

    #[test]
    fn lister_donne_les_champs_utiles_et_la_provenance() {
        let r = racine("lister");
        dossier(&r, "aaa", r#"{"name":"Alpha","version":"1.2.3","description":"Une extension","action":{"default_popup":"popup.html"},"permissions":["storage","tabs"]}"#);
        noter_source(&r, "aaa", "magasin");
        let liste = lister(&Dossiers::depuis(r.join("extensions-chrome")));
        assert_eq!(liste.len(), 1);
        let e = &liste[0];
        assert_eq!(e["nom"], "Alpha");
        assert_eq!(e["version"], "1.2.3");
        assert_eq!(e["actif"], true);
        assert_eq!(e["valide"], true);
        assert_eq!(e["popup"], "popup.html");
        assert_eq!(e["source"], "magasin");
        let permissions: Vec<String> = serde_json::from_value(e["permissions"].clone()).unwrap();
        assert!(permissions.contains(&"stockage local".to_string()));
        assert!(permissions.contains(&"accès aux onglets".to_string()));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn une_extension_sans_manifeste_est_signalee_invalide() {
        let r = racine("sans-manifeste");
        std::fs::create_dir_all(r.join("extensions-chrome").join("bbb")).unwrap();
        let liste = lister(&Dossiers::depuis(r.join("extensions-chrome")));
        assert_eq!(liste[0]["valide"], false);
        assert!(liste[0]["description"].as_str().unwrap().contains("illisible"));
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn desactiver_puis_reactiver_deplace_le_dossier() {
        let r = racine("basculer");
        dossier(&r, "ccc", r#"{"name":"C"}"#);
        let d = Dossiers::depuis(r.join("extensions-chrome"));
        desactiver(&d, "ccc").unwrap();
        assert!(!d.active.join("ccc").exists());
        assert!(d.desactivees().join("ccc").is_dir());
        // La liste la montre, mais coupee.
        let liste = lister(&d);
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0]["actif"], false);
        reactiver(&d, "ccc").unwrap();
        assert!(d.active.join("ccc").is_dir());
        assert!(!d.desactivees().join("ccc").exists());
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn desactiver_deux_fois_dit_pourquoi() {
        let r = racine("deux-fois");
        dossier(&r, "ddd", r#"{"name":"D"}"#);
        let d = Dossiers::depuis(r.join("extensions-chrome"));
        desactiver(&d, "ddd").unwrap();
        let message = desactiver(&d, "ddd").unwrap_err();
        assert!(message.contains("pas installée"), "{message}");
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn la_cle_du_magasin_est_inscrite_dans_le_manifeste_installe() {
        // Sans cette cle, Chromium nomme l'extension d'apres le chemin du dossier : l'interface
        // demanderait `chrome-extension://<identifiant du magasin>/…`, que le moteur ignore.
        let r = racine("cle");
        dossier(&r, "hhh", r#"{"name":"H","version":"2.0.0"}"#);
        let cible = r.join("extensions-chrome").join("hhh");
        inscrire_cle(&cible, "hhh", Some("QUJD"));
        let manifeste: Value =
            serde_json::from_str(&std::fs::read_to_string(cible.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifeste["key"], "QUJD");
        assert_eq!(manifeste["name"], "H", "le reste du manifeste doit survivre");
        assert_eq!(manifeste["version"], "2.0.0");

        // Une cle deja presente n'est pas remplacee, et l'absence de cle ne casse rien.
        inscrire_cle(&cible, "hhh", Some("WFla"));
        let manifeste: Value =
            serde_json::from_str(&std::fs::read_to_string(cible.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifeste["key"], "QUJD", "on n'ecrase pas une cle existante");
        dossier(&r, "iii", r#"{"name":"I"}"#);
        let autre = r.join("extensions-chrome").join("iii");
        inscrire_cle(&autre, "iii", None);
        let manifeste: Value =
            serde_json::from_str(&std::fs::read_to_string(autre.join("manifest.json")).unwrap())
                .unwrap();
        assert!(manifeste["key"].is_null());
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn desinstaller_archive_au_lieu_de_supprimer() {
        let r = racine("desinstaller");
        dossier(&r, "eee", r#"{"name":"E"}"#);
        noter_source(&r, "eee", "fichier");
        let d = Dossiers::depuis(r.join("extensions-chrome"));
        let ou = desinstaller(&d, "eee").unwrap();
        assert!(ou.starts_with(d.archives()));
        assert!(ou.join("manifest.json").is_file(), "l'extension doit rester lisible");
        assert!(!d.active.join("eee").exists());
        // La provenance disparait avec l'extension, sinon elle reapparaitrait fausse.
        assert_eq!(lire_sources(&r)["eee"], Value::Null);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn deux_archives_du_meme_identifiant_ne_s_ecrasent_pas() {
        let r = racine("archives");
        let d = Dossiers::depuis(r.join("extensions-chrome"));
        dossier(&r, "fff", r#"{"name":"F"}"#);
        let un = archiver(&d.active.join("fff"), &d.archives(), "100").unwrap();
        dossier(&r, "fff", r#"{"name":"F2"}"#);
        let deux = archiver(&d.active.join("fff"), &d.archives(), "100").unwrap();
        assert_ne!(un, deux, "deux archives ne doivent pas se confondre");
        assert!(deux.join("manifest.json").is_file());
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn le_manifeste_est_trouve_dans_un_sous_dossier_unique() {
        let r = racine("sous-dossier");
        let paquet = r.join("paquet");
        std::fs::create_dir_all(paquet.join("interne")).unwrap();
        std::fs::write(paquet.join("interne").join("manifest.json"), "{}").unwrap();
        assert_eq!(trouver_manifeste(&paquet).as_deref(), Some(paquet.join("interne").as_path()));
        // Deux sous-dossiers : ambigu, on refuse plutot que de deviner.
        std::fs::create_dir_all(paquet.join("autre")).unwrap();
        assert!(trouver_manifeste(&paquet).is_none());
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn les_permissions_inconnues_sont_affichees_telles_quelles() {
        let manifeste: Value = serde_json::from_str(
            r#"{"permissions":["storage","unePermissionMaison"],"host_permissions":["<all_urls>"]}"#,
        )
        .unwrap();
        let liste = permissions(&manifeste);
        assert!(liste.contains(&"stockage local".to_string()));
        assert!(liste.contains(&"unePermissionMaison".to_string()));
        assert!(liste.contains(&"toutes les pages".to_string()));
        // Aucun doublon.
        let mut uniques = liste.clone();
        uniques.sort();
        uniques.dedup();
        assert_eq!(uniques.len(), liste.len());
    }

    #[test]
    fn un_manifeste_localise_livre_son_nom_et_sa_description() {
        let r = racine("localise");
        let locales = r.join("abc").join("_locales").join("fr");
        std::fs::create_dir_all(&locales).unwrap();
        std::fs::write(
            locales.join("messages.json"),
            r#"{"appName":{"message":"Bitwarden"},"EXTDESC":{"message":"Gestionnaire de mots de passe"}}"#,
        )
        .unwrap();
        let manifeste: Value = serde_json::from_str(
            r#"{"name":"__MSG_appName__","description":"__MSG_extDesc__","default_locale":"fr"}"#,
        )
        .unwrap();
        assert_eq!(nom_extension_chrome(&manifeste, &r.join("abc")), "Bitwarden");
        assert_eq!(
            localiser_champ(&manifeste, &r.join("abc"), "description").as_deref(),
            Some("Gestionnaire de mots de passe")
        );
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn la_traduction_francaise_passe_devant_l_anglaise() {
        let r = racine("locales-fr");
        let base = r.join("abc");
        for (langue, nom) in [("en", "Password Manager"), ("fr", "Gestionnaire de mots de passe")] {
            let d = base.join("_locales").join(langue);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("messages.json"),
                format!(r#"{{"appName":{{"message":"{nom}"}}}}"#),
            )
            .unwrap();
        }
        let manifeste: Value =
            serde_json::from_str(r#"{"name":"__MSG_appName__","default_locale":"en"}"#).unwrap();
        assert_eq!(
            nom_extension_chrome(&manifeste, &base),
            "Gestionnaire de mots de passe",
            "le francais doit etre prefere quand il existe"
        );
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn la_taille_du_dossier_est_calculee() {
        let r = racine("taille");
        dossier(&r, "ggg", r#"{"name":"G"}"#);
        // Un fichier de 5 000 octets doit compter pour ~4 Ko.
        std::fs::write(r.join("extensions-chrome").join("ggg").join("gros.js"), vec![b'x'; 5000])
            .unwrap();
        let ko = taille_ko(&r.join("extensions-chrome").join("ggg"));
        assert!(ko >= 4, "au moins 4 kilo-octets, trouvé {ko}");
        // Un dossier absent vaut zero, sans panique.
        assert_eq!(taille_ko(&r.join("extensions-chrome").join("absent")), 0);
        let _ = std::fs::remove_dir_all(&r);
    }

    #[test]
    fn l_icone_d_action_passe_avant_les_icones_et_vise_32_px() {
        let r = racine("icone");
        let d = r.join("ext");
        std::fs::create_dir_all(d.join("img")).unwrap();
        for f in ["img/16.png", "img/32.png", "img/48.png", "img/128.png", "logo.png"] {
            std::fs::write(d.join(f), b"png").unwrap();
        }
        // action.default_icon (carte de tailles) : 32 px, la plus proche par le haut.
        let m: Value = serde_json::from_str(
            r#"{"action":{"default_icon":{"16":"img/16.png","32":"img/32.png","48":"img/48.png"}},
                "icons":{"128":"img/128.png"}}"#,
        )
        .unwrap();
        assert_eq!(icone_relative(&m, &d).as_deref(), Some("img/32.png"));
        // MV2 en chaine simple, avec une barre oblique de tete.
        let m: Value = serde_json::from_str(r#"{"browser_action":{"default_icon":"/logo.png"}}"#).unwrap();
        assert_eq!(icone_relative(&m, &d).as_deref(), Some("logo.png"));
        // Pas d'icone d'action : les icones de l'extension ; que des petites : la plus grande.
        let m: Value = serde_json::from_str(r#"{"icons":{"16":"img/16.png"}}"#).unwrap();
        assert_eq!(icone_relative(&m, &d).as_deref(), Some("img/16.png"));
        // Fichier absent ou chemin qui remonte : aucune icone, plutot qu'une image cassee.
        let m: Value = serde_json::from_str(r#"{"icons":{"48":"absent.png"}}"#).unwrap();
        assert_eq!(icone_relative(&m, &d), None);
        let m: Value = serde_json::from_str(r#"{"icons":{"48":"../secret.png"}}"#).unwrap();
        assert_eq!(icone_relative(&m, &d), None);
        let _ = std::fs::remove_dir_all(&r);
    }
}
