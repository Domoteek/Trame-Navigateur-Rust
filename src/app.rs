//! Coeur de Trame : l'etat, les surfaces, les commandes.
//!
//! Trois surfaces d'interface (chacune sa webview enfant) :
//!   - `rail`  : le rail des fils et des onglets, pleine hauteur, a gauche ;
//!   - `barre` : la Couture et les commandes, en haut a droite ;
//!   - `vue`   : la trame tissee, creee a la demande, dans la zone de contenu.
//! Le contenu des onglets occupe la zone restante. Le menu des extensions, les resultats de
//! la Couture et la fenetre d'une extension se posent PAR-DESSUS la page : `WS_CLIPSIBLINGS`,
//! ordre d'empilement et region de fenetre (`fenetres.rs`, `App::superposer`).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tao::event_loop::EventLoopProxy;
use tao::window::Window;
use wry::{NewWindowResponse, WebView, WebViewBuilder};

use crate::extensions;
use crate::extensions_chrome;
use crate::installation;
use crate::ressources;
use crate::log::maintenant_ms;
use crate::model::*;
use crate::profile::Profil;
use crate::protocol;
use crate::shell::{self, Geometrie};
use crate::{alerte, info, probe, search, urls, vitality, weave};

/// Hauteur logique du panneau de la Couture quand il est ouvert.
pub const HAUTEUR_PANNEAU: f64 = 420.0;
/// Page de reglages interne.
pub const PAGE_REGLAGES: &str = "trame://localhost/reglages.html";
pub const PAGE_TELECHARGEMENTS: &str = "trame://localhost/telechargements.html";

/// Quelle surface a envoye un message.
#[derive(Clone, Copy, Debug)]
pub enum Source {
    Rail,
    Barre,
    Vue,
    Zap,
    /// L'apercu flottant du coup d'oeil : une page web quelconque, donc aussi peu de droits
    /// qu'un onglet distant (voir `traiter_ipc`).
    Apercu,
    Onglet(TabId),
}

#[derive(Debug)]
pub enum UserEvent {
    Ipc {
        source: Source,
        corps: String,
    },
    Tick,
    /// Mesure reelle de l'arbre de processus (fil de mesure, toutes les 4 s).
    Memoire(probe::Mesure),
    Titre {
        onglet: TabId,
        titre: String,
    },
    Chargement {
        onglet: TabId,
        termine: bool,
        url: String,
    },
    Icone {
        onglet: TabId,
        url: String,
    },
    /// La page entre (`actif`) ou sort du plein ecran.
    PleinEcran {
        onglet: TabId,
        actif: bool,
    },
    /// L'image de l'icone d'un onglet, lue par WebView2 (`data:image/png;base64,…`).
    IconeImage {
        onglet: TabId,
        donnees: String,
    },
    NouvelleFenetre {
        onglet: TabId,
        url: String,
    },
    /// Trame est-il le navigateur par defaut de Windows ? (lu en arriere-plan)
    ParDefaut(bool),
    /// L'apercu a navigue (lien suivi dedans) : son adresse courante.
    ApercuAdresse(String),
    /// Reponse du moteur a une demande de mise en pause d'un onglet cache.
    PauseOnglet { onglet: TabId, faite: bool },
    /// Une navigation a ete refusee pour etre relancee sans ses parametres de pistage.
    AdresseNettoyee {
        onglet: TabId,
        origine: String,
        propre: String,
    },
    /// Une requete a ete refusee par le blocage d'une extension (`bloquer`).
    AdresseBloquee { onglet: TabId, adresse: String },
    /// Une navigation vers une adresse interne de Chromium a ete refusee.
    NavigationRefusee(String),
    /// Un telechargement commence (destination choisie, suivi branche).
    TelechargementDebut(crate::telechargements::Telechargement),
    /// Le moteur signale un changement d'etat d'un telechargement.
    TelechargementEtat {
        id: u64,
        termine: bool,
        interrompu: bool,
        reprenable: bool,
        raison: i32,
        recu: u64,
    },
    /// Un telechargement s'est termine (reussi ou non).
    Telechargement {
        url: String,
        chemin: Option<String>,
        reussi: bool,
    },
    Log(String),
    /// Une installation d'extension (fil annexe) est terminee.
    Installation {
        libelle: String,
        issue: installation::Issue,
    },
    /// Le moteur de WebView2 a repondu a une operation sur une extension.
    MoteurBilan(crate::moteur_ext::Bilan),
    /// Ce que le moteur de WebView2 a reellement charge.
    MoteurListe(Vec<crate::moteur_ext::Chargee>),
    /// La fiche produit a repondu : une version plus recente est-elle annoncee ?
    Maj(Option<crate::maj::Nouveaute>),
    /// Taille du contenu de la fenetre d'extension ouverte (pixels CSS).
    TaillePopup { largeur: f64, hauteur: f64 },
    /// Texte d'une page lue (memoire de lecture), rendu par le script de lecture.
    Lecture { onglet: TabId, url: String, valeur: String },
    /// Pages retrouvees dans la memoire de lecture (fil de recherche).
    LecturesTrouvees { rid: u64, items: Value },
    /// La page d'un guet a fini de charger dans la sentinelle.
    GuetCharge { id: u64 },
    /// Texte lu pour un guet (`None` : element introuvable).
    GuetLu { id: u64, texte: Option<String> },
    /// Script a executer dans une surface (reponse d'un travail fait en arriere-plan).
    Script { source: Source, js: String },
    /// Un element vient d'etre designe pour un nouveau guet.
    GuetPose { onglet: TabId, selecteur: String, texte: Option<String> },
    /// Le profil d'une cloison a ete aligne sur ses extensions voulues.
    CloisonAlignee {
        profil: String,
        ajoutees: usize,
        retirees: usize,
    },
    /// Une sauvegarde, une restauration ou un echange de favoris (fil annexe) est terminee.
    Sauvegarde(crate::sauvegarde::Issue),
}

/// Ce qu'on demande au moteur d'extensions de WebView2, a chaud.
#[derive(Debug, Clone, Copy)]
enum OperationMoteur {
    /// Charger (ou recharger, apres une mise a jour) le dossier de l'extension.
    Charger,
    Allumer,
    Eteindre,
    Retirer,
}

pub struct App {
    window: Window,
    /// Onglet dont une video occupe toute la fenetre (rail et barre caches), et si c'est Trame
    /// qui a passe la fenetre en plein ecran Windows (mode `ecran`) : il l'en sortira a la fin.
    plein_ecran: Option<TabId>,
    fenetre_basculee: bool,
    profil: Profil,
    proxy: EventLoopProxy<UserEvent>,

    session: Session,
    actif: Option<TabId>,
    rail_replie: bool,
    vue_trame: bool,
    panneau_ouvert: bool,
    /// Hauteur demandee par l'interface quand le panneau est ouvert (elle epouse son contenu).
    panneau_hauteur: f64,
    /// Colonne du menu des extensions dans la barre (x, largeur, en pixels logiques), quand
    /// c'est lui qui est ouvert : la barre est decoupee a cette colonne, la page reste visible
    /// et cliquable autour. `None` : le panneau prend toute la largeur (resultats de la Couture).
    /// Troisieme champ : haut de la colonne quand elle ne part pas de la ligne du haut (menu
    /// d'un onglet ouvert par clic droit, pose a la hauteur de l'onglet), 0 sinon.
    panneau_zone: Option<(f64, f64, f64)>,
    /// Taille de la fenetre d'extension ouverte (pixels logiques) : le panneau epouse son
    /// contenu, comme dans Chrome, au lieu de couvrir toute la hauteur de la page.
    popup_taille: Option<(f64, f64)>,
    /// Ombres portees du menu (barre agrandie) et de la fenetre d'extension (`ombre.rs`).
    ombre_menu: Option<crate::ombre::Ombre>,
    menu_sonde_fait: bool,
    apercu_sonde_fait: bool,
    sonde_js_faite: bool,
    ombre_popup: Option<crate::ombre::Ombre>,

    rail: Option<WebView>,
    barre: Option<WebView>,
    vue: Option<WebView>,
    zap: Option<WebView>,
    webviews: std::collections::HashMap<TabId, WebView>,

    /// Regles de masquage (le « zappeur ») et etat du panneau de droite.
    zap_regles: Vec<RegleZap>,
    /// Ce que chaque regle attrape dans chaque onglet (index de regle, nombre d'elements),
    /// remonte par le script vivant du zappeur.
    zap_comptes: std::collections::HashMap<TabId, Vec<(usize, u32)>>,
    /// Regles ajoutees pendant cette session, dans l'ordre (domaine, selecteur, action, texte) :
    /// le bouton « Annuler » retire la derniere.
    zap_ajouts: Vec<(String, String, String, String)>,
    /// Onglets qui jouent du son en ce moment (releve chaque seconde) : haut-parleur dans le rail.
    sonores: std::collections::HashSet<TabId>,
    /// Onglets dont l'utilisateur a coupe le son. Garde a travers la veille : un onglet reveille
    /// reste muet.
    muets: std::collections::HashSet<TabId>,
    /// Liste des telechargements (la plus recente a la fin), gardee dans `telechargements.json`.
    telechargements: Vec<crate::telechargements::Telechargement>,
    /// Dossier ou vont les telechargements, partage avec les gestionnaires du moteur.
    dossier_dl: std::rc::Rc<std::cell::RefCell<PathBuf>>,
    /// Version plus recente annoncee par la fiche produit, si elle a ete lue.
    maj: Option<crate::maj::Nouveaute>,
    zap_ouvert: bool,
    /// Ce que montre le panneau de droite : le zappeur, ou la popup d'une extension epinglee.
    panneau_extension: Option<String>,
    /// Popup a ouvrir au premier battement (sonde `TRAME_PANNEAU`, jamais utilisee en usage courant).
    panneau_demande: Option<String>,
    /// Adresse a naviguer au premier battement (sonde `TRAME_NAV`, diagnostic seulement).
    nav_demandee: Option<String>,
    /// Facteur de zoom a appliquer au premier battement (sonde `TRAME_ZOOM`, diagnostic).
    zoom_demande: Option<f64>,
    /// Fichier `.crx` a installer au premier battement (sonde `TRAME_CRX`, diagnostic).
    crx_demande: Option<String>,
    /// Identifiant d'extension a installer depuis le magasin (sonde `TRAME_EXT`, diagnostic).
    ext_demande: Option<String>,
    /// Extension Trame a allumer ou couper au premier battement, et dans quel sens
    /// (sonde `TRAME_EXT_ETAT`, diagnostic : verifier qu'un choix se memorise vraiment).
    ext_etat_demande: Option<(String, bool)>,
    /// Facteur de zoom par onglet (1.0 = taille normale). Non persiste : c'est un confort de
    /// lecture, pas un fait de profil.
    zooms: std::collections::HashMap<TabId, f64>,
    /// Extensions trouvees dans Chrome/Edge lors du dernier scan (import possible).
    chrome_disponibles: Vec<Value>,

    extensions: Vec<Extension>,
    historique: Vec<EntreeHistorique>,
    ext_storage: Value,
    reglages: Reglages,
    /// Dossier des extensions Chrome (decompressees) chargees par WebView2.
    extensions_chrome: PathBuf,
    /// Le chemin n'est transmis qu'a la PREMIERE webview creee : les extensions sont
    /// installees au niveau du profil, les redonner a chaque webview les reinstallerait.
    extensions_chrome_amorcees: bool,
    /// Ce que le moteur a charge (identifiant -> allumee). `None` tant qu'il n'a pas repondu.
    moteur_extensions: Option<std::collections::HashMap<String, bool>>,
    moteur_interroge: bool,
    /// Sonde `TRAME_EXT_CHROME=<id>` (diagnostic) : eteint puis rallume cette extension Chrome
    /// a chaud, pour prouver que les deux sens marchent sans redemarrage. Etape 0, 1, 2.
    sonde_chrome: Option<(String, u8)>,
    /// Depuis quand chaque onglet reveille est cache (ms), et lesquels sont en memoire basse.
    cache_depuis: std::collections::HashMap<TabId, u64>,
    en_memoire_basse: std::collections::HashSet<TabId>,
    /// Page a charger dans un onglet des qu'il est entierement mis en place.
    navigation_differee: std::collections::HashMap<TabId, String>,
    /// Les extensions du dossier ont-elles ete comparees a celles du moteur (au demarrage) ?
    extensions_alignees: bool,
    /// Les onglets de la session ont-ils ete ouverts ? (apres la reponse du moteur, 3 s au plus)
    restaure: bool,
    /// Ajouts de demarrage, sans message ni rechargement.
    ajouts_silencieux: std::collections::HashSet<String>,
    /// Onglets dont la premiere navigation n'a pas encore abouti, et ceux a recharger des la
    /// fin de leur chargement (voir `recharger_page_active`).
    premier_chargement: std::collections::HashMap<TabId, u64>,
    /// Onglets deja recharges une fois par le filet de securite (jamais deux).
    secours_faits: std::collections::HashSet<TabId>,
    recharger_apres_chargement: std::collections::HashSet<TabId>,
    /// Nettoyage des adresses : (actif, garder l'affiliation), partage avec les gardes de
    /// navigation (qui vivent dans les webviews, hors de la boucle).
    nettoyage: std::sync::Arc<(std::sync::atomic::AtomicBool, std::sync::atomic::AtomicBool)>,
    /// Adresses refusees au reseau (`bloquer` des extensions allumees), partagees avec le
    /// gestionnaire `WebResourceRequested` de chaque onglet.
    blocage: crate::moteur_ext::Blocage,
    /// Coup d'oeil : l'apercu flottant, son adresse courante, son ombre.
    apercu: Option<WebView>,
    /// Trame est-il le navigateur par defaut ? `None` tant que ce n'est pas verifie.
    par_defaut: Option<bool>,
    /// Le panneau droit montre (ou a montre) la fenetre d'une extension, pas les regles.
    zap_montre_extension: bool,
    apercu_url: String,
    ombre_apercu: Option<crate::ombre::Ombre>,
    /// Onglets mis en pause par le moteur, et ceux pour lesquels la demande est faite.
    en_pause: std::collections::HashSet<TabId>,
    pause_demandee: std::collections::HashSet<TabId>,
    /// Adresses nettoyees depuis le demarrage (affiche dans les reglages).
    adresses_nettoyees: u64,
    /// Requetes refusees par le blocage des extensions, et ce qui en a deja ete journalise.
    adresses_bloquees: u64,
    blocages_journalises: std::collections::HashSet<(TabId, String)>,
    /// Derniere mesure complete de la sonde (memoire, extensions, memoire libre).
    derniere_mesure: probe::Mesure,
    /// Nombre d'onglets ouverts permis par la memoire libre (voir `model::budget_adapte`),
    /// recalcule a chaque mesure seulement.
    budget_memoire: Option<usize>,
    /// Mesure en cours du cout d'une extension : (identifiant, memoire des extensions avant,
    /// instant, allumage ?). Le cout se lit 12 s apres l'operation, par difference.
    mesure_extension: Option<(String, f64, u64, bool)>,
    /// Profil WebView2 du panneau droit (`None` : principal). Une popup d'extension d'un fil
    /// cloisonne doit s'ouvrir dans le profil de la cloison, sinon l'extension n'y existe pas.
    zap_profil: Option<String>,
    /// Sonde `TRAME_REVEILLER` (diagnostic, mesures) : reveiller tous les onglets au demarrage.
    reveil_demande: bool,
    /// Installations en cours (libelles), affichees par l'interface.
    installations: Vec<String>,
    /// Noms lisibles des extensions qui viennent d'etre installees, le temps que le moteur reponde.
    noms_en_attente: std::collections::HashMap<String, String>,
    /// Liste des extensions Chrome du disque (voir `liste_extensions_chrome`).
    cache_extensions_chrome: std::cell::RefCell<Option<Vec<Value>>>,
    fermes: Vec<Onglet>,
    defilement_a_restaurer: std::collections::HashMap<TabId, f64>,

    memoire_mo: f64,
    memoire_processus: usize,
    demarrage_ms: u64,
    surfaces_verifiees: bool,
    derniere_taille: (u32, u32),
    /// La fenetre etait agrandie au dernier redimensionnement (bouton agrandir / restaurer).
    agrandie: bool,
    dernier_tick: u64,
    derniere_sauvegarde: u64,
    modifie: bool,
    dernier_toast: u64,

    // ---- carnet et outils (voir `app/outils.rs`)
    favoris: Vec<crate::carnet::Favori>,
    /// Acces rapides (sites), distincts des favoris (pages) depuis le 2026-11-22.
    acces: Vec<crate::carnet::Acces>,
    /// Groupes d'acces rapides, dans l'ordre d'affichage (un groupe peut etre vide).
    acces_groupes: Vec<String>,
    guets: Vec<crate::carnet::Guet>,
    sentinelle: Option<outils::Sentinelle>,
    /// Le prochain element designe dans la page est pour un guet (et non une regle de masquage).
    selection_guet: bool,
    /// Adresse deja gardee dans la memoire de lecture, par onglet.
    lectures_faites: std::collections::HashMap<TabId, String>,
    /// Page affichee et depuis quand (la lecture n'est gardee qu'apres quelques secondes).
    lecture_vue: Option<(TabId, String, u64)>,
    /// Generation de la recherche dans la memoire de lecture (une frappe plus recente l'arrete).
    recherche_lecture: std::sync::Arc<std::sync::atomic::AtomicU64>,
    dernier_guet: u64,
    /// La separation du cote a cote est en train d'etre deplacee.
    cote_glisse: bool,
    /// Le curseur est sur la separation (curseur de redimensionnement affiche).
    cote_survol: bool,
    /// Dernier defilement impose a une page liee (pour ne pas renvoyer l'echo).
    echo_defilement: Option<(TabId, f64)>,
    /// Sonde `TRAME_IPC` (diagnostic) : messages de la barre a rejouer, un par battement.
    sonde_ipc: std::collections::VecDeque<String>,
    /// Onglet sorti dans sa mini-fenetre (video flottante avec ses commandes), voir `flottant.rs`.
    flottant: Option<flottant::Flottant>,
    /// Demande de mini-fenetre en attente : seule la boucle d'evenements peut creer la fenetre.
    flottant_demande: Option<TabId>,
    /// Derniere place de la mini-fenetre (position, taille), reprise a la sortie suivante.
    flottant_place: Option<(tao::dpi::PhysicalPosition<i32>, tao::dpi::PhysicalSize<u32>)>,

    // ---- sauvegarde et restauration (voir `sauvegarde.rs`)
    /// Archive lue et verifiee, qui attend une confirmation : (fichier, resume, contenus).
    restauration: Option<(std::path::PathBuf, String, Vec<String>)>,
    /// Compte rendu de la derniere restauration, a annoncer au premier battement (la barre
    /// n'existe pas encore quand les donnees sont lues).
    message_restauration: Option<String>,
    /// Trame doit se fermer : une restauration s'applique au demarrage suivant.
    quitter: bool,
}

mod fenetre;
mod flottant;
mod outils;

const COULEUR_FOND: (u8, u8, u8, u8) = (13, 16, 22, 255);

/// Mesure la taille naturelle d'une fenetre d'extension : la boite du `body` (ses marges
/// comprises), pas celle de la fenetre — sinon elle ne pourrait jamais retrecir.
const MESURE_POPUP: &str = r#"(function(){
  var b = document.body; if (!b) return JSON.stringify([0,0]);
  var r = b.getBoundingClientRect(), s = getComputedStyle(b);
  var l = Math.ceil(r.width + parseFloat(s.marginLeft||0) + parseFloat(s.marginRight||0));
  var h = Math.ceil(Math.max(r.height + parseFloat(s.marginTop||0) + parseFloat(s.marginBottom||0), b.scrollHeight));
  return JSON.stringify([l, h]);
})()"#;

/// Position et taille d'une zone, en pixels physiques entiers.
fn rect_physique(r: wry::Rect) -> (i32, i32, i32, i32) {
    let (x, y) = match r.position {
        tao::dpi::Position::Physical(p) => (p.x, p.y),
        tao::dpi::Position::Logical(p) => (p.x.round() as i32, p.y.round() as i32),
    };
    let (l, h) = match r.size {
        tao::dpi::Size::Physical(s) => (s.width as i32, s.height as i32),
        tao::dpi::Size::Logical(s) => (s.width.round() as i32, s.height.round() as i32),
    };
    (x, y, l, h)
}

/// Delai avant la mise en pause d'un onglet cache (5 min ; `TRAME_PAUSE_APRES_S` pour les tests).
fn pause_apres_ms() -> u64 {
    std::env::var("TRAME_PAUSE_APRES_S")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|s| s * 1000)
        .unwrap_or(300_000)
}

/// Delai avant le rechargement de secours d'un onglet dont le premier chargement n'aboutit pas.
const SECOURS_CHARGEMENT_MS: u64 = 12_000;
/// Meme chose quand le chargement a commence : une page lente n'est pas une page bloquee.
const SECOURS_CHARGEMENT_LENT_MS: u64 = 30_000;

/// Un onglet cache passe en memoire basse apres ce delai : assez long pour qu'un aller-retour
/// rapide entre deux onglets ne fasse pas osciller le moteur, assez court pour que le gain
/// arrive avant la veille (qui, elle, detruit la webview).
const MEMOIRE_BASSE_APRES_MS: u64 = 20_000;

/// Bornes du zoom : en dessous de 50 % un site devient illisible, au-dessus de 200 % la
/// navigation n'a plus de sens sur un ecran de portable.
const ZOOM_MIN: f64 = 0.5;
const ZOOM_MAX: f64 = 2.0;

/// Ramene un facteur de zoom dans les bornes, arrondi au dixieme.
pub fn zoom_borne(facteur: f64) -> f64 {
    if !facteur.is_finite() {
        return 1.0;
    }
    ((facteur.clamp(ZOOM_MIN, ZOOM_MAX)) * 10.0).round() / 10.0
}

/// Facteur suivant, par pas de 10 %, en restant dans les bornes.
pub fn zoom_suivant(actuel: f64, sens: i32) -> f64 {
    let pas = if sens >= 0 { 0.1 } else { -0.1 };
    zoom_borne(actuel + pas)
}

/// Analyse la sonde `TRAME_EXT_ETAT` : `<id>` (allumer) ou `<id>=on` / `<id>=off`.
///
/// Une sonde, pas une fonctionnalite : elle permet a `outils/verifier-app.mjs` d'allumer une
/// extension sans souris, puis de verifier que ce choix a bien ete ecrit et relu.
pub fn analyser_sonde_etat(valeur: &str) -> Option<(String, bool)> {
    let (id, actif) = match valeur.rsplit_once('=') {
        Some((id, sens)) => {
            let actif = !matches!(
                sens.trim().to_ascii_lowercase().as_str(),
                "off" | "0" | "non" | "false"
            );
            (id, actif)
        }
        None => (valeur, true),
    };
    let id = id.trim();
    (!id.is_empty()).then(|| (id.to_string(), actif))
}

/// Ce qu'il y a a faire d'un telechargement qui vient de se terminer.
#[derive(Debug, PartialEq, Eq)]
pub enum SuiteTelechargement {
    /// Le magasin a fait telecharger un paquet : on l'installe a sa place.
    InstallerPaquet,
    /// Le magasin a refuse de servir : on reprend par cet identifiant, avec notre point d'entree.
    InstallerIdentifiant(String),
    /// Telechargement ordinaire : il reste dans « telechargements ».
    Ordinaire,
}

/// Decide du sort d'un telechargement, sans toucher a rien.
///
/// Deux reconnaissances, et deux seulement :
/// - **le magasin** (`crx::du_magasin`) : la fiche a demande une installation, ou elle a redirige
///   vers `permanently-removed.invalid` en gardant l'identifiant ;
/// - **le paquet** (`crx::fichier_est_paquet`) : le fichier recu est un CRX.
/// Un `.crx` telecharge depuis un autre site reste un simple fichier : Trame n'installe pas en
/// silence un paquet venu d'ailleurs.
pub fn suite_telechargement(
    url: &str,
    chemin: Option<&Path>,
    reussi: bool,
) -> SuiteTelechargement {
    if !crate::crx::du_magasin(url) {
        return SuiteTelechargement::Ordinaire;
    }
    if reussi {
        if let Some(fichier) = chemin {
            if crate::crx::fichier_est_paquet(fichier) {
                return SuiteTelechargement::InstallerPaquet;
            }
        }
    }
    match crate::crx::identifiant(url) {
        Some(id) => SuiteTelechargement::InstallerIdentifiant(id),
        None => SuiteTelechargement::Ordinaire,
    }
}

/// Chemin libre pour un telechargement : `rapport.pdf`, puis `rapport (2).pdf`, etc.
///
/// Ecraser un fichier deja telecharge serait une perte de donnees silencieuse ; le suffixe
/// numerote est ce que fait un navigateur, et il rend le dossier lisible. Les telechargements
/// passent desormais par `telechargements::destination`, qui tient compte de ceux en cours ;
/// celle-ci reste pour son test.
#[cfg_attr(not(test), allow(dead_code))]
pub fn nom_libre(dossier: &Path, nom: &str) -> PathBuf {
    let _ = std::fs::create_dir_all(dossier);
    let base = Path::new(nom);
    let tronc = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "telechargement".to_string());
    let extension = base
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut candidat = dossier.join(format!("{tronc}{extension}"));
    let mut numero = 2;
    while candidat.exists() && numero < 1000 {
        candidat = dossier.join(format!("{tronc} ({numero}){extension}"));
        numero += 1;
    }
    candidat
}

/// Barre de recherche dans la page (Ctrl+F), embarquee a la compilation.
const RECHERCHE_PAGE: &str = include_str!("../ui/recherche.js");

/// Contexte WebView2 partage par toutes les webviews.
///
/// Deux raisons, et les deux comptent :
/// 1. **un seul dossier de donnees**, fixe une fois pour toutes (dans le profil) : c'est lui qui
///    porte les cookies, les connexions aux extensions et les service workers ;
/// 2. WebView2 exige que toutes les webviews d'un meme dossier partagent le meme environnement —
///    un contexte unique est donc la bonne facon de le garantir.
static CONTEXTE_WEBVIEW: std::sync::OnceLock<std::sync::Mutex<wry::WebContext>> =
    std::sync::OnceLock::new();

/// Prepare (une fois) le contexte WebView2 sur un dossier de donnees donne.
fn preparer_contexte(dossier: &Path) {
    let _ = CONTEXTE_WEBVIEW.set(std::sync::Mutex::new(wry::WebContext::new(Some(
        dossier.to_path_buf(),
    ))));
}

/// Emprunte le contexte partage le temps de construire une webview.
///
/// Le verrou n'est tenu que pendant la construction : la boucle d'evenements est sur un seul
/// fil, il n'y a donc aucune contention, et aucune reference ne survit au-dela du `build`.
fn verrou_contexte() -> std::sync::MutexGuard<'static, wry::WebContext> {
    CONTEXTE_WEBVIEW
        .get()
        .expect("contexte WebView2 prepare au demarrage")
        .lock()
        .expect("verrou du contexte WebView2")
}

impl App {
    pub fn nouveau(window: Window, profil: Profil, proxy: EventLoopProxy<UserEvent>) -> App {
        // Tout ce dont le navigateur a besoin vit dans le profil :
        //  1. l'interface y est installee (miroir de la version livree) ;
        //  2. le profil WebView2 (cookies, connexions, extensions) y est rapatrie s'il vivait
        //     encore a cote de l'executable ;
        //  3. le contexte WebView2 est cree sur ce dossier, une fois pour toutes.
        ressources::au_demarrage(profil.racine());
        let webview2 = profil.rapatrier_webview2();
        preparer_contexte(&webview2);
        info!("profil WebView2 : {}", webview2.display());

        let mut app = App {
            window,
            plein_ecran: None,
            fenetre_basculee: false,
            profil,
            proxy,
            session: Session::default(),
            actif: None,
            rail_replie: false,
            vue_trame: false,
            panneau_ouvert: false,
            panneau_hauteur: HAUTEUR_PANNEAU,
            panneau_zone: None,
            popup_taille: None,
            ombre_menu: None,
            menu_sonde_fait: false,
            apercu_sonde_fait: false,
            sonde_js_faite: false,
            ombre_popup: None,
            rail: None,
            barre: None,
            vue: None,
            zap: None,
            webviews: std::collections::HashMap::new(),
            flottant: None,
            flottant_demande: None,
            flottant_place: None,
            zap_regles: Vec::new(),
            zap_comptes: std::collections::HashMap::new(),
            zap_ajouts: Vec::new(),
            sonores: std::collections::HashSet::new(),
            muets: std::collections::HashSet::new(),
            telechargements: Vec::new(),
            dossier_dl: std::rc::Rc::new(std::cell::RefCell::new(PathBuf::new())),
            maj: None,
            zap_ouvert: false,
            panneau_extension: None,
            panneau_demande: std::env::var("TRAME_PANNEAU").ok().filter(|s| !s.is_empty()),
            nav_demandee: std::env::var("TRAME_NAV").ok().filter(|s| !s.is_empty()),
            zoom_demande: std::env::var("TRAME_ZOOM")
                .ok()
                .and_then(|s| s.trim().replace(',', ".").parse::<f64>().ok()),
            crx_demande: std::env::var("TRAME_CRX").ok().filter(|s| !s.is_empty()),
            ext_demande: std::env::var("TRAME_EXT").ok().filter(|s| !s.is_empty()),
            ext_etat_demande: std::env::var("TRAME_EXT_ETAT")
                .ok()
                .filter(|s| !s.is_empty())
                .and_then(|s| analyser_sonde_etat(&s)),
            zooms: std::collections::HashMap::new(),
            chrome_disponibles: Vec::new(),
            extensions: Vec::new(),
            historique: Vec::new(),
            ext_storage: Value::Object(serde_json::Map::new()),
            reglages: Reglages::default(),
            extensions_chrome: PathBuf::new(),
            extensions_chrome_amorcees: false,
            moteur_extensions: None,
            moteur_interroge: false,
            sonde_chrome: std::env::var("TRAME_EXT_CHROME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|id| (id, 0)),
            installations: Vec::new(),
            cache_depuis: std::collections::HashMap::new(),
            en_memoire_basse: std::collections::HashSet::new(),
            zap_profil: None,
            derniere_mesure: probe::Mesure::default(),
            nettoyage: std::sync::Arc::new((
                std::sync::atomic::AtomicBool::new(true),
                std::sync::atomic::AtomicBool::new(true),
            )),
            blocage: crate::moteur_ext::Blocage::default(),
            adresses_nettoyees: 0,
            adresses_bloquees: 0,
            blocages_journalises: std::collections::HashSet::new(),
            en_pause: std::collections::HashSet::new(),
            apercu: None,
            par_defaut: None,
            zap_montre_extension: false,
            apercu_url: String::new(),
            ombre_apercu: None,
            pause_demandee: std::collections::HashSet::new(),
            budget_memoire: None,
            mesure_extension: None,
            premier_chargement: std::collections::HashMap::new(),
            navigation_differee: std::collections::HashMap::new(),
            extensions_alignees: false,
            restaure: false,
            ajouts_silencieux: std::collections::HashSet::new(),
            secours_faits: std::collections::HashSet::new(),
            recharger_apres_chargement: std::collections::HashSet::new(),
            reveil_demande: std::env::var("TRAME_REVEILLER").map(|v| !v.is_empty()).unwrap_or(false),
            noms_en_attente: std::collections::HashMap::new(),
            cache_extensions_chrome: std::cell::RefCell::new(None),
            fermes: Vec::new(),
            defilement_a_restaurer: std::collections::HashMap::new(),
            memoire_mo: 0.0,
            memoire_processus: 0,
            demarrage_ms: maintenant_ms(),
            surfaces_verifiees: false,
            derniere_taille: (0, 0),
            agrandie: false,
            dernier_tick: maintenant_ms(),
            derniere_sauvegarde: 0,
            modifie: false,
            dernier_toast: 0,
            favoris: Vec::new(),
            acces: Vec::new(),
            acces_groupes: Vec::new(),
            guets: Vec::new(),
            sentinelle: None,
            selection_guet: false,
            lectures_faites: std::collections::HashMap::new(),
            lecture_vue: None,
            recherche_lecture: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            dernier_guet: 0,
            cote_glisse: false,
            cote_survol: false,
            echo_defilement: None,
            sonde_ipc: std::env::var("TRAME_IPC")
                .ok()
                .and_then(|s| serde_json::from_str::<Vec<Value>>(&s).ok())
                .map(|v| v.into_iter().map(|m| m.to_string()).collect())
                .unwrap_or_default(),
            restauration: None,
            message_restauration: None,
            quitter: false,
        };
        {
            use tao::platform::windows::WindowExtWindows;
            let fenetre = app.window.hwnd();
            app.ombre_menu = crate::ombre::Ombre::nouvelle(fenetre);
            app.ombre_popup = crate::ombre::Ombre::nouvelle(fenetre);
            app.ombre_apercu = crate::ombre::Ombre::nouvelle(fenetre);
            if app.ombre_menu.is_none() {
                alerte!("ombres : fenetre d'ombre impossible, les menus seront sans ombre");
            }
        }
        app.charger_donnees();
        app.creer_surfaces();
        // Les onglets ne sont ouverts qu'une fois le moteur d'extensions interroge (voir
        // `UserEvent::MoteurListe`) : les extensions qui lui manquent sont ajoutees AVANT la
        // premiere page, et celles qu'il a deja ne sont pas reinstallees. Filet : 3 s au plus.
        app.moteur_interroge = true;
        app.interroger_moteur();
        app
    }

    // ---------------------------------------------------------------- chemins

    /// Dossier de l'interface : celui du profil, installe au demarrage.
    ///
    /// C'est la regle demandee : le navigateur ne depend plus de l'endroit ou se trouve
    /// l'executable. La source livree ne sert qu'au premier demarrage, ou si le profil est en
    /// lecture seule.
    fn racine_ui(&self) -> PathBuf {
        let profil = self.profil.racine().join("ui");
        if profil.join("chrome.html").is_file() {
            return profil;
        }
        match ressources::source() {
            Some(source) => source.join("ui"),
            None => profil,
        }
    }

    /// Les dossiers d'extensions Chrome du profil : actives, desactivees, archives.
    fn dossiers_extensions_chrome(&self) -> extensions_chrome::Dossiers {
        extensions_chrome::Dossiers::depuis(self.extensions_chrome.clone())
    }

    /// Applique les choix persistes a la liste chargee depuis le disque.
    ///
    /// Le manifeste ne donne qu'un defaut (`enabled_by_default`) ; ce qui compte, c'est le
    /// choix de l'utilisateur, **dans les deux sens** : une extension livree eteinte qu'on
    /// allume doit rester allumee au demarrage suivant.
    fn appliquer_etat_extensions(&mut self) {
        for e in self.extensions.iter_mut() {
            e.active = self.reglages.extension_active(&e.id, e.active);
        }
        for (id, actif) in &self.reglages.extensions_etat {
            if self.extensions.iter().any(|e| &e.id == id) {
                info!(
                    "extension {id} : {} (choix memorise)",
                    if *actif { "activee" } else { "coupee" }
                );
            }
        }
        let actives = self.extensions.iter().filter(|e| e.active).count();
        info!(
            "extensions : {actives} active(s) sur {} ({} choix memorise(s))",
            self.extensions.len(),
            self.reglages.extensions_etat.len()
        );
        self.appliquer_blocage();
    }

    /// Reporte les adresses a refuser (extensions allumees) dans l'etat partage avec les
    /// onglets. Effet immediat sur les onglets ouverts, sans les recreer.
    fn appliquer_blocage(&self) {
        let motifs = extensions::motifs_bloques(&self.extensions);
        let n = motifs.len();
        if let Ok(mut m) = self.blocage.motifs.write() {
            *m = motifs;
        }
        info!("blocage reseau : {n} motif(s) actif(s)");
    }

    /// Allume ou coupe une extension Trame, et **memorise le choix** dans `reglages.json`.
    ///
    /// Un seul chemin pour les deux sens (interface, sonde de test) : c'est ce qui garantit
    /// qu'un interrupteur allume ne se rallume pas eteint au demarrage suivant.
    fn basculer_extension(&mut self, id: &str, actif: bool) {
        let Some(nom) = self
            .extensions
            .iter_mut()
            .find(|e| e.id == id)
            .map(|e| {
                e.active = actif;
                e.manifeste.name.clone()
            })
        else {
            self.signaler(&format!("Extension inconnue : {id}."));
            return;
        };
        self.reglages.memoriser_extension(id, actif);
        self.profil.sauver_reglages(&self.reglages);
        info!(
            "extension {id} : {} (choix memorise)",
            if actif { "activee" } else { "coupee" }
        );
        // Le blocage reseau suit tout de suite ; les scripts `js_debut`, eux, sont poses a la
        // creation d'un onglet (ils valent pour les onglets suivants).
        self.appliquer_blocage();
        // L'injection a lieu en fin de chargement : on recharge la page pour que le changement
        // se voie tout de suite, dans les deux sens.
        let suite = if self.recharger_page_active() {
            " — page rechargée"
        } else {
            ""
        };
        self.signaler(&if actif {
            format!("« {nom} » activée{suite}.")
        } else {
            format!("« {nom} » désactivée{suite}.")
        });
        self.pousser_etat();
    }

    /// Extensions Trame livrees : la meme source que l'interface, a cote de l'executable.
    fn racine_extensions_livrees(&self) -> PathBuf {
        ressources::source()
            .map(|source| source.join("extensions"))
            .unwrap_or_else(|| PathBuf::from("extensions"))
    }

    // ---------------------------------------------------------------- demarrage

    fn charger_donnees(&mut self) {
        self.session = self.profil.charger_session();        self.historique = self.profil.charger_historique();
        // Une restauration a-t-elle eu lieu au demarrage ? (elle s'applique avant la lecture,
        // cf. `sauvegarde::appliquer_restauration_en_attente`) Le compte rendu s'annonce au
        // premier battement, quand la barre existe.
        self.message_restauration = crate::sauvegarde::message_de_restauration(self.profil.racine());
        self.ext_storage = self.profil.charger_ext_storage();
        self.reglages = self.profil.charger_reglages();
        self.appliquer_nettoyage();
        // Telechargements : la liste, et ceux qu'une fermeture a coupes en route.
        *self.dossier_dl.borrow_mut() = self.dossier_telechargements();
        self.telechargements = self.profil.charger_telechargements();
        let mut repris = 0;
        let mut coupes = 0;
        for t in self.telechargements.iter_mut().filter(|t| t.actif()) {
            // Fichier complet sur le disque : le moteur a omis de le dire (WebView2 #5638), et
            // la liste n'a jamais recu son evenement de fin. Ce n'est pas une coupure, et il
            // serait faux de proposer de le retelecharger : on le reconnait termine.
            if crate::telechargements::fichier_complet(&t.chemin, t.total) {
                t.etat = "termine".into();
                t.fin = if t.fin > 0 { t.fin } else { t.debut };
                if let Some(total) = t.total {
                    t.recu = t.recu.max(total);
                }
                repris += 1;
            } else {
                t.etat = "interrompu".into();
                t.raison = crate::telechargements::raison_lisible(41);
                coupes += 1;
            }
        }
        if repris > 0 {
            info!("telechargements : {repris} termine(s) reconnu(s) a la relecture (le moteur ne l'avait pas dit)");
        }
        if repris + coupes > 0 {
            self.profil.sauver_telechargements(&self.telechargements);
        }
        // Mise a jour : la fiche produit est lue en fil annexe, pour ne pas retarder l'affichage.
        // Un profil jetable (`TRAME_PROFIL`, celui des bancs d'essai) ne va pas chercher de mise
        // a jour : un essai ne doit ni ouvrir d'onglet ni telecharger quoi que ce soit.
        if std::env::var_os("TRAME_PROFIL").is_none() {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                let nouveaute = crate::maj::verifier(crate::maj::ACTUELLE);
                let _ = proxy.send_event(UserEvent::Maj(nouveaute));
            });
        }
        self.extensions_chrome = self.profil.racine().join("extensions-chrome");
        protocol::definir_extensions_chrome(self.extensions_chrome.clone());
        // Avant que le moteur ne lance les service workers : `chrome.windows.create` rendu
        // conforme (passkey de Bitwarden). N'ecrit que ce qui manque, ne reinstalle rien.
        extensions_chrome::adapter_toutes(&self.dossiers_extensions_chrome());
        self.zap_regles = self.profil.charger_zap();
        // Paquets de regles livres (« YouTube sans Shorts »…) : proposes une seule fois.
        let (ajoutees, traites) =
            crate::model::installer_paquets_zap(&mut self.zap_regles, &self.reglages.paquets_zap);
        if !traites.is_empty() {
            self.reglages.paquets_zap.extend(traites.iter().cloned());
            self.profil.sauver_reglages(&self.reglages);
            self.profil.sauver_zap(&self.zap_regles);
            info!("zappeur : paquet(s) {} installe(s), {ajoutees} regle(s) ajoutee(s)", traites.join(", "));
        }
        self.charger_carnet();
        info!("zappeur : {} regle(s) de masquage", self.zap_regles.len());
        if let Err(e) = std::fs::create_dir_all(&self.extensions_chrome) {
            alerte!("extensions Chrome : dossier impossible ({e})");
        }
        // On materialise les reglages au premier lancement : un fichier present vaut mieux
        // qu'un defaut invisible quand on veut verifier ce qui est reellement applique.
        if !self.profil.fichier_reglages().exists() {
            self.profil.sauver_reglages(&self.reglages);
        }
        info!(
            "moteur de recherche : {} (barre d'extensions : {})",
            urls::moteur(&self.reglages.moteur).nom,
            self.reglages.barre_extensions
        );
        self.profil.amorcer_extensions(&self.racine_extensions_livrees());
        self.extensions = extensions::charger(&self.profil.dossier_extensions());
        self.appliquer_etat_extensions();
        self.rail_replie = self.session.rail_replie;

        if self.session.onglets.is_empty() {
            let now = maintenant_ms();
            let fil = Fil::neuf(1, "Depart".to_string(), now);
            let mut o = Onglet::neuf(
                1,
                urls::NOUVEL_ONGLET.to_string(),
                1,
                Declencheur::NouvelOnglet,
                now,
            );
            o.titre = "Nouvel onglet".to_string();
            self.session.fils.push(fil);
            self.session.onglets.push(o);
            self.session.actif = Some(1);
            self.session.prochain_onglet = 2;
            self.session.prochain_fil = 2;
            self.modifie = true;
        } else {
            // Reprise : tous les onglets demarrent en veille, seul l'actif s'eveille.
            for o in self.session.onglets.iter_mut() {
                o.eveille = false;
            }
            self.session.prochain_onglet = self
                .session
                .onglets
                .iter()
                .map(|o| o.id)
                .max()
                .unwrap_or(0)
                .max(self.session.prochain_onglet)
                + 1;
            self.session.prochain_fil = self
                .session
                .fils
                .iter()
                .map(|f| f.id)
                .max()
                .unwrap_or(0)
                .max(self.session.prochain_fil)
                + 1;
        }
    }

    fn creer_surfaces(&mut self) {
        self.rail = self.creer_surface("rail", Source::Rail);
        self.barre = self.creer_surface("barre", Source::Barre);
        self.appliquer_anti_pistage(None);
        self.appliquer_geometrie();
        if self.rail.is_none() || self.barre.is_none() {
            alerte!("interface : une surface n'a pas pu etre creee, l'application sera muette");
        }
    }

    /// Active le moteur d'extensions de WebView2.
    ///
    /// Deux points appris dans la source de wry (0.57, `webview2/mod.rs`) :
    ///   - `with_extensions_path` installe **chaque sous-dossier** du chemin via
    ///     `ICoreWebView2Profile7::AddBrowserExtension` — c'est le vrai moteur de Chromium,
    ///     pas notre injecteur ;
    ///   - l'installation se fait **au niveau du profil**, donc on ne transmet le chemin
    ///     qu'a la premiere webview creee : le redonner a chaque onglet reinstallerait tout.
    ///
    /// Toutes les webviews doivent avoir le MEME reglage, sinon WebView2 refuse la connexion
    /// a un environnement deja lance (ERROR_INVALID_STATE). D'ou l'appel systematique ici.
    fn habiller<'a>(&mut self, builder: WebViewBuilder<'a>) -> WebViewBuilder<'a> {
        use wry::WebViewBuilderExtWindows;
        // WebView2 consomme les raccourcis de navigateur (Ctrl+W, Ctrl+R, F5, Ctrl+F…) avant
        // que la page ne les voie : constate ici, Ctrl+W ne fermait aucun onglet. On les
        // desactive — les raccourcis de Trame sont geres par le script d'amorce de l'onglet.
        // `TRAME_SANS_EXTENSIONS_CHROME` (diagnostic) : demarrer sans le moteur d'extensions, pour
        // comparer — rien n'est desinstalle, les dossiers restent en place.
        let sans_ext = std::env::var("TRAME_SANS_EXTENSIONS_CHROME").is_ok();
        let builder = builder
            .with_browser_extensions_enabled(!sans_ext)
            .with_browser_accelerator_keys(false);
        // Les extensions ne sont PLUS reinstallees au demarrage (`with_extensions_path`).
        // WebView2 les garde d'une fois sur l'autre ; les reinstaller a chaque lancement les
        // rechargeait pendant que la premiere page chargeait, et coupait le flux du service
        // worker de YouTube (page bloquee en « loading », F5 la reparait) — mesure du
        // 2026-11-22 sur le profil reel, port de debogage a l'appui. Seules les extensions que
        // le moteur n'a pas encore sont ajoutees, une fois sa liste connue
        // (`aligner_extensions_au_demarrage`).
        let _ = sans_ext;
        self.extensions_chrome_amorcees = true;
        builder
    }

    /// Au demarrage, une fois la liste du moteur connue : ajoute les extensions du dossier que
    /// le moteur n'a pas (premiere installation, dossier depose a la main). En silence : ni
    /// message, ni rechargement de page.
    fn aligner_extensions_au_demarrage(&mut self, chargees: &[crate::moteur_ext::Chargee]) {
        let Some(barre) = self.barre.as_ref() else { return };
        let manquantes: Vec<(String, String)> = self
            .liste_extensions_chrome()
            .iter()
            .filter(|e| e["actif"].as_bool().unwrap_or(false) && e["valide"].as_bool().unwrap_or(false))
            .filter_map(|e| Some((e["id"].as_str()?.to_string(), e["nom"].as_str().unwrap_or("").to_string())))
            .filter(|(id, nom)| !chargees.iter().any(|c| &c.id == id || (!nom.is_empty() && &c.nom == nom)))
            .collect();
        for (id, nom) in manquantes {
            info!("extensions Chrome : {id} absente du moteur, ajoutee");
            // Le moteur repond avec SON identifiant (l'empreinte du chemin pour une extension
            // sans cle) : on reconnait la reponse par l'identifiant du dossier ou par le nom.
            self.ajouts_silencieux.insert(id.clone());
            if !nom.is_empty() {
                self.ajouts_silencieux.insert(nom);
            }
            crate::moteur_ext::ajouter(barre, self.proxy.clone(), &self.extensions_chrome.join(&id), &id);
        }
        // Extensions deja chargees dont le service worker vient de recevoir l'adaptateur WebView2 :
        // Chromium garde l'ancien en cache, on les recharge une seule fois (meme dossier, donnees
        // gardees), en silence. Le temoin empeche de recommencer aux demarrages suivants.
        let dossiers = self.dossiers_extensions_chrome();
        for c in chargees {
            if !extensions_chrome::a_recharger(&dossiers, &c.id) {
                continue;
            }
            info!("extensions Chrome : {} rechargee une fois pour l'adaptateur WebView2", c.id);
            self.ajouts_silencieux.insert(c.id.clone());
            if !c.nom.is_empty() {
                self.ajouts_silencieux.insert(c.nom.clone());
            }
            crate::moteur_ext::ajouter(barre, self.proxy.clone(), &self.extensions_chrome.join(&c.id), &c.id);
            extensions_chrome::noter_rechargee(&dossiers, &c.id);
        }
    }

    /// Garde-fou de navigation : refuse les adresses internes de Chromium.
    ///
    /// `chrome-search://local-ntp/local-ntp.html` est la page d'accueil interne du moteur.
    /// Un code de page qui y navigue (`location`, ou un repli de popup d'extension) laissait la
    /// webview sur « ERR_INVALID_URL », sans retour possible — constate le 2026-11-21 sur la
    /// popup Bitwarden ouverte dans le panneau de droite. Refuser la navigation garde la page
    /// en place et rend le refus visible.
    fn garde_navigation(&self) -> impl Fn(String) -> bool + 'static {
        let proxy = self.proxy.clone();
        move |url: String| {
            if urls::interne_chromium(&url) {
                let _ = proxy.send_event(UserEvent::NavigationRefusee(url));
                false
            } else {
                true
            }
        }
    }

    /// Protection anti-pistage du profil de `webview` (la barre, donc le profil principal, si
    /// `None`). Chaque profil de cloison la recoit a la creation de sa premiere webview.
    fn appliquer_anti_pistage(&self, webview: Option<&WebView>) {
        // `TRAME_SANS_ANTI_PISTAGE` (diagnostic) : ne pas toucher au reglage du profil.
        if std::env::var("TRAME_SANS_ANTI_PISTAGE").is_ok() {
            info!("protection anti-pistage : non appliquee (sonde)");
            return;
        }
        let Some(w) = webview.or(self.barre.as_ref()) else { return };
        let niveau = self.reglages.anti_pistage.as_str();
        match crate::moteur_ext::niveau_pistage(w, niveau) {
            Ok(()) => info!("protection anti-pistage : {niveau}"),
            Err(e) => alerte!("protection anti-pistage : refusee ({e})"),
        }
    }

    /// Reporte les reglages de nettoyage dans les drapeaux partages avec les gardes.
    fn appliquer_nettoyage(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        self.nettoyage.0.store(self.reglages.nettoyer_adresses, Relaxed);
        self.nettoyage.1.store(self.reglages.garder_affiliation, Relaxed);
    }

    fn creer_surface(&mut self, nom: &str, source: Source) -> Option<WebView> {
        let racine = self.racine_ui();
        let geo = self.geometrie_courante();
        let rect = if nom == "rail" { geo.rail } else { geo.barre };
        let proxy = self.proxy.clone();
        let racine_handler = racine.clone();
        let url = format!("trame://localhost/chrome.html?surface={nom}");
        let garde = self.garde_navigation();
        let mut contexte = verrou_contexte();
        let resultat = self
            .habiller(WebViewBuilder::new_with_web_context(&mut contexte))
            .with_url(url)
            .with_bounds(rect)
            .with_background_color(COULEUR_FOND)
            .with_visible(true)
            .with_focused(false)
            .with_navigation_handler(garde)
            .with_initialization_script(AMORCE_UI)
            .with_custom_protocol("trame".to_string(), move |_id, req| {
                protocol::repondre(&racine_handler, &req.uri().to_string())
            })
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::Ipc {
                    source,
                    corps: req.body().clone(),
                });
            })
            .build_as_child(&self.window);
        match resultat {
            Ok(w) => {
                info!(
                    "surface {nom} creee, bounds demandes {:?} / reels {:?}",
                    rect,
                    w.bounds().ok()
                );
                Some(w)
            }
            Err(e) => {
                alerte!("surface {nom} : creation impossible ({e})");
                None
            }
        }
    }

    fn creer_vue_trame(&mut self) -> Option<WebView> {
        let racine_handler = self.racine_ui();
        let geo = self.geometrie_courante();
        let proxy = self.proxy.clone();
        let garde = self.garde_navigation();
        let mut contexte = verrou_contexte();
        let resultat = self
            .habiller(WebViewBuilder::new_with_web_context(&mut contexte))
            .with_url("trame://localhost/trame.html")
            .with_bounds(geo.contenu)
            .with_background_color(COULEUR_FOND)
            .with_visible(false)
            .with_focused(false)
            .with_navigation_handler(garde)
            .with_custom_protocol("trame".to_string(), move |_id, req| {
                protocol::repondre(&racine_handler, &req.uri().to_string())
            })
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::Ipc {
                    source: Source::Vue,
                    corps: req.body().clone(),
                });
            })
            .build_as_child(&self.window);
        match resultat {
            Ok(w) => Some(w),
            Err(e) => {
                alerte!("vue trame : creation impossible ({e})");
                None
            }
        }
    }

    fn creer_onglet_webview(&mut self, id: TabId) -> Option<WebView> {
        let onglet = self.trouver(id)?.clone();
        let geo = self.geometrie_courante();
        let racine = self.racine_ui();
        let visible = self.actif == Some(id) && !self.vue_trame;
        let url = if onglet.url.is_empty() {
            urls::NOUVEL_ONGLET.to_string()
        } else {
            self.adresse_propre(onglet.url.clone())
        };

        let racine_handler = racine.clone();
        let p_titre = self.proxy.clone();
        let p_charge = self.proxy.clone();
        let p_ipc = self.proxy.clone();
        let p_fenetre = self.proxy.clone();

        let garde = self.garde_navigation();
        // Un fil cloisonne a son propre profil WebView2 : connexions et extensions a part.
        let cloison = self.cloison_du_fil(onglet.fil);
        let mut contexte = verrou_contexte();
        let builder = self.habiller(WebViewBuilder::new_with_web_context(&mut contexte));
        let builder = match &cloison {
            Some(c) => {
                use wry::WebViewBuilderExtWindows;
                builder.with_profile_name(c.profil.clone())
            }
            None => builder,
        };
        // Une page web n'est PAS donnee a la creation : l'onglet nait vide (`about:blank`), est
        // entierement mis en place (style, empilement, interception, extensions de cloison), et
        // la page n'est chargee qu'ensuite (`reveiller`). Constate sur le profil reel (Amazon le
        // 2026-11-21, YouTube le 2026-11-22) : une page chargee pendant la creation restait
        // inerte, et un rechargement (F5) la reparait toujours — le rechargement, lui, se fait
        // dans un onglet deja etabli. C'est l'ordre que l'on reproduit ici.
        let web = url.starts_with("http://") || url.starts_with("https://");
        if web {
            self.navigation_differee.insert(id, url.clone());
        }
        let builder = builder
            .with_url(if web { "about:blank".to_string() } else { url })
            .with_bounds(geo.contenu)
            .with_visible(visible)
            .with_focused(false)
            .with_background_color(COULEUR_FOND)
            .with_devtools(true)
            .with_navigation_handler(garde);
        // Sondes de diagnostic (comparaison) : `TRAME_SANS_AMORCE` n'injecte pas les scripts de
        // Trame ; `TRAME_SANS_PROTOCOLE` n'installe pas le protocole interne sur l'onglet.
        let builder = if std::env::var("TRAME_SANS_AMORCE").is_ok() {
            builder
        } else {
            builder.with_initialization_script(format!(
                "{}\n{}\n{}",
                extensions::AMORCE,
                RACCOURCIS,
                extensions::script_de_debut(&self.extensions)
            ))
            // Aussi dans les cadres : un lecteur YouTube integre a un autre site doit avoir sa
            // pastille (`with_initialization_script` ne vise que la page principale).
            .with_initialization_script_for_main_only(INCRUSTATION, false)
        };
        let builder = if std::env::var("TRAME_SANS_PROTOCOLE").is_ok() {
            builder
        } else {
            builder.with_custom_protocol("trame".to_string(), move |_id, req| {
                protocol::repondre(&racine_handler, &req.uri().to_string())
            })
        };
        let resultat = builder
            .with_document_title_changed_handler(move |titre| {
                let _ = p_titre.send_event(UserEvent::Titre { onglet: id, titre });
            })
            .with_on_page_load_handler(move |evenement, url| {
                let termine = matches!(evenement, wry::PageLoadEvent::Finished);
                let _ = p_charge.send_event(UserEvent::Chargement {
                    onglet: id,
                    termine,
                    url,
                });
            })
            .with_new_window_req_handler(move |url, f| {
                if url.starts_with("chrome-extension:") {
                    let fenetre = fenetre_extension(&url, f.size.is_some());
                    let _ = p_fenetre.send_event(UserEvent::Log(format!(
                        "nouvelle fenetre d'extension (taille {:?}) -> {} : {url}",
                        f.size,
                        if fenetre { "fenetre" } else { "onglet" }
                    )));
                    if fenetre {
                        return NewWindowResponse::Allow;
                    }
                }
                let _ = p_fenetre.send_event(UserEvent::NouvelleFenetre { onglet: id, url });
                NewWindowResponse::Deny
            })
            // Telechargements : branches apres la creation (`telechargements::brancher`),
            // directement sur le moteur, pour la progression, la pause et l'annulation.
            .with_ipc_handler(move |req| {
                let _ = p_ipc.send_event(UserEvent::Ipc {
                    source: Source::Onglet(id),
                    corps: req.body().clone(),
                });
            })
            .build_as_child(&self.window);

        match resultat {
            Ok(w) => {
                // Un onglet reveille doit retrouver le zoom qu'il avait : sinon le confort de
                // lecture se perd a chaque mise en veille.
                if let Some(facteur) = self.zooms.get(&id) {
                    let _ = w.zoom(*facteur);
                }
                // De meme pour le son coupe : un onglet muet le reste apres la veille.
                if self.muets.contains(&id) {
                    crate::moteur_ext::regler_muet(&w, true);
                }
                let site = self.trouver(id).map(|o| o.domaine()).unwrap_or_default();
                if let Err(e) = crate::moteur_ext::suivre_plein_ecran(&w, self.proxy.clone(), id) {
                    alerte!("onglet {id} : suivi du plein ecran impossible ({e})");
                }
                if let Err(e) = crate::telechargements::brancher(&w, self.proxy.clone(), site, self.dossier_dl.clone()) {
                    alerte!("onglet {id} : suivi des telechargements impossible ({e})");
                }
                // Adresses nettoyees au niveau reseau (liens suivis par la page, redirections).
                // `TRAME_SANS_INTERCEPTION` (diagnostic) : ne pas l'installer, pour comparer.
                let interception = std::env::var("TRAME_SANS_INTERCEPTION").is_err();
                if !interception {
                    info!("onglet {id} : interception reseau desactivee (sonde)");
                } else if let Err(e) = crate::moteur_ext::intercepter_pistage(
                    &w,
                    self.nettoyage.clone(),
                    self.blocage.avec_filtres(extensions::filtres_bloquer(&self.extensions)),
                    self.proxy.clone(),
                    id,
                ) {
                    alerte!("onglet {id} : interception du pistage impossible ({e})");
                }
                if let Some(c) = &cloison {
                    drop(contexte);
                    self.aligner_cloison_sur(&w, c);
                    self.appliquer_anti_pistage(Some(&w));
                }
                Some(w)
            }
            Err(e) => {
                alerte!("onglet {id} : webview impossible ({e})");
                None
            }
        }
    }

    fn restaurer_maintenant(&mut self) {
        self.restaure = true;
        self.restaurer();
        self.pousser_etat();
    }

    fn restaurer(&mut self) {
        // Une page `chrome-extension://` ne doit pas etre l'onglet actif au demarrage :
        // l'installation des extensions est asynchrone, et la page se chargerait avant
        // qu'elles existent (ERR_INVALID_URL). Elle se rouvre tres bien plus tard.
        let est_extension = |o: &Onglet| o.url.starts_with("chrome-extension:");
        let cible = self
            .session
            .actif
            .filter(|id| {
                self.trouver(*id)
                    .map(|o| !est_extension(o) && !urls::interne_chromium(&o.url))
                    .unwrap_or(false)
            })
            .or_else(|| {
                self.session
                    .onglets
                    .iter()
                    .find(|o| !est_extension(o) && !urls::interne_chromium(&o.url))
                    .map(|o| o.id)
            })
            .or_else(|| self.session.onglets.first().map(|o| o.id));
        // Une adresse interne gardee en session (elle a pu y entrer avant ce garde-fou) est
        // remplacee par l'accueil : sinon l'onglet se restaure sur une page morte.
        let mut remplacees = 0usize;
        for o in self.session.onglets.iter_mut() {
            if urls::interne_chromium(&o.url) {
                info!("onglet {} restaure sur l'accueil (adresse interne {})", o.id, o.url);
                o.url = urls::NOUVEL_ONGLET.to_string();
                o.titre = String::new();
                remplacees += 1;
            }
        }
        if remplacees > 0 {
            self.marquer_modifie();
        }
        if let Some(id) = cible {
            self.activer(id);
        }
        // Les titres viennent de la session : l'evenement de titre ne changera rien, donc
        // les noms automatiques doivent etre recalcules ici (sinon un fil garde le nom
        // qu'il avait a sa creation, par exemple « Localhost »).
        self.renommer_fils_auto();
        self.pousser_etat();
    }

    // ---------------------------------------------------------------- geometrie

    pub fn geometrie_courante(&self) -> Geometrie {
        let taille = self.window.inner_size();
        // Style « bandeau » : la barre commence par la bande de la fenetre (28 px).
        let base = self.hauteur_titre()
            + if self.reglages.barre_extensions {
                shell::BARRE_H + shell::EXT_H
            } else {
                shell::BARRE_H
            };
        let barre = if self.panneau_ouvert { self.panneau_hauteur.max(base) } else { base };
        let echelle = self.window.scale_factor();
        let mut geo = shell::geometrie(
            taille.width,
            taille.height,
            echelle,
            self.rail_replie,
            barre,
            base,
            if self.zap_ouvert { shell::ZAP_LARGEUR } else { 0.0 },
        );
        // Menu en colonne (page, extensions, commandes) : la region ne decoupe que la colonne,
        // la page reste visible autour. Le plafond de 72 % (fait pour la Couture, pleine
        // largeur) coupait le bas du menu d'un onglet sans qu'on puisse le faire defiler
        // (retour de Clement, 2026-11-22) : ici, la barre peut aller jusqu'au bas de la fenetre.
        if self.panneau_ouvert && self.panneau_zone.is_some() {
            let voulu = (barre * echelle).round();
            let (_, _, _, h_barre) = rect_physique(geo.barre);
            if voulu > h_barre as f64 {
                let (bx, by, bl, _) = rect_physique(geo.barre);
                let h = (voulu as i32).min(taille.height as i32);
                geo.barre = wry::Rect {
                    position: tao::dpi::PhysicalPosition::new(bx, by).into(),
                    size: tao::dpi::PhysicalSize::new(bl.max(1) as u32, h.max(1) as u32).into(),
                };
            }
        }
        // Fenetre d'extension : a la taille de son contenu, accrochee sous la barre a droite
        // (Chrome l'accroche sous le bouton ; ici le bouton est a droite lui aussi).
        if let (true, Some(_), Some((l, h))) =
            (self.zap_ouvert, self.panneau_extension.as_ref(), self.popup_taille)
        {
            let (cx, cy, cl, ch) = rect_physique(geo.contenu);
            let marge = (8.0 * echelle).round() as i32;
            let l = ((l * echelle).round() as i32).clamp(1, (cl - 2 * marge).max(1));
            let h = ((h * echelle).round() as i32).clamp(1, (ch - marge).max(1));
            let x = (cx + cl - l - marge).max(cx);
            geo.zap = wry::Rect {
                position: tao::dpi::PhysicalPosition::new(x, cy).into(),
                size: tao::dpi::PhysicalSize::new(l as u32, h as u32).into(),
            };
        }
        geo
    }

    /// Une video demande (ou quitte) le plein ecran.
    fn sur_plein_ecran(&mut self, onglet: TabId, actif: bool) {
        if actif {
            let mode = self.reglages.plein_ecran.clone();
            if mode == "page" || Some(onglet) != self.actif || self.plein_ecran.is_some() {
                return;
            }
            self.plein_ecran = Some(onglet);
            self.fenetre_basculee = mode == "ecran" && self.window.fullscreen().is_none();
            if self.fenetre_basculee {
                self.window.set_fullscreen(Some(tao::window::Fullscreen::Borderless(None)));
            }
            info!("onglet {onglet} : plein ecran ({mode})");
        } else {
            if self.plein_ecran != Some(onglet) {
                return;
            }
            self.quitter_plein_ecran();
        }
        self.appliquer_geometrie();
    }

    /// Rend l'interface (fin du plein ecran, ou l'onglet n'est plus celui affiche).
    fn quitter_plein_ecran(&mut self) {
        if self.plein_ecran.take().is_none() {
            return;
        }
        if std::mem::take(&mut self.fenetre_basculee) {
            self.window.set_fullscreen(None);
        }
        for w in self.rail.iter().chain(self.barre.iter()) {
            let _ = w.set_visible(true);
        }
    }

    pub fn appliquer_geometrie(&mut self) {
        // Plein ecran d'une video : l'onglet couvre toute la fenetre, le reste est cache.
        if let Some(id) = self.plein_ecran {
            if Some(id) != self.actif || !self.webviews.contains_key(&id) {
                self.quitter_plein_ecran();
            } else {
                let taille = self.window.inner_size();
                let tout = wry::Rect {
                    position: tao::dpi::PhysicalPosition::new(0, 0).into(),
                    size: tao::dpi::PhysicalSize::new(taille.width.max(1), taille.height.max(1)).into(),
                };
                for w in self.rail.iter().chain(self.barre.iter()).chain(self.zap.iter()).chain(self.vue.iter()) {
                    let _ = w.set_visible(false);
                }
                for (autre, w) in self.webviews.iter() {
                    if *autre == id {
                        let _ = w.set_bounds(tout);
                        let _ = w.set_visible(true);
                        crate::fenetres::au_premier_plan(w);
                    } else {
                        let _ = w.set_visible(false);
                    }
                }
                return;
            }
        }
        let geo = self.geometrie_courante();
        if let Some(w) = &self.rail {
            let _ = w.set_bounds(geo.rail);
        }
        if let Some(w) = &self.barre {
            let _ = w.set_bounds(geo.barre);
        }
        if let Some(w) = &self.vue {
            let _ = w.set_bounds(geo.contenu);
        }
        if let Some(w) = &self.zap {
            let _ = w.set_bounds(geo.zap);
            let _ = w.set_visible(self.zap_ouvert);
        }
        let actif = self.actif;
        let vue = self.vue_trame;
        let cote = self.rects_cote(&geo);
        for (id, w) in self.webviews.iter() {
            match cote {
                Some((g, d, gauche, droite, _)) if *id == g || *id == d => {
                    let _ = w.set_bounds(if *id == g { gauche } else { droite });
                    let _ = w.set_visible(true);
                }
                _ => {
                    let _ = w.set_bounds(geo.contenu);
                    let _ = w.set_visible(Some(*id) == actif && !vue);
                }
            }
        }
        self.superposer(&geo);
    }

    /// Le menu des extensions, la Couture et la fenetre d'extension se posent PAR-DESSUS la
    /// page : toutes les webviews se clippent entre soeurs, le panneau de droite puis la barre
    /// passent au premier plan, et la barre agrandie est decoupee a la ligne du haut plus la
    /// colonne du menu (coins arrondis). Voir `fenetres.rs`.
    fn superposer(&self, geo: &Geometrie) {
        let toutes = self
            .webviews
            .values()
            .chain(self.vue.iter())
            .chain(self.zap.iter())
            .chain(self.barre.iter())
            .chain(self.rail.iter());
        for w in toutes {
            crate::fenetres::clip_soeurs(w);
        }
        let echelle = self.window.scale_factor();
        let reduite = self.window.is_minimized();
        // Coup d'oeil : au-dessus de la page, sous la fenetre d'extension et la barre ; coins
        // arrondis (region) et ombre.
        match (&self.apercu, &self.ombre_apercu) {
            (Some(w), ombre) if !reduite => {
                let (cx, cy, cl, ch) = rect_physique(geo.contenu);
                let (l, h) = (((cl as f64) * 0.84) as i32, ((ch as f64) * 0.88) as i32);
                let (x, y) = (cx + (cl - l) / 2, cy + (ch - h) / 2);
                let _ = w.set_bounds(wry::Rect {
                    position: tao::dpi::PhysicalPosition::new(x, y).into(),
                    size: tao::dpi::PhysicalSize::new(l.max(1) as u32, h.max(1) as u32).into(),
                });
                crate::fenetres::clip_soeurs(w);
                crate::fenetres::au_premier_plan(w);
                let rayon = (14.0 * echelle).round() as i32;
                crate::fenetres::region(w, Some((&[], &[(0, 0, l, h)], rayon)));
                if let Some(o) = ombre {
                    o.montrer(crate::ombre::Forme { x, y, largeur: l, hauteur: h, rayon }, echelle);
                }
            }
            (_, Some(o)) => o.cacher(),
            _ => {}
        }
        if self.zap_ouvert {
            if let Some(w) = &self.zap {
                crate::fenetres::au_premier_plan(w);
            }
        }
        // Ombre de la fenetre d'extension (pas du panneau du zappeur, colle au bord).
        if let Some(o) = &self.ombre_popup {
            if self.zap_ouvert && self.panneau_extension.is_some() && !reduite {
                let (x, y, l, h) = rect_physique(geo.zap);
                o.montrer(crate::ombre::Forme { x, y, largeur: l, hauteur: h, rayon: 0 }, echelle);
            } else {
                o.cacher();
            }
        }
        let Some(barre) = &self.barre else { return };
        crate::fenetres::au_premier_plan(barre);
        if !self.panneau_ouvert || reduite {
            crate::fenetres::region(barre, None);
            if let Some(o) = &self.ombre_menu {
                o.cacher();
            }
            return;
        }
        let (bx, by, bl, bh) = rect_physique(geo.barre);
        let (_, cy, _, _) = rect_physique(geo.contenu);
        let base = (cy - by).max(1);
        let ligne = [(0, 0, bl, base)];
        let dessous = (bh - base).max(1);
        match self.panneau_zone {
            Some((x, l, haut)) => {
                // Meme precaution que dans `shell::geometrie` : sur une fenetre minuscule, les
                // bornes hautes de ces `clamp` tombent sous leurs bornes basses (mesure du
                // 2026-11-22, fenetre reduite).
                let x = ((x * echelle).round() as i32).clamp(0, (bl - 1).max(0));
                let l = ((l * echelle).round() as i32).clamp(1, (bl - x).max(1));
                let rayon = (12.0 * echelle).round() as i32;
                let haut = (haut * echelle).round() as i32;
                // Menu pose plus bas (clic droit sur un onglet) : une carte detachee, quatre
                // coins arrondis. Sinon la colonne remonte sous la ligne du haut pour que ses
                // coins hauts arrondis n'y creusent pas d'encoche : seuls les bas se voient.
                let (colonne, ombre_y) = if haut > base && haut < bh - 1 {
                    ((x, haut, l, bh - haut), haut)
                } else {
                    ((x, base - rayon, l, dessous + rayon), base)
                };
                crate::fenetres::region(barre, Some((&ligne, &[colonne], rayon)));
                if let Some(o) = &self.ombre_menu {
                    o.montrer(
                        crate::ombre::Forme { x: bx + x, y: by + ombre_y, largeur: l, hauteur: bh - ombre_y, rayon },
                        echelle,
                    );
                }
            }
            None => {
                let tout = [(0, base, bl, dessous)];
                crate::fenetres::region(barre, Some((&ligne, &tout, 0)));
                if let Some(o) = &self.ombre_menu {
                    o.montrer(
                        crate::ombre::Forme { x: bx, y: by + base, largeur: bl, hauteur: dessous, rayon: 0 },
                        echelle,
                    );
                }
            }
        }
        let _ = bx;
    }

    /// La fenetre a bouge : les ombres (fenetres a part) la suivent.
    pub fn deplacee(&mut self) {
        let geo = self.geometrie_courante();
        self.superposer(&geo);
    }

    pub fn redimensionner(&mut self) {
        // Fenetre reduite dans la barre des taches : Windows ne lui laisse qu'une taille
        // symbolique. Rien a disposer tant qu'elle est la, et sa geometrie ne veut plus rien
        // dire ; la restauration renvoie un `Resized` avec la vraie taille.
        if self.window.is_minimized() {
            return;
        }
        let taille = self.window.inner_size();
        self.derniere_taille = (taille.width, taille.height);
        self.appliquer_geometrie();
        self.suivre_agrandissement();
    }

    /// La fenetre vient de reprendre le focus (alt-tab, clic sur la barre de titre) : on le
    /// redonne a la webview active, sinon les raccourcis clavier ne partent plus vers la page
    /// tant que l'utilisateur n'a pas reclique dedans.
    pub fn refocaliser(&mut self) {
        // De retour des Parametres de Windows ? L'etat « navigateur par defaut » a pu changer.
        if self.par_defaut.is_some() {
            self.verifier_par_defaut();
        }
        if self.vue_trame {
            if let Some(vue) = &self.vue {
                let _ = vue.focus();
            }
            return;
        }
        if let Some(id) = self.actif {
            if let Some(w) = self.webviews.get(&id) {
                let _ = w.focus();
            }
        }
    }

    // ---------------------------------------------------------------- acces

    fn trouver(&self, id: TabId) -> Option<&Onglet> {
        self.session.onglets.iter().find(|o| o.id == id)
    }

    fn onglet_mut(&mut self, id: TabId) -> Option<&mut Onglet> {
        self.session.onglets.iter_mut().find(|o| o.id == id)
    }

    fn fil_mut(&mut self, id: ThreadId) -> Option<&mut Fil> {
        self.session.fils.iter_mut().find(|f| f.id == id)
    }

    fn prochain_id_onglet(&mut self) -> TabId {
        let id = self.session.prochain_onglet.max(1);
        self.session.prochain_onglet = id + 1;
        id
    }

    fn prochain_id_fil(&mut self) -> ThreadId {
        let id = self.session.prochain_fil.max(1);
        self.session.prochain_fil = id + 1;
        id
    }

    // ---------------------------------------------------------------- onglets

    pub fn ouvrir_onglet(
        &mut self,
        url: String,
        declencheur: Declencheur,
        parent: Option<TabId>,
    ) -> TabId {
        let url = self.adresse_propre(url);
        // Une adresse interne de Chromium ne mene nulle part ici : WebView2 la refuse et la
        // webview reste bloquee sur « ERR_INVALID_URL ». Mieux vaut un onglet neuf et une
        // explication qu'une page morte dont on ne sait pas sortir.
        let url = if urls::interne_chromium(&url) {
            info!("adresse interne refusee : {url}");
            self.signaler(&format!(
                "« {} » est une adresse interne de Chromium : Trame ne peut pas la servir.",
                urls::affichage_court(&url)
            ));
            urls::NOUVEL_ONGLET.to_string()
        } else {
            url
        };
        let now = maintenant_ms();
        let id = self.prochain_id_onglet();
        let mut onglet = Onglet::neuf(id, url, 0, declencheur, now);
        onglet.parent = parent;

        // Tissage : on cherche un fil d'accueil.
        let candidat = weave::evaluer(&onglet, &self.session.onglets, &self.session.fils, now);
        let fil = match candidat {
            Some((fil, score)) if score >= weave::SEUIL_TISSAGE => fil,
            _ => {
                let nom = weave::nommer(&[], &self.session.onglets);
                let id_fil = self.prochain_id_fil();
                let nom = if nom.trim().is_empty() || nom == "Fil" {
                    format!("Fil {id_fil}")
                } else {
                    nom
                };
                self.session.fils.push(Fil::neuf(id_fil, nom, now));
                id_fil
            }
        };
        onglet.fil = fil;
        if let Some(f) = self.fil_mut(fil) {
            f.actif_le = now;
        }
        self.session.onglets.push(onglet);
        self.renommer_fils_auto();
        self.actif = Some(id);
        self.vue_trame = false;
        if let Some(v) = &self.vue {
            let _ = v.set_visible(false);
        }
        self.reveiller(id);
        // Un onglet ouvert depuis la Couture doit MASQUER les autres : sans cela, l'ancienne
        // page reste visible sous la nouvelle, et l'onglet actif semble ne pas s'ouvrir.
        for (tid, w) in self.webviews.iter() {
            let _ = w.set_visible(*tid == id && !self.vue_trame);
        }
        if let Some(w) = self.webviews.get(&id) {
            let _ = w.focus();
        }
        self.marquer_modifie();
        self.pousser_etat();
        id
    }

    /// L'adresse telle que Trame doit la demander : sans ses parametres de pistage si le
    /// reglage est actif. A appliquer **avant** toute navigation lancee par Trame : la garde de
    /// navigation arrive trop tard pour elles — mesure du 2026-11-22, l'adresse sale atteignait
    /// le serveur avant d'etre refusee (scenario `w`).
    fn adresse_propre(&self, url: String) -> String {
        if !self.reglages.nettoyer_adresses {
            return url;
        }
        match urls::nettoyer(&url, self.reglages.garder_affiliation) {
            Some(propre) => {
                info!("adresse nettoyee a la source ({} caracteres de pistage retires)", url.len().saturating_sub(propre.len()));
                propre
            }
            None => url,
        }
    }

    fn naviguer_actif(&mut self, saisie: &str) {
        let url = urls::normaliser_avec(saisie, &self.reglages.moteur);
        let url = self.adresse_propre(url);
        let Some(id) = self.actif else {
            self.ouvrir_onglet(url, Declencheur::Saisie, None);
            return;
        };
        if let Some(o) = self.onglet_mut(id) {
            o.url = url.clone();
            o.titre = urls::nom_page_interne(&url).unwrap_or("").to_string();
            o.chargement = true;
            o.defilement = 0.0;
        }
        if !self.webviews.contains_key(&id) {
            self.reveiller(id);
        }
        if let Some(w) = self.webviews.get(&id) {
            if let Err(e) = charger(w, &url) {
                alerte!("navigation vers {url} impossible : {e}");
            }
            let _ = w.set_visible(!self.vue_trame);
        }
        self.defilement_a_restaurer.remove(&id);
        self.marquer_modifie();
        self.pousser_etat();
    }

    fn activer(&mut self, id: TabId) {
        if self.trouver(id).is_none() {
            return;
        }
        // Activer l'onglet en mini-fenetre le ramene dans la fenetre principale.
        if self.onglet_flottant() == Some(id) {
            self.ramener_flottant();
        }
        // Changer d'onglet referme le coup d'oeil : il appartient a la page qu'on quitte.
        if self.actif != Some(id) {
            self.fermer_apercu();
        }
        if self.actif != Some(id) {
            // Temps passe sur l'onglet precedent.
            let now = maintenant_ms();
            if let Some(ancien) = self.actif {
                if let Some(o) = self.onglet_mut(ancien) {
                    o.temps_actif_ms = o
                        .temps_actif_ms
                        .saturating_add(now.saturating_sub(o.dernier_focus));
                    o.dernier_focus = now;
                }
            }
        }
        self.actif = Some(id);
        let now = maintenant_ms();
        if let Some(o) = self.onglet_mut(id) {
            vitality::sur_focus(o, now);
        }
        let fil = self.trouver(id).map(|o| o.fil);
        if let Some(fil) = fil {
            if let Some(f) = self.fil_mut(fil) {
                f.actif_le = now;
            }
        }
        self.vue_trame = false;
        if let Some(v) = &self.vue {
            let _ = v.set_visible(false);
        }
        if self.panneau_ouvert {
            self.panneau_ouvert = false;
            self.appliquer_geometrie();
        }
        if let Some(o) = self.onglet_mut(id) {
            o.rendez_vous = None;
            o.revenu = false;
        }
        if !self.webviews.contains_key(&id) {
            self.reveiller(id);
        }
        for (tid, w) in self.webviews.iter() {
            let _ = w.set_visible(*tid == id);
        }
        if self.session.cote.is_some() {
            self.preparer_cote();
            self.appliquer_geometrie();
        }
        // L'onglet montre repasse en memoire normale tout de suite, pas au battement suivant.
        self.ajuster_memoire(maintenant_ms());
        if let Some(w) = self.webviews.get(&id) {
            let _ = w.focus();
        }
        if let Some(o) = self.trouver(id) {
            let titre = o.libelle();
            self.window.set_title(&format!("{titre} — Trame"));
        }
        self.marquer_modifie();
        self.pousser_etat();
    }

    /// Liens recus de l'exterieur (Trame navigateur par defaut) : chacun s'ouvre dans un onglet,
    /// et la fenetre revient au premier plan. Une demande vide = simplement revenir devant.
    fn relever_liens_externes(&mut self) {
        let demandes = crate::externe::relever(self.profil.racine());
        if demandes.is_empty() {
            return;
        }
        for adresse in demandes {
            if adresse.is_empty() {
                info!("lien externe : retour au premier plan");
                continue;
            }
            info!("lien externe : {}", urls::affichage_court(&adresse));
            let id = self.ouvrir_onglet(adresse, Declencheur::NouvelOnglet, None);
            self.activer(id);
        }
        self.window.set_minimized(false);
        self.window.set_focus();
        self.pousser_etat();
    }

    /// Verifie en arriere-plan si Trame est le navigateur par defaut (lecture du registre).
    fn verifier_par_defaut(&self) {
        // Une lecture toutes les 5 s au plus : `refocaliser` l'appelle a chaque retour du focus,
        // et une rafale de retours (fenetre qui clignote) ne doit pas lancer une rafale de `reg`.
        static DERNIERE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let maintenant = maintenant_ms();
        let avant = DERNIERE.load(std::sync::atomic::Ordering::Relaxed);
        if maintenant.saturating_sub(avant) < 5_000 {
            return;
        }
        DERNIERE.store(maintenant, std::sync::atomic::Ordering::Relaxed);
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let oui = crate::externe::est_par_defaut();
            let _ = proxy.send_event(UserEvent::ParDefaut(oui));
        });
    }

    /// Filet de securite : un onglet rouvert, affiche, dont le premier chargement n'a toujours
    /// pas abouti apres `SECOURS_CHARGEMENT_MS` est recharge **une fois**.
    ///
    /// Constate le 2026-11-21 sur le profil reel : l'onglet Amazon, recree a la fermeture d'un
    /// voisin, est reste noir — aucun chargement n'a abouti (rien a l'historique) jusqu'a ce que
    /// l'utilisateur recharge a la main. La cause n'est pas reproduite ; ce filet fait ce qu'il a
    /// fait, et le journal dira la prochaine fois si le chargement avait commence.
    fn secours_chargement(&mut self, now: u64) {
        let Some(actif) = self.actif else { return };
        if self.vue_trame || self.secours_faits.contains(&actif) {
            return;
        }
        let Some(debut) = self.premier_chargement.get(&actif).copied() else { return };
        // Une page lente qui charge n'est pas une page bloquee : on lui laisse 30 s ; une page
        // dont le chargement n'a meme pas commence, 12 s.
        let commence = self.trouver(actif).map(|o| o.chargement).unwrap_or(false);
        let delai = if commence { SECOURS_CHARGEMENT_LENT_MS } else { SECOURS_CHARGEMENT_MS };
        if now.saturating_sub(debut) < delai {
            return;
        }
        let web = self
            .trouver(actif)
            .map(|o| o.url.starts_with("http://") || o.url.starts_with("https://"))
            .unwrap_or(false);
        if !web {
            return;
        }
        alerte!(
            "onglet {actif} : premier chargement sans fin apres {} s ({}) — rechargement de secours",
            delai / 1000,
            if commence { "commence, jamais termine" } else { "jamais commence" }
        );
        self.secours_faits.insert(actif);
        self.premier_chargement.insert(actif, now);
        if let Some(w) = self.webviews.get(&actif) {
            let _ = w.set_visible(true);
            let _ = w.reload();
        }
    }

    /// Coup d'oeil : ouvre `url` dans un apercu flottant, par-dessus la page, dans le profil du
    /// fil affiche (cloison comprise). Un second coup d'oeil remplace le premier.
    fn ouvrir_apercu(&mut self, url: &str) {
        let url = urls::nettoyer(url, self.reglages.garder_affiliation)
            .filter(|_| self.reglages.nettoyer_adresses)
            .unwrap_or_else(|| url.to_string());
        if let Some(w) = &self.apercu {
            let _ = charger(w, &url);
            self.apercu_url = url;
            return;
        }
        let fil = self.actif.and_then(|a| self.trouver(a)).map(|o| o.fil);
        let cloison = fil.and_then(|f| self.cloison_du_fil(f));
        let geo = self.geometrie_courante();
        let proxy = self.proxy.clone();
        let p_charge = self.proxy.clone();
        let p_fenetre = self.proxy.clone();
        let garde = self.garde_navigation();
        let mut contexte = verrou_contexte();
        let builder = self.habiller(WebViewBuilder::new_with_web_context(&mut contexte));
        let builder = match &cloison {
            Some(c) => {
                use wry::WebViewBuilderExtWindows;
                builder.with_profile_name(c.profil.clone())
            }
            None => builder,
        };
        let resultat = builder
            .with_url(&url)
            .with_bounds(geo.contenu)
            .with_visible(true)
            .with_focused(true)
            .with_background_color(COULEUR_FOND)
            .with_navigation_handler(garde)
            .with_initialization_script(APERCU)
            .with_on_page_load_handler(move |_, url| {
                let _ = p_charge.send_event(UserEvent::ApercuAdresse(url));
            })
            .with_new_window_req_handler(move |url, f| {
                if fenetre_extension(&url, f.size.is_some()) {
                    return NewWindowResponse::Allow;
                }
                let _ = p_fenetre.send_event(UserEvent::NouvelleFenetre { onglet: 0, url });
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::Ipc {
                    source: Source::Apercu,
                    corps: req.body().clone(),
                });
            })
            .build_as_child(&self.window);
        drop(contexte);
        match resultat {
            Ok(w) => {
                info!("coup d'oeil : {}", urls::affichage_court(&url));
                let _ = crate::moteur_ext::intercepter_pistage(
                    &w,
                    self.nettoyage.clone(),
                    self.blocage.avec_filtres(extensions::filtres_bloquer(&self.extensions)),
                    self.proxy.clone(),
                    0,
                );
                self.apercu = Some(w);
                self.apercu_url = url;
                if self.panneau_ouvert {
                    self.panneau_ouvert = false;
                }
                self.appliquer_geometrie();
                self.pousser_etat();
            }
            Err(e) => alerte!("coup d'oeil impossible ({e})"),
        }
    }

    fn fermer_apercu(&mut self) {
        if self.apercu.take().is_some() {
            info!("coup d'oeil ferme");
            self.apercu_url.clear();
            if let Some(o) = &self.ombre_apercu {
                o.cacher();
            }
            if let Some(id) = self.actif {
                if let Some(w) = self.webviews.get(&id) {
                    let _ = w.focus();
                }
            }
            self.pousser_etat();
        }
    }

    /// Le palier entre « ouvert » et « en veille » : un onglet reveille mais cache depuis
    /// `MEMOIRE_BASSE_APRES_MS` passe en memoire basse (WebView2 rend caches et tas) ; il garde
    /// sa page, sa position, ses scripts. L'onglet visible est toujours en niveau normal. Fenetre
    /// reduite : tous les onglets sont caches, donc tous y passent apres le meme delai.
    fn ajuster_memoire(&mut self, now: u64) {
        let actif_ok = self.reglages.memoire_basse;
        let reduite = self.window.is_minimized();
        let ids: Vec<TabId> = self.webviews.keys().copied().collect();
        for id in ids {
            let visible = !reduite && !self.vue_trame && (Some(id) == self.actif || self.dans_cote_affiche(id));
            // L'instant ou l'onglet a ete cache est note dans tous les cas : la pause s'en sert
            // aussi, meme quand la memoire basse est coupee.
            let cache_depuis = if visible {
                now
            } else {
                *self.cache_depuis.entry(id).or_insert(now)
            };
            let voulu_bas = actif_ok && !visible && now.saturating_sub(cache_depuis) >= MEMOIRE_BASSE_APRES_MS;
            if visible {
                self.cache_depuis.remove(&id);
            }
            let est_bas = self.en_memoire_basse.contains(&id);
            if voulu_bas == est_bas {
                continue;
            }
            let Some(w) = self.webviews.get(&id) else { continue };
            match crate::moteur_ext::niveau_memoire(w, voulu_bas) {
                Ok(()) => {
                    if voulu_bas {
                        self.en_memoire_basse.insert(id);
                        info!("onglet {id} : memoire basse (cache depuis {} s)", MEMOIRE_BASSE_APRES_MS / 1000);
                    } else {
                        self.en_memoire_basse.remove(&id);
                        info!("onglet {id} : memoire normale");
                    }
                }
                Err(e) => {
                    alerte!("onglet {id} : niveau de memoire refuse ({e})");
                    // On ne reessaie pas a chaque battement : on le note comme fait.
                    if voulu_bas {
                        self.en_memoire_basse.insert(id);
                    } else {
                        self.en_memoire_basse.remove(&id);
                    }
                }
            }
        }
        // Pause : un onglet cache depuis longtemps arrete ses scripts et ses minuteries (CPU ~ 0),
        // sauf s'il joue du son, s'il charge, ou si c'est une page interne de Trame. L'onglet
        // affiche est repris avant tout.
        let ids: Vec<TabId> = self.webviews.keys().copied().collect();
        for id in ids {
            let visible = !reduite && !self.vue_trame && (Some(id) == self.actif || self.dans_cote_affiche(id));
            if visible {
                if self.en_pause.remove(&id) | self.pause_demandee.remove(&id) {
                    if let Some(w) = self.webviews.get(&id) {
                        crate::moteur_ext::reprendre(w);
                    }
                    info!("onglet {id} : repris");
                }
                continue;
            }
            if !self.reglages.pause_onglets || self.pause_demandee.contains(&id) {
                continue;
            }
            let cache = now.saturating_sub(self.cache_depuis.get(&id).copied().unwrap_or(now));
            if cache < pause_apres_ms() {
                continue;
            }
            let Some(o) = self.trouver(id) else { continue };
            if o.chargement || o.interne() {
                continue;
            }
            let Some(w) = self.webviews.get(&id) else { continue };
            if crate::moteur_ext::joue_du_son(w) {
                continue;
            }
            self.pause_demandee.insert(id);
            crate::moteur_ext::suspendre(w, self.proxy.clone(), id);
        }

        // Onglets endormis ou fermes : leur webview n'existe plus, on oublie leur etat.
        let vivantes = &self.webviews;
        self.cache_depuis.retain(|id, _| vivantes.contains_key(id));
        self.en_memoire_basse.retain(|id| vivantes.contains_key(id));
        self.en_pause.retain(|id| vivantes.contains_key(id));
        self.pause_demandee.retain(|id| vivantes.contains_key(id));
    }

    fn reveiller(&mut self, id: TabId) {
        // L'onglet en mini-fenetre est eveille, ailleurs : ne pas lui creer une seconde page.
        if self.webviews.contains_key(&id) || self.onglet_flottant() == Some(id) {
            return;
        }
        if let Some(defilement) = self.trouver(id).map(|o| o.defilement) {
            if defilement > 1.0 {
                self.defilement_a_restaurer.insert(id, defilement);
            }
        }
        // Avant la creation : l'alignement d'une cloison, lance pendant la creation, peut
        // demander un rechargement — il doit savoir que la premiere navigation n'a pas abouti.
        self.premier_chargement.insert(id, maintenant_ms());
        self.secours_faits.remove(&id);
        if let Some(w) = self.creer_onglet_webview(id) {
            // wry cree chaque webview au premier plan : un onglet reveille pendant que le menu
            // est ouvert passerait par-dessus. On retablit l'ordre.
            crate::fenetres::clip_soeurs(&w);
            self.webviews.insert(id, w);
            let geo = self.geometrie_courante();
            self.superposer(&geo);
            // L'onglet est en place : maintenant seulement, sa page (voir `creer_onglet_webview`).
            if let Some(url) = self.navigation_differee.remove(&id) {
                if let Some(w) = self.webviews.get(&id) {
                    if let Err(e) = charger(w, &url) {
                        alerte!("onglet {id} : chargement de {url} impossible ({e})");
                    }
                }
            }
            if let Some(o) = self.onglet_mut(id) {
                o.eveille = true;
            }
            info!("onglet {id} reveille");
        }
    }

    fn endormir(&mut self, id: TabId) -> bool {
        if self.actif == Some(id) || self.dans_cote_affiche(id) {
            return false;
        }
        let retire = self.webviews.remove(&id).is_some();
        if retire {
            if let Some(o) = self.onglet_mut(id) {
                o.eveille = false;
                o.chargement = false;
            }
            info!("onglet {id} endormi (webview detruite)");
        }
        retire
    }

    fn fermer_onglet(&mut self, id: TabId) {
        // Fermer l'onglet ferme aussi sa mini-fenetre (page puis fenetre).
        if self.onglet_flottant() == Some(id) {
            self.flottant = None;
        }
        if matches!(self.session.cote, Some((g, d)) if g == id || d == id) {
            self.session.cote = None;
        }
        let Some(position) = self.session.onglets.iter().position(|o| o.id == id) else {
            return;
        };
        // Voisins calcules AVANT la suppression. On prend le voisin le plus proche, et on
        // privilegie le meme fil : sauter au PREMIER onglet du fil faisait atterrir sur une
        // page sans rapport (constate a l'usage).
        // Un onglet en rendez-vous n'est pas un voisin : il doit rester de cote jusqu'a son heure.
        let present = |o: &&Onglet| o.rendez_vous.is_none();
        let suivant_fil = self.session.onglets[position + 1..]
            .iter()
            .filter(present)
            .find(|o| o.fil == self.session.onglets[position].fil)
            .map(|o| o.id);
        let precedent_fil = self.session.onglets[..position]
            .iter()
            .rev()
            .filter(present)
            .find(|o| o.fil == self.session.onglets[position].fil)
            .map(|o| o.id);
        let precedent = self.session.onglets[..position].iter().rev().find(present).map(|o| o.id);
        let suivant = self.session.onglets[position + 1..].iter().find(present).map(|o| o.id);

        let onglet = self.session.onglets.remove(position);
        self.webviews.remove(&id);
        // On garde de quoi rouvrir (Ctrl+Maj+T), sans limite raisonnable.
        self.fermes.push(onglet.clone());
        if self.fermes.len() > 25 {
            self.fermes.remove(0);
        }
        if self.session.onglets.is_empty() {
            let now = maintenant_ms();
            let fil = self.prochain_id_fil();
            self.session.fils.push(Fil::neuf(fil, "Depart".to_string(), now));
            let mut o = Onglet::neuf(
                self.prochain_id_onglet(),
                urls::NOUVEL_ONGLET.to_string(),
                fil,
                Declencheur::NouvelOnglet,
                now,
            );
            o.titre = "Nouvel onglet".to_string();
            let nouvel = o.id;
            self.session.onglets.push(o);
            self.actif = Some(nouvel);
            self.reveiller(nouvel);
        } else if self.actif == Some(id) {
            // Ordre de choix : voisin suivant du meme fil, puis precedent du meme fil, puis
            // precedent dans la liste, puis suivant, puis le premier onglet.
            let voisin = suivant_fil
                .or(precedent_fil)
                .or(precedent)
                .or(suivant)
                .or_else(|| self.session.onglets.iter().find(present).map(|o| o.id))
                .or_else(|| self.session.onglets.first().map(|o| o.id));
            if let Some(v) = voisin {
                if let Some(o) = self.trouver(v) {
                    info!("fermeture de l'onglet {id} : on active {} ({})", v, o.libelle());
                }
                self.actif = None;
                self.activer(v);
            }
        }
        self.nettoyer_fils();
        self.renommer_fils_auto();
        self.marquer_modifie();
        self.pousser_etat();
    }

    fn rouvrir_ferme(&mut self) {
        if let Some(mut onglet) = self.fermes.pop() {
            let id = self.prochain_id_onglet();
            onglet.id = id;
            onglet.declencheur = Declencheur::Reouverture;
            onglet.eveille = false;
            onglet.chargement = false;
            if !self.session.fils.iter().any(|f| f.id == onglet.fil) {
                let fil = self.prochain_id_fil();
                self.session.fils.push(Fil::neuf(fil, "Reouverture".to_string(), maintenant_ms()));
                onglet.fil = fil;
            }
            self.session.onglets.push(onglet);
            self.actif = Some(id);
            self.reveiller(id);
            self.marquer_modifie();
            self.pousser_etat();
        }
    }

    fn nettoyer_fils(&mut self) {
        let utilises: Vec<ThreadId> = self.session.onglets.iter().map(|o| o.fil).collect();
        self.session.fils.retain(|f| utilises.contains(&f.id));
    }

    fn renommer_fils_auto(&mut self) {
        let mut mises: Vec<(ThreadId, String)> = Vec::new();
        {
            let onglets = &self.session.onglets;
            for f in self.session.fils.iter() {
                if !f.auto_nom {
                    continue;
                }
                let membres: Vec<&Onglet> = onglets.iter().filter(|o| o.fil == f.id).collect();
                if membres.is_empty() {
                    continue;
                }
                let nom = weave::nommer(&membres, onglets);
                if !nom.trim().is_empty() && nom != f.nom {
                    mises.push((f.id, nom));
                }
            }
        }
        for (id, nom) in mises {
            if let Some(f) = self.fil_mut(id) {
                f.nom = nom;
            }
        }
    }

    fn deplacer_vers_fil(&mut self, onglet: TabId, fil: ThreadId) {
        if self.onglet_mut(onglet).is_none() {
            return;
        }
        // Fil 0 = la zone « Deposer ici pour ouvrir un nouveau fil » : on le cree ici.
        // Sans ce cas, le depot etait ignore (aucun fil ne porte l'id 0).
        let fil = if fil == 0 {
            let id_fil = self.prochain_id_fil();
            self.session.fils.push(Fil::neuf(id_fil, format!("Fil {id_fil}"), maintenant_ms()));
            id_fil
        } else {
            fil
        };
        if !self.session.fils.iter().any(|f| f.id == fil) {
            return;
        }
        if let Some(o) = self.onglet_mut(onglet) {
            o.fil = fil;
        }
        self.nettoyer_fils();
        self.renommer_fils_auto();
        self.marquer_modifie();
        self.pousser_etat();
    }

    fn fusionner_fils(&mut self, a: ThreadId, b: ThreadId) {
        if a == b {
            return;
        }
        for o in self.session.onglets.iter_mut() {
            if o.fil == b {
                o.fil = a;
            }
        }
        if let Some(f) = self.fil_mut(a) {
            f.auto_nom = false;
            if f.nom.trim().is_empty() {
                f.auto_nom = true;
            }
        }
        self.nettoyer_fils();
        self.renommer_fils_auto();
        self.marquer_modifie();
        self.pousser_etat();
    }

    fn sceller_fil(&mut self, fil: ThreadId) {
        let Some(f) = self.session.fils.iter().find(|f| f.id == fil).cloned() else {
            return;
        };
        let onglets: Vec<OngletCapsule> = self
            .session
            .onglets
            .iter()
            .filter(|o| o.fil == fil && !o.interne())
            .map(|o| OngletCapsule {
                url: o.url.clone(),
                titre: o.libelle(),
                defilement: o.defilement,
                a_lire: o.a_lire,
                vitalite: o.vitalite,
            })
            .collect();
        if onglets.is_empty() {
            self.signaler("Rien a sceller : ce fil ne contient aucune page.");
            return;
        }
        let capsule = Capsule {
            version: 1,
            nom: f.nom.clone(),
            cree_le: maintenant_ms(),
            onglets,
        };
        match self.profil.sauver_capsule(&capsule) {
            Ok(chemin) => self.signaler(&format!("Capsule « {} » : {}", capsule.nom, chemin.display())),
            Err(e) => self.signaler(&format!("Capsule impossible : {e}")),
        }
        self.pousser_etat();
    }

    fn ouvrir_capsule(&mut self, chemin: &str) {
        let Some(capsule) = self.profil.charger_capsule(Path::new(chemin)) else {
            self.signaler("Capsule illisible.");
            return;
        };
        let now = maintenant_ms();
        let fil = self.prochain_id_fil();
        self.session
            .fils
            .push(Fil::neuf(fil, capsule.nom.clone(), now));
        let mut premier = None;
        for o in capsule.onglets {
            let id = self.prochain_id_onglet();
            let mut onglet = Onglet::neuf(id, o.url, fil, Declencheur::Capsule, now);
            onglet.titre = o.titre;
            onglet.defilement = o.defilement;
            onglet.a_lire = o.a_lire;
            onglet.vitalite = o.vitalite.clamp(0.3, 1.0);
            if premier.is_none() {
                premier = Some(id);
            }
            self.session.onglets.push(onglet);
        }
        if let Some(id) = premier {
            self.actif = Some(id);
            self.reveiller(id);
        }
        self.signaler(&format!("Capsule « {} » ouverte.", capsule.nom));
        self.marquer_modifie();
        self.pousser_etat();
    }

    // ---------------------------------------------------------------- battement

    pub fn tick(&mut self) {
        let now = maintenant_ms();
        let dt = now.saturating_sub(self.dernier_tick).max(1) as f64;
        self.dernier_tick = now;

        let actif = self.actif;
        let mut endormir: Vec<TabId> = Vec::new();
        if self.reglages.veille_auto {
            for o in self.session.onglets.iter_mut() {
                vitality::decroitre(o, dt, now);
                if Some(o.id) != actif && o.eveille && vitality::doit_dormir(o) {
                    endormir.push(o.id);
                }
            }
        } else {
            for o in self.session.onglets.iter_mut() {
                vitality::decroitre(o, dt, now);
            }
        }

        // Budget : on ne garde pas plus de `budget` webviews vivantes.
        let budget = self.budget_courant();
        let mut vivantes = self.webviews.len();
        if vivantes > budget {
            let mut candidats: Vec<(TabId, f32)> = self
                .webviews
                .keys()
                .copied()
                .filter(|id| Some(*id) != actif && !endormir.contains(id))
                .map(|id| (id, self.trouver(id).map(|o| o.vitalite).unwrap_or(0.0)))
                .collect();
            candidats.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
            for (id, _) in candidats {
                if vivantes <= budget {
                    break;
                }
                endormir.push(id);
                vivantes -= 1;
            }
        }

        let mut effectivement = 0usize;
        for id in endormir {
            if self.endormir(id) {
                effectivement += 1;
            }
        }
        if effectivement > 0 {
            if now.saturating_sub(self.dernier_toast) > 20_000 {
                self.dernier_toast = now;
                self.signaler(&format!(
                    "{effectivement} onglet(s) en veille — un clic les reveille, rien n'est ferme."
                ));
            }
            self.marquer_modifie();
            self.pousser_etat();
        }

        self.ajuster_memoire(now);
        self.secours_chargement(now);
        self.relever_liens_externes();
        self.outils_tick(now);

        // 8 s au lieu de 3 : chaque sauvegarde fait un `fsync` (ecriture atomique), et pendant
        // une lecture on marque l'etat modifie a chaque defilement — soit un fsync toutes les
        // 3 s pour une position de lecture. La session est de toute facon sauvegardee a la
        // fermeture ; le pire cas d'un plantage est de perdre 8 s de position, pas des onglets.
        if self.modifie && now.saturating_sub(self.derniere_sauvegarde) > 8_000 {
            self.sauver();
        }

        // Garde-fou de taille : si un evenement de redimensionnement s'est perdu (fenetre
        // deplacee entre deux ecrans, maximisation, changement de DPI), les webviews
        // garderaient l'ancienne taille et la page serait rognee. On compare, on reapplique.
        let taille = self.window.inner_size();
        if taille.width != self.derniere_taille.0 || taille.height != self.derniere_taille.1 {
            info!(
                "taille de fenetre : {:?} -> {}x{}",
                self.derniere_taille, taille.width, taille.height
            );
            self.redimensionner();
        }

        // Verification unique que l'interface a bien demarre.
        if !self.surfaces_verifiees && now.saturating_sub(self.demarrage_ms) > 3_000 {
            self.surfaces_verifiees = true;
            self.verifier_surfaces();
        }
    }

    pub fn memoire(&mut self, mut mesure: probe::Mesure) {
        // Sonde `TRAME_MEMOIRE_LIBRE=<Mo>` (diagnostic) : simule un PC a court de memoire, pour
        // eprouver le budget adapte sans saturer la machine de test.
        if let Some(simulee) = std::env::var("TRAME_MEMOIRE_LIBRE")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
        {
            mesure.dispo_mo = simulee;
        }
        let variation = (mesure.mo - self.memoire_mo).abs();
        let variation_ext = (mesure.extensions_mo - self.derniere_mesure.extensions_mo).abs();
        self.memoire_mo = mesure.mo;
        self.memoire_processus = mesure.processus;
        self.derniere_mesure = mesure;

        // Budget adapte a la memoire libre : calcule ICI, a la mesure, et pas a chaque battement
        // (sinon une memoire libre perimee ferait endormir des onglets en rafale).
        if self.reglages.budget_auto {
            let plafond = self.reglages.budget_borne();
            let permis = budget_adapte(plafond, self.webviews.len(), mesure.dispo_mo);
            if Some(permis) != self.budget_memoire {
                if permis < plafond {
                    info!(
                        "memoire libre {:.0} Mo : {permis} onglet(s) ouvert(s) au plus (plafond {plafond})",
                        mesure.dispo_mo
                    );
                } else if self.budget_memoire.map(|b| b < plafond).unwrap_or(false) {
                    info!("memoire libre {:.0} Mo : retour au plafond ({plafond})", mesure.dispo_mo);
                }
                self.budget_memoire = Some(permis);
            }
        } else {
            self.budget_memoire = None;
        }

        // Cout d'une extension, par difference, 12 s apres son allumage ou son extinction.
        if let Some((id, avant, t0, allumage)) = self.mesure_extension.clone() {
            if maintenant_ms().saturating_sub(t0) >= 12_000 {
                self.mesure_extension = None;
                let ecart = if allumage {
                    mesure.extensions_mo - avant
                } else {
                    avant - mesure.extensions_mo
                };
                if ecart >= 1.0 {
                    info!(
                        "extension {id} : {:.0} Mo mesures ({})",
                        ecart,
                        if allumage { "a l'allumage" } else { "liberes a l'extinction" }
                    );
                    self.reglages.extensions_cout.insert(
                        id,
                        CoutExtension {
                            mo: (ecart * 10.0).round() / 10.0,
                            quand: maintenant_ms(),
                        },
                    );
                    self.profil.sauver_reglages(&self.reglages);
                    self.pousser_etat();
                } else {
                    info!("extension {id} : ecart de memoire non significatif ({ecart:.1} Mo), rien n'est note");
                }
            }
        }

        if variation > 8.0 || variation_ext > 4.0 {
            self.pousser_budget();
        }
    }

    /// Nombre d'onglets ouverts permis maintenant : le plafond choisi, abaisse par la memoire
    /// libre quand le reglage automatique est actif.
    fn budget_courant(&self) -> usize {
        let plafond = self.reglages.budget_borne();
        match (self.reglages.budget_auto, self.budget_memoire) {
            (true, Some(permis)) => permis.min(plafond),
            _ => plafond,
        }
    }

    fn marquer_modifie(&mut self) {
        self.modifie = true;
    }

    pub fn sauver(&mut self) {
        // Les adresses changees sans rechargement (YouTube) : sinon la session rouvrirait
        // l'accueil du site au lieu de la video.
        let vivants: Vec<TabId> = self.webviews.keys().copied().collect();
        for id in vivants {
            self.actualiser_adresse(id);
        }
        self.session.rail_replie = self.rail_replie;
        self.session.actif = self.actif;
        // On ne touche JAMAIS a l'etat d'execution en memoire : la copie persistee n'a pas
        // a connaitre les webviews vivantes, mais l'application, elle, en a besoin.
        let mut session = self.session.clone();
        for o in session.onglets.iter_mut() {
            o.eveille = false;
            o.chargement = false;
        }
        self.profil.sauver_session(&session);
        self.profil.sauver_ext_storage(&self.ext_storage);
        self.derniere_sauvegarde = maintenant_ms();
        self.modifie = false;
    }

    // ---------------------------------------------------------------- etat vers l'interface

    fn instantane(&self) -> Value {
        let now = maintenant_ms();
        let onglets: Vec<Value> = self
            .session
            .onglets
            .iter()
            .map(|o| {
                json!({
                    "id": o.id,
                    "url": o.url,
                    "urlCourte": urls::affichage_court(&o.url),
                    "titre": o.libelle(),
                    "fil": o.fil,
                    "domaine": o.domaine(),
                    "vitalite": (o.vitalite * 1000.0).round() / 1000.0,
                    "palier": vitality::palier(o.vitalite),
                    "eveille": o.eveille,
                    "ancre": o.ancre,
                    "alire": o.a_lire,
                    "chargement": o.chargement,
                    "actif": Some(o.id) == self.actif,
                    "favicon": o.favicon,
                    "interne": o.interne(),
                    "age": now.saturating_sub(o.cree_le) / 1000,
                    "parent": o.parent,
                    "declencheur": o.declencheur.libelle(),
                    "rdv": o.rendez_vous,
                    "fermeture": o.fermeture,
                    "revenu": o.revenu,
                    // Haut-parleur du rail : l'onglet joue du son, et/ou son son est coupe.
                    "son": self.sonores.contains(&o.id),
                    "muet": self.muets.contains(&o.id),
                })
            })
            .collect();

        let fils: Vec<Value> = self
            .session
            .fils
            .iter()
            .map(|f| {
                let membres: Vec<TabId> = self
                    .session
                    .onglets
                    .iter()
                    .filter(|o| o.fil == f.id)
                    .map(|o| o.id)
                    .collect();
                json!({
                    "id": f.id,
                    "nom": f.nom,
                    "replie": f.replie,
                    "auto": f.auto_nom,
                    "domaine": weave::domaine_du_fil(f.id, &self.session.onglets),
                    "onglets": membres,
                    "cloison": f.cloison.as_ref().map(|c| json!({ "extensions": c.extensions })),
                })
            })
            .collect();

        let suggestion = weave::suggestion_fusion(&self.session.fils, &self.session.onglets)
            .map(|(a, b, score)| {
                let nom = |id: ThreadId| {
                    self.session
                        .fils
                        .iter()
                        .find(|f| f.id == id)
                        .map(|f| f.nom.clone())
                        .unwrap_or_default()
                };
                json!({"a": a, "b": b, "nomA": nom(a), "nomB": nom(b), "score": (score * 100.0).round() / 100.0})
            });

        let actif = self.actif.and_then(|id| self.trouver(id));

        let extensions: Vec<Value> = self
            .extensions
            .iter()
            .map(|e| {
                json!({
                    "id": e.id,
                    "nom": e.manifeste.name,
                    "version": e.manifeste.version,
                    "description": e.manifeste.description,
                    "active": e.active,
                    "accent": e.manifeste.accent,
                    "motifs": e.manifeste.matches,
                    "dossier": e.dossier.display().to_string(),
                })
            })
            .collect();

        let capsules: Vec<Value> = self
            .profil
            .lister_capsules()
            .into_iter()
            .map(|(chemin, nom)| json!({"nom": nom, "chemin": chemin.to_string_lossy()}))
            .collect();

        let budget = self.budget_json();
        // Cout reel des extensions : une seule extension Chrome active dans le profil principal
        // et un seul processus d'extension -> le chiffre est direct ; sinon, la mesure faite par
        // difference a son allumage.
        let actives_principal: Vec<String> = self
            .liste_extensions_chrome()
            .iter()
            .filter(|e| e["actif"].as_bool().unwrap_or(false))
            .filter_map(|e| e["id"].as_str().map(|s| s.to_string()))
            .collect();
        let direct = (actives_principal.len() == 1 && self.derniere_mesure.extensions_processus == 1)
            .then(|| (actives_principal[0].clone(), self.derniere_mesure.extensions_mo));
        // L'etat du moteur, quand il a repondu : « chargee » = elle tourne maintenant.
        let extensions_chrome_etat: Vec<Value> = self
            .liste_extensions_chrome()
            .into_iter()
            .map(|mut e| {
                let id = e["id"].as_str().unwrap_or("").to_string();
                e["chargee"] = match &self.moteur_extensions {
                    Some(m) => json!(m.get(&id).copied().unwrap_or(false)),
                    None => Value::Null,
                };
                match &direct {
                    Some((d, mo)) if *d == id => {
                        e["coutMo"] = json!((mo * 10.0).round() / 10.0);
                        e["coutDirect"] = json!(true);
                    }
                    _ => {
                        if let Some(c) = self.reglages.extensions_cout.get(&id) {
                            e["coutMo"] = json!(c.mo);
                            e["coutDirect"] = json!(false);
                        }
                    }
                }
                e
            })
            .collect();

        json!({
            "onglets": onglets,
            "fils": fils,
            "actif": self.actif,
            "filActif": actif.map(|o| o.fil),
            "url": actif.map(|o| urls::affichage_court(&o.url)).unwrap_or_default(),
            "urlBrute": actif.map(|o| o.url.clone()).unwrap_or_default(),
            "titre": actif.map(|o| o.libelle()).unwrap_or_default(),
            "chargement": actif.map(|o| o.chargement).unwrap_or(false),
            "interne": actif.map(|o| o.interne()).unwrap_or(true),
            "railReplie": self.rail_replie,
            "panneau": self.panneau_ouvert,
            "version": crate::maj::ACTUELLE,
            "theme": self.reglages.theme,
            "maj": self.maj.as_ref().map(|m| json!({ "version": m.version, "url": m.url })),
            "moteur": {
                "id": urls::moteur(&self.reglages.moteur).id,
                "nom": urls::moteur(&self.reglages.moteur).nom,
            },
            "moteurs": urls::MOTEURS
                .iter()
                .map(|m| json!({"id": m.id, "nom": m.nom, "langue": m.langue}))
                .collect::<Vec<_>>(),
            "barreExtensions": self.reglages.barre_extensions,
            "retourPossible": actif.and_then(|o| self.webviews.get(&o.id)).and_then(|w| w.can_go_back().ok()).unwrap_or(false),
            "avantPossible": actif.and_then(|o| self.webviews.get(&o.id)).and_then(|w| w.can_go_forward().ok()).unwrap_or(false),
            "vue": if self.vue_trame { "trame" } else { "page" },
            "extensions": extensions,
            "capsules": capsules,
            "suggestion": suggestion,
            "budget": budget,
            "reglages": {
                "veilleAuto": self.reglages.veille_auto,
                "memoireBasse": self.reglages.memoire_basse,
                "budgetAuto": self.reglages.budget_auto,
                "nettoyerAdresses": self.reglages.nettoyer_adresses,
                "garderAffiliation": self.reglages.garder_affiliation,
                "antiPistage": self.reglages.anti_pistage.clone(),
                "pauseOnglets": self.reglages.pause_onglets,
                "memoireLecture": self.reglages.memoire_lecture,
                "lectureExclus": self.reglages.lecture_exclus.clone(),
                "parDefaut": self.par_defaut,
                "adressesNettoyees": self.adresses_nettoyees,
                "adressesBloquees": self.adresses_bloquees,
                "budget": self.reglages.budget_borne(),
                "barreExtensions": self.reglages.barre_extensions,
                "pleinEcran": self.reglages.plein_ecran.clone(),
                "barreFenetre": self.reglages.barre_fenetre(),
                "moteur": self.reglages.moteur.clone(),
            },
            "profil": self.profil.racine().display().to_string(),
            "dossierExtensions": self.profil.dossier_extensions().display().to_string(),
            "dossierExtensionsChrome": self.extensions_chrome.display().to_string(),
            "extensionsChrome": extensions_chrome_etat,
            "installations": self.installations.clone(),
            "extensionsEpinglees": self.reglages.extensions_epinglees.clone(),
            "panneauExtension": self.panneau_extension.clone(),
            "chromeDisponibles": self.chrome_disponibles,
            "version": env!("CARGO_PKG_VERSION"),
            "fenetre": { "style": self.reglages.barre_fenetre(), "agrandie": self.window.is_maximized() },
            "carnet": self.carnet_json(),
            "sauvegarde": {
                // Le catalogue des contenus vient d'ici, pas du HTML : deux listes ecrites a
                // deux endroits finissent toujours par diverger, et l'interface proposerait
                // alors une case qui ne sauvegarde rien.
                "contenus": crate::sauvegarde::CONTENUS
                    .iter()
                    .map(|c| json!({
                        "cle": c.cle,
                        "libelle": c.libelle,
                        "detail": c.detail,
                        "defaut": c.defaut,
                    }))
                    .collect::<Vec<_>>(),
                "restauration": self.restauration.as_ref().map(|(chemin, resume, contenus)| json!({
                    "fichier": chemin.display().to_string(),
                    "resume": resume,
                    "contenus": contenus,
                })),
            },
            "historique": self.historique.len(),
            "telechargements": self.telechargements_json(),
            "zap": {
                "ouvert": self.zap_ouvert,
                "domaine": self.domaine_actif(),
                "regles": self.zap_regles.iter().enumerate().map(|(index, r)| json!({
                    "index": index,
                    "domaine": r.domaine,
                    "selecteur": r.selecteur,
                    "libelle": r.libelle,
                    "actif": r.actif,
                    "texte": r.texte,
                    "action": r.action,
                    "nomAction": r.nom_action(),
                    "valeur": r.valeur,
                    "paquet": r.paquet,
                    "vuLe": r.vu_le,
                    // Nombre d'elements attrapes dans l'onglet affiche (absent : pas mesure).
                    "n": self.actif
                        .and_then(|a| self.zap_comptes.get(&a))
                        .and_then(|c| c.iter().find(|(i, _)| *i == index).map(|(_, n)| *n)),
                })).collect::<Vec<_>>(),
                "paquets": crate::model::paquets_zap().iter().map(|p| json!({
                    "id": p.id,
                    "titre": p.titre,
                })).collect::<Vec<_>>(),
                "maintenant": maintenant_ms(),
            },
        })
    }

    fn pousser_etat(&mut self) {
        let etat = self.instantane();
        let js = format!("window.trame && window.trame.etat({etat})");
        if let Some(w) = &self.rail {
            let _ = w.evaluate_script(&js);
        }
        if let Some(w) = &self.barre {
            let _ = w.evaluate_script(&js);
        }
        if let Some(w) = &self.vue {
            let _ = w.evaluate_script(&js);
        }
        if let Some(w) = &self.zap {
            let _ = w.evaluate_script(&js);
        }
        // La page d'accueil interne a besoin du meme etat : c'est elle qui montre les fils.
        let internes: Vec<TabId> = self
            .session
            .onglets
            .iter()
            .filter(|o| o.interne() && self.webviews.contains_key(&o.id))
            .map(|o| o.id)
            .collect();
        for id in internes {
            if let Some(w) = self.webviews.get(&id) {
                let _ = w.evaluate_script(&js);
            }
        }
        if let Some(o) = self.actif.and_then(|id| self.trouver(id)) {
            let titre = o.libelle();
            self.window.set_title(&format!("{titre} — Trame"));
        }
    }

    /// Le budget tel que l'interface l'affiche : memoire reelle, onglets, plafond effectif, et
    /// ce que coutent les extensions (total exact des processus `--extension-process`).
    fn budget_json(&self) -> Value {
        let m = &self.derniere_mesure;
        let arrondi = |v: f64| (v * 10.0).round() / 10.0;
        json!({
            "reveilles": self.webviews.len(),
            "endormis": self.session.onglets.iter().filter(|o| !o.eveille).count(),
            "mo": arrondi(self.memoire_mo),
            "processus": self.memoire_processus,
            "plafond": self.budget_courant(),
            "plafondChoisi": self.reglages.budget_borne(),
            "auto": self.reglages.budget_auto,
            "dispoMo": arrondi(m.dispo_mo),
            "extensionsMo": arrondi(m.extensions_mo),
            "extensionsProcessus": m.extensions_processus,
        })
    }

    fn pousser_budget(&mut self) {
        let budget = self.budget_json();
        let js = format!("window.trame && window.trame.budget({budget})");
        for w in [
            self.rail.as_ref(),
            self.barre.as_ref(),
            self.vue.as_ref(),
            self.zap.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = w.evaluate_script(&js);
        }
    }

    fn signaler(&mut self, message: &str) {
        info!("toast : {message}");
        let js = format!(
            "window.trame && window.trame.toast({})",
            serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string())
        );
        // Un seul endroit : la barre, toujours visible. Envoye a toutes les surfaces, le meme
        // message s'affichait deux fois — dont une, tassee sur trois lignes, par-dessus les
        // boutons du pied du rail (constate le 2026-11-21).
        if let Some(w) = self.barre.as_ref().or(self.rail.as_ref()) {
            let _ = w.evaluate_script(&js);
        }
    }

    fn eval_sur(&self, source: Source, js: &str) {
        let cible = match source {
            Source::Rail => self.rail.as_ref(),
            Source::Barre => self.barre.as_ref(),
            Source::Vue => self.vue.as_ref(),
            Source::Zap => self.zap.as_ref(),
            Source::Apercu => self.apercu.as_ref(),
            Source::Onglet(id) => self.webviews.get(&id),
        };
        if let Some(w) = cible {
            if let Err(e) = w.evaluate_script(js) {
                alerte!("evaluate_script a echoue : {e}");
            }
        }
    }

    // ---------------------------------------------------------------- evenements

    pub fn traiter(&mut self, evenement: UserEvent) {
        match evenement {
            UserEvent::Ipc { source, corps } => self.traiter_ipc(source, &corps),
            UserEvent::Lecture { onglet, url, valeur } => self.lecture_recue(onglet, url, valeur),
            UserEvent::LecturesTrouvees { rid, items } => self.lectures_trouvees(rid, items),
            UserEvent::GuetCharge { id } => self.guet_charge(id),
            UserEvent::Script { source, js } => self.eval_sur(source, &js),
            UserEvent::GuetLu { id, texte } => self.guet_lu(id, texte),
            UserEvent::GuetPose { onglet, selecteur, texte } => self.guet_pose(onglet, selecteur, texte),
            UserEvent::Tick => {
                if !self.restaure {
                    if maintenant_ms().saturating_sub(self.demarrage_ms) < 3_000 {
                        return;
                    }
                    alerte!("moteur d'extensions : pas de reponse en 3 s, les onglets s'ouvrent quand meme");
                    self.restaurer_maintenant();
                }
                self.tick();
                self.suivre_le_son();
                self.suivre_telechargements();
                // Compte rendu d'une restauration faite au demarrage : la barre existe
                // desormais, donc le message peut s'afficher autrement que dans le journal.
                if let Some(message) = self.message_restauration.take() {
                    self.signaler(&message);
                }
                // Sonde de diagnostic : `TRAME_EXT_ETAT=<id>` (ou `<id>=off`) allume ou coupe une
                // extension au premier battement. Elle sert a verifier que le choix part bien
                // dans `reglages.json` et se relit au demarrage suivant.
                if let Some((id, actif)) = self.ext_etat_demande.take() {
                    self.basculer_extension(&id, actif);
                }
                // Sonde de diagnostic : `TRAME_PANNEAU=<id>` ouvre la popup d'une extension
                // au premier battement. Elle sert a eprouver le panneau sans souris, une
                // verification visuelle au pixel pres etant trompeuse.
                if let Some(id) = self.panneau_demande.take() {
                    self.ouvrir_popup_extension(&id);
                }
                // Sonde de diagnostic : `TRAME_NAV=<adresse>` navigue l'onglet actif au premier
                // battement. Elle sert a eprouver les garde-fous d'adresse sans souris.
                if let Some(adresse) = self.nav_demandee.take() {
                    self.naviguer_actif(&adresse);
                }
                // Sonde de diagnostic : `TRAME_ZOOM=1.3` applique un zoom au premier battement.
                if let Some(facteur) = self.zoom_demande.take() {
                    if let Some(id) = self.actif {
                        self.appliquer_zoom(id, facteur);
                    }
                }
                // Sonde de diagnostic : `TRAME_CRX=<chemin>` installe un paquet local.
                if let Some(chemin) = self.crx_demande.take() {
                    self.lancer_installation(installation::Demande::Fichier {
                        chemin: Some(PathBuf::from(chemin)),
                        source: "fichier".into(),
                    });
                }
                // Sonde de diagnostic : `TRAME_EXT=<identifiant>` installe depuis le magasin.
                if let Some(identifiant) = self.ext_demande.take() {
                    self.installer_extension_magasin(&identifiant);
                }
                // Ce que le moteur a reellement charge : wry ajoute les extensions de facon
                // asynchrone au demarrage, on interroge donc le moteur une fois installe.
                // Fenetre d'extension ouverte : on mesure son contenu a chaque battement (elle
                // change de taille en naviguant entre ses vues), et le panneau l'epouse.
                if self.zap_ouvert && self.panneau_extension.is_some() {
                    if let Some(w) = &self.zap {
                        let proxy = self.proxy.clone();
                        let _ = w.evaluate_script_with_callback(MESURE_POPUP, move |brut| {
                            let v: Value = serde_json::from_str(&brut)
                                .ok()
                                .and_then(|t: Value| t.as_str().and_then(|s| serde_json::from_str(s).ok()).or(Some(t)))
                                .unwrap_or(Value::Null);
                            if let (Some(l), Some(h)) = (v[0].as_f64(), v[1].as_f64()) {
                                if l > 0.0 && h > 0.0 {
                                    let _ = proxy.send_event(UserEvent::TaillePopup { largeur: l, hauteur: h });
                                }
                            }
                        });
                    }
                }
                // Sonde `TRAME_SONDE_JS=<script>` (diagnostic) : execute un script dans l'onglet actif
                // 15 s apres le demarrage et journalise son resultat — mesurer une page sans la
                // regarder (fenetre cachee, aucun focus pris).
                if let Ok(js) = std::env::var("TRAME_SONDE_JS") {
                    if !self.sonde_js_faite && maintenant_ms().saturating_sub(self.demarrage_ms) > 15_000 {
                        self.sonde_js_faite = true;
                        if let Some(w) = self.actif.and_then(|a| self.webviews.get(&a)) {
                            let proxy = self.proxy.clone();
                            let _ = w.evaluate_script_with_callback(&js, move |r| {
                                let _ = proxy.send_event(UserEvent::Log(format!("sonde js : {r}")));
                            });
                        }
                    }
                }
                // Sonde `TRAME_APERCU=<adresse>` (diagnostic) : ouvre un coup d'oeil sans souris.
                if let Ok(url) = std::env::var("TRAME_APERCU") {
                    if !self.apercu_sonde_fait && maintenant_ms().saturating_sub(self.demarrage_ms) > 5_000 {
                        self.apercu_sonde_fait = true;
                        self.ouvrir_apercu(&url);
                    }
                }
                // Sonde `TRAME_MENU` (diagnostic) : ouvre le menu des extensions sans souris ni
                // focus, pour eprouver la superposition et l'ombre sans voler le premier plan.
                if std::env::var("TRAME_MENU").is_ok()
                    && !self.menu_sonde_fait
                    && maintenant_ms().saturating_sub(self.demarrage_ms) > 5_000
                {
                    self.menu_sonde_fait = true;
                    if let Some(b) = &self.barre {
                        let _ = b.evaluate_script("document.getElementById('btn-ext').click()");
                    }
                }
                // Sonde `TRAME_REVEILLER` : reveiller tous les onglets (dans la limite du
                // budget), pour mesurer la memoire d'une session chargee.
                if std::mem::take(&mut self.reveil_demande) {
                    let ids: Vec<TabId> = self
                        .session
                        .onglets
                        .iter()
                        .filter(|o| !o.eveille)
                        .map(|o| o.id)
                        .collect();
                    for id in ids {
                        self.reveiller(id);
                    }
                    info!("sonde : {} onglet(s) reveille(s)", self.webviews.len());
                    self.pousser_etat();
                }
                // Sonde `TRAME_EXT_CHROME` : une fois le moteur pret, on eteint ; le rallumage
                // suit la confirmation de l'extinction (voir `bilan_moteur`).
                if self.moteur_extensions.is_some() {
                    if let Some((id, 0)) = self.sonde_chrome.clone() {
                        self.sonde_chrome = Some((id.clone(), 1));
                        self.desactiver_chrome(&id);
                    }
                }
                if !self.moteur_interroge && self.barre.is_some() {
                    let ecoule = maintenant_ms().saturating_sub(self.demarrage_ms);
                    if ecoule > 2500 {
                        self.moteur_interroge = true;
                        self.interroger_moteur();
                    }
                }
            }
            UserEvent::Memoire(mesure) => self.memoire(mesure),
            UserEvent::Titre { onglet, titre } => {
                // Un site a page unique (YouTube…) change d'adresse sans recharger : seul le
                // titre le signale. On relit donc l'adresse reelle a chaque titre.
                if self.actualiser_adresse(onglet) {
                    self.marquer_modifie();
                    self.pousser_etat();
                }
                if let Some(o) = self.onglet_mut(onglet) {
                    if o.titre != titre {
                        o.titre = titre;
                        // Le nom d'un fil suit les titres : sans cela, un fil ouvert sur une
                        // page interne resterait nomme d'apres elle (« Localhost »).
                        self.renommer_fils_auto();
                        self.marquer_modifie();
                        self.pousser_etat();
                    }
                }
            }
            UserEvent::Chargement {
                onglet,
                termine,
                url,
            } => self.sur_chargement(onglet, termine, url),
            UserEvent::Icone { onglet, url } => {
                if let Some(o) = self.onglet_mut(onglet) {
                    if o.favicon.as_deref() != Some(url.as_str()) {
                        o.favicon = if url.is_empty() { None } else { Some(url) };
                        self.pousser_etat();
                    }
                }
                self.capturer_icone_si_utile(onglet);
            }
            UserEvent::IconeImage { onglet, donnees } => self.icone_capturee(onglet, donnees),
            UserEvent::PleinEcran { onglet, actif } => self.sur_plein_ecran(onglet, actif),
            UserEvent::NouvelleFenetre { onglet, url } => {
                let url = urls::normaliser_avec(&url, &self.reglages.moteur);
                // Depuis le panneau de droite (popup d'extension), il n'y a pas d'onglet
                // d'origine : on tisse alors sans parent plutot que sur un identifiant fictif.
                let parent = if self.trouver(onglet).is_some() {
                    Some(onglet)
                } else {
                    None
                };
                self.ouvrir_onglet(url, Declencheur::Lien, parent);
            }
            UserEvent::Log(ligne) => info!("{ligne}"),
            UserEvent::Installation { libelle, issue } => self.fin_installation(libelle, issue),
            UserEvent::Sauvegarde(issue) => self.fin_sauvegarde(issue),
            UserEvent::TaillePopup { largeur, hauteur } => {
                // Bornes d'une fenetre d'action de Chrome : 25 a 800 px de large, 25 a 600 de haut.
                let taille = (largeur.clamp(120.0, 800.0), hauteur.clamp(80.0, 600.0));
                let change = self
                    .popup_taille
                    .map(|(l, h)| (l - taille.0).abs() > 2.0 || (h - taille.1).abs() > 2.0)
                    .unwrap_or(true);
                if change && self.panneau_extension.is_some() {
                    self.popup_taille = Some(taille);
                    self.appliquer_geometrie();
                }
            }
            UserEvent::CloisonAlignee {
                profil,
                ajoutees,
                retirees,
            } => {
                info!("cloison {profil} : {ajoutees} extension(s) ajoutee(s), {retirees} retiree(s)");
                if ajoutees + retirees > 0 {
                    // Un script de contenu ne s'injecte qu'au chargement : on recharge ce qui
                    // se voit de cette cloison (l'onglet affiche, la popup ouverte).
                    let fil_actif = self.actif.and_then(|a| self.trouver(a)).map(|o| o.fil);
                    let dans_cloison = fil_actif
                        .and_then(|f| self.cloison_du_fil(f))
                        .map(|c| c.profil == profil)
                        .unwrap_or(false);
                    if dans_cloison {
                        self.recharger_page_active();
                    }
                    if self.zap_profil.as_deref() == Some(profil.as_str()) {
                        if let Some(w) = &self.zap {
                            let _ = w.reload();
                        }
                    }
                }
            }
            UserEvent::MoteurBilan(bilan) => self.bilan_moteur(bilan),
            UserEvent::MoteurListe(chargees) => {
                let resume = chargees
                    .iter()
                    .map(|c| format!("{} ({})", c.nom, if c.active { "active" } else { "eteinte" }))
                    .collect::<Vec<_>>()
                    .join(", ");
                info!("moteur d'extensions : {} chargee(s) — {resume}", chargees.len());
                if !self.extensions_alignees {
                    self.extensions_alignees = true;
                    self.aligner_extensions_au_demarrage(&chargees);
                }
                if !self.restaure {
                    self.restaurer_maintenant();
                }
                self.moteur_extensions =
                    Some(chargees.into_iter().map(|c| (c.id, c.active)).collect());
                self.pousser_etat();
            }
            UserEvent::ParDefaut(oui) => {
                if self.par_defaut != Some(oui) {
                    info!("navigateur par defaut de Windows : {}", if oui { "Trame" } else { "un autre" });
                    self.par_defaut = Some(oui);
                    self.pousser_etat();
                }
            }
            UserEvent::ApercuAdresse(url) => {
                if self.apercu.is_some() && (url.starts_with("http://") || url.starts_with("https://")) {
                    self.apercu_url = url;
                }
            }
            UserEvent::PauseOnglet { onglet, faite } => {
                if faite && self.webviews.contains_key(&onglet) {
                    info!("onglet {onglet} : en pause (cache depuis {} min)", pause_apres_ms() / 60_000);
                    self.en_pause.insert(onglet);
                } else {
                    info!("onglet {onglet} : pause refusee par le moteur");
                }
            }
            UserEvent::AdresseNettoyee { onglet, origine, propre } => {
                // La requete sale a ete remplacee par une redirection vers `propre`, avant envoi.
                let retires = origine.len().saturating_sub(propre.len());
                info!("onglet {onglet} : adresse nettoyee ({retires} caracteres de pistage retires)");
                self.adresses_nettoyees = self.adresses_nettoyees.saturating_add(1);
            }
            UserEvent::AdresseBloquee { onglet, adresse } => {
                self.adresses_bloquees = self.adresses_bloquees.saturating_add(1);
                // Une ligne par hote et par onglet suffit au journal.
                let hote = url::Url::parse(&adresse)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| format!("{h}{}", u.path())))
                    .unwrap_or_default();
                if self.blocages_journalises.insert((onglet, hote.clone())) {
                    info!("onglet {onglet} : adresse bloquee {}", urls::affichage_court(&hote));
                }
            }
            UserEvent::NavigationRefusee(url) => {
                info!("navigation refusee (adresse interne de Chromium) : {url}");
                self.signaler(&format!(
                    "« {} » est interne à Chromium : navigation refusée, la page reste en place.",
                    urls::affichage_court(&url)
                ));
            }
            UserEvent::TelechargementDebut(t) => self.telechargement_debut(t),
            UserEvent::TelechargementEtat { id, termine, interrompu, reprenable, raison, recu } => {
                self.telechargement_etat(id, termine, interrompu, reprenable, raison, recu)
            }
            UserEvent::Maj(nouveaute) => self.maj(nouveaute),
            UserEvent::Telechargement {
                url,
                chemin,
                reussi,
            } => {
                // « Ajouter à Chrome » sur la fiche du magasin : WebView2 ne sait pas installer
                // depuis une page, il telecharge le `.crx` — et le fichier restait inerte dans
                // « telechargements ». On reprend ce paquet, a la place de l'utilisateur.
                match suite_telechargement(&url, chemin.as_deref().map(Path::new), reussi) {
                    SuiteTelechargement::InstallerPaquet => {
                        if let Some(fichier) = chemin.as_deref() {
                            self.lancer_installation(installation::Demande::PaquetDuMagasin {
                                chemin: PathBuf::from(fichier),
                                url: url.clone(),
                            });
                            return;
                        }
                    }
                    SuiteTelechargement::InstallerIdentifiant(id) => {
                        info!("magasin : telechargement non exploitable ({url}) — on reprend par l'identifiant {id}");
                        self.lancer_installation(installation::Demande::Magasin {
                            id,
                            source: "magasin".into(),
                        });
                        return;
                    }
                    SuiteTelechargement::Ordinaire => {}
                }
                let nom = chemin
                    .as_deref()
                    .map(|c| {
                        std::path::Path::new(c)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| c.to_string())
                    })
                    .unwrap_or_else(|| urls::affichage_court(&url));
                if reussi {
                    self.signaler(&format!("Téléchargé : {nom}"));
                } else {
                    alerte!("telechargement echoue : {url}");
                    self.signaler(&format!("Téléchargement échoué : {nom}"));
                }
            }
        }
    }

    /// Relit l'adresse affichee par la webview d'un onglet (navigation sans rechargement :
    /// `history.pushState`). Rend `true` si elle a change.
    pub(super) fn actualiser_adresse(&mut self, id: TabId) -> bool {
        let Some(url) = self.webviews.get(&id).and_then(|w| w.url().ok()) else { return false };
        let url = retranscrire_interne(&url);
        if !(url.starts_with("http://") || url.starts_with("https://") || url.starts_with("trame:")) {
            return false;
        }
        match self.onglet_mut(id) {
            Some(o) if o.url != url => {
                o.url = url;
                true
            }
            _ => false,
        }
    }

    fn sur_chargement(&mut self, id: TabId, termine: bool, url: String) {
        // wry reecrit `trame://localhost/x` en `http://trame.localhost/x` sous Windows.
        // Sans cette retranscription, la page interne ne serait plus reconnue comme telle
        // (adresse affichee fausse, etat non pousse a la page d'accueil).
        let url = retranscrire_interne(&url);
        // La page vide de naissance d'un onglet (voir `creer_onglet_webview`) ne compte pas : ni
        // comme adresse, ni comme premier chargement.
        if url == "about:blank" {
            return;
        }
        let url_utilisable = url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("trame:");
        if let Some(o) = self.onglet_mut(id) {
            if url_utilisable && !url.is_empty() {
                o.url = url.clone();
            }
            o.chargement = !termine;
        }
        if !termine {
            self.pousser_etat();
            return;
        }
        if let Some(debut) = self.premier_chargement.remove(&id) {
            info!(
                "onglet {id} : premier chargement termine en {} ms",
                maintenant_ms().saturating_sub(debut)
            );
        }
        if self.recharger_apres_chargement.remove(&id) {
            info!("onglet {id} recharge (differe a la fin de son chargement)");
            self.recharger_onglet(id);
        }

        // Historique : uniquement du web reel.
        if url.starts_with("http") {
            let titre = self.trouver(id).map(|o| o.libelle()).unwrap_or_default();
            let deja = self
                .historique
                .last()
                .map(|e| e.url == url && maintenant_ms().saturating_sub(e.ts) < 3_000)
                .unwrap_or(false);
            if !deja {
                let entree = EntreeHistorique {
                    url: url.clone(),
                    titre,
                    ts: maintenant_ms(),
                };
                self.profil.ajouter_historique(&entree);
                self.historique.push(entree);
                if self.historique.len() > 5000 {
                    self.historique.remove(0);
                }
            }
        }

        // Extensions : injection en fin de chargement.
        let a_injecter: Vec<String> = extensions::applicables(&self.extensions, &url)
            .into_iter()
            .map(extensions::injection)
            .collect();
        for js in a_injecter {
            self.eval_sur(Source::Onglet(id), &js);
        }

        // Regles de masquage (le zappeur), apres les extensions pour passer au-dessus.
        self.appliquer_zap(id);

        // Position de lecture : on remet l'onglet la ou il etait.
        if let Some(y) = self.defilement_a_restaurer.remove(&id) {
            let js = format!("window.scrollTo(0, {});", y.max(0.0));
            self.eval_sur(Source::Onglet(id), &js);
        }

        // Favicon : aucune API WebView2, on interroge le document.
        let proxy = self.proxy.clone();
        if let Some(w) = self.webviews.get(&id) {
            let _ = w.evaluate_script_with_callback(
                r#"(function(){try{var l=document.querySelector('link[rel~="icon"][href]');if(!l)return "";return new URL(l.getAttribute("href"),location.href).href;}catch(e){return "";}})()"#,
                move |valeur| {
                    let url = serde_json::from_str::<String>(&valeur).unwrap_or_default();
                    let _ = proxy.send_event(UserEvent::Icone { onglet: id, url });
                },
            );
        }

        self.marquer_modifie();
        self.pousser_etat();
    }

    fn traiter_ipc(&mut self, source: Source, corps: &str) {
        let Ok(v) = serde_json::from_str::<Value>(corps) else {
            return;
        };
        let Some(m) = v["m"].as_str() else { return };

        // Un onglet distant n'a le droit qu'a trois messages. Les commandes ne sont
        // acceptees que depuis les surfaces d'interface, ou depuis nos pages internes.
        let interne = self.source_interne(source);
        let autorise = match m {
            "extCall" | "defilement" | "raccourci" | "pret" | "hello" | "clicPage" | "zapChoisi"
            | "zapCompte" | "coupDoeil" | "flottant" => !matches!(source, Source::Apercu),
            // L'apercu n'a droit qu'a ses deux boutons.
            "apercuOuvrir" | "apercuFermer" => true,
            _ => !matches!(source, Source::Onglet(_) | Source::Apercu) || interne,
        };
        if !autorise {
            return;
        }

        // Fermer le panneau des que l'utilisateur agit ailleurs que dans la barre.
        // Un clic dans la page ne retirait pas le focus de la barre (deux webviews voisines),
        // donc `window.blur` ne suffisait pas : le panneau restait ouvert par-dessus la page.
        if self.panneau_ouvert && !matches!(source, Source::Barre) {
            self.panneau_ouvert = false;
            self.appliquer_geometrie();
            self.pousser_etat();
        }
        // Une fenetre d'extension se referme quand on clique dans la page, comme dans Chrome.
        // Elle se pose maintenant PAR-DESSUS la page : rester ouverte la ferait masquer ce
        // qu'on est en train de lire. (Le panneau du zappeur, lui, est un outil : il reste.)
        if m == "clicPage" && self.panneau_extension.is_some() && matches!(source, Source::Onglet(_)) {
            self.fermer_panneau_droit();
        }
        // L'apercu se referme quand on clique dans la page dessous, comme dans Arc et Zen.
        if m == "clicPage" && self.apercu.is_some() && matches!(source, Source::Onglet(_)) {
            self.fermer_apercu();
        }

        if self.traiter_ipc_outils(source, m, &v) || self.traiter_ipc_fenetre(source, m, &v) {
            return;
        }

        match m {
            "pret" | "hello" => self.pousser_etat(),
            "naviguer" => {
                let saisie = v["url"].as_str().unwrap_or("").to_string();
                self.naviguer_actif(&saisie);
            }
            "nouvelOnglet" => {
                let url = v["url"].as_str().unwrap_or("").to_string();
                let url = if url.trim().is_empty() {
                    urls::NOUVEL_ONGLET.to_string()
                } else {
                    urls::normaliser_avec(&url, &self.reglages.moteur)
                };
                self.ouvrir_onglet(url, Declencheur::NouvelOnglet, None);
            }
            "fermer" => {
                if let Some(id) = v["id"].as_u64() {
                    self.fermer_onglet(id as TabId);
                }
            }
            "activer" => {
                if let Some(id) = v["id"].as_u64() {
                    self.activer(id as TabId);
                }
            }
            "retour" => {
                if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                    let _ = w.go_back();
                }
            }
            "avant" => {
                if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                    let _ = w.go_forward();
                }
            }
            "recharger" => {
                let id = v["id"].as_u64().map(|n| n as TabId).or(self.actif);
                if let Some(w) = id.and_then(|id| self.webviews.get(&id)) {
                    let _ = w.reload();
                }
            }
            "reveillerFond" => {
                // Reveiller sans afficher (menu de l'onglet).
                if let Some(id) = v["id"].as_u64().map(|n| n as TabId) {
                    self.reveiller(id);
                    self.marquer_modifie();
                    self.pousser_etat();
                }
            }
            "endormir" => {
                let id = v["id"].as_u64().map(|n| n as TabId).or(self.actif);
                // L'onglet affiche ne peut pas dormir : on passe d'abord au precedent.
                if let Some(id) = id.filter(|id| Some(*id) == self.actif) {
                    match self.onglet_precedent().filter(|p| *p != id) {
                        Some(p) => self.activer(p),
                        None => {
                            self.signaler("C'est le seul onglet : il reste éveillé.");
                            return;
                        }
                    }
                }
                if let Some(id) = id {
                    if self.endormir(id) {
                        self.marquer_modifie();
                        self.pousser_etat();
                    }
                }
            }
            "endormirFroids" => {
                let mut n = 0;
                let ids: Vec<TabId> = self
                    .session
                    .onglets
                    .iter()
                    .filter(|o| o.eveille && o.vitalite < 0.5 && Some(o.id) != self.actif)
                    .map(|o| o.id)
                    .collect();
                for id in ids {
                    if self.endormir(id) {
                        n += 1;
                    }
                }
                self.signaler(&format!("{n} onglet(s) endormi(s)."));
                self.pousser_etat();
            }
            "ancrer" => {
                let id = v["id"].as_u64().map(|n| n as TabId).or(self.actif);
                let actif = v["actif"].as_bool().unwrap_or(true);
                if let Some(id) = id {
                    if let Some(o) = self.onglet_mut(id) {
                        o.ancre = actif;
                        if actif {
                            o.vitalite = 1.0;
                        }
                    }
                    self.marquer_modifie();
                    self.pousser_etat();
                }
            }
            "alire" => {
                let id = v["id"].as_u64().map(|n| n as TabId).or(self.actif);
                let actif = v["actif"].as_bool().unwrap_or(true);
                if let Some(id) = id {
                    if let Some(o) = self.onglet_mut(id) {
                        o.a_lire = actif;
                    }
                    self.marquer_modifie();
                    self.pousser_etat();
                }
            }
            "filRenommer" => {
                let id = v["id"].as_u64().map(|n| n as ThreadId);
                let nom = v["nom"].as_str().unwrap_or("").trim().to_string();
                if let (Some(id), false) = (id, nom.is_empty()) {
                    if let Some(f) = self.fil_mut(id) {
                        f.nom = nom;
                        f.auto_nom = false;
                    }
                    self.marquer_modifie();
                    self.pousser_etat();
                }
            }
            "filReplier" => {
                let id = v["id"].as_u64().map(|n| n as ThreadId);
                let replie = v["replie"].as_bool().unwrap_or(false);
                if let Some(id) = id {
                    if let Some(f) = self.fil_mut(id) {
                        f.replie = replie;
                    }
                    self.marquer_modifie();
                    self.pousser_etat();
                }
            }
            "filDeplacer" => {
                // Ranger les groupes : `avant` est le fil devant lequel poser, 0 (ou absent)
                // voulant dire « en dernier » — meme convention que `fil: 0` de « tisser ».
                let id = v["id"].as_u64().map(|n| n as ThreadId);
                let avant = v["avant"]
                    .as_u64()
                    .map(|n| n as ThreadId)
                    .filter(|n| *n != 0);
                if let Some(id) = id {
                    if self.session.deplacer_fil(id, avant) {
                        self.marquer_modifie();
                        self.pousser_etat();
                    }
                }
            }
            "tisser" => {
                let onglet = v["onglet"].as_u64().map(|n| n as TabId);
                let fil = v["fil"].as_u64().map(|n| n as ThreadId);
                if let (Some(onglet), Some(fil)) = (onglet, fil) {
                    self.deplacer_vers_fil(onglet, fil);
                }
            }
            "fusionner" => {
                let a = v["a"].as_u64().map(|n| n as ThreadId);
                let b = v["b"].as_u64().map(|n| n as ThreadId);
                if let (Some(a), Some(b)) = (a, b) {
                    self.fusionner_fils(a, b);
                    self.signaler("Fils tisses ensemble.");
                }
            }
            "filFermer" => {
                if let Some(fil) = v["id"].as_u64().map(|n| n as ThreadId) {
                    let ids: Vec<TabId> = self
                        .session
                        .onglets
                        .iter()
                        .filter(|o| o.fil == fil)
                        .map(|o| o.id)
                        .collect();
                    for id in ids {
                        self.fermer_onglet(id);
                    }
                }
            }
            "trame" => self.basculer_trame(),
            "panneau" => {
                let ouvert = v["open"].as_bool().unwrap_or(false);
                // Colonne du menu des extensions (sinon : toute la largeur).
                let zone = match (v["zone"]["x"].as_f64(), v["zone"]["l"].as_f64()) {
                    (Some(x), Some(l)) if l > 10.0 => Some((x, l, v["zone"]["y"].as_f64().unwrap_or(0.0).max(0.0))),
                    _ => None,
                };
                if zone != self.panneau_zone {
                    self.panneau_zone = zone;
                    if ouvert && self.panneau_ouvert {
                        let geo = self.geometrie_courante();
                        self.superposer(&geo);
                    }
                }
                // Hauteur demandee par l'interface : elle mesure son contenu, ce qui evite un
                // grand bloc vide sous une liste courte. Bornee pour ne pas manger la page — sauf
                // le menu d'un onglet pose bas dans le rail : il descend jusqu'au bas de la fenetre
                // (la region ne decoupe que lui, la page reste visible autour).
                let posee_bas = zone.map(|z| z.2 > 0.0).unwrap_or(false);
                let plafond = if posee_bas {
                    let fenetre = self.window.inner_size().height as f64 / self.window.scale_factor();
                    fenetre.max(720.0)
                } else {
                    720.0
                };
                let hauteur = v["hauteur"]
                    .as_f64()
                    .map(|h| h.clamp(150.0, plafond))
                    .unwrap_or(self.panneau_hauteur);
                let change = ouvert != self.panneau_ouvert || (ouvert && (hauteur - self.panneau_hauteur).abs() > 1.0);
                if change {
                    self.panneau_ouvert = ouvert;
                    if ouvert {
                        self.panneau_hauteur = hauteur;
                    }
                    self.appliquer_geometrie();
                    self.pousser_etat();
                }
            }
            "rail" => {
                self.rail_replie = !self.rail_replie;
                self.appliquer_geometrie();
                self.marquer_modifie();
                self.pousser_etat();
            }
            "requete" => {
                let rid = v["rid"].as_u64().unwrap_or(0);
                let q = v["q"].as_str().unwrap_or("").to_string();
                let mut items = search::classer(
                    &q,
                    &self.session.onglets,
                    &self.session.fils,
                    &self.historique,
                    maintenant_ms(),
                );
                let propre = q.trim().to_string();
                let commandes_demandees = propre.starts_with('>');
                // « ? mots » : seulement la memoire de lecture (resultats envoyes a part).
                let lectures_seules = propre.starts_with('?');
                if lectures_seules {
                    items.clear();
                } else {
                    items.extend(self.resultats_favoris(&propre));
                    items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
                    items.truncate(14);
                }
                if !commandes_demandees {
                    self.chercher_dans_lectures(rid, propre.trim_start_matches('?').trim(), lectures_seules);
                }

                // Toujours proposer l'action de navigation en tete : taper « youtube » doit
                // offrir d'y aller (ou de chercher), pas repondre « rien trouve ». C'est le
                // role d'une barre d'adresse, et c'est ce qui manquait.
                if !propre.is_empty() && !commandes_demandees && !lectures_seules {
                    let url = urls::normaliser_avec(&propre, &self.reglages.moteur);
                    let moteur = urls::moteur(&self.reglages.moteur);
                    let prefixe = moteur.gabarit.split("{q}").next().unwrap_or("");
                    let est_recherche = !prefixe.is_empty() && url.starts_with(prefixe);
                    let titre = if est_recherche {
                        format!("Chercher « {propre} » sur {}", moteur.nom)
                    } else {
                        format!("Aller sur {}", urls::affichage_court(&url))
                    };
                    items.insert(
                        0,
                        search::Resultat {
                            genre: "navigation".to_string(),
                            id: url.clone(),
                            titre,
                            sous_titre: urls::affichage_court(&url),
                            score: 10.0,
                            action: json!({"type": "navigation", "url": url}),
                        },
                    );
                }

                // Les commandes ne s'affichent plus d'elles-memes sous une barre vide (elles
                // masquaient tout, constate le 2026-11-22) : « > » dans la barre, ou clic droit
                // sur le logo du rail (`menuCommandes`).
                if commandes_demandees {
                    let mut cmds = search::commandes(&q);
                    cmds.append(&mut items);
                    items = cmds;
                }
                let reponse = json!({
                    "rid": rid,
                    "q": q,
                    "items": items.iter().map(|r| json!({
                        "genre": r.genre, "id": r.id, "titre": r.titre,
                        "sousTitre": r.sous_titre, "action": r.action,
                    })).collect::<Vec<_>>(),
                });
                let js = format!(
                    "window.trame && window.trame.resultats({})",
                    reponse
                );
                self.eval_sur(source, &js);
            }
            "ouvrir" => {
                if let Some(action) = v.get("action") {
                    self.ouvrir_action(action);
                }
            }
            "extensionsRecharger" => {
                self.extensions = extensions::charger(&self.profil.dossier_extensions());
                self.appliquer_etat_extensions();
                self.signaler(&format!("{} extension(s) chargee(s).", self.extensions.len()));
                self.pousser_etat();
            }
            "dossierExtensions" => {
                let dossier = self.profil.dossier_extensions();
                let _ = std::process::Command::new("explorer").arg(&dossier).spawn();
                self.signaler(&format!("Dossier des extensions : {}", dossier.display()));
            }
            "dossierExtensionsChrome" => {
                let dossier = self.extensions_chrome.clone();
                let _ = std::process::Command::new("explorer").arg(&dossier).spawn();
                self.signaler(&format!("Extensions Chrome : {}", dossier.display()));
            }
            "chromeOuvrir" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                let page = v["page"].as_str().unwrap_or("").trim_start_matches('/').to_string();
                if id.is_empty() || page.is_empty() {
                    return;
                }
                let url = format!("chrome-extension://{id}/{page}");
                info!("ouverture de l'interface d'extension : {url}");
                self.ouvrir_onglet(url, Declencheur::NouvelOnglet, None);
            }
            // ---------------------------------------------------- installateur d'extensions
            //
            // Trois entrees, un seul chemin : telecharger ou lire un fichier, retirer l'en-tete
            // du CRX, decompresser, verifier le manifeste, mettre en place. WebView2 ne sait
            // rien faire de tout cela : il ne lit que des dossiers deja decompresses.
            "chromeInstallerMagasin" => {
                let saisie = v["saisie"].as_str().unwrap_or("").to_string();
                self.installer_extension_magasin(&saisie);
            }
            "chromeInstallerFichier" => {
                // Deux possibilites : un chemin deja connu (test, glisser-deposer) ou le
                // selecteur de Windows.
                let chemin = v["chemin"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .map(PathBuf::from);
                self.lancer_installation(installation::Demande::Fichier {
                    chemin,
                    source: "fichier".into(),
                });
            }
            "chromeMettreAJour" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() {
                    self.lancer_installation(installation::Demande::Magasin {
                        id,
                        source: "magasin".into(),
                    });
                }
            }
            "chromeDesactiver" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() {
                    self.desactiver_chrome(&id);
                }
            }
            "chromeReactiver" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() {
                    self.reactiver_chrome(&id);
                }
            }
            "chromeDesinstaller" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() {
                    return;
                }
                let nom = self.nom_extension_chrome(&id);
                self.noms_en_attente.insert(id.clone(), nom);
                let dossiers = self.dossiers_extensions_chrome();
                match extensions_chrome::desinstaller(&dossiers, &id) {
                    Ok(ou) => {
                        info!("extension desinstallee : {id} -> {}", ou.display());
                        // Elle quitte aussi toutes les cloisons qui l'utilisaient.
                        let fils: Vec<ThreadId> = self
                            .session
                            .fils
                            .iter_mut()
                            .filter_map(|f| {
                                let c = f.cloison.as_mut()?;
                                let avant = c.extensions.len();
                                c.extensions.retain(|e| e != &id);
                                (c.extensions.len() != avant).then_some(f.id)
                            })
                            .collect();
                        for fil in fils {
                            if let Some(w) = self.webview_de_cloison(fil) {
                                crate::moteur_ext::retirer(w, self.proxy.clone(), &id, "");
                            }
                            self.marquer_modifie();
                        }
                        self.deplier_extension_epinglee(&id);
                        self.fermer_panneau_si_extension(&id);
                        self.operer_moteur(&id, OperationMoteur::Retirer);
                    }
                    Err(e) => self.signaler(&format!("Désinstallation impossible : {e}")),
                }
                self.invalider_extensions_chrome();
                self.pousser_etat();
            }
            "chromeEpingler" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() {
                    return;
                }
                let deja = self.reglages.extensions_epinglees.iter().any(|e| e == &id);
                if deja {
                    self.reglages.extensions_epinglees.retain(|e| e != &id);
                    info!("extension depinglee : {id}");
                } else {
                    self.reglages.extensions_epinglees.push(id.clone());
                    info!("extension epinglee : {id}");
                }
                self.profil.sauver_reglages(&self.reglages);
                self.pousser_etat();
            }
            "chromePopup" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() {
                    return;
                }
                if self.panneau_extension.as_deref() == Some(id.as_str()) {
                    self.fermer_panneau_droit();
                } else {
                    self.ouvrir_popup_extension(&id);
                }
            }
            "chromeScan" => {
                self.chrome_disponibles = scanner_navigateurs(&self.extensions_chrome);
                info!(
                    "extensions Chromium trouvees : {}",
                    self.chrome_disponibles.len()
                );
                self.signaler(&format!(
                    "{} extension(s) trouvée(s) dans Chrome/Edge.",
                    self.chrome_disponibles.len()
                ));
                self.pousser_etat();
            }
            "chromeInstaller" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                let chemin = v["chemin"].as_str().unwrap_or("").to_string();
                if id.is_empty() || chemin.is_empty() {
                    return;
                }
                let destination = self.extensions_chrome.join(&id);
                let source = PathBuf::from(&chemin);
                if !source.is_dir() {
                    self.signaler("Dossier source introuvable.");
                    return;
                }
                // On ne supprime rien : on recopie par-dessus. Un fichier devenu inutile dans
                // une ancienne version peut rester, ce n'est pas grave pour une extension Chargee.
                match extensions_chrome::copier_arbre(&source, &destination) {
                    Ok(()) => {
                        info!("extension Chrome importee : {id} <- {}", source.display());
                        extensions_chrome::noter_source(&self.profil.racine().to_path_buf(), &id, "import");
                        self.invalider_extensions_chrome();
                        self.operer_moteur(&id, OperationMoteur::Charger);
                    }
                    Err(e) => {
                        alerte!("import de {id} impossible : {e}");
                        self.signaler(&format!("Import impossible : {e}"));
                    }
                }
                self.chrome_disponibles = scanner_navigateurs(&self.extensions_chrome);
                self.pousser_etat();
            }
            "moteur" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() {
                    self.reglages.moteur = id.clone();
                    self.profil.sauver_reglages(&self.reglages);
                    let nom = urls::moteur(&id).nom;
                    info!("moteur de recherche choisi : {nom}");
                    self.signaler(&format!("Moteur de recherche : {nom}"));
                    self.pousser_etat();
                }
            }
            "barreExtensions" => {
                let visible = v["visible"].as_bool().unwrap_or(true);
                if visible != self.reglages.barre_extensions {
                    self.reglages.barre_extensions = visible;
                    self.profil.sauver_reglages(&self.reglages);
                    self.appliquer_geometrie();
                    self.pousser_etat();
                }
            }
            "pleinEcran" => {
                if let Some(mode) = v["mode"].as_str().filter(|m| ["page", "fenetre", "ecran"].contains(m)) {
                    self.reglages.plein_ecran = mode.to_string();
                    self.profil.sauver_reglages(&self.reglages);
                    self.pousser_etat();
                }
            }
            "veilleAuto" => {
                self.reglages.veille_auto = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                self.signaler(&format!(
                    "Mise en veille automatique : {}",
                    if self.reglages.veille_auto { "activée" } else { "désactivée" }
                ));
                self.pousser_etat();
            }
            "filCloisonner" => {
                let fil = v["fil"].as_u64().unwrap_or(0) as ThreadId;
                let oui = v["actif"].as_bool().unwrap_or(true);
                if fil != 0 {
                    self.cloisonner_fil(fil, oui);
                }
            }
            "cloisonExtension" => {
                let fil = v["fil"].as_u64().unwrap_or(0) as ThreadId;
                let id = v["id"].as_str().unwrap_or("").to_string();
                let oui = v["actif"].as_bool().unwrap_or(true);
                if fil != 0 && !id.is_empty() {
                    self.cloison_extension(fil, &id, oui);
                }
            }
            "navigateurParDefaut" => {
                // Windows interdit qu'un programme se declare lui-meme navigateur par defaut : on
                // ouvre la page des Parametres consacree a Trame, ou l'utilisateur choisit.
                info!("ouverture des Parametres de Windows (applications par defaut)");
                // La page de Trame d'abord ; si Windows ne la connait pas, la liste generale.
                if !crate::externe::ouvrir_uri("ms-settings:defaultapps?registeredAppUser=Trame")
                    && !crate::externe::ouvrir_uri("ms-settings:defaultapps")
                {
                    alerte!("Parametres de Windows : ouverture impossible");
                }
                self.signaler("Dans les Paramètres, cliquez « Définir par défaut » en haut de la page de Trame.");
            }
            "etatParDefaut" => self.verifier_par_defaut(),
            "confidentialite" => {
                // Un seul message pour les reglages de confidentialite et de pause : chaque champ
                // present est applique et enregistre.
                if let Some(b) = v["nettoyerAdresses"].as_bool() {
                    self.reglages.nettoyer_adresses = b;
                }
                if let Some(b) = v["garderAffiliation"].as_bool() {
                    self.reglages.garder_affiliation = b;
                }
                if let Some(b) = v["pauseOnglets"].as_bool() {
                    self.reglages.pause_onglets = b;
                    if !b {
                        // Tout ce qui etait en pause reprend.
                        for id in self.en_pause.drain().collect::<Vec<_>>() {
                            if let Some(w) = self.webviews.get(&id) {
                                crate::moteur_ext::reprendre(w);
                            }
                        }
                        self.pause_demandee.clear();
                    }
                }
                if let Some(n) = v["antiPistage"].as_str() {
                    if matches!(n, "aucune" | "equilibree" | "stricte") {
                        self.reglages.anti_pistage = n.to_string();
                        self.appliquer_anti_pistage(None);
                        // Les cloisons vivantes aussi.
                        let vues: Vec<TabId> = self
                            .session
                            .onglets
                            .iter()
                            .filter(|o| self.cloison_du_fil(o.fil).is_some())
                            .map(|o| o.id)
                            .collect();
                        for id in vues {
                            if let Some(w) = self.webviews.get(&id) {
                                let _ = crate::moteur_ext::niveau_pistage(w, n);
                            }
                        }
                    }
                }
                self.appliquer_nettoyage();
                self.profil.sauver_reglages(&self.reglages);
                info!(
                    "confidentialite : nettoyage {} (affiliation {}), anti-pistage {}, pause {}",
                    self.reglages.nettoyer_adresses,
                    self.reglages.garder_affiliation,
                    self.reglages.anti_pistage,
                    self.reglages.pause_onglets
                );
                self.pousser_etat();
            }
            "budgetAuto" => {
                self.reglages.budget_auto = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                self.budget_memoire = None;
                self.signaler(&format!(
                    "Onglets ouverts selon la mémoire libre : {}",
                    if self.reglages.budget_auto { "activé" } else { "désactivé" }
                ));
                self.pousser_etat();
            }
            "memoireBasse" => {
                self.reglages.memoire_basse = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                // Coupee : tout repasse en normal tout de suite ; allumee : au prochain delai.
                self.ajuster_memoire(maintenant_ms());
                self.signaler(&format!(
                    "Mémoire basse des onglets cachés : {}",
                    if self.reglages.memoire_basse { "activée" } else { "désactivée" }
                ));
                self.pousser_etat();
            }
            "budget" => {
                if let Some(n) = v["valeur"].as_u64() {
                    let avant = self.reglages.budget_borne();
                    self.reglages.budget = (n as u32).clamp(2, 16);
                    self.profil.sauver_reglages(&self.reglages);
                    info!("budget de webviews : {avant} -> {}", self.reglages.budget_borne());
                    self.pousser_etat();
                }
            }
            "ouvrirReglages" => self.ouvrir_reglages(),
            "zap" => self.basculer_zap(),
            "zapChoisir" => self.demarrer_selecteur(),
            "zapChoisi" if self.selection_guet => {
                self.selection_guet = false;
                let selecteur = v["selecteur"].as_str().unwrap_or("").to_string();
                if let Source::Onglet(id) = source {
                    self.lire_element_pour_guet(id, selecteur);
                }
            }
            "zapChoisi" => {
                let selecteur = v["selecteur"].as_str().unwrap_or("").to_string();
                let domaine = match source {
                    Source::Onglet(id) => self.trouver(id).map(|o| o.domaine()).unwrap_or_default(),
                    _ => self.domaine_actif(),
                };
                // Le libelle vient de la page : borne, sans retour a la ligne.
                let libelle: String = v["libelle"]
                    .as_str()
                    .unwrap_or("choisi dans la page")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(80)
                    .collect();
                let texte: String = v["texte"].as_str().unwrap_or("").trim().chars().take(80).collect();
                // Action et parametre viennent aussi de la page : action connue seulement, valeur
                // bornee et sans caractere de controle (le CSS de `style` est filtre a part).
                let action = v["action"].as_str().unwrap_or("").to_string();
                if !crate::model::ACTIONS_ZAP.iter().any(|(a, _)| *a == action) {
                    return;
                }
                let valeur: String = v["valeur"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(200)
                    .collect();
                self.ajouter_regle_zap_action(domaine, selecteur, libelle, texte, action, valeur);
            }
            "zapAnnuler" => self.annuler_derniere_regle_zap(),
            "telechargementPause" | "telechargementReprendre" | "telechargementAnnuler"
            | "telechargementOuvrir" | "telechargementDossier" | "telechargementRelancer"
            | "telechargementRetirer" | "telechargementsEffacer" | "telechargementsDossier"
            | "telechargementsChoisirDossier" | "telechargementsDossierWindows"
            | "ouvrirTelechargements" => self.commande_telechargement(m, &v),
            "majVerifier" => self.verifier_maj(),
            "majInstaller" => self.installer_maj(),
            "theme" => {
                let voulu = v["valeur"].as_str().unwrap_or("defaut").to_string();
                if voulu != self.reglages.theme {
                    info!("apparence : teinte {voulu}");
                    self.reglages.theme = voulu;
                    self.profil.sauver_reglages(&self.reglages);
                    self.pousser_etat();
                }
            }
            "muet" => {
                if let Some(id) = v["id"].as_u64() {
                    self.basculer_muet(id as TabId);
                }
            }
            "zapOrigine" => {
                // Maintenir « voir la page d'origine » : les regles s'effacent le temps de l'appui.
                let voir = v["voir"].as_bool().unwrap_or(false);
                if let Some(id) = self.actif {
                    self.eval_sur(
                        Source::Onglet(id),
                        &format!("window.__trame_zap_vivant && window.__trame_zap_vivant.origine({voir})"),
                    );
                }
            }
            "zapCompte" => {
                // Le script vivant d'un onglet dit combien d'elements chaque regle attrape.
                // Seul le domaine de l'onglet emetteur compte : une page ne parle que d'elle.
                if let Source::Onglet(id) = source {
                    self.recevoir_compte_zap(id, &v["comptes"]);
                }
            }
            "zapAjouter" => {
                let selecteur = v["selecteur"].as_str().unwrap_or("").to_string();
                let domaine = v["domaine"]
                    .as_str()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| self.domaine_actif());
                self.ajouter_regle_zap(domaine, selecteur, "saisi".to_string(), String::new());
            }
            "zapSupprimer" => {
                if let Some(index) = v["index"].as_u64().map(|n| n as usize) {
                    if index < self.zap_regles.len() {
                        let retiree = self.zap_regles.remove(index);
                        self.profil.sauver_zap(&self.zap_regles);
                        info!("zappeur : regle supprimee {}", retiree.selecteur);
                        self.pousser_etat();
                        if let Some(id) = self.actif {
                            self.recharger_onglet(id);
                        }
                    }
                }
            }
            "zapBasculer" => {
                if let Some(index) = v["index"].as_u64().map(|n| n as usize) {
                    if let Some(regle) = self.zap_regles.get_mut(index) {
                        regle.actif = !regle.actif;
                        let actif = regle.actif;
                        self.profil.sauver_zap(&self.zap_regles);
                        self.pousser_etat();
                        if let Some(id) = self.actif {
                            self.recharger_onglet(id);
                        }
                        info!("zappeur : regle {} {}", index, if actif { "activee" } else { "coupee" });
                    }
                }
            }
            "zapAppliquer" => {
                if let Some(id) = self.actif {
                    self.recharger_onglet(id);
                }
            }
            "zapCopier" => {
                // Copie les regles d'un site vers un autre (« Copier vers… » du panneau).
                let de = v["de"].as_str().unwrap_or("").to_string();
                let Some(vers) = crate::model::domaine_saisi(v["vers"].as_str().unwrap_or("")) else {
                    self.signaler("Ce n'est pas un nom de site (exemple : youtube.com).");
                    return;
                };
                if de.is_empty() || de == vers {
                    return;
                }
                let n = crate::model::copier_regles_zap(&mut self.zap_regles, &de, &vers);
                if n > 0 {
                    self.profil.sauver_zap(&self.zap_regles);
                }
                info!("zappeur : {n} regle(s) copiee(s) de {de} vers {vers}");
                self.signaler(&format!("{n} règle(s) copiée(s) de {de} vers {vers}."));
                self.pousser_etat();
            }
            "zapImporter" => {
                // Regles collees sous forme de texte (export d'un autre Trame, ou de ce profil).
                let texte = v["texte"].as_str().unwrap_or("");
                if texte.len() > 400_000 {
                    self.signaler("Texte trop long pour des règles.");
                    return;
                }
                match crate::model::regles_importees(texte, &self.domaine_actif()) {
                    Ok(importees) => {
                        let mut n = 0;
                        for r in importees {
                            let doublon = self.zap_regles.iter().any(|e| {
                                e.domaine == r.domaine && e.selecteur == r.selecteur && e.texte == r.texte && e.action == r.action && e.valeur == r.valeur
                            });
                            if !doublon {
                                self.zap_regles.push(r);
                                n += 1;
                            }
                        }
                        if n > 0 {
                            self.profil.sauver_zap(&self.zap_regles);
                        }
                        info!("zappeur : {n} regle(s) importee(s)");
                        self.signaler(&format!("{n} règle(s) importée(s)."));
                        self.pousser_etat();
                        if n > 0 {
                            if let Some(id) = self.actif {
                                self.recharger_onglet(id);
                            }
                        }
                    }
                    Err(e) => self.signaler(&format!("Import impossible : {e}")),
                }
            }
            "zapPaquet" => {
                // Allume ou coupe toutes les regles d'un paquet ; s'il n'en reste aucune (toutes
                // supprimees), le paquet est remis en entier.
                let id = v["id"].as_str().unwrap_or("").to_string();
                let actif = v["actif"].as_bool().unwrap_or(true);
                let mut touchees = 0;
                for r in self.zap_regles.iter_mut().filter(|r| r.paquet == id) {
                    r.actif = actif;
                    touchees += 1;
                }
                if touchees == 0 && actif {
                    if let Some(p) = crate::model::paquets_zap().into_iter().find(|p| p.id == id) {
                        touchees = p.regles.len();
                        self.zap_regles.extend(p.regles);
                    }
                }
                if touchees > 0 {
                    self.profil.sauver_zap(&self.zap_regles);
                    info!("zappeur : paquet {id} {} ({touchees} regle(s))", if actif { "active" } else { "coupe" });
                    self.pousser_etat();
                    if let Some(onglet) = self.actif {
                        self.recharger_onglet(onglet);
                    }
                }
            }
            "profilDossier" => {
                let dossier = self.profil.racine().to_path_buf();
                let _ = std::process::Command::new("explorer").arg(&dossier).spawn();
                self.signaler(&format!("Profil : {}", dossier.display()));
            }
            // ---- sauvegarde et restauration du profil (voir `sauvegarde.rs`)
            "sauvegardeEnregistrer" => {
                let contenus: Vec<String> = v["contenus"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|c| c.as_str().map(str::to_string))
                            .filter(|c| crate::sauvegarde::contenu(c).is_some())
                            .collect()
                    })
                    .unwrap_or_else(crate::sauvegarde::cles_par_defaut);
                self.lancer_sauvegarde(crate::sauvegarde::Demande::Sauver {
                    chemin: None,
                    contenus,
                });
            }
            "sauvegardeRestaurer" => {
                self.lancer_sauvegarde(crate::sauvegarde::Demande::Restaurer);
            }
            "sauvegardeRestaurerConfirmer" => {
                let Some((archive, _, contenus)) = self.restauration.clone() else { return };
                match crate::sauvegarde::demander(self.profil.racine(), &archive, &contenus) {
                    Ok(()) => {
                        // On ne restaure pas sous les pieds du moteur : la commande ecrit la
                        // demande, Trame se ferme, et le demarrage suivant applique l'archive
                        // avant de lire la session. Sans cela, la sauvegarde de sortie
                        // reecrirait `session.json` par-dessus la restauration.
                        info!(
                            "restauration demandee ({} contenus) : Trame se ferme, l'archive \
                             s'appliquera au prochain demarrage",
                            contenus.len()
                        );
                        self.quitter = true;
                    }
                    Err(e) => {
                        alerte!("restauration : demande impossible ({e})");
                        self.signaler(&format!("Restauration impossible : {e}"));
                    }
                }
            }
            "sauvegardeRestaurerAnnuler" => {
                self.restauration = None;
                self.pousser_etat();
            }
            "favorisExporterHtml" => {
                self.lancer_sauvegarde(crate::sauvegarde::Demande::ExporterFavoris {
                    chemin: None,
                    favoris: self.favoris.clone(),
                });
            }
            "favorisImporterHtml" => {
                self.lancer_sauvegarde(crate::sauvegarde::Demande::ImporterFavoris { chemin: None });
            }
            "extensionReinitialiser" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() {
                    if let Some(racine) = self.ext_storage.as_object_mut() {
                        racine.remove(&id);
                    }
                    self.profil.sauver_ext_storage(&self.ext_storage);
                    info!("extension {id} : stockage efface");
                    self.signaler(&format!(
                        "Données de l'extension {id} effacées — rechargez la page (Ctrl+R)."
                    ));
                    self.pousser_etat();
                }
            }
            "extensionBasculer" => {
                let id = v["id"].as_str().unwrap_or("").to_string();
                let actif = v["actif"].as_bool().unwrap_or(true);
                self.basculer_extension(&id, actif);
            }
            "sceller" => {
                let fil = v["fil"]
                    .as_u64()
                    .map(|n| n as ThreadId)
                    .or_else(|| self.actif.and_then(|id| self.trouver(id)).map(|o| o.fil));
                if let Some(fil) = fil {
                    self.sceller_fil(fil);
                }
            }
            "capsuleOuvrir" => {
                if let Some(chemin) = v["chemin"].as_str() {
                    self.ouvrir_capsule(chemin);
                }
            }
            "devtools" => {
                #[cfg(debug_assertions)]
                if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                    w.open_devtools();
                }
                #[cfg(not(debug_assertions))]
                self.signaler("Inspecteur disponible en build de developpement uniquement.");
            }
            "historiqueVider" => {
                self.historique.clear();
                self.profil.reecrire_historique(&[]);
                self.signaler("Historique vide.");
                self.pousser_etat();
            }
            "raccourci" => {
                let action = v["action"].as_str().unwrap_or("").to_string();
                info!("raccourci recu : {action} (depuis {:?})", source);
                // Depuis la mini-fenetre, les raccourcis viseraient l'onglet actif de la fenetre
                // principale : seul Ctrl+W (fermer cet onglet) y garde un sens.
                match source {
                    Source::Onglet(id) if self.onglet_flottant() == Some(id) => {
                        if action == "fermerOnglet" {
                            self.fermer_onglet(id);
                        }
                    }
                    _ => self.raccourci(&action),
                }
            }
            "erreurUi" => {
                let message = v["message"].as_str().unwrap_or("");
                let source = v["source"].as_str().unwrap_or("");
                let ligne = v["ligne"].as_u64().unwrap_or(0);
                alerte!("erreur JavaScript dans l'interface : {message} ({source}:{ligne})");
            }
            "defilement" => {
                if let Source::Onglet(id) = source {
                    let y = v["y"].as_f64().unwrap_or(0.0);
                    if let Some(o) = self.onglet_mut(id) {
                        o.defilement = y;
                    }
                    self.lier_defilement(id, y);
                }
            }
            "coupDoeil" => {
                let url = v["url"].as_str().unwrap_or("").to_string();
                if url.starts_with("http://") || url.starts_with("https://") {
                    self.ouvrir_apercu(&url);
                }
            }
            "flottant" => self.traiter_ipc_flottant(source, &v),
            "apercuFermer" => self.fermer_apercu(),
            "apercuOuvrir" => {
                let url = self.apercu_url.clone();
                self.fermer_apercu();
                if !url.is_empty() {
                    info!("coup d'oeil garde comme onglet : {}", urls::affichage_court(&url));
                    let parent = self.actif;
                    self.ouvrir_onglet(url, Declencheur::Lien, parent);
                }
            }
            "clicPage" => {
                // Un clic dans l'autre page du cote a cote la rend active (barre d'adresse,
                // raccourcis), sans rien deplacer.
                if let Source::Onglet(id) = source {
                    if Some(id) != self.actif && self.dans_cote_affiche(id) {
                        self.activer(id);
                    }
                }
            }
            "extCall" => {
                let rid = v["rid"].as_str().unwrap_or("").to_string();
                let ext = v["ext"].as_str().unwrap_or("").to_string();
                let op = v["op"].as_str().unwrap_or("").to_string();
                let payload = v["payload"].clone();
                let valeur = self.extension_stockage(&ext, &op, &payload);
                let js = format!(
                    "window.__trame_reponse && window.__trame_reponse({}, {})",
                    serde_json::to_string(&rid).unwrap_or_else(|_| "\"\"".to_string()),
                    valeur
                );
                self.eval_sur(source, &js);
            }
            _ => {}
        }
    }

    /// Verifie que les surfaces ont bien demarre (une seule fois, en journal).
    fn verifier_surfaces(&mut self) {
        let Some(w) = self.rail.as_ref() else { return };
        let js = r#"JSON.stringify({amorce: typeof window.__trame_amorce, ipc: typeof window.ipc, trame: typeof window.trame})"#;
        let proxy = self.proxy.clone();
        let _ = w.evaluate_script_with_callback(js, move |v| {
            let _ = proxy.send_event(UserEvent::Log(format!("interface : {v}")));
        });
    }

    fn source_interne(&self, source: Source) -> bool {
        match source {
            Source::Onglet(id) => self.trouver(id).map(|o| o.interne()).unwrap_or(false),
            Source::Apercu => false,
            _ => true,
        }
    }

    fn ouvrir_action(&mut self, action: &Value) {
        match action["type"].as_str().unwrap_or("") {
            "onglet" => {
                if let Some(id) = action["id"].as_u64() {
                    self.activer(id as TabId);
                }
            }
            "fil" => {
                if let Some(fil) = action["id"].as_u64() {
                    let cible = self
                        .session
                        .onglets
                        .iter()
                        .filter(|o| o.fil == fil as ThreadId)
                        .max_by(|a, b| {
                            a.vitalite
                                .partial_cmp(&b.vitalite)
                                .unwrap_or(Ordering::Equal)
                        })
                        .map(|o| o.id);
                    if let Some(id) = cible {
                        self.activer(id);
                    }
                }
            }
            "historique" | "favori" | "lecture" => {
                if let Some(url) = action["url"].as_str() {
                    self.naviguer_actif(url);
                }
            }
            "navigation" => {
                if let Some(url) = action["url"].as_str() {
                    self.naviguer_actif(url);
                }
            }
            "commande" => {
                if let Some(id) = action["id"].as_str() {
                    self.commande(id);
                }
            }
            _ => {}
        }
    }

    fn commande(&mut self, id: &str) {
        match id {
            "nouvel-onglet" => {
                self.ouvrir_onglet(
                    urls::NOUVEL_ONGLET.to_string(),
                    Declencheur::NouvelOnglet,
                    None,
                );
            }
            "fermer-onglet" => {
                if let Some(id) = self.actif {
                    self.fermer_onglet(id);
                }
            }
            "rouvrir" => self.rouvrir_ferme(),
            "reglages" => self.ouvrir_reglages(),
            "telechargements" => self.ouvrir_page_interne(PAGE_TELECHARGEMENTS),
            "zap" => self.basculer_zap(),
            "trame" => self.basculer_trame(),
            "veille" => {
                let ids: Vec<TabId> = self
                    .session
                    .onglets
                    .iter()
                    .filter(|o| o.eveille && Some(o.id) != self.actif)
                    .map(|o| o.id)
                    .collect();
                let mut n = 0;
                for id in ids {
                    if self.endormir(id) {
                        n += 1;
                    }
                }
                self.signaler(&format!("{n} onglet(s) en veille."));
                self.pousser_etat();
            }
            "capsule" => {
                if let Some(fil) = self.actif.and_then(|id| self.trouver(id)).map(|o| o.fil) {
                    self.sceller_fil(fil);
                }
            }
            "capsules" => {
                let dossier = self.profil.dossier_capsules();
                let _ = std::process::Command::new("explorer").arg(&dossier).spawn();
                self.signaler(&format!("Capsules : {}", dossier.display()));
            }
            "extensions" => {
                self.extensions = extensions::charger(&self.profil.dossier_extensions());
                self.appliquer_etat_extensions();
                self.signaler(&format!("{} extension(s).", self.extensions.len()));
                self.pousser_etat();
            }
            "rail" => {
                self.rail_replie = !self.rail_replie;
                self.appliquer_geometrie();
                self.pousser_etat();
            }
            "devtools" => {
                let _ = self.traiter_json(json!({"m": "devtools"}));
            }
            "anchrer" => {
                let _ = self.traiter_json(json!({"m": "ancrer", "actif": true}));
            }
            "alire" => {
                let _ = self.traiter_json(json!({"m": "alire", "actif": true}));
            }
            "vider-historique" => {
                let _ = self.traiter_json(json!({"m": "historiqueVider"}));
            }
            _ => {}
        }
    }

    /// Rejoue une commande JSON comme si elle venait de l'interface.
    fn traiter_json(&mut self, v: Value) -> Option<()> {
        self.traiter_ipc(Source::Rail, &v.to_string());
        Some(())
    }

    /// Zoom du texte et de la page, par onglet.
    ///
    /// `WebView2::SetZoomFactor` existe et wry l'expose : inutile de bricoler un `zoom` CSS,
    /// qui casserait la mise en page des sites. Le facteur est garde **par onglet** (relire
    /// une page de documentation grossie ne doit pas grossir la banque en ligne).
    fn zoom_pas(&mut self, sens: i32) {
        let Some(id) = self.actif else { return };
        let actuel = *self.zooms.get(&id).unwrap_or(&1.0);
        let suivant = zoom_suivant(actuel, sens);
        self.appliquer_zoom(id, suivant);
    }

    fn zoom_reinitialiser(&mut self) {
        if let Some(id) = self.actif {
            self.appliquer_zoom(id, 1.0);
        }
    }

    fn appliquer_zoom(&mut self, id: TabId, facteur: f64) {
        let facteur = zoom_borne(facteur);
        if (facteur - 1.0).abs() < 0.001 {
            self.zooms.remove(&id);
        } else {
            self.zooms.insert(id, facteur);
        }
        if let Some(w) = self.webviews.get(&id) {
            if let Err(e) = w.zoom(facteur) {
                alerte!("zoom impossible : {e}");
                return;
            }
        }
        info!("zoom onglet {id} : {} %", (facteur * 100.0).round());
        self.signaler(&format!("Zoom : {} %", (facteur * 100.0).round()));
    }

    /// Barre de recherche dans la page (Ctrl+F) : injectee dans l'onglet actif.
    ///
    /// Le code vit dans `ui/recherche.js`, embarque a la compilation pour ne pas dependre du
    /// dossier `ui/` au moment du raccourci. Il ne s'installe que sur une vraie page web :
    /// l'injecter dans l'accueil ou dans une popup d'extension n'aurait aucun sens.
    fn chercher_dans_page(&mut self) {
        let Some(id) = self.actif else { return };
        let Some(onglet) = self.trouver(id) else { return };
        if onglet.interne() || onglet.url.starts_with("chrome-extension:") {
            self.signaler("La recherche dans la page s'applique aux pages web.");
            return;
        }
        if self.vue_trame {
            return;
        }
        let Some(w) = self.webviews.get(&id) else {
            self.signaler("Cet onglet est en veille : un clic le réveille d'abord.");
            return;
        };
        if let Err(e) = w.evaluate_script(RECHERCHE_PAGE) {
            alerte!("recherche dans la page impossible : {e}");
            return;
        }
        if let Some(w) = self.webviews.get(&id) {
            let _ = w.focus();
        }
    }

    fn raccourci(&mut self, action: &str) {
        match action {
            "nouvelOnglet" => {
                self.ouvrir_onglet(
                    urls::NOUVEL_ONGLET.to_string(),
                    Declencheur::NouvelOnglet,
                    None,
                );
            }
            "fermerOnglet" => {
                if let Some(id) = self.actif {
                    self.fermer_onglet(id);
                }
            }
            "reouverture" => self.rouvrir_ferme(),
            "zoomPlus" => self.zoom_pas(1),
            "zoomMoins" => self.zoom_pas(-1),
            "zoomZero" => self.zoom_reinitialiser(),
            "chercherPage" => self.chercher_dans_page(),
            "favori" => self.basculer_favori(),
            "imprimer" => self.imprimer(),
            "recharger" => {
                if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                    let _ = w.reload();
                }
            }
            "arret" => {
                // Pas d'API d'arret propre dans wry : on recharge la page courante, ce qui
                // interrompt un chargement en cours sans perdre l'onglet.
                if let Some(o) = self.actif.and_then(|id| self.trouver(id)) {
                    if o.chargement {
                        let url = o.url.clone();
                        if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                            let _ = charger(w, &url);
                        }
                    }
                }
            }
            "couture" => {
                if let Some(w) = &self.barre {
                    let _ = w.evaluate_script("window.trame && window.trame.focusCouture()");
                    let _ = w.focus();
                }
            }
            "trame" => self.basculer_trame(),
            "rail" => {
                self.rail_replie = !self.rail_replie;
                self.appliquer_geometrie();
                self.pousser_etat();
            }
            "devtools" => {
                #[cfg(debug_assertions)]
                if let Some(w) = self.actif.and_then(|id| self.webviews.get(&id)) {
                    w.open_devtools();
                }
            }
            "reglages" => self.ouvrir_reglages(),
            "telechargements" => self.ouvrir_page_interne(PAGE_TELECHARGEMENTS),
            "ongletSuivant" | "ongletPrecedent" => {
                let ids: Vec<TabId> = self.session.onglets.iter().map(|o| o.id).collect();
                if ids.len() < 2 {
                    return;
                }
                let courant = self
                    .actif
                    .and_then(|id| ids.iter().position(|x| *x == id))
                    .unwrap_or(0);
                let suivant = if action == "ongletSuivant" {
                    (courant + 1) % ids.len()
                } else {
                    (courant + ids.len() - 1) % ids.len()
                };
                self.activer(ids[suivant]);
            }
            autre => {
                if let Some(n) = autre.strip_prefix("onglet:") {
                    if let Ok(index) = n.parse::<usize>() {
                        if let Some(id) = self.session.onglets.get(index.saturating_sub(1)).map(|o| o.id) {
                            self.activer(id);
                        }
                    }
                }
            }
        }
    }

    /// Ouvre la page de reglages : dans l'onglet courant s'il est deja interne, sinon
    /// dans un nouvel onglet — comme le fait un navigateur pour ses pages internes.
    fn ouvrir_reglages(&mut self) {
        self.ouvrir_page_interne(PAGE_REGLAGES);
    }

    /// Ouvre une page interne : dans l'onglet affiche s'il est deja une page interne, sinon dans
    /// un nouvel onglet. Si un onglet montre deja cette page, on y va simplement.
    fn ouvrir_page_interne(&mut self, page: &str) {
        if let Some(id) = self.session.onglets.iter().find(|o| o.url == page).map(|o| o.id) {
            self.activer(id);
            return;
        }
        let deja_interne = self
            .actif
            .and_then(|id| self.trouver(id))
            .map(|o| o.interne())
            .unwrap_or(false);
        info!("page interne {page} : onglet actif {:?}, {}", self.actif, if deja_interne { "navigation sur place" } else { "nouvel onglet" });
        if deja_interne {
            self.naviguer_actif(page);
        } else {
            self.ouvrir_onglet(page.to_string(), Declencheur::NouvelOnglet, None);
        }
    }

    fn creer_vue_zap(&mut self, profil: Option<String>) -> Option<WebView> {
        let racine_handler = self.racine_ui();
        let geo = self.geometrie_courante();
        let proxy = self.proxy.clone();
        let p_fenetre = self.proxy.clone();
        let garde = self.garde_navigation();
        let mut contexte = verrou_contexte();
        let builder = self.habiller(WebViewBuilder::new_with_web_context(&mut contexte));
        let builder = match profil {
            Some(p) => {
                use wry::WebViewBuilderExtWindows;
                builder.with_profile_name(p)
            }
            None => builder,
        };
        let resultat = builder
            .with_url("trame://localhost/zap.html")
            .with_bounds(geo.zap)
            .with_background_color(COULEUR_FOND)
            .with_visible(false)
            .with_focused(false)
            .with_navigation_handler(garde)
            // Une popup d'extension ouverte ici peut demander `window.open` : sans ce
            // gestionnaire, WebView2 laisse la demande filer et la webview finit sur une page
            // interne morte. On la refuse, et on ouvre l'adresse dans un onglet.
            .with_new_window_req_handler(move |url, f| {
                if fenetre_extension(&url, f.size.is_some()) {
                    return NewWindowResponse::Allow;
                }
                let _ = p_fenetre.send_event(UserEvent::NouvelleFenetre {
                    onglet: 0,
                    url,
                });
                NewWindowResponse::Deny
            })
            .with_custom_protocol("trame".to_string(), move |_id, req| {
                protocol::repondre(&racine_handler, &req.uri().to_string())
            })
            .with_ipc_handler(move |req| {
                let _ = proxy.send_event(UserEvent::Ipc {
                    source: Source::Zap,
                    corps: req.body().clone(),
                });
            })
            .build_as_child(&self.window);
        match resultat {
            Ok(w) => Some(w),
            Err(e) => {
                alerte!("panneau des regles : creation impossible ({e})");
                None
            }
        }
    }

    // ---------------------------------------------------------------- zappeur

    fn domaine_actif(&self) -> String {
        self.actif
            .and_then(|id| self.trouver(id))
            .map(|o| o.domaine())
            .unwrap_or_default()
    }

    /// CSS a appliquer sur une page : les regles de son domaine, plus les regles globales.
    fn css_zap_pour(&self, domaine: &str) -> Option<String> {
        let mut css = String::new();
        for r in self
            .zap_regles
            .iter()
            .filter(|r| r.actif && !r.selecteur.trim().is_empty())
        {
            if r.domaine.is_empty() || r.domaine == domaine {
                css.push_str(&r.css());
                css.push('\n');
            }
        }
        if css.is_empty() {
            None
        } else {
            Some(css)
        }
    }

    /// Pose (ou remplace) la feuille de masquage dans un onglet, puis le script vivant : il
    /// cache les regles a condition de texte et compte ce que chaque regle attrape, y compris
    /// dans ce que la page charge ensuite (YouTube charge en continu).
    fn appliquer_zap(&mut self, id: TabId) {
        let Some(onglet) = self.trouver(id) else { return };
        if !onglet.url.starts_with("http") {
            return;
        }
        let domaine = onglet.domaine();
        self.zap_comptes.remove(&id);
        let css = self.css_zap_pour(&domaine).unwrap_or_default();
        let regles: Vec<Value> = self
            .zap_regles
            .iter()
            .enumerate()
            .filter(|(_, r)| r.actif && (r.domaine.is_empty() || r.domaine == domaine))
            .filter(|(_, r)| !r.selecteur_sur().is_empty())
            .map(|(i, r)| json!({ "i": i, "s": r.selecteur_sur(), "t": r.texte, "a": r.action, "v": if r.action == "texte" { r.valeur.as_str() } else { "" } }))
            .collect();
        if css.is_empty() && regles.is_empty() {
            return;
        }
        let js = format!(
            r#"(function(){{var s=document.querySelector('style[data-trame-zap]');if(!s){{s=document.createElement('style');s.setAttribute('data-trame-zap','1');(document.head||document.documentElement).appendChild(s);}}s.textContent={css};}})();
{vivant}({regles});"#,
            css = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_string()),
            vivant = ZAP_VIVANT_JS,
            regles = Value::Array(regles),
        );
        self.eval_sur(Source::Onglet(id), &js);
    }

    /// Ou vont les telechargements : le dossier choisi, sinon « Telechargements » de Windows,
    /// sinon (introuvable) l'ancien dossier du profil.
    fn dossier_telechargements(&self) -> PathBuf {
        let choisi = self.reglages.dossier_telechargements.trim();
        if !choisi.is_empty() {
            return PathBuf::from(choisi);
        }
        crate::externe::dossier_telechargements_windows()
            .unwrap_or_else(|| self.profil.dossier_telechargements())
    }

    /// Un telechargement commence : il entre en tete de liste, et l'interface le montre.
    fn telechargement_debut(&mut self, mut t: crate::telechargements::Telechargement) {
        // Le site du fichier lui-meme : celui de l'onglet a sa creation peut etre la page
        // d'accueil interne (« localhost ») ou un site quitte depuis.
        if let Ok(u) = url::Url::parse(&t.url) {
            if let Some(hote) = u.host_str() {
                t.site = hote.trim_start_matches("www.").to_string();
            }
        }
        info!("telechargement : {} -> {}", t.url, t.chemin);
        self.signaler(&format!("Téléchargement : {}", t.fichier));
        self.telechargements.push(t);
        self.profil.sauver_telechargements(&self.telechargements);
        self.pousser_etat();
    }

    /// Le moteur signale un changement d'etat (fin, echec, annulation, pause).
    fn telechargement_etat(&mut self, id: u64, termine: bool, interrompu: bool, reprenable: bool, raison: i32, recu: u64) {
        let Some(t) = self.telechargements.iter_mut().find(|t| t.id == id) else { return };
        // Deja clos : le moteur emet parfois un second evenement apres notre propre deduction
        // (voir `suivre_telechargements`). Il ne doit ni rejouer le toast, ni reinstaller un
        // paquet du Chrome Web Store, ni redater la fin.
        if termine && t.etat == "termine" {
            return;
        }
        t.recu = recu.max(t.recu);
        if termine {
            t.etat = "termine".into();
            t.fin = maintenant_ms();
            if t.total.is_none() {
                t.total = Some(t.recu);
            }
        } else if interrompu {
            // Pause demandee : l'etat reste « en pause » tant qu'on peut reprendre.
            if t.etat == "en_pause" && reprenable {
                return;
            }
            // Annulation demandee par l'utilisateur : le moteur la signale parfois avec un autre
            // motif (vu le 2026-11-22 : « le serveur a refusé ») ; elle reste une annulation.
            if t.etat == "annule" {
                return;
            }
            t.etat = if raison == 40 { "annule".into() } else { "echoue".into() };
            t.raison = crate::telechargements::raison_lisible(raison);
            t.fin = maintenant_ms();
        } else {
            if t.etat == "en_pause" {
                return;
            }
            t.etat = "en_cours".into();
        }
        let fini = !t.actif();
        let (url, chemin, nom, etat, raison_txt) = (t.url.clone(), t.chemin.clone(), t.fichier.clone(), t.etat.clone(), t.raison.clone());
        if fini {
            crate::telechargements::oublier(id);
        }
        self.profil.sauver_telechargements(&self.telechargements);
        self.pousser_etat();
        match etat.as_str() {
            "termine" => {
                // Paquet du Chrome Web Store : Trame l'installe (voir `suite_telechargement`).
                self.traiter(UserEvent::Telechargement { url, chemin: Some(chemin), reussi: true });
                info!("telechargement termine : {nom}");
            }
            "echoue" => {
                alerte!("telechargement echoue : {url} ({raison_txt})");
                self.signaler(&format!("Téléchargement échoué : {nom} — {raison_txt}"));
            }
            _ => {}
        }
    }

    /// La fiche produit a repondu : s'il y a plus recent, on l'annonce et on fait telecharger le
    /// nouveau programme par le gestionnaire de telechargements de Trame, pas par un second
    /// chemin qui aurait ses propres regles.
    ///
    /// L'installation elle-meme reste un geste : le setup est signe, il arrive dans le dossier
    /// de telechargements, et c'est lui qui remplace l'application.
    fn maj(&mut self, nouveaute: Option<crate::maj::Nouveaute>) {
        let Some(nouveaute) = nouveaute else {
            info!("mise a jour : rien de plus recent que {}", crate::maj::ACTUELLE);
            return;
        };
        info!("mise a jour : {} disponible ({})", nouveaute.version, nouveaute.url);
        // On PROPOSE, on ne telecharge pas : c'est a Clement de valider (reglages > Mises a jour).
        self.signaler(&format!(
            "Trame {} est disponible — Réglages (Ctrl+,) puis « Mises à jour ».",
            nouveaute.version
        ));
        self.maj = Some(nouveaute);
        self.pousser_etat();
    }

    /// Relit la fiche produit (bouton « Verifier maintenant » des reglages).
    fn verifier_maj(&mut self) {
        self.signaler("Recherche d'une mise à jour…");
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let nouveaute = crate::maj::verifier(crate::maj::ACTUELLE);
            let _ = proxy.send_event(UserEvent::Maj(nouveaute));
        });
    }

    /// Lance le setup de la mise a jour, deja telecharge par le gestionnaire de telechargements.
    ///
    /// Le setup est un installeur Inno : c'est lui qui ferme Trame (`/CLOSEAPPLICATIONS`), parce
    /// qu'il remplace `trame.exe` — le faire fermer depuis l'interieur serait plus fragile.
    fn installer_maj(&mut self) {
        let Some(maj) = self.maj.clone() else {
            self.signaler("Aucune mise à jour en attente.");
            return;
        };
        // On retrouve le setup par son ADRESSE, pas par son nom : Trame renomme en « (2) » pour
        // ne jamais ecraser un fichier existant.
        let deja = self
            .telechargements
            .iter()
            .find(|t| t.url == maj.url && t.etat == "termine")
            .cloned();
        let Some(telechargement) = deja else {
            self.signaler("La mise à jour n'est pas encore téléchargée : elle commence.");
            self.ouvrir_onglet(maj.url.clone(), Declencheur::Lien, None);
            return;
        };
        if !Path::new(&telechargement.chemin).exists() {
            self.signaler("Le fichier de mise à jour a été déplacé ou supprimé.");
            return;
        }
        info!("mise a jour : installation de {} ({})", maj.version, telechargement.chemin);
        let lancement = std::process::Command::new(&telechargement.chemin)
            .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS", "/CURRENTUSER"])
            .spawn();
        match lancement {
            Ok(_) => self.signaler(&format!("Installation de Trame {} en cours…", maj.version)),
            Err(e) => {
                alerte!("mise a jour : installeur impossible a lancer ({e})");
                self.signaler("Windows n'a pas pu lancer l'installeur de mise à jour.");
            }
        }
    }

    /// Chaque seconde, la progression des telechargements en cours.
    fn suivre_telechargements(&mut self) {
        let mut change = false;
        let mut clos: Vec<(u64, u64)> = Vec::new();
        for t in self.telechargements.iter_mut().filter(|t| t.etat == "en_cours") {
            if let Some((recu, total)) = crate::telechargements::progression(t.id) {
                if recu != t.recu || (total.is_some() && total != t.total) {
                    t.recu = recu;
                    if total.is_some() {
                        t.total = total;
                    }
                    change = true;
                }
            }
            // Le moteur oublie parfois de dire qu'il a fini (WebView2 #5638) : l'operation reste
            // « en cours » alors que tous les octets sont arrives. L'interroger n'aide pas, son
            // `State` reste bloque — mais le fichier, lui, porte la taille annoncee. Cela vaut
            // preuve : `BytesReceived` compte les octets ecrits, pas ceux en vol.
            if t.total.map_or(false, |total| total > 0 && t.recu >= total)
                && crate::telechargements::fichier_complet(&t.chemin, t.total)
            {
                clos.push((t.id, t.recu));
            }
        }
        if change {
            self.pousser_etat();
        }
        for (id, recu) in clos {
            info!("telechargement {id} : fin deduite du fichier (le moteur n'a rien signale)");
            self.telechargement_etat(id, true, false, false, 0, recu);
        }
    }

    /// L'etat des telechargements pour l'interface : la liste (plus recents d'abord) et la
    /// progression d'ensemble pour l'indicateur de la barre.
    fn telechargements_json(&self) -> Value {
        let actifs: Vec<&crate::telechargements::Telechargement> =
            self.telechargements.iter().filter(|t| t.actif()).collect();
        let (recu, total): (u64, u64) = actifs
            .iter()
            .filter_map(|t| t.total.map(|tot| (t.recu, tot)))
            .fold((0, 0), |(r, s), (a, b)| (r + a, s + b));
        let progression = if total > 0 { Some(recu as f64 / total as f64) } else { None };
        let liste: Vec<Value> = self
            .telechargements
            .iter()
            .rev()
            .take(150)
            .map(|t| {
                let existe = t.etat != "termine" || Path::new(&t.chemin).exists();
                json!({
                    "id": t.id,
                    "url": t.url,
                    "fichier": t.fichier,
                    "chemin": t.chemin,
                    "site": t.site,
                    "total": t.total,
                    "recu": t.recu,
                    "recuLisible": crate::telechargements::taille_lisible(t.recu),
                    "totalLisible": t.total.map(crate::telechargements::taille_lisible),
                    "etat": t.etat,
                    "raison": t.raison,
                    "debut": t.debut,
                    "fin": t.fin,
                    // Un fichier termine puis deplace ou supprime : on le dit.
                    "existe": existe,
                })
            })
            .collect();
        json!({
            "liste": liste,
            "actifs": actifs.len(),
            "progression": progression,
            "dossier": self.dossier_dl.borrow().display().to_string(),
            "dernierTermine": self.telechargements.iter().rev().find(|t| t.etat == "termine").map(|t| t.fin).unwrap_or(0),
        })
    }

    /// Commandes de la page « Téléchargements » et de l'indicateur de la barre.
    fn commande_telechargement(&mut self, m: &str, v: &Value) {
        info!("telechargements : commande {m}");
        let id = v["id"].as_u64().unwrap_or(0);
        let trouve = self.telechargements.iter().find(|t| t.id == id).cloned();
        match m {
            "telechargementPause" | "telechargementReprendre" | "telechargementAnnuler" => {
                let Some(t) = trouve else { return };
                let ordre = match m {
                    "telechargementPause" => "pause",
                    "telechargementReprendre" => "reprendre",
                    _ => "annuler",
                };
                if !crate::telechargements::commander(id, ordre) {
                    self.signaler("Ce téléchargement n'est plus en cours.");
                    return;
                }
                if let Some(e) = self.telechargements.iter_mut().find(|e| e.id == id) {
                    match ordre {
                        "pause" => e.etat = "en_pause".into(),
                        "reprendre" => e.etat = "en_cours".into(),
                        _ => {
                            e.etat = "annule".into();
                            e.raison = crate::telechargements::raison_lisible(40);
                            e.fin = maintenant_ms();
                        }
                    }
                }
                info!("telechargement {} : {ordre}", t.fichier);
                self.profil.sauver_telechargements(&self.telechargements);
                self.pousser_etat();
            }
            "telechargementOuvrir" => {
                let Some(t) = trouve else { return };
                if !Path::new(&t.chemin).exists() {
                    self.signaler("Ce fichier a été déplacé ou supprimé.");
                } else if !crate::externe::ouvrir_uri(&t.chemin) {
                    self.signaler("Windows n'a pas pu ouvrir ce fichier.");
                }
            }
            "telechargementDossier" => {
                let Some(t) = trouve else { return };
                let chemin = Path::new(&t.chemin).to_path_buf();
                if chemin.exists() {
                    info!("telechargements : {} montre dans son dossier ({})", t.fichier, chemin.display());
                    if !crate::externe::montrer_dans_dossier(&chemin) {
                        self.signaler("Windows n'a pas pu ouvrir l'Explorateur.");
                    }
                } else {
                    // Le fichier a bouge ou disparu : on ouvre le dossier ou il devait etre. Et
                    // jamais par `explorer.exe` : sur un chemin invalide il n'echoue pas, il
                    // ouvre son dossier par defaut (« Documents ») et on croit s'etre trompe.
                    let dossier = chemin.parent().map(Path::to_path_buf).unwrap_or_else(|| self.dossier_dl.borrow().clone());
                    if !crate::externe::ouvrir_dossier(&dossier) {
                        self.signaler(&format!("Ce dossier est introuvable : {}", dossier.display()));
                    }
                }
            }
            "telechargementRelancer" => {
                let Some(t) = trouve else { return };
                // Le meme lien, dans un onglet : le moteur relance le telechargement.
                self.ouvrir_onglet(t.url.clone(), Declencheur::Lien, None);
            }
            "telechargementRetirer" => {
                self.telechargements.retain(|t| t.id != id || t.actif());
                self.profil.sauver_telechargements(&self.telechargements);
                self.pousser_etat();
            }
            "telechargementsEffacer" => {
                let avant = self.telechargements.len();
                self.telechargements.retain(|t| t.actif());
                info!("telechargements : liste videe ({} entree(s))", avant - self.telechargements.len());
                self.profil.sauver_telechargements(&self.telechargements);
                self.pousser_etat();
            }
            "telechargementsDossier" => {
                // Le dossier est relu au moment du clic : s'il a change depuis le demarrage,
                // c'est le nouveau qui compte, pas une valeur gardee en memoire.
                let dossier = self.dossier_telechargements();
                *self.dossier_dl.borrow_mut() = dossier.clone();
                let _ = std::fs::create_dir_all(&dossier);
                info!("telechargements : dossier ouvert ({})", dossier.display());
                if !crate::externe::ouvrir_dossier(&dossier) {
                    self.signaler(&format!("Windows n'a pas pu ouvrir ce dossier : {}", dossier.display()));
                }
            }
            "telechargementsChoisirDossier" => match crate::systeme::choisir_dossier("Dossier des téléchargements") {
                Ok(Some(d)) => {
                    self.reglages.dossier_telechargements = d.clone();
                    self.profil.sauver_reglages(&self.reglages);
                    *self.dossier_dl.borrow_mut() = PathBuf::from(&d);
                    info!("telechargements : dossier {d}");
                    self.signaler(&format!("Téléchargements : {d}"));
                    self.pousser_etat();
                }
                Ok(None) => {}
                Err(e) => self.signaler(&e),
            },
            "telechargementsDossierWindows" => {
                self.reglages.dossier_telechargements.clear();
                self.profil.sauver_reglages(&self.reglages);
                *self.dossier_dl.borrow_mut() = self.dossier_telechargements();
                self.pousser_etat();
            }
            "ouvrirTelechargements" => self.ouvrir_page_interne(PAGE_TELECHARGEMENTS),
            _ => {}
        }
    }

    /// Releve, chaque seconde, quels onglets jouent du son. Le rail n'est repousse que si la liste
    /// change (une page qui joue en continu ne coute rien de plus).
    fn suivre_le_son(&mut self) {
        let maintenant: std::collections::HashSet<TabId> = self
            .webviews
            .iter()
            .filter(|(_, w)| crate::moteur_ext::son_en_cours(w))
            .map(|(id, _)| *id)
            .collect();
        // Un onglet ferme n'a plus a etre muet.
        let ouverts: std::collections::HashSet<TabId> = self.session.onglets.iter().map(|o| o.id).collect();
        self.muets.retain(|id| ouverts.contains(id));
        if maintenant != self.sonores {
            self.sonores = maintenant;
            self.pousser_etat();
        }
    }

    /// Coupe ou rend le son d'un onglet (clic sur son haut-parleur dans le rail).
    fn basculer_muet(&mut self, id: TabId) {
        let muet = !self.muets.contains(&id);
        if muet {
            self.muets.insert(id);
        } else {
            self.muets.remove(&id);
        }
        if let Some(w) = self.webviews.get(&id) {
            crate::moteur_ext::regler_muet(w, muet);
        }
        info!("onglet {id} : son {}", if muet { "coupe" } else { "rendu" });
        self.pousser_etat();
    }

    /// Comptes remontes par le script vivant d'un onglet. Met a jour « vu le » des regles qui
    /// attrapent quelque chose (ecrit au plus une fois par heure et par regle, pour ne pas
    /// reecrire zap.json a chaque defilement).
    fn recevoir_compte_zap(&mut self, id: TabId, comptes: &Value) {
        let Some(domaine) = self.trouver(id).map(|o| o.domaine()) else { return };
        let Some(liste) = comptes.as_array() else { return };
        let maintenant = maintenant_ms();
        let mut releves = Vec::new();
        let mut a_sauver = false;
        for c in liste.iter().take(500) {
            let (Some(i), Some(n)) = (c["i"].as_u64(), c["n"].as_u64()) else { continue };
            let i = i as usize;
            // Seules les regles qui s'appliquent vraiment a ce domaine sont acceptees.
            let Some(regle) = self.zap_regles.get_mut(i) else { continue };
            if !(regle.domaine.is_empty() || regle.domaine == domaine) {
                continue;
            }
            let n = n.min(100_000) as u32;
            if n > 0 && maintenant.saturating_sub(regle.vu_le) > 3_600_000 {
                regle.vu_le = maintenant;
                a_sauver = true;
            }
            releves.push((i, n));
        }
        self.zap_comptes.insert(id, releves);
        if a_sauver {
            self.profil.sauver_zap(&self.zap_regles);
        }
        if self.actif == Some(id) && self.zap_ouvert {
            self.pousser_etat();
        }
    }

    fn ajouter_regle_zap(&mut self, domaine: String, selecteur: String, libelle: String, texte: String) {
        self.ajouter_regle_zap_action(domaine, selecteur, libelle, texte, String::new(), String::new());
    }

    /// Retire la derniere regle ajoutee pendant cette session (bouton « Annuler »).
    fn annuler_derniere_regle_zap(&mut self) {
        let Some((domaine, selecteur, action, texte)) = self.zap_ajouts.pop() else {
            self.signaler("Rien à annuler.");
            return;
        };
        let avant = self.zap_regles.len();
        if let Some(pos) = self.zap_regles.iter().rposition(|r| {
            r.domaine == domaine && r.selecteur == selecteur && r.action == action && r.texte == texte
        }) {
            self.zap_regles.remove(pos);
        }
        if self.zap_regles.len() < avant {
            self.profil.sauver_zap(&self.zap_regles);
            info!("zappeur : derniere regle annulee ({selecteur})");
            self.signaler("Dernière modification annulée.");
            self.pousser_etat();
            if let Some(id) = self.actif {
                self.recharger_onglet(id);
            }
        }
    }

    fn ajouter_regle_zap_action(
        &mut self,
        domaine: String,
        selecteur: String,
        libelle: String,
        texte: String,
        action: String,
        valeur: String,
    ) {
        let selecteur = selecteur.trim().to_string();
        if selecteur.is_empty() {
            return;
        }
        if self.zap_regles.iter().any(|r| {
            r.domaine == domaine && r.selecteur == selecteur && r.texte == texte && r.action == action && r.valeur == valeur
        }) {
            self.signaler("Cette règle existe déjà.");
            return;
        }
        self.zap_ajouts.push((domaine.clone(), selecteur.clone(), action.clone(), texte.clone()));
        self.zap_regles.push(RegleZap {
            texte,
            action,
            valeur,
            ..RegleZap::nouvelle(&domaine, &selecteur, &libelle)
        });
        self.profil.sauver_zap(&self.zap_regles);
        let etiquette = if domaine.is_empty() {
            "tous les sites".to_string()
        } else {
            domaine
        };
        info!("zappeur : regle ajoutee ({etiquette}) {selecteur}");
        let nom = self.zap_regles.last().map(|r| r.nom_action()).unwrap_or("masqué");
        let mut lettres = nom.chars();
        let nom_maj: String = lettres.next().map(|c| c.to_uppercase().chain(lettres).collect()).unwrap_or_default();
        self.signaler(&format!("{nom_maj} sur {etiquette} : {selecteur}"));
        if let Some(id) = self.actif {
            self.appliquer_zap(id);
        }
        self.pousser_etat();
    }

    fn basculer_zap(&mut self) {
        // Si le panneau montre une extension, un clic le referme et le remet en mode zappeur.
        if self.zap_ouvert && self.panneau_extension.is_some() {
            self.fermer_panneau_droit();
            return;
        }
        self.zap_ouvert = !self.zap_ouvert;
        // Le panneau a servi a une extension : il en montre encore la fenetre. Le recharger sur
        // les regles laissait voir l'extension le temps du chargement (« masquer la publicite me
        // fait voir les extensions », 2026-11-22). On repart d'un panneau neuf, dans le profil
        // principal, qui ne montre que les regles.
        if self.zap_ouvert && (self.zap_montre_extension || self.zap_profil.is_some()) {
            if let Some(w) = &self.zap {
                let _ = w.set_visible(false);
            }
            self.zap = None;
            self.zap_profil = None;
            self.zap_montre_extension = false;
        }
        if self.zap_ouvert && self.zap.is_none() {
            let panneau = self.creer_vue_zap(None);
            self.zap = panneau;
        }
        if self.zap_ouvert {
            self.panneau_extension = None;
            self.popup_taille = None;
        }
        self.appliquer_geometrie();
        if let Some(w) = &self.zap {
            let _ = w.set_visible(self.zap_ouvert);
            if self.zap_ouvert {
                let _ = w.focus();
            }
        }
        self.pousser_etat();
    }

    fn fermer_panneau_droit(&mut self) {
        self.zap_ouvert = false;
        let ancien = self.panneau_extension.take();
        if let Some(id) = ancien {
            info!("panneau droit ferme (extension {id})");
        }
        self.appliquer_geometrie();
        if let Some(w) = &self.zap {
            let _ = w.set_visible(false);
        }
        self.pousser_etat();
    }

    // ---------------------------------------------------------------- installateur d'extensions

    /// Installe depuis une saisie libre : identifiant, ou lien de fiche du Chrome Web Store.
    fn installer_extension_magasin(&mut self, saisie: &str) {
        let Some(id) = crate::crx::identifiant(saisie) else {
            self.signaler(
                "Identifiant introuvable : collez les 32 caractères de l'extension, ou le lien \
                 de sa fiche sur le Chrome Web Store.",
            );
            return;
        };
        self.lancer_installation(installation::Demande::Magasin {
            id,
            source: "magasin".into(),
        });
    }

    // ---------------------------------------------------------------- sauvegarde du profil

    /// Lance une sauvegarde, une restauration ou un echange de favoris dans un fil annexe.
    ///
    /// Pourquoi un fil : les trois passent par le selecteur de Windows, qui **bloque** le fil
    /// appelant. Sur la boucle d'evenements, la fenetre entiere se figerait — c'est la lecon
    /// des 24 s d'installation d'uBlock Origin Lite (voir `installation.rs`).
    fn lancer_sauvegarde(&mut self, demande: crate::sauvegarde::Demande) {
        // Une sauvegarde doit refleter le moment du clic. La session n'est ecrite qu'a l'arret,
        // ou toutes les 8 s par le battement (voir `tick`) : sans cette materialisation, une
        // sauvegarde faite juste apres avoir ouvert trois onglets les aurait ignores, sans que
        // rien ne le dise.
        if matches!(demande, crate::sauvegarde::Demande::Sauver { .. }) {
            self.sauver();
        }
        let racine = self.profil.racine().to_path_buf();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let issue = crate::sauvegarde::executer(&racine, demande);
            let _ = proxy.send_event(UserEvent::Sauvegarde(issue));
        });
    }

    /// Une sauvegarde, une restauration ou un import de favoris vient de se terminer.
    fn fin_sauvegarde(&mut self, issue: crate::sauvegarde::Issue) {
        use crate::sauvegarde::Issue;
        match issue {
            Issue::Sauvegardee {
                chemin,
                octets,
                contenus,
            } => {
                info!(
                    "sauvegarde terminee : {contenus} contenu(s), {octets} octets -> {}",
                    chemin.display()
                );
                self.signaler(&format!(
                    "Sauvegarde de {contenus} contenu(s) — {:.0} Ko : {}",
                    octets as f64 / 1024.0,
                    chemin.display()
                ));
            }
            Issue::AConfirmer {
                chemin,
                resume,
                contenus,
            } => {
                // Rien n'est applique ici : c'est le demarrage suivant qui le fera, une fois
                // Trame ferme (sinon la sauvegarde de sortie ecraserait `session.json`).
                self.restauration = Some((chemin, resume, contenus));
                self.pousser_etat();
            }
            Issue::FavorisExportes { chemin, nombre } => {
                self.signaler(&format!(
                    "{nombre} favori(s) exporté(s) — {}. Ce fichier s'importe dans Chrome, Edge ou Firefox.",
                    chemin.display()
                ));
            }
            Issue::FavorisImportes(favoris) => {
                let (ajoutes, doublons) = crate::sauvegarde::fusionner_favoris(&mut self.favoris, favoris);
                if ajoutes > 0 {
                    self.profil
                        .sauver_liste(&self.profil.fichier_favoris(), &self.favoris);
                }
                let reste = if doublons > 0 {
                    format!(" ({doublons} déjà présent(s))")
                } else {
                    String::new()
                };
                self.signaler(&format!("{ajoutes} favori(s) importé(s){reste}."));
                self.pousser_etat();
            }
            Issue::Annulee => info!("sauvegarde : choix annule"),
            Issue::Echec(e) => {
                alerte!("sauvegarde : echec ({e})");
                self.signaler(&format!("Sauvegarde : {e}"));
            }
        }
    }

    /// Trame doit-il se fermer ? (une restauration attend le prochain demarrage)
    pub fn a_quitter(&self) -> bool {
        self.quitter
    }

    /// Lance une installation dans un fil annexe : la fenetre reste vivante pendant le
    /// telechargement et la decompression. L'issue revient en `UserEvent::Installation`.
    fn lancer_installation(&mut self, demande: installation::Demande) {
        let libelle = demande.libelle();
        if self.installations.iter().any(|l| l == &libelle) {
            self.signaler("Cette installation est déjà en cours.");
            return;
        }
        match &demande {
            installation::Demande::Magasin { .. } => {
                self.signaler("Téléchargement de l'extension depuis le Chrome Web Store…")
            }
            installation::Demande::Fichier { chemin: None, .. } => {}
            _ => self.signaler("Installation de l'extension…"),
        }
        self.installations.push(libelle.clone());
        self.pousser_etat();
        let actives = self.extensions_chrome.clone();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let issue = installation::executer(&actives, demande);
            let _ = proxy.send_event(UserEvent::Installation { libelle, issue });
        });
    }

    /// Une installation vient de se terminer : on charge l'extension dans le moteur, sans
    /// redemarrer.
    fn fin_installation(&mut self, libelle: String, issue: installation::Issue) {
        self.installations.retain(|l| l != &libelle);
        self.invalider_extensions_chrome();
        match issue {
            installation::Issue::Installee(i) => {
                info!(
                    "extension installee : {} « {} » ({} octets, CRX v{}, source {}) -> {}",
                    i.id,
                    i.nom,
                    i.octets,
                    i.version_crx,
                    i.source,
                    i.dossier.display()
                );
                self.noms_en_attente.insert(i.id.clone(), i.nom.clone());
                self.operer_moteur(&i.id, OperationMoteur::Charger);
                // Installee depuis un fil cloisonne (« Ajouter a Chrome » sur une fiche ouverte
                // dans ce fil) : elle y est voulue, on l'y active aussi.
                let fil = self.actif.and_then(|a| self.trouver(a)).map(|o| o.fil);
                if let Some(fil) = fil.filter(|f| self.cloison_du_fil(*f).is_some()) {
                    self.cloison_extension(fil, &i.id, true);
                }
            }
            installation::Issue::Annulee => info!("choix de fichier annule"),
            installation::Issue::Echec(e) => {
                alerte!("installation impossible ({libelle}) : {e}");
                self.signaler(&format!("Installation impossible : {e}"));
            }
        }
        self.pousser_etat();
    }

    /// Aligne le moteur de WebView2 sur le disque, tout de suite.
    fn operer_moteur(&mut self, id: &str, operation: OperationMoteur) {
        if self.barre.is_none() {
            self.signaler("Le moteur d'extensions est indisponible : le changement prendra effet au redémarrage.");
            return;
        }
        let proxy = self.proxy.clone();
        let dossier = self.extensions_chrome.join(id);
        let nom = self.nom_extension_chrome(id);
        info!("moteur d'extensions : {operation:?} {id}");
        // Mesure du cout par difference : la memoire des processus d'extension maintenant, puis
        // 12 s apres l'operation (voir `memoire`).
        let allumage = match operation {
            OperationMoteur::Charger | OperationMoteur::Allumer => Some(true),
            OperationMoteur::Eteindre => Some(false),
            OperationMoteur::Retirer => None,
        };
        if let Some(allumage) = allumage {
            self.mesure_extension = Some((
                id.to_string(),
                self.derniere_mesure.extensions_mo,
                maintenant_ms(),
                allumage,
            ));
        }
        let Some(barre) = self.barre.as_ref() else { return };
        match operation {
            OperationMoteur::Charger => crate::moteur_ext::ajouter(barre, proxy, &dossier, id),
            OperationMoteur::Allumer => {
                crate::moteur_ext::allumer(barre, proxy, id, &nom, true, Some(&dossier))
            }
            OperationMoteur::Eteindre => {
                crate::moteur_ext::allumer(barre, proxy, id, &nom, false, None)
            }
            OperationMoteur::Retirer => crate::moteur_ext::retirer(barre, proxy, id, &nom),
        }
    }

    /// Eteint une extension Chrome : son dossier part dans les desactivees (ce qui fait foi au
    /// demarrage suivant) et le moteur l'eteint tout de suite, en gardant ses donnees. Son
    /// epinglage est garde : il reviendra a la reactivation, comme dans Chrome.
    fn desactiver_chrome(&mut self, id: &str) {
        // Le nom se lit AVANT le deplacement : apres, le dossier n'est plus parmi les actives.
        let nom = self.nom_extension_chrome(id);
        self.noms_en_attente.insert(id.to_string(), nom);
        let dossiers = self.dossiers_extensions_chrome();
        match extensions_chrome::desactiver(&dossiers, id) {
            Ok(()) => {
                self.fermer_panneau_si_extension(id);
                self.operer_moteur(id, OperationMoteur::Eteindre);
                self.recharger_dans_cloisons(id);
            }
            Err(e) => self.signaler(&format!("Désactivation impossible : {e}")),
        }
        self.invalider_extensions_chrome();
        self.pousser_etat();
    }

    fn reactiver_chrome(&mut self, id: &str) {
        let dossiers = self.dossiers_extensions_chrome();
        match extensions_chrome::reactiver(&dossiers, id) {
            Ok(()) => {
                self.invalider_extensions_chrome();
                self.operer_moteur(id, OperationMoteur::Allumer);
                self.recharger_dans_cloisons(id);
            }
            Err(e) => self.signaler(&format!("Réactivation impossible : {e}")),
        }
        self.invalider_extensions_chrome();
        self.pousser_etat();
    }

    // ---------------------------------------------------------------- fils cloisonnes

    fn cloison_du_fil(&self, fil: ThreadId) -> Option<Cloison> {
        self.session
            .fils
            .iter()
            .find(|f| f.id == fil)
            .and_then(|f| f.cloison.clone())
    }

    /// Dossier d'une extension Chrome installee, active ou non dans le profil principal : une
    /// cloison peut utiliser une extension qu'on a coupee partout ailleurs.
    fn dossier_extension(&self, id: &str) -> Option<PathBuf> {
        let dossiers = self.dossiers_extensions_chrome();
        [dossiers.active.join(id), dossiers.desactivees().join(id)]
            .into_iter()
            .find(|d| d.join("manifest.json").is_file())
    }

    fn gerees(&self, ids: &[String]) -> Vec<crate::moteur_ext::Geree> {
        ids.iter()
            .filter_map(|id| {
                Some(crate::moteur_ext::Geree {
                    id: id.clone(),
                    nom: self.nom_extension_chrome(id),
                    dossier: self.dossier_extension(id)?,
                })
            })
            .collect()
    }

    /// Aligne le profil d'une cloison sur ses extensions, a travers une webview de ce profil.
    fn aligner_cloison_sur(&self, webview: &WebView, cloison: &Cloison) {
        let toutes: Vec<String> = self
            .liste_extensions_chrome()
            .iter()
            .filter_map(|e| e["id"].as_str().map(|s| s.to_string()))
            .collect();
        crate::moteur_ext::reconcilier(
            webview,
            self.proxy.clone(),
            &cloison.profil,
            self.gerees(&cloison.extensions),
            self.gerees(&toutes),
        );
    }

    /// Une webview vivante du profil d'un fil cloisonne (onglet reveille, ou panneau droit).
    fn webview_de_cloison(&self, fil: ThreadId) -> Option<&WebView> {
        let cloison = self.cloison_du_fil(fil)?;
        let onglet = self
            .session
            .onglets
            .iter()
            .filter(|o| o.fil == fil)
            .find_map(|o| self.webviews.get(&o.id));
        onglet.or_else(|| {
            (self.zap_profil.as_deref() == Some(cloison.profil.as_str()))
                .then_some(self.zap.as_ref())
                .flatten()
        })
    }

    fn aligner_cloison(&self, fil: ThreadId) {
        if let (Some(cloison), Some(w)) = (self.cloison_du_fil(fil), self.webview_de_cloison(fil)) {
            self.aligner_cloison_sur(w, &cloison);
        }
    }

    /// Recree les webviews d'un fil (changement de profil) : les onglets caches s'endorment, ils
    /// se rouvriront dans le bon profil au clic ; l'onglet affiche est rouvert tout de suite.
    fn recreer_webviews_du_fil(&mut self, fil: ThreadId) {
        let ids: Vec<TabId> = self
            .session
            .onglets
            .iter()
            .filter(|o| o.fil == fil && self.webviews.contains_key(&o.id))
            .map(|o| o.id)
            .collect();
        for id in &ids {
            self.webviews.remove(id);
            if let Some(o) = self.onglet_mut(*id) {
                o.eveille = false;
                o.chargement = false;
            }
        }
        if self.zap_profil.is_some() {
            self.fermer_panneau_droit();
            self.zap = None;
            self.zap_profil = None;
        }
        if let Some(actif) = self.actif.filter(|a| ids.contains(a)) {
            self.reveiller(actif);
            if let Some(w) = self.webviews.get(&actif) {
                let _ = w.set_visible(!self.vue_trame);
            }
        }
    }

    fn cloisonner_fil(&mut self, fil: ThreadId, cloisonner: bool) {
        let Some(f) = self.session.fils.iter_mut().find(|f| f.id == fil) else { return };
        if f.cloison.is_some() == cloisonner {
            return;
        }
        f.cloison = cloisonner.then(|| Cloison::pour_fil(fil));
        let nom = f.nom.clone();
        info!(
            "fil {fil} ({nom}) : {}",
            if cloisonner { "cloisonne" } else { "decloisonne" }
        );
        self.recreer_webviews_du_fil(fil);
        self.marquer_modifie();
        self.sauver();
        self.signaler(&if cloisonner {
            format!("« {nom} » est cloisonné : connexions et extensions à part. Choisissez ses extensions dans le puzzle.")
        } else {
            format!("« {nom} » rejoint le profil principal. Ses connexions de cloison sont gardées pour un prochain cloisonnement.")
        });
        self.pousser_etat();
    }

    /// Le dossier d'une extension vient de changer de place (coupee ou rallumee dans le profil
    /// principal) : les cloisons vivantes qui l'utilisent la rechargent depuis son nouveau
    /// chemin — couper une extension « partout ailleurs » ne la casse pas dans sa cloison.
    fn recharger_dans_cloisons(&self, id: &str) {
        let Some(dossier) = self.dossier_extension(id) else { return };
        for f in &self.session.fils {
            let Some(c) = &f.cloison else { continue };
            if !c.extensions.iter().any(|e| e == id) {
                continue;
            }
            if let Some(w) = self.webview_de_cloison(f.id) {
                info!("cloison {} : {id} rechargee depuis {}", c.profil, dossier.display());
                crate::moteur_ext::ajouter(w, self.proxy.clone(), &dossier, id);
            }
        }
    }

    /// Ajoute ou retire une extension d'une cloison, tout de suite.
    fn cloison_extension(&mut self, fil: ThreadId, id: &str, active: bool) {
        let Some(c) = self
            .session
            .fils
            .iter_mut()
            .find(|f| f.id == fil)
            .and_then(|f| f.cloison.as_mut())
        else {
            return;
        };
        c.extensions.retain(|e| e != id);
        if active {
            c.extensions.push(id.to_string());
        }
        let nom = self.nom_extension_chrome(id);
        info!("cloison du fil {fil} : {id} {}", if active { "ajoutee" } else { "retiree" });
        self.marquer_modifie();
        self.sauver();
        if self.webview_de_cloison(fil).is_some() {
            self.aligner_cloison(fil);
        }
        self.signaler(&format!(
            "« {nom} » {} ce fil.",
            if active { "est active dans" } else { "est retirée de" }
        ));
        self.pousser_etat();
    }

    fn interroger_moteur(&self) {
        if let Some(barre) = self.barre.as_ref() {
            crate::moteur_ext::lister(barre, self.proxy.clone());
        }
    }

    /// Nom lisible d'une extension Chrome installee (le manifeste localise, pas l'identifiant).
    fn nom_extension_chrome(&self, id: &str) -> String {
        if let Some(nom) = self.noms_en_attente.get(id) {
            return nom.clone();
        }
        self.liste_extensions_chrome()
            .iter()
            .find(|e| e["id"] == id)
            .and_then(|e| e["nom"].as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| id.to_string())
    }

    /// Le moteur a repondu a une operation.
    fn bilan_moteur(&mut self, bilan: crate::moteur_ext::Bilan) {
        use crate::moteur_ext::Action;
        let nom = self.nom_extension_chrome(&bilan.id);
        self.noms_en_attente.remove(&bilan.id);
        if !bilan.reussi {
            alerte!(
                "moteur d'extensions : {:?} de {} refuse ({})",
                bilan.action,
                bilan.id,
                bilan.detail
            );
            self.signaler(&format!(
                "« {nom} » : le moteur a refusé ({}). Le changement prendra effet au redémarrage.",
                bilan.detail
            ));
            self.interroger_moteur();
            return;
        }
        info!("moteur d'extensions : {:?} de {} fait", bilan.action, bilan.id);
        // Ajout de demarrage (extension deja installee, absente du moteur) : sans bruit. Seuls
        // les AJOUTS sont concernes : une extinction ou un rallumage suivent leur cours normal.
        if bilan.action == Action::Ajout
            && (self.ajouts_silencieux.remove(&bilan.id) | self.ajouts_silencieux.remove(&bilan.detail))
        {
            self.interroger_moteur();
            self.pousser_etat();
            return;
        }
        // Le moteur peut nommer l'extension autrement que le dossier (cle absente) : on le dit.
        if bilan.action == Action::Ajout && !self.extensions_chrome.join(&bilan.id).is_dir() {
            alerte!(
                "moteur d'extensions : identifiant attribue {} sans dossier du meme nom",
                bilan.id
            );
        }
        let rechargee = self.recharger_page_active();
        let suite = if rechargee { " — page rechargée" } else { "" };
        let message = match bilan.action {
            Action::Ajout => {
                let epinglee = self.reglages.extensions_epinglees.iter().any(|e| e == &bilan.id);
                if epinglee {
                    format!("« {nom} » est installée et active{suite}.")
                } else {
                    format!("« {nom} » est installée et active{suite}. Épinglez-la depuis le menu des extensions.")
                }
            }
            Action::Allumage => format!("« {nom} » activée{suite}."),
            Action::Extinction => format!("« {nom} » désactivée{suite}."),
            Action::Retrait => format!("« {nom} » retirée — son dossier est gardé dans les archives{suite}."),
        };
        self.signaler(&message);
        self.interroger_moteur();
        self.pousser_etat();
        if bilan.action == Action::Extinction {
            if let Some((id, 1)) = self.sonde_chrome.clone() {
                if id == bilan.id {
                    self.sonde_chrome = Some((id.clone(), 2));
                    self.reactiver_chrome(&id);
                }
            }
        }
    }

    /// Recharge l'onglet actif s'il affiche une page web : un script de contenu ne s'injecte
    /// qu'au chargement, c'est ce qui rend un changement d'extension visible tout de suite.
    /// Une page interne de Trame (reglages, accueil) n'en a pas besoin. Rend `true` si un
    /// rechargement a eu lieu.
    fn recharger_page_active(&mut self) -> bool {
        let Some(id) = self.actif else { return false };
        let web = self
            .trouver(id)
            .map(|o| o.url.starts_with("http://") || o.url.starts_with("https://"))
            .unwrap_or(false);
        if !web || !self.webviews.contains_key(&id) {
            return false;
        }
        // Un onglet encore en train de charger (ou dont la premiere navigation n'a pas abouti)
        // ne se recharge pas tout de suite : constate le 2026-11-21, un `reload` lance avant
        // la premiere navigation laissait la page vide. On recharge a la fin du chargement.
        let charge = self.trouver(id).map(|o| o.chargement).unwrap_or(false);
        if charge || self.premier_chargement.contains_key(&id) {
            info!("onglet {id} : rechargement differe a la fin du chargement");
            self.recharger_apres_chargement.insert(id);
            return true;
        }
        info!("onglet {id} recharge apres un changement d'extension");
        self.recharger_onglet(id);
        true
    }

    /// La liste des extensions Chrome du disque, gardee en cache : la calculer parcourt chaque
    /// dossier (poids sur le disque), et l'etat est pousse plusieurs fois par seconde.
    fn liste_extensions_chrome(&self) -> Vec<Value> {
        if let Some(liste) = self.cache_extensions_chrome.borrow().as_ref() {
            return liste.clone();
        }
        let liste = extensions_chrome::lister(&self.dossiers_extensions_chrome());
        *self.cache_extensions_chrome.borrow_mut() = Some(liste.clone());
        liste
    }

    fn invalider_extensions_chrome(&self) {
        *self.cache_extensions_chrome.borrow_mut() = None;
    }

    /// Retire une extension des epingles (desinstallee, ou coupee).
    fn deplier_extension_epinglee(&mut self, id: &str) {
        let avant = self.reglages.extensions_epinglees.len();
        self.reglages.extensions_epinglees.retain(|e| e != id);
        if self.reglages.extensions_epinglees.len() != avant {
            self.profil.sauver_reglages(&self.reglages);
        }
        if self.panneau_extension.as_deref() == Some(id) {
            self.fermer_panneau_droit();
        }
    }

    fn fermer_panneau_si_extension(&mut self, id: &str) {
        if self.panneau_extension.as_deref() == Some(id) {
            self.fermer_panneau_droit();
        }
    }

    /// Ouvre la popup d'une extension epinglee dans le panneau de droite.
    fn ouvrir_popup_extension(&mut self, id: &str) {
        let dossier = self
            .dossier_extension(id)
            .unwrap_or_else(|| self.extensions_chrome.join(id));
        // Dans un fil cloisonne, l'extension vit dans le profil de la cloison : le panneau doit
        // y etre aussi, sinon sa fenetre d'action n'existe pas (ou c'est celle d'une autre
        // connexion). Le panneau est recree quand le profil change.
        let fil = self.actif.and_then(|a| self.trouver(a)).map(|o| o.fil);
        let profil = fil
            .and_then(|f| self.cloison_du_fil(f))
            .filter(|c| c.extensions.iter().any(|e| e == id))
            .map(|c| c.profil);
        if self.zap.is_some() && self.zap_profil != profil {
            if let Some(w) = &self.zap {
                let _ = w.set_visible(false);
            }
            self.zap = None;
        }
        if self.zap.is_none() {
            let panneau = self.creer_vue_zap(profil.clone());
            self.zap = panneau;
            self.zap_profil = profil.clone();
            // Un panneau neuf dans une cloison : on y aligne les extensions avant d'ouvrir.
            if let (Some(f), Some(w), Some(_)) = (fil, self.zap.as_ref(), profil.as_ref()) {
                if let Some(c) = self.cloison_du_fil(f) {
                    self.aligner_cloison_sur(w, &c);
                }
            }
        }
        let Ok(texte) = std::fs::read_to_string(dossier.join("manifest.json")) else {
            self.signaler("Extension introuvable.");
            return;
        };
        let Ok(manifeste) = serde_json::from_str::<Value>(&texte) else {
            self.signaler("Manifeste illisible.");
            return;
        };
        let page = manifeste["action"]["default_popup"]
            .as_str()
            .or_else(|| manifeste["browser_action"]["default_popup"].as_str())
            .map(|s| s.trim_start_matches('/').to_string());
        let Some(page) = page else {
            self.signaler("Cette extension n'a pas de fenêtre d'action.");
            return;
        };
        let url = format!("chrome-extension://{id}/{page}");
        if let Some(w) = &self.zap {
            if let Err(e) = charger(w, &url) {
                alerte!("popup d'extension : {e}");
                self.signaler("Impossible d'ouvrir l'interface de l'extension.");
                return;
            }
        }
        self.panneau_extension = Some(id.to_string());
        self.zap_montre_extension = true;
        self.popup_taille = None;
        self.zap_ouvert = true;
        self.appliquer_geometrie();
        if let Some(w) = &self.zap {
            let _ = w.set_visible(true);
            let _ = w.focus();
        }
        info!("panneau droit : popup de l'extension {id}");
        self.pousser_etat();
    }

    fn recharger_onglet(&mut self, id: TabId) {
        if let Some(w) = self.webviews.get(&id) {
            let _ = w.reload();
        }
    }

    fn demarrer_selecteur(&mut self) {
        self.selection_guet = false;
        let Some(id) = self.actif else { return };
        self.eval_sur(Source::Onglet(id), SELECTEUR_JS);
        self.signaler("Cliquez l'élément à masquer — Échap annule.");
    }

    fn basculer_trame(&mut self) {
        self.vue_trame = !self.vue_trame;
        if self.vue_trame && self.vue.is_none() {
            let nouvelle = self.creer_vue_trame();
            self.vue = nouvelle;
        }
        let geo = self.geometrie_courante();
        match (self.vue_trame, self.vue.as_ref()) {
            (true, Some(vue)) => {
                let _ = vue.set_bounds(geo.contenu);
                let _ = vue.set_visible(true);
                let _ = vue.focus();
                for w in self.webviews.values() {
                    let _ = w.set_visible(false);
                }
            }
            (false, Some(vue)) => {
                let _ = vue.set_visible(false);
                let actif = self.actif;
                for (id, w) in self.webviews.iter() {
                    let _ = w.set_visible(Some(*id) == actif);
                }
            }
            _ => {
                // La vue n'a pas pu etre creee : on ne ment pas sur l'etat.
                self.vue_trame = false;
            }
        }
        // De retour de la trame : les deux pages du cote a cote reprennent leur place.
        if !self.vue_trame && self.session.cote.is_some() {
            self.preparer_cote();
            self.appliquer_geometrie();
        }
        self.ajuster_memoire(maintenant_ms());
        self.pousser_etat();
    }

    // ---------------------------------------------------------------- stockage des extensions

    fn extension_stockage(&mut self, ext: &str, op: &str, payload: &Value) -> Value {
        if ext.is_empty() {
            return Value::Null;
        }
        if !self.ext_storage.is_object() {
            self.ext_storage = json!({});
        }
        let racine = self.ext_storage.as_object_mut().expect("objet");
        let domaine = racine
            .entry(ext.to_string())
            .or_insert_with(|| json!({}));
        if !domaine.is_object() {
            *domaine = json!({});
        }
        let domaine_obj = domaine.as_object_mut().expect("objet");

        match op {
            "storage.get" => {
                let mut sortie = serde_json::Map::new();
                match payload {
                    Value::Null => {
                        for (k, v) in domaine_obj.iter() {
                            sortie.insert(k.clone(), v.clone());
                        }
                    }
                    Value::String(cle) => {
                        if let Some(v) = domaine_obj.get(cle) {
                            sortie.insert(cle.clone(), v.clone());
                        }
                    }
                    Value::Array(cles) => {
                        for c in cles {
                            if let Some(cle) = c.as_str() {
                                if let Some(v) = domaine_obj.get(cle) {
                                    sortie.insert(cle.to_string(), v.clone());
                                }
                            }
                        }
                    }
                    Value::Object(defauts) => {
                        for (cle, defaut) in defauts {
                            let valeur = domaine_obj.get(cle).cloned().unwrap_or_else(|| defaut.clone());
                            sortie.insert(cle.clone(), valeur);
                        }
                    }
                    _ => {}
                }
                Value::Object(sortie)
            }
            "storage.set" => {
                if let Value::Object(paires) = payload {
                    for (k, v) in paires {
                        domaine_obj.insert(k.clone(), v.clone());
                    }
                    self.profil.sauver_ext_storage(&self.ext_storage);
                }
                Value::Null
            }
            "storage.remove" => {
                match payload {
                    Value::String(cle) => {
                        domaine_obj.remove(cle);
                    }
                    Value::Array(cles) => {
                        for c in cles {
                            if let Some(cle) = c.as_str() {
                                domaine_obj.remove(cle);
                            }
                        }
                    }
                    _ => {}
                }
                self.profil.sauver_ext_storage(&self.ext_storage);
                Value::Null
            }
            "storage.clear" => {
                domaine_obj.clear();
                self.profil.sauver_ext_storage(&self.ext_storage);
                Value::Null
            }
            _ => Value::Null,
        }
    }
}

/// Amorce des surfaces d'interface : remonte les erreurs JavaScript a l'hote.
/// Sans cela, une interface cassee est un ecran noir et muet — constate une fois de trop.
const AMORCE_UI: &str = r#"(() => {
  const envoi = (o) => { try { window.ipc && window.ipc.postMessage(JSON.stringify(o)); } catch (e) {} };
  window.addEventListener('error', (e) => {
    envoi({ m: 'erreurUi', message: String((e && e.message) || 'erreur inconnue'), source: String((e && e.filename) || ''), ligne: (e && e.lineno) || 0 });
  });
  window.addEventListener('unhandledrejection', (e) => {
    envoi({ m: 'erreurUi', message: 'promesse rejetee : ' + String((e && e.reason) || ''), source: '', ligne: 0 });
  });
})();"#;

/// Selecteur intelligent du zappeur (molette = bloc parent, semblables, menu de choix).
/// Le script vit dans `ui/zap-choisir.js`, embarque a la compilation.
const SELECTEUR_JS: &str = include_str!("../ui/zap-choisir.js");
/// Script vivant du zappeur : regles a condition de texte et comptage (`ui/zap-vivant.js`).
/// C'est une expression de fonction, appelee avec les regles du domaine.
const ZAP_VIVANT_JS: &str = include_str!("../ui/zap-vivant.js");

/// Amorce installee dans chaque onglet : raccourcis clavier et position de lecture.
/// Ecrite en JS minimal, sans dependance : elle tourne sur chaque page, y compris hostiles.
/// Raccourcis clavier, joues dans chaque page. Le script vit dans `ui/raccourcis.js` :
/// il est embarque a la compilation et teste par `outils/tester-ui.mjs` (vraies frappes).
const RACCOURCIS: &str = include_str!("../ui/raccourcis.js");

/// Video flottante : pastille au survol d'une video, et Alt+P (`ui/incrustation.js`).
const INCRUSTATION: &str = include_str!("../ui/incrustation.js");

/// Une extension qui demande une vraie **fenetre** (`chrome.windows.create` avec une taille :
/// passkey de Bitwarden, « popout » d'un gestionnaire de mots de passe) doit l'obtenir. Ces
/// extensions suivent la fenetre creee (identifiant, fermeture par `window.close()`) : rouverte
/// en onglet, la session passkey de Bitwarden s'interrompait et la page restait blanche
/// (2026-11-22). On laisse alors WebView2 ouvrir sa fenetre, dans le meme profil.
/// Tout le reste (`window.open` d'une page, onglet demande par une extension) devient un onglet.
/// WebView2 ne transmet pas toujours la taille demandee par `chrome.windows.create` (essai du
/// 2026-11-22 : passkey Bitwarden recue sans taille) : la marque `uilocation=popout` suffit.
fn fenetre_extension(url: &str, taille_demandee: bool) -> bool {
    let u = url.to_ascii_lowercase();
    u.starts_with("chrome-extension://") && (taille_demandee || u.contains("uilocation=popout"))
}

/// La petite barre du coup d'oeil, injectee dans l'apercu.
const APERCU: &str = include_str!("../ui/apercu.js");

/// Navigue `w` vers `url`. wry ne reecrit `trame://localhost/x` en `http://trame.localhost/x`
/// que dans `with_url` : `load_url` passe l'adresse telle quelle a WebView2, qui ignore alors
/// le schema `trame:` (l'onglet prenait le nom « Reglages » mais gardait la page d'accueil).
fn charger(w: &WebView, url: &str) -> wry::Result<()> {
    w.load_url(&adresse_moteur(url))
}

/// Forme que WebView2 comprend pour une page interne : `http://trame.localhost/...`.
fn adresse_moteur(url: &str) -> String {
    match url.strip_prefix("trame://localhost") {
        Some(reste) if reste.is_empty() || reste.starts_with('/') => {
            format!("http://trame.localhost{}", if reste.is_empty() { "/" } else { reste })
        }
        _ => url.to_string(),
    }
}

/// wry reecrit les URL du protocole personnalise sous Windows. On revient a la forme
/// `trame://localhost/...` pour que les pages internes restent reconnaissables.
fn retranscrire_interne(url: &str) -> String {
    for prefixe in ["http://trame.localhost/", "https://trame.localhost/"] {
        if let Some(reste) = url.strip_prefix(prefixe) {
            return format!("trame://localhost/{reste}");
        }
    }
    if url == "http://trame.localhost" || url == "https://trame.localhost" {
        return "trame://localhost/".to_string();
    }
    url.to_string()
}

/// Inventaire des extensions Chrome presentes dans un dossier (un sous-dossier = une extension).


/// Resout une valeur de manifeste qui peut etre un jeton `__MSG_cle__`.
///
/// Un manifeste localise ne stocke pas « Bitwarden » ni sa description : il stocke
/// `__MSG_appName__` / `__MSG_extDesc__` et laisse le navigateur lire `_locales`. Sans
/// cette resolution, l'interface affichait le jeton brut a la place de la description.


/// Cherche les extensions installees dans les navigateurs Chromium de la machine.
///
/// C'est le raccourci honnete quand le Chrome Web Store ne peut pas installer ici : une
/// extension deja installee dans Chrome est un dossier decompresse, exactement ce que
/// WebView2 sait charger.
fn scanner_navigateurs(extensions_chrome: &Path) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let Some(local) = dirs::data_local_dir() else {
        return out;
    };
    let racines = [
        ("Chrome", local.join("Google/Chrome/User Data")),
        ("Edge", local.join("Microsoft/Edge/User Data")),
        ("Brave", local.join("BraveSoftware/Brave-Browser/User Data")),
    ];
    for (navigateur, racine) in racines {
        let Ok(profils) = std::fs::read_dir(&racine) else { continue };
        for profil in profils.flatten() {
            let dossier_ext = profil.path().join("Extensions");
            if !dossier_ext.is_dir() {
                continue;
            }
            let Ok(identifiants) = std::fs::read_dir(&dossier_ext) else { continue };
            for identifiant in identifiants.flatten() {
                if !identifiant.path().is_dir() {
                    continue;
                }
                let Ok(versions) = std::fs::read_dir(identifiant.path()) else { continue };
                let mut versions: Vec<std::path::PathBuf> = versions
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect();
                versions.sort();
                let Some(version) = versions.last() else { continue };
                let Ok(texte) = std::fs::read_to_string(version.join("manifest.json")) else {
                    continue;
                };
                let Ok(manifeste) = serde_json::from_str::<Value>(&texte) else { continue };
                let id = identifiant.file_name().to_string_lossy().to_string();
                let nom = extensions_chrome::nom_extension_chrome(&manifeste, version);
                if nom.trim().is_empty() {
                    continue;
                }
                out.push(json!({
                    "id": id,
                    "nom": nom,
                    "version": manifeste["version"].as_str().unwrap_or("").to_string(),
                    "chemin": version.display().to_string(),
                    "profil": profil.file_name().to_string_lossy().to_string(),
                    "navigateur": navigateur,
                    "installe": extensions_chrome.join(&id).exists(),
                }));
            }
        }
    }
    out.sort_by(|a, b| {
        a["nom"].as_str().unwrap_or("").to_lowercase().cmp(&b["nom"].as_str().unwrap_or("").to_lowercase())
    });
    out
}

/// Copie recursive (import d'une extension Chrome vers le profil).


/// Point d'entree du fil de mesure : renvoie le budget memoire au coeur.
pub fn mesurer_memoire(pid: u32) -> probe::Mesure {
    probe::mesurer(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_page_interne_est_chargee_sous_la_forme_de_webview2() {
        assert_eq!(adresse_moteur(PAGE_REGLAGES), "http://trame.localhost/reglages.html");
        assert_eq!(adresse_moteur("trame://localhost/"), "http://trame.localhost/");
        assert_eq!(adresse_moteur("trame://localhost"), "http://trame.localhost/");
        assert_eq!(adresse_moteur("https://exemple.fr/"), "https://exemple.fr/");
        assert_eq!(adresse_moteur("trame://localhostile/x"), "trame://localhostile/x");
        assert_eq!(retranscrire_interne(&adresse_moteur(PAGE_REGLAGES)), PAGE_REGLAGES);
    }

    #[test]
    fn fenetre_extension_seulement_pour_une_popup_d_extension() {
        let bw = "chrome-extension://nngceckbapebfimnlniiiahkandclblb/popup/index.html?uilocation=popout&singleActionPopout=vault_Fido2Popout#/fido2?sessionId=x";
        assert!(fenetre_extension(bw, true));
        // WebView2 peut omettre la taille : la marque popout suffit
        assert!(fenetre_extension(bw, false));
        // page d'extension ordinaire sans taille : un onglet, comme avant
        assert!(!fenetre_extension("chrome-extension://abc/options.html", false));
        // une page web qui ouvre une popup reste un onglet
        assert!(!fenetre_extension("https://accounts.google.com/x", true));
    }

    #[test]
    fn l_amorce_des_raccourcis_est_du_javascript_non_vide() {
        assert!(RACCOURCIS.contains("keydown"));
        assert!(RACCOURCIS.len() > 200);
        assert!(INCRUSTATION.contains("requestPictureInPicture"));
        // YouTube impose les Trusted Types : une affectation HTML ferait echouer le script.
        assert!(!INCRUSTATION.contains("innerHTML"));
    }

    #[test]
    fn la_sonde_d_etat_d_extension_lit_les_deux_sens() {
        assert_eq!(analyser_sonde_etat("nuit-douce"), Some(("nuit-douce".into(), true)));
        assert_eq!(analyser_sonde_etat("nuit-douce=on"), Some(("nuit-douce".into(), true)));
        assert_eq!(analyser_sonde_etat("nuit-douce=off"), Some(("nuit-douce".into(), false)));
        assert_eq!(analyser_sonde_etat("nuit-douce=0"), Some(("nuit-douce".into(), false)));
        assert_eq!(analyser_sonde_etat("  "), None, "une sonde vide ne vise rien");
        assert_eq!(analyser_sonde_etat("=off"), None, "sans identifiant, rien a allumer");
    }

    #[test]
    fn le_sort_d_un_telechargement_depend_du_magasin_et_du_paquet() {
        let dossier = std::env::temp_dir().join(format!("trame-suite-tel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        let paquet = dossier.join("paquet.crx");
        // CRX3 minimal, sans en-tete utile (16 octets d'en-tete nuls + un ZIP factice).
        let mut crx = b"Cr24".to_vec();
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&16u32.to_le_bytes());
        crx.extend_from_slice(&[0u8; 16]);
        crx.extend_from_slice(b"PK\x03\x04zip");
        std::fs::write(&paquet, &crx).unwrap();
        let image = dossier.join("photo.jpg");
        std::fs::write(&image, b"\xFF\xD8\xFF\xE0photo").unwrap();

        let magasin = "https://clients2.googleusercontent.com/crx/blobs/abc";
        let refus = "https://permanently-removed.invalid/?response=redirect&x=id%3Deimadpbcbfnmbkopoojfekhnkhdbieeh%26uc";

        // Un paquet arrive du magasin : on l'installe.
        assert_eq!(
            suite_telechargement(magasin, Some(&paquet), true),
            SuiteTelechargement::InstallerPaquet
        );
        // Le magasin a redirige : l'identifiant est dans l'URL, on reprend par la.
        assert_eq!(
            suite_telechargement(refus, None, false),
            SuiteTelechargement::InstallerIdentifiant("eimadpbcbfnmbkopoojfekhnkhdbieeh".into())
        );
        // Une photo servie par le meme CDN reste un telechargement.
        assert_eq!(
            suite_telechargement(magasin, Some(&image), true),
            SuiteTelechargement::Ordinaire
        );
        // Un `.crx` venu d'ailleurs n'est PAS installe en silence.
        assert_eq!(
            suite_telechargement("https://exemple.fr/mon-extension.crx", Some(&paquet), true),
            SuiteTelechargement::Ordinaire
        );
        // Un telechargement du magasin sans identifiant ni paquet : rien a installer.
        assert_eq!(
            suite_telechargement("https://clients2.googleusercontent.com/fichier.bin", None, false),
            SuiteTelechargement::Ordinaire
        );
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn le_zoom_reste_dans_des_bornes_utiles() {
        assert_eq!(zoom_borne(1.0), 1.0);
        assert_eq!(zoom_borne(0.1), ZOOM_MIN, "on ne descend pas sous 50 %");
        assert_eq!(zoom_borne(9.0), ZOOM_MAX, "on ne monte pas au-dela de 200 %");
        assert_eq!(zoom_borne(f64::NAN), 1.0, "une valeur absurde revient a la normale");
        assert_eq!(zoom_borne(f64::INFINITY), 1.0, "l'infini n'est pas un zoom : 100 %");
        // Arrondi au dixieme : le facteur est affiche en pourcentage entier.
        assert_eq!(zoom_borne(1.234), 1.2);
    }

    #[test]
    fn le_zoom_avance_par_pas_de_dix_pour_cent() {
        assert_eq!(zoom_suivant(1.0, 1), 1.1);
        assert_eq!(zoom_suivant(1.0, -1), 0.9);
        assert_eq!(zoom_suivant(2.0, 1), 2.0, "plafond atteint");
        assert_eq!(zoom_suivant(0.5, -1), 0.5, "plancher atteint");
        // Trois crans depuis 1.0 puis retour : on doit revenir exactement a 1.0.
        let mut f = 1.0;
        for _ in 0..3 {
            f = zoom_suivant(f, 1);
        }
        assert_eq!(f, 1.3);
        for _ in 0..3 {
            f = zoom_suivant(f, -1);
        }
        assert_eq!(f, 1.0, "aucune derive apres un aller-retour");
    }

    #[test]
    fn un_telechargement_ne_remplace_jamais_un_fichier_existant() {
        let dossier = std::env::temp_dir().join(format!("trame-tel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        let premier = nom_libre(&dossier, "rapport.pdf");
        assert_eq!(
            premier.file_name().unwrap().to_string_lossy(),
            "rapport.pdf"
        );
        std::fs::write(&premier, b"x").unwrap();
        let second = nom_libre(&dossier, "rapport.pdf");
        assert_eq!(
            second.file_name().unwrap().to_string_lossy(),
            "rapport (2).pdf"
        );
        std::fs::write(&second, b"x").unwrap();
        let troisieme = nom_libre(&dossier, "rapport.pdf");
        assert_eq!(
            troisieme.file_name().unwrap().to_string_lossy(),
            "rapport (3).pdf"
        );
        // Un nom sans extension garde son nom, et le dossier est cree au besoin.
        let sans = nom_libre(&dossier, "photo");
        assert_eq!(sans.file_name().unwrap().to_string_lossy(), "photo");
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn la_recherche_dans_la_page_est_du_javascript_autonome() {
        // Le script est injecte tel quel dans la page : il doit etre une expression
        // immediate, sans dependance a Trame, et refermer la barre sur Echap.
        assert!(RECHERCHE_PAGE.contains("trame-recherche"));
        assert!(RECHERCHE_PAGE.contains("Escape"));
        assert!(RECHERCHE_PAGE.contains("window.find"));
        // Commentaire d'en-tete admis, mais le script doit etre une expression immediate :
        // sans cela, `evaluate_script` l'evaluerait sans jamais l'executer.
        assert!(RECHERCHE_PAGE.contains("(() =>"));
        assert!(RECHERCHE_PAGE.trim_end().ends_with("})();"));
    }

    #[test]
    fn un_onglet_distant_ne_peut_pas_commander() {
        // Verifie la regle d'autorisation sans instancier de fenetre.
        let autorise = |m: &str, source: Source| match m {
            "extCall" | "defilement" | "raccourci" | "pret" | "hello" => true,
            _ => !matches!(source, Source::Onglet(_)),
        };
        assert!(autorise("naviguer", Source::Rail));
        assert!(!autorise("naviguer", Source::Onglet(3)));
        assert!(autorise("extCall", Source::Onglet(3)));
    }

    #[test]
    fn un_manifeste_localise_livre_son_nom_et_sa_description() {
        // Un manifeste d'extension localisee ne contient que des jetons `__MSG_…__`.
        let racine = std::env::temp_dir().join(format!("trame-localisation-{}", std::process::id()));
        let locales = racine.join("_locales").join("fr");
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
        assert_eq!(extensions_chrome::nom_extension_chrome(&manifeste, &racine), "Bitwarden");
        // La cle de `messages.json` est insensible a la casse, comme dans Chromium.
        assert_eq!(
            extensions_chrome::localiser_champ(&manifeste, &racine, "description").as_deref(),
            Some("Gestionnaire de mots de passe")
        );
        let _ = std::fs::remove_dir_all(&racine);
    }
}
