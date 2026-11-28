//! Pilotage **a chaud** du moteur d'extensions de WebView2.
//!
//! wry n'utilise `ICoreWebView2Profile7::AddBrowserExtension` qu'au demarrage, et n'en expose
//! ni le resultat ni les autres operations. Or le profil WebView2 sait tout faire en cours de
//! route : ajouter une extension (« elle tourne des la fin de l'installation », dit la
//! documentation), l'eteindre ou la rallumer sans perdre ses donnees (`Enable`), la retirer
//! (`Remove`), et lister ce qu'il a reellement charge (`GetBrowserExtensions`).
//!
//! Ce module fait ces appels sur le profil partage par toutes les webviews (n'importe laquelle
//! convient : on passe la barre, qui existe toujours). Chaque operation est asynchrone : son
//! resultat revient dans la boucle d'evenements sous forme de [`UserEvent`], jamais par une
//! attente bloquante.
//!
//! Le disque reste la verite persistante (`extensions_chrome.rs`) ; ce module ne fait
//! qu'aligner le moteur sur lui, tout de suite, au lieu d'attendre le redemarrage.

use std::path::Path;

use tao::event_loop::EventLoopProxy;
use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2BrowserExtension, ICoreWebView2BrowserExtensionList, ICoreWebView2Profile7,
    ICoreWebView2_13,
};
use webview2_com::{
    take_pwstr, BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
    ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
use wry::{WebView, WebViewExtWindows};

use crate::app::UserEvent;

/// Ce que le moteur a reellement charge : identifiant, nom, allumee ou non.
#[derive(Debug, Clone)]
pub struct Chargee {
    pub id: String,
    pub nom: String,
    pub active: bool,
}

/// Ce qu'une operation a donne. `id` est celui que le moteur attribue (pour un ajout) ou celui
/// qui etait vise.
#[derive(Debug, Clone)]
pub struct Bilan {
    pub action: Action,
    pub id: String,
    pub reussi: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Ajout,
    Allumage,
    Extinction,
    Retrait,
}

/// Demande au moteur de viser une consommation memoire basse (`true`) ou normale pour une
/// webview. En niveau bas, WebView2 rend la memoire qu'il peut (caches, tas des scripts) ; la
/// page reste ouverte, a sa position, et repasse en niveau normal des qu'on la montre.
pub fn niveau_memoire(webview: &WebView, basse: bool) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    let niveau = if basse {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_19>()
            .and_then(|w| w.SetMemoryUsageTargetLevel(niveau))
            .map_err(|e| e.message().to_string())
    }
}

/// Regle la protection anti-pistage du profil de la webview : "aucune", "equilibree" ou
/// "stricte". La stricte bloque la plupart des traqueurs connus : les pages chargent moins de
/// scripts, donc vont plus vite et prennent moins de memoire — au prix, rarement, d'un contenu
/// integre (connexion via un autre site, video embarquee) qui ne s'affiche pas.
pub fn niveau_pistage(webview: &WebView, niveau: &str) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Profile3, COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
        COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE, COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
    };
    let valeur = match niveau {
        "aucune" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_NONE,
        "equilibree" => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_BALANCED,
        _ => COREWEBVIEW2_TRACKING_PREVENTION_LEVEL_STRICT,
    };
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_13>()
            .and_then(|w| w.Profile())
            .and_then(|p| p.cast::<ICoreWebView2Profile3>())
            .and_then(|p| p.SetPreferredTrackingPreventionLevel(valeur))
            .map_err(|e| e.message().to_string())
    }
}

/// Intercepte, au niveau reseau, les requetes de **document** dont l'adresse porte des
/// parametres de pistage, et y repond par une redirection 307 vers l'adresse propre : la requete
/// sale ne quitte jamais le PC.
///
/// La garde de navigation de wry n'y suffit pas : mesure du 2026-11-22 (scenario `w`), une page
/// qui suivait elle-meme un lien sale envoyait la requete au serveur AVANT que la navigation soit
/// refusee. `WebResourceRequested` intervient avant l'envoi.
/// Adresses refusees au niveau reseau : les motifs `bloquer` des extensions allumees,
/// remplaces d'un coup quand une extension change d'etat (`App::appliquer_blocage`).
/// `filtres` : les motifs poses sur l'onglet a sa creation (toutes extensions chargees).
#[derive(Clone, Default)]
pub struct Blocage {
    pub motifs: std::sync::Arc<std::sync::RwLock<Vec<crate::model::Motif>>>,
    pub filtres: Vec<String>,
}

impl Blocage {
    pub fn avec_filtres(&self, filtres: Vec<String>) -> Blocage {
        Blocage { motifs: self.motifs.clone(), filtres }
    }
}

/// Intercepte les requetes reseau d'un onglet AVANT leur envoi :
/// - adresses `bloquer` des extensions allumees (toutes ressources) : reponse 204 vide ;
/// - documents a parametres de pistage : redirection 307 vers l'adresse propre.
pub fn intercepter_pistage(
    webview: &WebView,
    reglage: std::sync::Arc<(std::sync::atomic::AtomicBool, std::sync::atomic::AtomicBool)>,
    blocage: Blocage,
    proxy: EventLoopProxy<UserEvent>,
    onglet: u32,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT,
    };
    use webview2_com::WebResourceRequestedEventHandler;
    let coeur = webview.webview();
    let environnement = webview.environment();
    let filtres = blocage.filtres.clone();
    let motifs = blocage.motifs.clone();
    let gestionnaire = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
        use std::sync::atomic::Ordering::Relaxed;
        let Some(args) = args else { return Ok(()) };
        let mut brut = PWSTR::null();
        let uri = unsafe {
            match args.Request().and_then(|r| r.Uri(&mut brut)) {
                Ok(()) => take_pwstr(brut),
                Err(_) => return Ok(()),
            }
        };
        let bloquee = motifs
            .read()
            .map(|m| !m.is_empty() && crate::extensions::est_bloquee(&m, &uri))
            .unwrap_or(false);
        if bloquee {
            unsafe {
                let reponse = environnement.CreateWebResourceResponse(
                    None::<&windows::Win32::System::Com::IStream>,
                    204,
                    &HSTRING::from("No Content"),
                    &HSTRING::from("Cache-Control: no-store"),
                )?;
                args.SetResponse(&reponse)?;
            }
            let _ = proxy.send_event(UserEvent::AdresseBloquee { onglet, adresse: uri });
            return Ok(());
        }
        // Le nettoyage ne vaut que pour les documents (les filtres de blocage font aussi passer
        // ici des images, scripts et appels).
        let mut contexte = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
        let document = unsafe { args.ResourceContext(&mut contexte) }.is_ok()
            && contexte == COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT;
        if !document || !reglage.0.load(Relaxed) {
            return Ok(());
        }
        let Some(propre) = crate::urls::nettoyer(&uri, reglage.1.load(Relaxed)) else {
            return Ok(());
        };
        let entetes = HSTRING::from(format!("Location: {propre}\r\nCache-Control: no-store"));
        unsafe {
            let reponse = environnement.CreateWebResourceResponse(
                None::<&windows::Win32::System::Com::IStream>,
                307,
                &HSTRING::from("Temporary Redirect"),
                &entetes,
            )?;
            args.SetResponse(&reponse)?;
        }
        let _ = proxy.send_event(UserEvent::AdresseNettoyee {
            onglet,
            origine: uri,
            propre,
        });
        Ok(())
    }));
    unsafe {
        let mut jeton = 0i64;
        coeur
            .AddWebResourceRequestedFilter(&HSTRING::from("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_DOCUMENT)
            .map_err(|e| e.message().to_string())?;
        // Filtres etroits (une adresse publicitaire), pas `*` sur toutes les ressources : chaque
        // requete filtree traverse le fil principal.
        for f in &filtres {
            if let Err(e) = coeur.AddWebResourceRequestedFilter(&HSTRING::from(f.as_str()), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL) {
                crate::alerte!("onglet {onglet} : filtre de blocage {f} refuse ({})", e.message());
            }
        }
        coeur
            .add_WebResourceRequested(&gestionnaire, &mut jeton)
            .map_err(|e| e.message().to_string())
    }
}

/// Met en pause une webview cachee (scripts et minuteries arretes). Le resultat revient en
/// `UserEvent::PauseOnglet`. WebView2 la reveille seul quand elle redevient visible.
pub fn suspendre(webview: &WebView, proxy: EventLoopProxy<UserEvent>, onglet: u32) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    use webview2_com::TrySuspendCompletedHandler;
    let retour = proxy.clone();
    let fin = TrySuspendCompletedHandler::create(Box::new(move |resultat, fait| {
        let _ = retour.send_event(UserEvent::PauseOnglet {
            onglet,
            faite: resultat.is_ok() && fait,
        });
        Ok(())
    }));
    let appel = unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_3>()
            .and_then(|w| w.TrySuspend(&fin))
    };
    if appel.is_err() {
        let _ = proxy.send_event(UserEvent::PauseOnglet { onglet, faite: false });
    }
}

/// « Réparer Trame » : vide les caches du profil de cette webview — cache disque, caches des
/// sites (Cache Storage) et service workers. Ni cookies, ni stockage des sites, ni mots de
/// passe : les connexions restent. C'est ce que remettait a zero une desinstallation complete
/// (lecteur YouTube fige chez un fils de Clement, 2026-11-24). Le resultat va au journal.
pub fn vider_caches(webview: &WebView, proxy: EventLoopProxy<UserEvent>, quoi: String) {
    use webview2_com::ClearBrowsingDataCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Profile2, COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
        COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE, COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS,
    };
    let retour = proxy.clone();
    let nom = quoi.clone();
    let fin = ClearBrowsingDataCompletedHandler::create(Box::new(move |resultat| {
        let _ = retour.send_event(UserEvent::Log(format!(
            "reparation : caches de {nom} {}",
            if resultat.is_ok() { "vides" } else { "NON vides" }
        )));
        Ok(())
    }));
    let sortes = COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE
        | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE
        | COREWEBVIEW2_BROWSING_DATA_KINDS_SERVICE_WORKERS;
    let appel = unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_13>()
            .and_then(|w| w.Profile())
            .and_then(|p| p.cast::<ICoreWebView2Profile2>())
            .and_then(|p| p.ClearBrowsingData(sortes, &fin))
    };
    if let Err(e) = appel {
        let _ = proxy.send_event(UserEvent::Log(format!("reparation : caches de {quoi} refuses ({})", e.message())));
    }
}

/// Vrai si un cookie de domaine `domaine` (`.aliexpress.com`, `fr.aliexpress.com`) appartient au
/// site `site` (domaine enregistrable : `aliexpress.com`).
pub fn cookie_du_site(domaine: &str, site: &str) -> bool {
    let d = domaine.trim_start_matches('.').to_ascii_lowercase();
    let s = site.to_ascii_lowercase();
    !s.is_empty() && (d == s || d.ends_with(&format!(".{s}")))
}

/// « Effacer les cookies de ce site » : retire tous les cookies du site (domaine enregistrable,
/// sous-domaines compris) et le stockage local de l'origine affichee — c'est souvent la que les
/// bandeaux de consentement gardent le choix. Les autres sites ne sont pas touches.
///
/// Par le protocole DevTools de la webview (aucun port ouvert) : `Storage.getCookies` rend
/// tous les cookies du profil, `Network.deleteCookies` les retire un par un, puis
/// `Storage.clearDataForOrigin`. Les appels passent dans l'ordre sur la meme session : le
/// dernier rappel arrive apres les retraits. Le bilan revient en `UserEvent::CookiesEffaces`.
pub fn effacer_cookies_site(webview: &WebView, proxy: EventLoopProxy<UserEvent>, onglet: u32, site: String, origines: Vec<String>) {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2;

    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler as Rappel;
    fn appeler(coeur: &ICoreWebView2, methode: &str, params: &str, fin: Rappel) -> windows::core::Result<()> {
        unsafe { coeur.CallDevToolsProtocolMethod(&HSTRING::from(methode), &HSTRING::from(params), &fin) }
    }
    let rien = || CallDevToolsProtocolMethodCompletedHandler::create(Box::new(|_, _| Ok(())));

    let coeur = webview.webview();
    let pour_suite = coeur.clone();
    let retour = proxy.clone();
    let site_suite = site.clone();
    let lire = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |resultat, json: String| {
        let echec = |detail: String| {
            let _ = retour.send_event(UserEvent::CookiesEffaces { onglet, site: site_suite.clone(), nombre: 0, erreur: Some(detail) });
        };
        if let Err(e) = resultat {
            echec(e.message().to_string());
            return Ok(());
        }
        let cookies = serde_json::from_str::<serde_json::Value>(&json)
            .ok()
            .and_then(|v| v["cookies"].as_array().cloned())
            .unwrap_or_default();
        let a_retirer: Vec<&serde_json::Value> = cookies
            .iter()
            .filter(|c| cookie_du_site(c["domain"].as_str().unwrap_or(""), &site_suite))
            .collect();
        let nombre = a_retirer.len();
        for c in a_retirer {
            let params = serde_json::json!({
                "name": c["name"], "domain": c["domain"], "path": c["path"],
            });
            if let Err(e) = appeler(&pour_suite, "Network.deleteCookies", &params.to_string(), rien()) {
                echec(e.message().to_string());
                return Ok(());
            }
        }
        for origine in &origines {
            let params = serde_json::json!({
                "origin": origine,
                "storageTypes": "local_storage,indexeddb,websql,cache_storage,service_workers",
            });
            let _ = appeler(&pour_suite, "Storage.clearDataForOrigin", &params.to_string(), rien());
        }
        // Dernier appel de la file : son rappel dit que tout ce qui precede est fait.
        let fin_retour = retour.clone();
        let fin_site = site_suite.clone();
        let fin = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |_, _| {
            let _ = fin_retour.send_event(UserEvent::CookiesEffaces { onglet, site: fin_site, nombre, erreur: None });
            Ok(())
        }));
        if let Err(e) = appeler(&pour_suite, "Storage.getCookies", "{}", fin) {
            echec(e.message().to_string());
        }
        Ok(())
    }));
    if let Err(e) = appeler(&coeur, "Storage.getCookies", "{}", lire) {
        let _ = proxy.send_event(UserEvent::CookiesEffaces { onglet, site, nombre: 0, erreur: Some(e.message().to_string()) });
    }
}

/// Lit l'image de l'icone de la page (PNG), telle que WebView2 l'a deja chargee pour l'onglet :
/// aucune requete de plus, et rien que l'anti-pistage puisse bloquer (l'adresse de l'icone,
/// rechargee depuis l'interface de Trame, est un tiers : celle d'Amazon y etait refusee).
/// Le resultat revient en `UserEvent::IconeImage` ; rien si la page n'a pas d'icone.
pub fn lire_icone(webview: &WebView, proxy: EventLoopProxy<UserEvent>, onglet: u32) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_15, COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG,
    };
    use webview2_com::GetFaviconCompletedHandler;
    let fin = GetFaviconCompletedHandler::create(Box::new(move |resultat, flux| {
        let Some(flux) = flux.filter(|_| resultat.is_ok()) else { return Ok(()) };
        let mut octets = Vec::new();
        let mut tampon = [0u8; 8192];
        loop {
            let mut lus = 0u32;
            let r = unsafe { flux.Read(tampon.as_mut_ptr() as *mut _, tampon.len() as u32, Some(&mut lus)) };
            if r.is_err() || lus == 0 {
                break;
            }
            octets.extend_from_slice(&tampon[..lus as usize]);
            if octets.len() > 512 * 1024 {
                return Ok(()); // une icone, pas une affiche
            }
        }
        if !octets.is_empty() {
            let donnees = format!("data:image/png;base64,{}", crate::carnet::base64(&octets));
            let _ = proxy.send_event(UserEvent::IconeImage { onglet, donnees });
        }
        Ok(())
    }));
    unsafe {
        let _ = webview
            .webview()
            .cast::<ICoreWebView2_15>()
            .and_then(|w| w.GetFavicon(COREWEBVIEW2_FAVICON_IMAGE_FORMAT_PNG, &fin));
    }
}

/// Suit le plein ecran demande par la page (bouton plein ecran d'une video). WebView2 ne
/// l'etend qu'a la webview, donc a la zone de contenu : Trame doit agrandir lui-meme.
/// Chaque changement revient en `UserEvent::PleinEcran`.
pub fn suivre_plein_ecran(webview: &WebView, proxy: EventLoopProxy<UserEvent>, onglet: u32) -> Result<(), String> {
    use webview2_com::ContainsFullScreenElementChangedEventHandler;
    let gestionnaire = ContainsFullScreenElementChangedEventHandler::create(Box::new(move |coeur, _| {
        let mut actif = BOOL(0);
        if let Some(c) = coeur {
            unsafe {
                let _ = c.ContainsFullScreenElement(&mut actif);
            }
        }
        let _ = proxy.send_event(UserEvent::PleinEcran { onglet, actif: actif.as_bool() });
        Ok(())
    }));
    let mut jeton = 0i64;
    unsafe {
        webview
            .webview()
            .add_ContainsFullScreenElementChanged(&gestionnaire, &mut jeton)
            .map_err(|e| e.message().to_string())
    }
}

/// Reprend une webview mise en pause (avant de l'afficher).
pub fn reprendre(webview: &WebView) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_3;
    unsafe {
        let _ = webview.webview().cast::<ICoreWebView2_3>().and_then(|w| w.Resume());
    }
}

/// La page joue-t-elle du son ? Une page qui joue de la musique n'est jamais mise en pause.
pub fn joue_du_son(webview: &WebView) -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    let mut valeur = BOOL(0);
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_8>()
            .and_then(|w| w.IsDocumentPlayingAudio(&mut valeur))
            .map(|_| valeur.as_bool())
            .unwrap_or(true) // dans le doute : on ne coupe pas
    }
}

/// Coupe le son d'une webview (la page de surveillance d'un guet ne doit jamais s'entendre).
pub fn rendre_muette(webview: &WebView) {
    regler_muet(webview, true);
}

/// Coupe ou rend le son d'une webview (bouton haut-parleur d'un onglet dans le rail).
pub fn regler_muet(webview: &WebView, muet: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    unsafe {
        let _ = webview
            .webview()
            .cast::<ICoreWebView2_8>()
            .and_then(|w| w.SetIsMuted(muet));
    }
}

/// Montre ou cache une webview, sans rien demander au moteur si elle est deja dans cet etat.
///
/// Mesure du 2026-11-23 (vrai binaire, 9 onglets) : changer d'onglet appelait `set_visible` sur
/// CHAQUE webview, et chaque appel coute 4 a 9 ms meme quand rien ne change — 30 a 55 ms par
/// changement d'onglet. Lire `IsVisible` sur le controleur, lui, ne coute rien : seules la page
/// quittee et la page ouverte sont reellement touchees.
pub fn est_visible(webview: &WebView) -> bool {
    let mut actuel = BOOL(0);
    unsafe { webview.controller().IsVisible(&mut actuel) }.is_ok() && actuel.as_bool()
}

pub fn regler_visible(webview: &WebView, visible: bool) {
    let mut actuel = BOOL(0);
    let lu = unsafe { webview.controller().IsVisible(&mut actuel) }.is_ok();
    if lu && actuel.as_bool() == visible {
        return;
    }
    let _ = webview.set_visible(visible);
}
/// La page joue-t-elle du son en ce moment ? Contrairement a `joue_du_son`, le doute vaut
/// « non » : c'est pour l'affichage du haut-parleur, pas pour une decision de mise en pause.
pub fn son_en_cours(webview: &WebView) -> bool {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_8;
    let mut valeur = BOOL(0);
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_8>()
            .and_then(|w| w.IsDocumentPlayingAudio(&mut valeur))
            .map(|_| valeur.as_bool())
            .unwrap_or(false)
    }
}

fn profil(webview: &WebView) -> windows::core::Result<ICoreWebView2Profile7> {
    unsafe {
        webview
            .webview()
            .cast::<ICoreWebView2_13>()?
            .Profile()?
            .cast::<ICoreWebView2Profile7>()
    }
}

fn identifiant(ext: &ICoreWebView2BrowserExtension) -> String {
    let mut brut = PWSTR::null();
    unsafe {
        if ext.Id(&mut brut).is_ok() {
            return take_pwstr(brut);
        }
    }
    String::new()
}

fn nom(ext: &ICoreWebView2BrowserExtension) -> String {
    let mut brut = PWSTR::null();
    unsafe {
        if ext.Name(&mut brut).is_ok() {
            return take_pwstr(brut);
        }
    }
    String::new()
}

fn allumee(ext: &ICoreWebView2BrowserExtension) -> bool {
    let mut valeur = BOOL(0);
    unsafe {
        if ext.IsEnabled(&mut valeur).is_ok() {
            return valeur.as_bool();
        }
    }
    false
}

fn decrire(liste: &ICoreWebView2BrowserExtensionList) -> Vec<(Chargee, ICoreWebView2BrowserExtension)> {
    let mut sortie = Vec::new();
    let mut nombre = 0u32;
    unsafe {
        if liste.Count(&mut nombre).is_err() {
            return sortie;
        }
        for index in 0..nombre {
            if let Ok(ext) = liste.GetValueAtIndex(index) {
                sortie.push((
                    Chargee {
                        id: identifiant(&ext),
                        nom: nom(&ext),
                        active: allumee(&ext),
                    },
                    ext,
                ));
            }
        }
    }
    sortie
}

/// Retrouve une extension du moteur : par identifiant, sinon par nom (s'il est unique).
///
/// Le repli par le nom sert aux extensions deposees a la main **sans cle** dans leur
/// manifeste : Chromium leur donne l'empreinte de leur chemin comme identifiant, qui n'est pas
/// le nom de leur dossier (mesure du 2026-11-21 : `preuve-chrome` devient
/// `cfmnfkohahigcphgcoimlckammkneabk`).
fn chercher(
    liste: &ICoreWebView2BrowserExtensionList,
    id: &str,
    nom: &str,
) -> Option<ICoreWebView2BrowserExtension> {
    let toutes = decrire(liste);
    if let Some((_, ext)) = toutes.iter().find(|(c, _)| c.id == id) {
        return Some(ext.clone());
    }
    if nom.is_empty() {
        return None;
    }
    let memes: Vec<_> = toutes.iter().filter(|(c, _)| c.nom == nom).collect();
    if memes.len() == 1 {
        return Some(memes[0].1.clone());
    }
    None
}

fn envoyer(proxy: &EventLoopProxy<UserEvent>, bilan: Bilan) {
    let _ = proxy.send_event(UserEvent::MoteurBilan(bilan));
}

fn echec(proxy: &EventLoopProxy<UserEvent>, action: Action, id: &str, detail: String) {
    envoyer(
        proxy,
        Bilan {
            action,
            id: id.to_string(),
            reussi: false,
            detail,
        },
    );
}

/// Demande au moteur la liste de ce qu'il a charge. Le resultat revient en
/// `UserEvent::MoteurListe`.
pub fn lister(webview: &WebView, proxy: EventLoopProxy<UserEvent>) {
    let profil = match profil(webview) {
        Ok(p) => p,
        Err(e) => {
            crate::alerte!("moteur d'extensions : profil inaccessible ({e})");
            return;
        }
    };
    let retour = proxy.clone();
    let gestionnaire = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
        move |resultat, liste| {
            let chargees = match (resultat, liste) {
                (Ok(()), Some(liste)) => decrire(&liste).into_iter().map(|(c, _)| c).collect(),
                (Err(e), _) => {
                    crate::alerte!("moteur d'extensions : liste illisible ({e})");
                    Vec::new()
                }
                _ => Vec::new(),
            };
            let _ = retour.send_event(UserEvent::MoteurListe(chargees));
            Ok(())
        },
    ));
    if let Err(e) = unsafe { profil.GetBrowserExtensions(&gestionnaire) } {
        crate::alerte!("moteur d'extensions : liste impossible ({e})");
    }
}

/// Charge un dossier d'extension dans le moteur, sans redemarrer.
///
/// Sert a l'installation, a la mise a jour (meme dossier : Chromium recharge l'extension en
/// gardant ses donnees) et a la reactivation d'une extension que le moteur ne connait plus.
pub fn ajouter(webview: &WebView, proxy: EventLoopProxy<UserEvent>, dossier: &Path, attendu: &str) {
    let profil = match profil(webview) {
        Ok(p) => p,
        Err(e) => return echec(&proxy, Action::Ajout, attendu, format!("profil inaccessible : {e}")),
    };
    let attendu_cb = attendu.to_string();
    let retour = proxy.clone();
    let gestionnaire = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
        move |resultat, ext| {
            let bilan = match (resultat, ext) {
                (Ok(()), Some(ext)) => Bilan {
                    action: Action::Ajout,
                    id: identifiant(&ext),
                    reussi: true,
                    detail: nom(&ext),
                },
                (Ok(()), None) => Bilan {
                    action: Action::Ajout,
                    id: attendu_cb,
                    reussi: true,
                    detail: String::new(),
                },
                (Err(e), _) => Bilan {
                    action: Action::Ajout,
                    id: attendu_cb,
                    reussi: false,
                    detail: e.message().to_string(),
                },
            };
            envoyer(&retour, bilan);
            Ok(())
        },
    ));
    let chemin = HSTRING::from(dossier);
    if let Err(e) = unsafe { profil.AddBrowserExtension(&chemin, &gestionnaire) } {
        echec(&proxy, Action::Ajout, attendu, e.message().to_string());
    }
}

/// Allume ou eteint une extension deja connue du moteur — ses donnees sont gardees (une
/// connexion Bitwarden survit a une extinction). Si le moteur ne la connait pas et qu'on veut
/// l'allumer, on la charge depuis `dossier`.
pub fn allumer(
    webview: &WebView,
    proxy: EventLoopProxy<UserEvent>,
    id: &str,
    nom: &str,
    active: bool,
    dossier: Option<&Path>,
) {
    let action = if active { Action::Allumage } else { Action::Extinction };
    let profil = match profil(webview) {
        Ok(p) => p,
        Err(e) => return echec(&proxy, action, id, format!("profil inaccessible : {e}")),
    };
    let id_cb = id.to_string();
    let nom_cb = nom.to_string();
    let dossier_cb = dossier.map(|d| d.to_path_buf());
    let profil_cb = profil.clone();
    let retour = proxy.clone();
    let gestionnaire = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
        move |resultat, liste| {
            let trouvee = match (resultat, liste) {
                (Ok(()), Some(liste)) => chercher(&liste, &id_cb, &nom_cb),
                _ => None,
            };
            match trouvee {
                Some(ext) => {
                    let retour_fin = retour.clone();
                    let id_fin = id_cb.clone();
                    let fin = BrowserExtensionEnableCompletedHandler::create(Box::new(
                        move |resultat| {
                            envoyer(
                                &retour_fin,
                                Bilan {
                                    action,
                                    id: id_fin,
                                    reussi: resultat.is_ok(),
                                    detail: resultat.err().map(|e| e.message().to_string()).unwrap_or_default(),
                                },
                            );
                            Ok(())
                        },
                    ));
                    if let Err(e) = unsafe { ext.Enable(active, &fin) } {
                        echec(&retour, action, &id_cb, e.message().to_string());
                    }
                }
                None if active => match dossier_cb {
                    Some(dossier) => {
                        // Inconnue du moteur : on la charge, ce qui l'allume.
                        let retour_fin = retour.clone();
                        let id_fin = id_cb.clone();
                        let fin = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
                            move |resultat, _ext| {
                                envoyer(
                                    &retour_fin,
                                    Bilan {
                                        action: Action::Allumage,
                                        id: id_fin,
                                        reussi: resultat.is_ok(),
                                        detail: resultat.err().map(|e| e.message().to_string()).unwrap_or_default(),
                                    },
                                );
                                Ok(())
                            },
                        ));
                        let chemin = HSTRING::from(dossier.as_path());
                        if let Err(e) = unsafe { profil_cb.AddBrowserExtension(&chemin, &fin) } {
                            echec(&retour, action, &id_cb, e.message().to_string());
                        }
                    }
                    None => echec(&retour, action, &id_cb, "inconnue du moteur".into()),
                },
                // A eteindre, et deja absente du moteur : c'est l'etat voulu.
                None => envoyer(
                    &retour,
                    Bilan {
                        action,
                        id: id_cb.clone(),
                        reussi: true,
                        detail: String::new(),
                    },
                ),
            }
            Ok(())
        },
    ));
    if let Err(e) = unsafe { profil.GetBrowserExtensions(&gestionnaire) } {
        echec(&proxy, action, id, e.message().to_string());
    }
}

/// Une extension que Trame gere : identifiant, nom lisible, dossier sur le disque.
#[derive(Debug, Clone)]
pub struct Geree {
    pub id: String,
    pub nom: String,
    pub dossier: std::path::PathBuf,
}

/// Aligne le profil de la webview (une cloison) sur `voulues` : ajoute celles qui manquent,
/// retire celles que Trame gere (`gerees`) mais qui n'y sont plus voulues. Les extensions
/// integrees du moteur (lecteur PDF, presse-papiers de Microsoft) ne sont jamais touchees.
/// Le resultat revient en `UserEvent::CloisonAlignee`.
pub fn reconcilier(
    webview: &WebView,
    proxy: EventLoopProxy<UserEvent>,
    profil_nom: &str,
    voulues: Vec<Geree>,
    gerees: Vec<Geree>,
) {
    let profil = match profil(webview) {
        Ok(p) => p,
        Err(e) => {
            crate::alerte!("cloison {profil_nom} : profil inaccessible ({e})");
            return;
        }
    };
    let profil_cb = profil.clone();
    let nom_profil = profil_nom.to_string();
    let retour = proxy.clone();
    let gestionnaire = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
        move |resultat, liste| {
            let chargees = match (resultat, liste) {
                (Ok(()), Some(liste)) => decrire(&liste),
                _ => Vec::new(),
            };
            let correspond = |c: &Chargee, g: &Geree| c.id == g.id || (!g.nom.is_empty() && c.nom == g.nom);
            let mut retirees = 0usize;
            // 1. retirer ce qui n'est plus voulu
            for (c, ext) in &chargees {
                let geree = gerees.iter().any(|g| correspond(c, g));
                let voulue = voulues.iter().any(|g| correspond(c, g));
                if geree && !voulue {
                    let fin = BrowserExtensionRemoveCompletedHandler::create(Box::new(|_| Ok(())));
                    if unsafe { ext.Remove(&fin) }.is_ok() {
                        retirees += 1;
                    }
                } else if voulue && !c.active {
                    // Presente mais eteinte (profil reutilise) : on la rallume.
                    let fin = BrowserExtensionEnableCompletedHandler::create(Box::new(|_| Ok(())));
                    let _ = unsafe { ext.Enable(true, &fin) };
                }
            }
            // 2. ajouter ce qui manque. Le bilan ne part qu'une fois TOUS les ajouts termines :
            //    l'annoncer des la demande faisait recharger la page pendant son premier
            //    chargement, qui n'aboutissait plus (constate le 2026-11-21, scenario `t`).
            let manquantes: Vec<&Geree> = voulues
                .iter()
                .filter(|g| !chargees.iter().any(|(c, _)| correspond(c, g)))
                .collect();
            let reste = std::rc::Rc::new(std::cell::Cell::new(manquantes.len()));
            let reussies = std::rc::Rc::new(std::cell::Cell::new(0usize));
            let annoncer = {
                let retour = retour.clone();
                let nom_profil = nom_profil.clone();
                move |ajoutees: usize| {
                    let _ = retour.send_event(UserEvent::CloisonAlignee {
                        profil: nom_profil.clone(),
                        ajoutees,
                        retirees,
                    });
                }
            };
            if manquantes.is_empty() {
                annoncer(0);
                return Ok(());
            }
            let annoncer = std::rc::Rc::new(annoncer);
            for g in manquantes {
                let reste_cb = reste.clone();
                let reussies_cb = reussies.clone();
                let annoncer_cb = annoncer.clone();
                let id = g.id.clone();
                let profil_log = nom_profil.clone();
                let fin = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(
                    move |resultat, _| {
                        match resultat {
                            Ok(()) => reussies_cb.set(reussies_cb.get() + 1),
                            Err(e) => crate::alerte!("cloison {profil_log} : ajout de {id} echoue ({e})"),
                        }
                        reste_cb.set(reste_cb.get().saturating_sub(1));
                        if reste_cb.get() == 0 {
                            annoncer_cb(reussies_cb.get());
                        }
                        Ok(())
                    },
                ));
                let chemin = HSTRING::from(g.dossier.as_path());
                if let Err(e) = unsafe { profil_cb.AddBrowserExtension(&chemin, &fin) } {
                    crate::alerte!("cloison {nom_profil} : ajout de {} refuse ({e})", g.id);
                    reste.set(reste.get().saturating_sub(1));
                    if reste.get() == 0 {
                        annoncer(reussies.get());
                    }
                }
            }
            Ok(())
        },
    ));
    if let Err(e) = unsafe { profil.GetBrowserExtensions(&gestionnaire) } {
        crate::alerte!("cloison {profil_nom} : liste impossible ({e})");
    }
}

/// Retire une extension du moteur (desinstallation). Ses donnees sont effacees par Chromium,
/// comme dans Chrome ; son dossier, lui, est deja aux archives.
pub fn retirer(webview: &WebView, proxy: EventLoopProxy<UserEvent>, id: &str, nom: &str) {
    let profil = match profil(webview) {
        Ok(p) => p,
        Err(e) => return echec(&proxy, Action::Retrait, id, format!("profil inaccessible : {e}")),
    };
    let id_cb = id.to_string();
    let nom_cb = nom.to_string();
    let retour = proxy.clone();
    let gestionnaire = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
        move |resultat, liste| {
            let trouvee = match (resultat, liste) {
                (Ok(()), Some(liste)) => chercher(&liste, &id_cb, &nom_cb),
                _ => None,
            };
            match trouvee {
                Some(ext) => {
                    let retour_fin = retour.clone();
                    let id_fin = id_cb.clone();
                    let fin = BrowserExtensionRemoveCompletedHandler::create(Box::new(
                        move |resultat| {
                            envoyer(
                                &retour_fin,
                                Bilan {
                                    action: Action::Retrait,
                                    id: id_fin,
                                    reussi: resultat.is_ok(),
                                    detail: resultat.err().map(|e| e.message().to_string()).unwrap_or_default(),
                                },
                            );
                            Ok(())
                        },
                    ));
                    if let Err(e) = unsafe { ext.Remove(&fin) } {
                        echec(&retour, Action::Retrait, &id_cb, e.message().to_string());
                    }
                }
                None => envoyer(
                    &retour,
                    Bilan {
                        action: Action::Retrait,
                        id: id_cb.clone(),
                        reussi: true,
                        detail: String::new(),
                    },
                ),
            }
            Ok(())
        },
    ));
    if let Err(e) = unsafe { profil.GetBrowserExtensions(&gestionnaire) } {
        echec(&proxy, Action::Retrait, id, e.message().to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::cookie_du_site;

    #[test]
    fn un_cookie_appartient_au_site_et_a_ses_sous_domaines_seulement() {
        assert!(cookie_du_site(".aliexpress.com", "aliexpress.com"));
        assert!(cookie_du_site("fr.aliexpress.com", "aliexpress.com"));
        assert!(cookie_du_site("aliexpress.com", "aliexpress.com"));
        assert!(cookie_du_site(".AliExpress.com", "aliexpress.com"));
        // Un autre site qui finit pareil n'est pas le meme site.
        assert!(!cookie_du_site("notaliexpress.com", "aliexpress.com"));
        assert!(!cookie_du_site(".google.com", "aliexpress.com"));
        // Un site vide ne doit jamais tout effacer.
        assert!(!cookie_du_site(".google.com", ""));
    }
}
