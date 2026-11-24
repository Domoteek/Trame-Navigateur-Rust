//! Types du domaine : onglets, fils, capsules, session, extensions.
//!
//! Tout ce qui doit survivre a un redemarrage est `Serialize`/`Deserialize`.
//! L'etat de rendu (webview vivante, chargement en cours) est `#[serde(skip)]` :
//! c'est un fait d'execution, pas un fait de profil.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub type TabId = u32;
pub type ThreadId = u32;

/// D'ou vient un onglet. C'est la matiere premiere de la provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Declencheur {
    /// Lien clique dans un onglet existant (le parent est connu).
    Lien,
    /// Saisie dans la Couture.
    Saisie,
    /// Nouvel onglet vide.
    NouvelOnglet,
    /// Restaure d'une session precedente.
    Restauration,
    /// Ouvert depuis une capsule.
    Capsule,
    /// Rouvert apres fermeture (Ctrl+Maj+T).
    Reouverture,
}

impl Declencheur {
    pub fn libelle(self) -> &'static str {
        match self {
            Declencheur::Lien => "lien",
            Declencheur::Saisie => "saisie",
            Declencheur::NouvelOnglet => "nouvel onglet",
            Declencheur::Restauration => "restauration",
            Declencheur::Capsule => "capsule",
            Declencheur::Reouverture => "reouverture",
        }
    }
}

fn vitalite_par_defaut() -> f32 {
    0.70
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Onglet {
    pub id: TabId,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub titre: String,
    #[serde(default)]
    pub favicon: Option<String>,
    pub fil: ThreadId,
    #[serde(default)]
    pub parent: Option<TabId>,
    #[serde(default = "declencheur_defaut")]
    pub declencheur: Declencheur,
    pub cree_le: u64,
    #[serde(default)]
    pub dernier_focus: u64,
    #[serde(default)]
    pub nb_focus: u32,
    #[serde(default)]
    pub temps_actif_ms: u64,
    #[serde(default)]
    pub defilement: f64,
    /// Ancrage : echappe a la decroissance et a la veille.
    #[serde(default)]
    pub ancre: bool,
    /// Mis de cote pour lecture ulterieure : la decroissance est ralentie.
    #[serde(default)]
    pub a_lire: bool,
    #[serde(default = "vitalite_par_defaut")]
    pub vitalite: f32,
    /// Etat d'execution : une webview vit-elle pour cet onglet ?
    #[serde(skip)]
    pub eveille: bool,
    #[serde(skip)]
    pub chargement: bool,
    /// Rendez-vous : l'onglet est mis de cote (cache du rail, en veille) jusqu'a cette heure
    /// (ms), ou il revient de lui-meme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendez_vous: Option<u64>,
    /// Fermeture programmee (ms) : l'onglet se ferme seul a cette heure (il reste rouvrable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fermeture: Option<u64>,
    /// Revenu d'un rendez-vous et pas encore regarde.
    #[serde(skip)]
    pub revenu: bool,
}

fn declencheur_defaut() -> Declencheur {
    Declencheur::Saisie
}

impl Onglet {
    pub fn neuf(id: TabId, url: String, fil: ThreadId, declencheur: Declencheur, now: u64) -> Self {
        Onglet {
            id,
            url,
            titre: String::new(),
            favicon: None,
            fil,
            parent: None,
            declencheur,
            cree_le: now,
            dernier_focus: now,
            nb_focus: 1,
            temps_actif_ms: 0,
            defilement: 0.0,
            ancre: false,
            a_lire: false,
            vitalite: vitalite_par_defaut(),
            eveille: false,
            chargement: false,
            rendez_vous: None,
            fermeture: None,
            revenu: false,
        }
    }

    pub fn domaine(&self) -> String {
        crate::urls::domaine_enregistrable(&self.url)
    }

    pub fn interne(&self) -> bool {
        self.url.starts_with("trame:")
    }

    /// Libelle affiche : le titre, sinon le nom de la page interne, sinon l'URL courte.
    pub fn libelle(&self) -> String {
        if !self.titre.trim().is_empty() {
            return self.titre.clone();
        }
        if let Some(nom) = crate::urls::nom_page_interne(&self.url) {
            return nom.to_string();
        }
        if self.interne() {
            return "Page interne".to_string();
        }
        crate::urls::affichage_court(&self.url)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Fil {
    pub id: ThreadId,
    #[serde(default)]
    pub nom: String,
    /// `true` tant que le nom est genere : il sera remplace a la premiere occasion.
    #[serde(default)]
    pub auto_nom: bool,
    pub cree_le: u64,
    #[serde(default)]
    pub actif_le: u64,
    #[serde(default)]
    pub replie: bool,
    /// Fil cloisonne : ses onglets vivent dans leur propre profil WebView2 — connexions,
    /// cookies et extensions a part. `None` : le profil principal, comme tous les autres fils.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloison: Option<Cloison>,
    /// Fil de navigation privee : profil WebView2 jetable, jamais ecrit dans la session.
    /// Il ne survit donc pas au redemarrage, et c'est exactement ce qu'on lui demande.
    #[serde(default)]
    pub prive: bool,
}

/// Ce qu'un fil cloisonne a en propre.
///
/// WebView2 n'active une extension que pour un **profil** entier, jamais pour un onglet : un fil
/// qui veut ses propres extensions doit donc avoir son propre profil. C'est aussi ce qui separe
/// ses connexions — un compte ouvert dans la cloison ne l'est pas ailleurs, et inversement.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Cloison {
    /// Nom du profil WebView2 (`cloison-<id du fil>`). Son dossier vit dans le profil WebView2
    /// de Trame et survit au decloisonnement : recloisonner retrouve les connexions.
    pub profil: String,
    /// Extensions Chrome installees (identifiants) actives dans ce fil.
    #[serde(default)]
    pub extensions: Vec<String>,
}

impl Cloison {
    pub fn pour_fil(id: ThreadId) -> Self {
        Cloison {
            profil: format!("cloison-{id}"),
            extensions: Vec::new(),
        }
    }
}

impl Fil {
    pub fn neuf(id: ThreadId, nom: String, now: u64) -> Self {
        Fil {
            id,
            nom,
            auto_nom: true,
            cree_le: now,
            actif_le: now,
            replie: false,
            cloison: None,
            prive: false,
        }
    }

    /// Un fil de **navigation privee** : profil WebView2 jetable, onglets jamais ecrits dans la
    /// session, profil efface a la fermeture. Il ne survit pas au redemarrage, par definition.
    pub fn prive(id: ThreadId, now: u64) -> Self {
        Fil {
            id,
            nom: "Navigation privée".into(),
            auto_nom: false,
            cree_le: now,
            actif_le: now,
            replie: false,
            cloison: None,
            prive: true,
        }
    }
}

/// La session telle qu'elle doit etre **ecrite sur le disque** : sans les fils prives, sans
/// leurs onglets, et sans l'onglet actif s'il appartenait a l'un d'eux.
///
/// C'est une fonction pure, et c'est voulu : la regle « une navigation privee ne laisse aucune
/// trace » se verifie alors par un test unitaire, au lieu de dependre d'un chemin de code.
pub fn session_sans_prives(session: &Session) -> Session {
    let prives: Vec<ThreadId> = session.fils.iter().filter(|f| f.prive).map(|f| f.id).collect();
    if prives.is_empty() {
        return session.clone();
    }
    let mut propre = session.clone();
    propre.fils.retain(|f| !f.prive);
    propre.onglets.retain(|o| !prives.contains(&o.fil));
    // L'onglet actif d'un fil prive ne doit pas rester « actif » dans un fichier qui ne le
    // contient pas : au redemarrage, Trame chercherait un onglet inexistant.
    if propre.actif.map(|a| !propre.onglets.iter().any(|o| o.id == a)).unwrap_or(false) {
        propre.actif = propre.onglets.first().map(|o| o.id);
    }
    // Le cote a cote non plus.
    if let Some((g, d)) = propre.cote {
        let present = |id: TabId| propre.onglets.iter().any(|o| o.id == id);
        if !present(g) || !present(d) {
            propre.cote = None;
        }
    }
    propre
}

/// Un onglet tel qu'il est scelle dans une capsule.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OngletCapsule {
    pub url: String,
    #[serde(default)]
    pub titre: String,
    #[serde(default)]
    pub defilement: f64,
    #[serde(default)]
    pub a_lire: bool,
    #[serde(default = "vitalite_par_defaut")]
    pub vitalite: f32,
}

/// Fil scelle, deplacable d'une machine a l'autre (fichier JSON lisible).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Capsule {
    pub version: u32,
    pub nom: String,
    pub cree_le: u64,
    pub onglets: Vec<OngletCapsule>,
}

/// Session persistee.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub onglets: Vec<Onglet>,
    #[serde(default)]
    pub fils: Vec<Fil>,
    #[serde(default)]
    pub actif: Option<TabId>,
    #[serde(default)]
    pub rail_replie: bool,
    #[serde(default)]
    pub prochain_onglet: TabId,
    #[serde(default)]
    pub prochain_fil: ThreadId,
    /// Deux onglets affiches cote a cote (gauche, droite), quand l'un d'eux est actif.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cote: Option<(TabId, TabId)>,
    /// Part de la largeur donnee a la page de gauche (0,2 a 0,8).
    #[serde(default = "ratio_defaut")]
    pub ratio_cote: f64,
    /// Le defilement d'une page du cote a cote entraine l'autre.
    #[serde(default)]
    pub defilement_lie: bool,
}

fn ratio_defaut() -> f64 {
    0.5
}

impl Session {
    /// Range un fil juste avant `avant` (`None` : en dernier).
    ///
    /// L'ordre des fils n'est pas un detail d'affichage : c'est `session.fils` lui-meme, donc il
    /// survit au redemarrage, et c'est lui qui ordonne **les deux** surfaces — les groupes du rail
    /// et les bandes de la trame. Une seule liste, une seule verite.
    ///
    /// Rend `true` si l'ordre a reellement change : un depot sur place (devant le fil lui-meme, ou
    /// juste apres lui) ne doit ni salir la session ni faire redessiner l'interface.
    pub fn deplacer_fil(&mut self, id: ThreadId, avant: Option<ThreadId>) -> bool {
        if avant == Some(id) {
            return false;
        }
        let Some(index) = self.fils.iter().position(|f| f.id == id) else {
            return false;
        };
        let fil = self.fils.remove(index);
        // Un `avant` qui ne designe plus rien (fil ferme entre-temps) vaut « en dernier » :
        // deposer un groupe ne doit jamais le faire disparaitre.
        let cible = avant
            .and_then(|a| self.fils.iter().position(|f| f.id == a))
            .unwrap_or(self.fils.len());
        self.fils.insert(cible, fil);
        cible != index
    }
}

/// Teinte par defaut de la coquille : l'aspect d'origine, celui de `:root`.
fn theme_defaut() -> String {
    "defaut".into()
}

/// Reglages du navigateur, persistes dans `reglages.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Reglages {
    /// Teinte de la coquille : « defaut », une teinte predefinie, ou « dynamique ».
    #[serde(default = "theme_defaut")]
    pub theme: String,
    /// Identifiant du moteur de recherche (`urls::MOTEURS`).
    #[serde(default = "moteur_defaut")]
    pub moteur: String,
    /// Afficher la barre d'extensions sous la Couture.
    #[serde(default = "vrai")]
    pub barre_extensions: bool,
    /// Plein ecran d'une video (YouTube…) : `page` (la zone de la page seulement, comme
    /// WebView2 le fait seul), `fenetre` (toute la fenetre de Trame, rail et barre caches, la
    /// fenetre garde sa taille) ou `ecran` (en plus, la fenetre passe en plein ecran Windows).
    #[serde(default = "plein_ecran_defaut")]
    pub plein_ecran: String,
    /// Barre de la fenetre : `bande` (boutons reduire, agrandir, fermer au bout de la barre
    /// d'adresse, comme Opera GX), `bandeau` (bande fine aux couleurs de Trame au-dessus de
    /// tout), `rail` (trois pastilles en haut du rail) ou `windows` (la barre de titre de
    /// Windows). Les trois premiers retirent la barre de Windows.
    #[serde(default = "barre_fenetre_defaut")]
    pub barre_fenetre: String,
    /// Rail replie, acces rapides : `cases` (icones nues, l'aspect d'origine), `socle` (posees sur
    /// un socle en relief), `ronds` (pastilles rondes) ou `grille` (lanceur 2 x 2). Choix de
    /// Clement sur `docs/maquettes/rail-replie-acces.html` (2026-11-23).
    #[serde(default = "rail_acces_defaut")]
    pub rail_acces: String,
    /// Rail replie, groupes d'onglets : `etiquettes` (nom du groupe en couleur, l'origine),
    /// `cartes` (chaque groupe dans une carte teintee) ou `trait` (un trait de couleur, sans nom).
    #[serde(default = "rail_onglets_defaut")]
    pub rail_onglets: String,
    /// Endormir automatiquement les onglets refroidis.
    #[serde(default = "vrai")]
    pub veille_auto: bool,
    /// Nombre maximal de webviews vivantes (hors surfaces d'interface).
    #[serde(default = "budget_defaut")]
    pub budget: u32,
    /// Choix de l'utilisateur par extension Trame : identifiant -> allumee (`true`) ou coupee
    /// (`false`). Le manifeste ne donne qu'un defaut (`enabled_by_default`) ; ce qui doit
    /// survivre au redemarrage, c'est ce choix-ci, **dans les deux sens** : une extension
    /// livree eteinte qu'on allume doit rester allumee.
    #[serde(default)]
    pub extensions_etat: BTreeMap<String, bool>,
    /// Ancien emplacement de ce choix (une liste d'extensions coupees), ecrit par les versions
    /// precedentes. Relu une fois pour migrer (cf. `Reglages::migrer_extensions`), plus ecrit.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions_desactivees: Vec<String>,
    /// Extensions Chrome epinglees dans la barre (leur popup s'ouvre dans le panneau droit).
    #[serde(default)]
    pub extensions_epinglees: Vec<String>,
    /// Passer en « memoire basse » (WebView2 `MemoryUsageTargetLevel = Low`) un onglet reveille
    /// mais cache depuis un moment : il reste ouvert, a sa place, mais le moteur rend ce qu'il
    /// peut de sa memoire. C'est le palier entre « ouvert » et « en veille ».
    #[serde(default = "vrai")]
    pub memoire_basse: bool,
    /// Le budget d'onglets ouverts est un **plafond** : quand la memoire libre du PC passe sous
    /// la reserve, Trame endort les onglets les plus froids, sans jamais depasser ce plafond.
    #[serde(default = "vrai")]
    pub budget_auto: bool,
    /// Cout memoire mesure de chaque extension Chrome (identifiant -> mesure), par difference
    /// au moment ou elle s'allume ou s'eteint dans le profil principal.
    #[serde(default)]
    pub extensions_cout: BTreeMap<String, CoutExtension>,
    /// Retirer des adresses les parametres de pistage (`urls::nettoyer`).
    #[serde(default = "vrai")]
    pub nettoyer_adresses: bool,
    /// Garder les identifiants d'affiliation (`tag=`...), qui remunerent un createur.
    #[serde(default = "vrai")]
    pub garder_affiliation: bool,
    /// Protection anti-pistage de WebView2 : "aucune", "equilibree" ou "stricte".
    #[serde(default = "pistage_defaut")]
    pub anti_pistage: String,
    /// Mettre en pause (scripts, minuteries) un onglet cache depuis `pause_apres_s`, sauf s'il
    /// joue du son.
    #[serde(default = "vrai")]
    pub pause_onglets: bool,
    /// Garder, sur le PC, le texte des pages lues pour les retrouver par une phrase.
    #[serde(default = "vrai")]
    pub memoire_lecture: bool,
    /// Sites (domaines) jamais gardes dans la memoire de lecture.
    #[serde(default)]
    pub lecture_exclus: Vec<String>,
    /// Paquets de regles du zappeur deja proposes une fois (`paquets_zap`) : un paquet n'est
    /// ajoute qu'une fois, pour qu'une regle que l'utilisateur a supprimee ne revienne pas.
    #[serde(default)]
    pub paquets_zap: Vec<String>,
    /// Dossier des telechargements choisi par l'utilisateur ; vide = le dossier
    /// « Telechargements » de Windows.
    #[serde(default)]
    pub dossier_telechargements: String,
    /// Sauvegarde automatique du profil : une archive `.trame` deposee dans
    /// `dossier_sauvegardes` toutes les `sauvegarde_heures` heures, sans rien demander.
    #[serde(default)]
    pub sauvegarde_auto: bool,
    /// Dossier des sauvegardes automatiques, choisi par l'utilisateur. **Vide = pas encore
    /// choisi**, et dans ce cas rien ne part : une sauvegarde qu'on ne sait pas ou mettre ne
    /// doit pas atterrir au hasard (le dossier de l'application serait efface par une
    /// desinstallation).
    #[serde(default)]
    pub dossier_sauvegardes: String,
    /// Intervalle entre deux sauvegardes automatiques, en heures.
    #[serde(default = "sauvegarde_heures_defaut")]
    pub sauvegarde_heures: u32,
    /// Nombre de sauvegardes automatiques gardees ; au-dela, les plus anciennes sont effacees
    /// (seulement celles que Trame a ecrites : prefixe et extension verifies).
    #[serde(default = "sauvegarde_garder_defaut")]
    pub sauvegarde_garder: u32,
    /// Astuces au bon moment (`src/aide.rs`) : une seule a la fois, au plus une par jour,
    /// jamais deux fois la meme. Demande de Clement (2026-11-24).
    #[serde(default = "vrai")]
    pub astuces: bool,
    /// Astuces deja montrees (identifiants de `ui/notice.js`) : chacune ne sort qu'une fois.
    #[serde(default)]
    pub astuces_vues: Vec<String>,
    /// Instant (ms) de la derniere astuce montree, pour en espacer deux d'au moins un jour.
    #[serde(default)]
    pub astuce_derniere: u64,
}

fn sauvegarde_heures_defaut() -> u32 {
    24
}

fn sauvegarde_garder_defaut() -> u32 {
    5
}

/// Frequences proposees pour la sauvegarde automatique, en heures.
pub const SAUVEGARDE_HEURES: [u32; 6] = [1, 3, 12, 24, 72, 168];
/// Nombres de sauvegardes a garder proposes.
pub const SAUVEGARDE_GARDER: [u32; 4] = [3, 5, 10, 20];

fn plein_ecran_defaut() -> String {
    "fenetre".into()
}

fn barre_fenetre_defaut() -> String {
    "bande".into()
}

/// Les styles de barre de fenetre acceptes (`Reglages::barre_fenetre`).
pub const BARRES_FENETRE: [&str; 4] = ["bande", "bandeau", "rail", "windows"];

fn rail_acces_defaut() -> String {
    "cases".into()
}

fn rail_onglets_defaut() -> String {
    "etiquettes".into()
}

/// Styles des acces rapides du rail replie (`Reglages::rail_acces`).
pub const RAIL_ACCES: [&str; 4] = ["cases", "socle", "ronds", "grille"];
/// Styles des groupes d'onglets du rail replie (`Reglages::rail_onglets`).
pub const RAIL_ONGLETS: [&str; 3] = ["etiquettes", "cartes", "trait"];

impl Reglages {
    /// Le style des acces du rail replie, ramene a une valeur connue.
    pub fn rail_acces(&self) -> &str {
        if RAIL_ACCES.contains(&self.rail_acces.as_str()) {
            &self.rail_acces
        } else {
            "cases"
        }
    }

    /// Le style des groupes d'onglets du rail replie, ramene a une valeur connue.
    pub fn rail_onglets(&self) -> &str {
        if RAIL_ONGLETS.contains(&self.rail_onglets.as_str()) {
            &self.rail_onglets
        } else {
            "etiquettes"
        }
    }

    /// Le style de barre de fenetre, ramene a une valeur connue.
    pub fn barre_fenetre(&self) -> &str {
        if BARRES_FENETRE.contains(&self.barre_fenetre.as_str()) {
            &self.barre_fenetre
        } else {
            "bande"
        }
    }
}

fn pistage_defaut() -> String {
    "stricte".to_string()
}

/// Ce qu'une extension coute, mesure et date.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CoutExtension {
    pub mo: f64,
    /// Horodatage de la mesure (ms).
    pub quand: u64,
}

/// Memoire a laisser libre au systeme (Mo) avant de restreindre les onglets ouverts.
pub const RESERVE_MEMOIRE_MO: f64 = 1500.0;
/// Cout d'un onglet ouvert (Mo) : mesure du 2026-11-20, 112 Mo par onglet reveille, arrondi
/// au-dessus pour garder de la marge.
pub const COUT_ONGLET_MO: f64 = 120.0;

/// Nombre d'onglets qu'on peut garder ouverts, d'apres la memoire libre.
///
/// Le resultat est **absolu**, calcule au moment de la mesure : apres avoir endormi des onglets,
/// on attend la mesure suivante (4 s) au lieu de recalculer depuis une memoire libre perimee,
/// ce qui endormirait trop d'onglets d'un coup. Jamais au-dessus du `plafond` choisi par
/// l'utilisateur, jamais sous 2 ; une mesure absente (`dispo_mo <= 0`) laisse le plafond.
pub fn budget_adapte(plafond: usize, ouverts: usize, dispo_mo: f64) -> usize {
    if !dispo_mo.is_finite() || dispo_mo <= 0.0 {
        return plafond;
    }
    let marge = dispo_mo - RESERVE_MEMOIRE_MO;
    let cible = if marge >= 0.0 {
        ouverts + (marge / COUT_ONGLET_MO).floor() as usize
    } else {
        ouverts.saturating_sub((-marge / COUT_ONGLET_MO).ceil() as usize)
    };
    cible.clamp(2, plafond.max(2))
}

fn moteur_defaut() -> String {
    crate::urls::MOTEUR_DEFAUT.to_string()
}

fn vrai() -> bool {
    true
}

fn budget_defaut() -> u32 {
    6
}

impl Reglages {
    /// L'intervalle de sauvegarde, ramene a une valeur raisonnable : sous l'heure on
    /// ecraserait le disque, au-dela d'une semaine ce n'est plus une sauvegarde.
    pub fn sauvegarde_heures_bornees(&self) -> u32 {
        self.sauvegarde_heures.clamp(1, 168)
    }

    /// Le nombre de sauvegardes gardees, borne : zero effacerait la sauvegarde qu'on vient
    /// d'ecrire (le contraire du but), et au-dela de 50 le dossier devient un archiveur.
    pub fn sauvegarde_garder_bornees(&self) -> u32 {
        self.sauvegarde_garder.clamp(1, 50)
    }

    /// Vrai si la sauvegarde automatique peut reellement tourner : l'interrupteur **et** un
    /// dossier choisi.
    pub fn sauvegarde_auto_prete(&self) -> bool {
        self.sauvegarde_auto && !self.dossier_sauvegardes.trim().is_empty()
    }

    /// Borne le budget : en dessous de 2, la navigation devient penible ; au-dela de 16,
    /// la veille ne sert plus a rien sur une machine a 8 Go.
    pub fn budget_borne(&self) -> usize {
        self.budget.clamp(2, 16) as usize
    }

    /// Etat effectif d'une extension : le choix memorise s'il existe, sinon le defaut du
    /// manifeste. Une seule question a poser, un seul endroit pour y repondre.
    pub fn extension_active(&self, id: &str, defaut: bool) -> bool {
        self.extensions_etat.get(id).copied().unwrap_or(defaut)
    }

    /// Memorise le choix de l'utilisateur pour une extension, dans les deux sens.
    pub fn memoriser_extension(&mut self, id: &str, actif: bool) {
        self.extensions_etat.insert(id.to_string(), actif);
    }

    /// Reprend l'ancien champ `extensions_desactivees` dans la carte des choix explicites.
    ///
    /// Sans cela, un profil ecrit par une version precedente rouvrirait les extensions que
    /// l'utilisateur avait coupees. Rend le nombre de choix repris.
    pub fn migrer_extensions(&mut self) -> usize {
        let anciennes: Vec<String> = self.extensions_desactivees.drain(..).collect();
        for id in &anciennes {
            // Un choix explicite deja present est plus recent : on ne l'ecrase pas.
            self.extensions_etat.entry(id.clone()).or_insert(false);
        }
        anciennes.len()
    }
}

impl Default for Reglages {
    fn default() -> Self {
        Reglages {
            theme: theme_defaut(),
            moteur: moteur_defaut(),
            barre_extensions: true,
            plein_ecran: plein_ecran_defaut(),
            barre_fenetre: barre_fenetre_defaut(),
            rail_acces: rail_acces_defaut(),
            rail_onglets: rail_onglets_defaut(),
            veille_auto: true,
            budget: budget_defaut(),
            extensions_etat: BTreeMap::new(),
            extensions_desactivees: Vec::new(),
            extensions_epinglees: Vec::new(),
            memoire_basse: true,
            budget_auto: true,
            extensions_cout: BTreeMap::new(),
            nettoyer_adresses: true,
            garder_affiliation: true,
            anti_pistage: pistage_defaut(),
            pause_onglets: true,
            memoire_lecture: true,
            lecture_exclus: Vec::new(),
            paquets_zap: Vec::new(),
            dossier_telechargements: String::new(),
            sauvegarde_auto: false,
            dossier_sauvegardes: String::new(),
            sauvegarde_heures: sauvegarde_heures_defaut(),
            sauvegarde_garder: sauvegarde_garder_defaut(),
            astuces: true,
            astuces_vues: Vec::new(),
            astuce_derniere: 0,
        }
    }
}

/// Une regle de masquage : un selecteur CSS cache, sur un domaine donne.
///
/// C'est l'equivalent du « zappeur » d'un bloqueur de publicite : l'utilisateur designe un
/// element dans la page, Trame en fait une regle qu'il peut relire, couper ou supprimer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegleZap {
    /// Domaine enregistrable (`youtube.com`). Vide = tous les sites (a eviter). Absent d'une
    /// regle importee : le site affiche (`regles_importees`).
    #[serde(default)]
    pub domaine: String,
    #[serde(default)]
    pub selecteur: String,
    /// Libelle lisible, facultatif (ce que l'element semblait etre).
    #[serde(default)]
    pub libelle: String,
    #[serde(default = "vrai")]
    pub actif: bool,
    /// Condition de texte : seuls les elements du selecteur dont le texte vaut exactement ceci
    /// sont caches (la puce « Shorts » des filtres de YouTube n'a aucun attribut qui la
    /// distingue). Le CSS ne sait pas lire un texte : ces regles passent par le script vivant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub texte: String,
    /// Paquet d'origine (`youtube-sans-shorts`…), vide pour une regle faite a la main.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub paquet: String,
    /// Derniere fois (ms) que la regle a attrape au moins un element : une regle qui n'attrape
    /// plus rien depuis longtemps est sans doute cassee (le site a change ses noms).
    #[serde(default, skip_serializing_if = "est_zero")]
    pub vu_le: u64,
    /// Ce que la regle fait de l'element : vide = masquer (le zappeur d'origine), sinon
    /// `elargir`, `replier`, `garder` (seulement ceci), `texte` (le reecrire), `style`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub action: String,
    /// Parametre de l'action : le nouveau texte (`texte`), ou les declarations (`style`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub valeur: String,
}

/// Les actions qu'une regle sait faire, avec leur nom affiche.
pub const ACTIONS_ZAP: &[(&str, &str)] = &[
    ("", "masqué"),
    ("elargir", "élargi"),
    ("replier", "replié"),
    ("garder", "seul gardé"),
    ("texte", "texte réécrit"),
    ("style", "restylé"),
    ("haut", "remonté en premier"),
    ("bas", "descendu en dernier"),
];

/// Nom de domaine saisi a la main (« Copier vers… ») ramene a la forme des regles : sans
/// schema, sans chemin, sans `www.`, en minuscules. `None` si ce n'est pas un domaine.
pub fn domaine_saisi(saisie: &str) -> Option<String> {
    let mut d = saisie.trim().to_ascii_lowercase();
    if let Some(pos) = d.find("://") {
        d = d[pos + 3..].to_string();
    }
    let d = d.split(['/', '?', '#', ':']).next().unwrap_or("");
    let d = d.strip_prefix("www.").unwrap_or(d);
    let valide = d.contains('.')
        && d.len() <= 253
        && !d.starts_with('.')
        && !d.ends_with('.')
        && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    valide.then(|| d.to_string())
}

/// Copie les regles d'un domaine vers un autre, sans doublon. Les copies ne sont rattachees a
/// aucun paquet (le paquet reste celui du site d'origine). Rend le nombre de regles copiees.
pub fn copier_regles_zap(regles: &mut Vec<RegleZap>, de: &str, vers: &str) -> usize {
    let a_copier: Vec<RegleZap> = regles.iter().filter(|r| r.domaine == de).cloned().collect();
    let mut n = 0;
    for mut r in a_copier {
        r.domaine = vers.to_string();
        r.paquet.clear();
        r.vu_le = 0;
        let doublon = regles.iter().any(|e| {
            e.domaine == r.domaine && e.selecteur == r.selecteur && e.texte == r.texte && e.action == r.action && e.valeur == r.valeur
        });
        if !doublon {
            regles.push(r);
            n += 1;
        }
    }
    n
}

/// Regles importees depuis un texte JSON (une liste, comme `zap.json`, ou un seul objet).
/// Chaque regle est controlee : action connue, selecteur non vide, domaine valide ou vide
/// (alors le domaine `par_defaut`), champs bornes. Au plus 300 regles.
pub fn regles_importees(texte: &str, par_defaut: &str) -> Result<Vec<RegleZap>, String> {
    let valeur: serde_json::Value =
        serde_json::from_str(texte.trim()).map_err(|e| format!("texte illisible : {e}"))?;
    let liste = match valeur {
        serde_json::Value::Array(l) => l,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => return Err("il faut une liste de règles".into()),
    };
    let borne = |s: &str, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
    let mut out = Vec::new();
    for v in liste.into_iter().take(300) {
        let Ok(r) = serde_json::from_value::<RegleZap>(v) else { continue };
        if r.selecteur.trim().is_empty() || !ACTIONS_ZAP.iter().any(|(a, _)| *a == r.action) {
            continue;
        }
        let domaine = if r.domaine.trim().is_empty() {
            par_defaut.to_string()
        } else {
            match domaine_saisi(&r.domaine) {
                Some(d) => d,
                None => continue,
            }
        };
        out.push(RegleZap {
            domaine,
            selecteur: borne(&r.selecteur, 400),
            libelle: borne(&r.libelle, 80),
            texte: borne(&r.texte, 80),
            valeur: borne(&r.valeur, 200),
            vu_le: 0,
            paquet: String::new(),
            ..r
        });
    }
    if out.is_empty() {
        return Err("aucune règle utilisable dans ce texte".into());
    }
    Ok(out)
}

/// Proprietes qu'une regle `style` a le droit de poser : de l'apparence, jamais de position
/// ni de contenu (`content`, `background-image: url(…)` feraient charger ou afficher n'importe
/// quoi). Une valeur ne garde que des caracteres simples.
const PROPRIETES_STYLE: &[&str] = &[
    "color",
    "background-color",
    "font-size",
    "font-weight",
    "line-height",
    "letter-spacing",
    "opacity",
    "filter",
];

/// Declarations `style` nettoyees : `prop: valeur` separees par `;`, proprietes de la liste,
/// valeurs sans `url(`, accolade, chevron, barre oblique inverse ni guillemet.
pub fn style_sur(valeur: &str) -> String {
    let mut out = Vec::new();
    for decl in valeur.split(';') {
        let Some((p, v)) = decl.split_once(':') else { continue };
        let p = p.trim().to_ascii_lowercase();
        let v = v.trim().trim_end_matches("!important").trim();
        if !PROPRIETES_STYLE.contains(&p.as_str()) || v.is_empty() || v.len() > 60 {
            continue;
        }
        if v.to_ascii_lowercase().contains("url(")
            || v.contains(['{', '}', '<', '>', '\\', '"', '\'', ';'])
            || v.chars().any(|c| c.is_control())
        {
            continue;
        }
        out.push(format!("{p}: {v} !important"));
    }
    out.join("; ")
}

fn est_zero(n: &u64) -> bool {
    *n == 0
}

/// Un ensemble de regles pret a l'emploi, pour un site.
pub struct PaquetZap {
    pub id: &'static str,
    pub titre: &'static str,
    pub regles: Vec<RegleZap>,
}

/// Les paquets livres avec Trame. Selecteurs releves sur YouTube le 2026-11-22 (recherche :
/// grille `grid-shelf-view-model` de 40 Shorts ; menu : `a[href="/shorts/"]` ; puce de filtre
/// sans attribut distinctif, d'ou la condition de texte).
pub fn paquets_zap() -> Vec<PaquetZap> {
    let yt = |selecteur: &str, libelle: &str| RegleZap {
        paquet: "youtube-sans-shorts".into(),
        ..RegleZap::nouvelle("youtube.com", selecteur, libelle)
    };
    vec![PaquetZap {
        id: "youtube-sans-shorts",
        titre: "YouTube sans Shorts",
        regles: vec![
            yt("ytd-rich-shelf-renderer[is-shorts]", "Rayon Shorts (accueil, abonnements)"),
            yt("ytd-rich-section-renderer:has(ytm-shorts-lockup-view-model)", "Section Shorts de l'accueil"),
            yt("ytd-reel-shelf-renderer", "Etageres de Shorts"),
            yt("ytd-rich-item-renderer:has(a[href^=\"/shorts\"])", "Vignettes de Shorts dans le fil"),
            yt("grid-shelf-view-model:has(ytm-shorts-lockup-view-model)", "Grille de Shorts (recherche)"),
            yt("ytd-video-renderer:has(a[href^=\"/shorts/\"])", "Short isole dans les resultats"),
            yt("yt-lockup-view-model:has(a[href^=\"/shorts/\"])", "Short dans les suggestions"),
            yt("ytd-compact-video-renderer:has(a[href^=\"/shorts/\"])", "Short dans les suggestions (ancien)"),
            yt("ytd-guide-entry-renderer:has(a[href=\"/shorts/\"])", "Entree Shorts du menu"),
            yt("ytd-mini-guide-entry-renderer:has(a[href=\"/shorts/\"])", "Entree Shorts du menu replie"),
            yt("yt-tab-shape[tab-title=\"Shorts\"]", "Onglet Shorts d'une chaine"),
            RegleZap {
                texte: "Shorts".into(),
                ..yt("yt-chip-cloud-chip-renderer", "Filtre « Shorts » de la recherche")
            },
        ],
    }]
}

/// Ajoute a `regles` les paquets pas encore proposes (`deja`), sans doublon : une regle
/// identique deja presente est seulement rattachee au paquet. Rend le nombre de regles ajoutees
/// et les paquets traites.
pub fn installer_paquets_zap(regles: &mut Vec<RegleZap>, deja: &[String]) -> (usize, Vec<String>) {
    let mut ajoutees = 0;
    let mut traites = Vec::new();
    for paquet in paquets_zap() {
        if deja.iter().any(|d| d == paquet.id) {
            continue;
        }
        for r in paquet.regles {
            match regles
                .iter_mut()
                .find(|e| e.domaine == r.domaine && e.selecteur == r.selecteur && e.texte == r.texte)
            {
                Some(existante) => {
                    if existante.paquet.is_empty() {
                        existante.paquet = r.paquet.clone();
                    }
                }
                None => {
                    regles.push(r);
                    ajoutees += 1;
                }
            }
        }
        traites.push(paquet.id.to_string());
    }
    (ajoutees, traites)
}

impl RegleZap {
    pub fn nouvelle(domaine: &str, selecteur: &str, libelle: &str) -> Self {
        RegleZap {
            domaine: domaine.to_string(),
            selecteur: selecteur.to_string(),
            libelle: libelle.to_string(),
            actif: true,
            texte: String::new(),
            paquet: String::new(),
            vu_le: 0,
            action: String::new(),
            valeur: String::new(),
        }
    }

    /// Nom affiche de l'action (« masqué », « élargi »…).
    pub fn nom_action(&self) -> &'static str {
        ACTIONS_ZAP
            .iter()
            .find(|(a, _)| *a == self.action)
            .map(|(_, n)| *n)
            .unwrap_or("masqué")
    }

    /// Selecteur nettoye : on ne garde que ce qui precede la premiere accolade.
    ///
    /// Sans cela, un selecteur contenant `{` ou `}` referme le bloc CSS et **casse toutes les
    /// regles suivantes** de la page. Tronquer est plus sur que filtrer : `a} b { color: red`
    /// filtré donnerait `a b color: red`, un selecteur invalide que le navigateur jette en
    /// silence — la regle ne servirait a rien.
    pub fn selecteur_sur(&self) -> String {
        let coupe = self
            .selecteur
            .split(['{', '}'])
            .next()
            .unwrap_or("")
            .to_string();
        coupe
            .chars()
            .filter(|c| !matches!(c, '<' | '>') && !c.is_control())
            .collect::<String>()
            .trim()
            .to_string()
    }

    pub fn css(&self) -> String {
        let selecteur = self.selecteur_sur();
        // Une condition de texte ne s'exprime pas en CSS : le script vivant s'en charge.
        if selecteur.is_empty() || !self.texte.is_empty() {
            return String::new();
        }
        match self.action.as_str() {
            "" => format!("{selecteur} {{ display: none !important; }}"),
            "elargir" => format!(
                "{selecteur} {{ width: 100% !important; max-width: none !important; flex: 1 1 auto !important; }}"
            ),
            // Replie : une bande de 2,6 lignes, attenuee ; le script vivant deplie au clic.
            "replier" => format!(
                "{selecteur}:not([data-trame-deplie]) {{ max-height: 2.6em !important; overflow: hidden !important; opacity: .55 !important; cursor: zoom-in !important; }}"
            ),
            // Seulement ceci : tout ce qui n'est ni l'element, ni dedans, ni autour, disparait.
            // `:has()` ne s'imbrique pas : un selecteur qui en contient deja un est refuse.
            "garder" if !selecteur.contains(":has(") => format!(
                "body :not(:has({selecteur})):not({selecteur}):not({selecteur} *) {{ display: none !important; }}"
            ),
            // Deplacer sans toucher au document : `order` reordonne un enfant de flex ou de grille
            // (le selecteur ne le propose que la). Ailleurs, la regle est sans effet, sans casse.
            "haut" => format!("{selecteur} {{ order: -9999 !important; }}"),
            "bas" => format!("{selecteur} {{ order: 9999 !important; }}"),
            "style" => {
                let decl = style_sur(&self.valeur);
                if decl.is_empty() {
                    String::new()
                } else {
                    format!("{selecteur} {{ {decl}; }}")
                }
            }
            _ => String::new(),
        }
    }
}

/// Jeu de regles par defaut, ecrit au premier lancement.
///
/// Selecteurs YouTube publics et stables (cales sur les composants de la page) ; ils peuvent
/// changer un jour — c'est justement pour cela que le panneau permet de les corriger a la main
/// plutot que de les cacher dans un binaire.
pub fn regles_par_defaut() -> Vec<RegleZap> {
    let mut out = Vec::new();
    let mut ajouter = |domaine: &str, selecteur: &str, libelle: &str| {
        out.push(RegleZap::nouvelle(domaine, selecteur, libelle));
    };
    // YouTube
    ajouter("youtube.com", "ytd-rich-shelf-renderer[is-shorts]", "Rayon Shorts (accueil)");
    ajouter("youtube.com", "ytd-reel-shelf-renderer", "Etageres de Shorts");
    ajouter("youtube.com", "ytd-rich-item-renderer:has(a[href^=\"/shorts\"])", "Vignettes de Shorts");
    ajouter("youtube.com", "#masthead-ad", "Banniere publicitaire en tete");
    ajouter("youtube.com", "ytd-ad-slot-renderer", "Emplacement publicitaire");
    ajouter("youtube.com", "ytd-display-ad-renderer", "Annonce dans le fil");
    ajouter("youtube.com", "ytd-promoted-sparkles-web-renderer", "Annonce sponsorisee");
    ajouter("youtube.com", "ytd-in-feed-ad-layout-renderer", "Annonce dans le fil (2)");
    ajouter("youtube.com", "ytd-companion-slot-renderer", "Encart a cote du lecteur");
    // Generique
    ajouter("", "ins.adsbygoogle", "Emplacement Google Ads");
    ajouter("", "[id^=\"div-gpt-ad\"]", "Emplacement Google Publisher");
    ajouter("", "[class*=\"-ad-slot\"]", "Emplacement publicitaire generique");
    out
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntreeHistorique {    pub url: String,
    #[serde(default)]
    pub titre: String,
    pub ts: u64,
}

/// Manifeste d'extension (`manifest.json`).
#[derive(Clone, Debug, Deserialize)]
pub struct Manifeste {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Motifs `*://*.exemple.fr/*`.
    #[serde(default)]
    pub matches: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
    #[serde(default)]
    pub css: Vec<String>,
    /// Scripts injectes au **debut** de chaque document qui correspond (avant les scripts de la
    /// page, dans son monde). Pour agir avant le site (ex. retirer les pubs d'une reponse avant
    /// que le lecteur ne la lise). Pris en compte a la creation d'un onglet.
    #[serde(default)]
    pub js_debut: Vec<String>,
    /// Adresses refusees au niveau reseau tant que l'extension est allumee
    /// (motifs `*://*.exemple.fr/chemin*`, comme `matches`).
    #[serde(default)]
    pub bloquer: Vec<String>,
    /// Seul `document_end` est honore (cf. docs/FORMAT-EXTENSION.md).
    #[serde(default = "run_at_defaut")]
    pub run_at: String,
    /// Couleur d'accent dans l'interface.
    #[serde(default)]
    pub accent: Option<String>,
    /// Activee au premier chargement. Une extension qui modifie l'apparence des pages doit
    /// etre livree desactivee : c'est a l'utilisateur de l'allumer.
    #[serde(default = "vrai")]
    pub enabled_by_default: bool,
}

fn run_at_defaut() -> String {
    "document_end".to_string()
}

/// Extension chargee (manifeste + fichiers lus).
#[derive(Clone, Debug)]
pub struct Extension {
    pub id: String,
    pub dossier: std::path::PathBuf,
    pub manifeste: Manifeste,
    pub js: Vec<String>,
    /// Contenu des scripts `js_debut`.
    pub js_debut: Vec<String>,
    pub css: String,
    pub active: bool,
    /// Motifs deja compiles (`hote`, `chemin`).
    pub motifs: Vec<Motif>,
    /// Motifs `bloquer` compiles, avec leur texte d'origine (filtre WebView2).
    pub bloque: Vec<(String, Motif)>,
}

#[derive(Clone, Debug)]
pub struct Motif {
    pub tout_schema: bool,
    pub schema: String,
    pub hote: String,
    pub chemin: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Une session privee ne laisse rien sur le disque : c'est la regle, et elle se verifie ici
    /// plutot qu'en esperant qu'aucun chemin de code ne l'oublie.
    #[test]
    fn la_session_ecrite_ne_contient_aucun_onglet_prive() {
        let mut s = Session::default();
        s.fils.push(Fil::neuf(1, "Travail".into(), 1_000));
        s.fils.push(Fil::prive(2, 2_000));
        let mut public = Onglet::neuf(10, "https://a.fr/".into(), 1, Declencheur::Saisie, 1_000);
        public.fil = 1;
        let mut secret = Onglet::neuf(11, "https://banque.fr/".into(), 1, Declencheur::Saisie, 2_000);
        secret.fil = 2;
        s.onglets.push(public);
        s.onglets.push(secret);
        s.actif = Some(11); // l'onglet prive est celui qui est actif

        let propre = session_sans_prives(&s);
        assert_eq!(propre.fils.len(), 1, "le fil prive ne s'ecrit pas");
        assert!(propre.fils.iter().all(|f| !f.prive));
        assert_eq!(propre.onglets.len(), 1, "l'onglet prive ne s'ecrit pas");
        assert_eq!(propre.onglets[0].id, 10);
        assert_eq!(
            propre.actif,
            Some(10),
            "l'onglet actif retombe sur un onglet qui existe vraiment"
        );
        // La session en memoire n'est pas touchee : le privé reste utilisable jusqu'a la fermeture.
        assert_eq!(s.fils.len(), 2);
        assert_eq!(s.onglets.len(), 2);
    }

    #[test]
    fn une_session_sans_prive_est_rendue_telle_quelle() {
        let mut s = Session::default();
        s.fils.push(Fil::neuf(1, "Travail".into(), 1_000));
        let mut o = Onglet::neuf(10, "https://a.fr/".into(), 1, Declencheur::Saisie, 1_000);
        o.fil = 1;
        s.onglets.push(o);
        s.actif = Some(10);
        let propre = session_sans_prives(&s);
        assert_eq!(propre.fils.len(), 1);
        assert_eq!(propre.onglets.len(), 1);
        assert_eq!(propre.actif, Some(10));
    }

    /// Un cote a cote dont un cote etait prive est abandonne : au redemarrage, Trame
    /// chercherait un onglet absent et refuserait d'afficher.
    #[test]
    fn un_cote_a_cote_prive_ne_survit_pas() {
        let mut s = Session::default();
        s.fils.push(Fil::prive(2, 1_000));
        let mut a = Onglet::neuf(10, "https://a.fr/".into(), 1, Declencheur::Saisie, 1_000);
        a.fil = 0;
        let mut b = Onglet::neuf(11, "https://b.fr/".into(), 2, Declencheur::Saisie, 1_000);
        b.fil = 2;
        s.onglets.push(a);
        s.onglets.push(b);
        s.cote = Some((10, 11));
        let propre = session_sans_prives(&s);
        assert_eq!(propre.cote, None);
    }

    #[test]
    fn les_reglages_de_sauvegarde_sont_bornes_et_pret_seulement_avec_un_dossier() {
        let mut r = Reglages::default();
        assert_eq!(r.sauvegarde_heures_bornees(), 24, "defaut : une fois par jour");
        assert_eq!(r.sauvegarde_garder_bornees(), 5);
        assert!(!r.sauvegarde_auto_prete(), "coupee par defaut");

        r.sauvegarde_auto = true;
        assert!(!r.sauvegarde_auto_prete(), "allumee sans dossier : rien ne part");
        r.dossier_sauvegardes = "   ".into();
        assert!(!r.sauvegarde_auto_prete(), "un dossier blanc ne compte pas");
        r.dossier_sauvegardes = "D:\\sauvegardes".into();
        assert!(r.sauvegarde_auto_prete());

        r.sauvegarde_heures = 0;
        assert_eq!(r.sauvegarde_heures_bornees(), 1, "zero heure martelerait le disque");
        r.sauvegarde_heures = 10_000;
        assert_eq!(r.sauvegarde_heures_bornees(), 168);
        r.sauvegarde_garder = 0;
        assert_eq!(r.sauvegarde_garder_bornees(), 1, "garder zero effacerait tout");
        r.sauvegarde_garder = 999;
        assert_eq!(r.sauvegarde_garder_bornees(), 50);
    }

    #[test]
    fn le_budget_s_adapte_a_la_memoire_libre_sans_depasser_le_plafond() {
        // Beaucoup de memoire libre : le plafond choisi s'applique.
        assert_eq!(budget_adapte(6, 3, 12_000.0), 6);
        // Juste au-dessus de la reserve : on peut ouvrir 1 onglet de plus (120 Mo de marge).
        assert_eq!(budget_adapte(6, 3, RESERVE_MEMOIRE_MO + 130.0), 4);
        // A la reserve exacte : on garde ce qui est ouvert, rien de plus.
        assert_eq!(budget_adapte(6, 4, RESERVE_MEMOIRE_MO), 4);
        // 300 Mo sous la reserve : 3 onglets a endormir (300 / 120 arrondi au-dessus).
        assert_eq!(budget_adapte(6, 6, RESERVE_MEMOIRE_MO - 300.0), 3);
        // Jamais sous 2, meme machine a genoux.
        assert_eq!(budget_adapte(6, 3, 200.0), 2);
        // Mesure absente ou absurde : le plafond, sans rien endormir par erreur.
        assert_eq!(budget_adapte(6, 6, 0.0), 6);
        assert_eq!(budget_adapte(6, 6, f64::NAN), 6);
    }

    #[test]
    fn un_fil_sans_cloison_se_relit_et_une_cloison_survit_au_disque() {
        // Session ecrite avant les cloisons : le champ manque, le fil est dans le profil principal.
        let ancien: Fil = serde_json::from_str(
            r#"{"id":3,"nom":"Home lab","auto_nom":false,"cree_le":1,"actif_le":2,"replie":false}"#,
        )
        .unwrap();
        assert_eq!(ancien.cloison, None);
        // Un fil non cloisonne n'ecrit pas le champ : les sessions restent lisibles par une
        // version precedente.
        assert!(!serde_json::to_string(&ancien).unwrap().contains("cloison"));

        let mut f = Fil::neuf(7, "Comptes".into(), 10);
        let mut c = Cloison::pour_fil(7);
        c.extensions.push("nngceckbapebfimnlniiiahkandclblb".into());
        f.cloison = Some(c.clone());
        let relu: Fil = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(relu.cloison, Some(c));
        assert_eq!(relu.cloison.unwrap().profil, "cloison-7");
    }

    #[test]
    fn ranger_un_fil_change_l_ordre_et_le_range_devant_le_fil_vise() {
        let mut s = Session::default();
        for id in [1, 2, 3] {
            s.fils.push(Fil::neuf(id, format!("Fil {id}"), 1_000 + id as u64));
        }
        let ordre = |s: &Session| s.fils.iter().map(|f| f.id).collect::<Vec<_>>();

        // Le dernier devant le premier, comme on glisse un groupe en haut du rail.
        assert!(s.deplacer_fil(3, Some(1)));
        assert_eq!(ordre(&s), vec![3, 1, 2]);
        // Vers le bas : `None` = en dernier.
        assert!(s.deplacer_fil(3, None));
        assert_eq!(ordre(&s), vec![1, 2, 3]);
        // Deposer sur sa propre place ne change rien et le dit.
        assert!(!s.deplacer_fil(1, Some(1)), "devant soi-meme : aucun changement");
        assert!(!s.deplacer_fil(1, Some(2)), "juste apres soi : aucun changement");
        assert_eq!(ordre(&s), vec![1, 2, 3]);
        // Un fil inconnu n'invente rien ; un `avant` disparu vaut « en dernier ».
        assert!(!s.deplacer_fil(99, Some(1)));
        assert!(s.deplacer_fil(1, Some(99)));
        assert_eq!(ordre(&s), vec![2, 3, 1]);

        // L'ordre vit dans la session : il survit a un redemarrage, et c'est la meme liste qui
        // ordonne le rail et les bandes de la trame.
        let relu: Session = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(ordre(&relu), vec![2, 3, 1]);
    }

    #[test]
    fn le_budget_de_webviews_est_borne() {
        let borne = |b: u32| {
            let mut r = Reglages::default();
            r.budget = b;
            r.budget_borne()
        };
        assert_eq!(borne(0), 2, "un budget nul rendrait la navigation penible");
        assert_eq!(borne(1), 2);
        assert_eq!(borne(6), 6);
        assert_eq!(borne(99), 16, "au-dela, la veille ne sert plus a rien");
    }

    #[test]
    fn les_reglages_par_defaut_sont_ceux_annonces_a_l_utilisateur() {
        let r = Reglages::default();
        assert_eq!(r.moteur, crate::urls::MOTEUR_DEFAUT);
        assert!(r.barre_extensions);
        assert!(r.veille_auto);
        assert_eq!(r.budget, 6);
        assert!(r.extensions_etat.is_empty());
        assert!(r.extensions_desactivees.is_empty());
        assert!(r.extensions_epinglees.is_empty());
    }

    #[test]
    fn le_choix_d_activation_prime_sur_le_manifeste_et_survit_a_la_relecture() {
        // Le cas qui se perdait : « Nuit douce » est livree eteinte (`enabled_by_default:
        // false`). Allumee par l'utilisateur, elle doit rester allumee au demarrage suivant.
        let mut r = Reglages::default();
        r.memoriser_extension("nuit-douce", true);
        let relu: Reglages =
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).expect("reglages relus");
        assert!(
            relu.extension_active("nuit-douce", false),
            "un choix explicite d'activation doit primer sur le defaut du manifeste"
        );

        // Et dans l'autre sens : coupee, elle le reste.
        let mut r = Reglages::default();
        r.memoriser_extension("temps-lecture", false);
        let relu: Reglages =
            serde_json::from_str(&serde_json::to_string(&r).unwrap()).expect("reglages relus");
        assert!(!relu.extension_active("temps-lecture", true));

        // Sans choix, c'est le manifeste qui decide.
        assert!(Reglages::default().extension_active("inconnue", true));
        assert!(!Reglages::default().extension_active("inconnue", false));
    }

    #[test]
    fn l_ancien_champ_des_extensions_coupees_est_repris_une_fois() {
        let mut r: Reglages = serde_json::from_str(r#"{"extensions_desactivees":["nuit-douce"]}"#)
            .expect("ancien fichier relu");
        assert_eq!(r.migrer_extensions(), 1);
        assert!(!r.extension_active("nuit-douce", true), "elle restait coupee");
        assert!(r.extensions_desactivees.is_empty(), "l'ancien champ est vide apres migration");
        let texte = serde_json::to_string(&r).unwrap();
        assert!(
            !texte.contains("extensions_desactivees"),
            "l'ancien champ ne doit plus etre ecrit : {texte}"
        );
        assert!(texte.contains("\"nuit-douce\":false"), "le choix doit etre ecrit : {texte}");
    }

    #[test]
    fn la_migration_n_ecrase_pas_un_choix_plus_recent() {
        let mut r: Reglages = serde_json::from_str(
            r#"{"extensions_etat":{"nuit-douce":true},"extensions_desactivees":["nuit-douce"]}"#,
        )
        .expect("reglages relus");
        assert_eq!(r.migrer_extensions(), 1);
        assert!(r.extension_active("nuit-douce", false), "le choix recent gagne");
    }

    #[test]
    fn le_libelle_priorise_le_titre_puis_la_page_interne_puis_l_url() {
        let mut o = Onglet::neuf(1, "https://exemple.fr/chemin/tres/long".into(), 1, Declencheur::Saisie, 0);
        o.titre = "Mon titre".into();
        assert_eq!(o.libelle(), "Mon titre");

        o.titre = "   ".into();
        // Une URL vide de titre affiche le domaine, jamais une chaine vide.
        assert!(!o.libelle().is_empty());

        let interne = Onglet::neuf(2, crate::urls::NOUVEL_ONGLET.into(), 1, Declencheur::Saisie, 0);
        assert_eq!(interne.libelle(), "Nouvel onglet");
        assert!(interne.interne());

        // Page `trame:` sans nom connu : libelle generique, pas l'URL brute.
        let inconnue = Onglet::neuf(3, "trame://localhost/inconnu.html".into(), 1, Declencheur::Saisie, 0);
        assert_eq!(inconnue.libelle(), "Page interne");
    }

    #[test]
    fn le_domaine_d_un_onglet_est_le_domaine_enregistrable() {
        let o = Onglet::neuf(1, "https://www.exemple.fr/a".into(), 1, Declencheur::Saisie, 0);
        assert_eq!(o.domaine(), "exemple.fr");
        // Une adresse IP n'a pas de domaine enregistrable : elle se represente elle-meme.
        let ip = Onglet::neuf(2, "http://192.0.2.5:8006/".into(), 1, Declencheur::Saisie, 0);
        assert_eq!(ip.domaine(), "192.0.2.5");
    }

    #[test]
    fn un_onglet_neuf_part_avec_une_vitalite_utilisable() {
        let o = Onglet::neuf(1, "https://a.fr/".into(), 1, Declencheur::Lien, 42);
        assert!((o.vitalite - 0.70).abs() < 1e-6);
        assert_eq!(o.cree_le, 42);
        assert_eq!(o.dernier_focus, 42);
        assert_eq!(o.nb_focus, 1);
        assert!(!o.ancre && !o.a_lire && !o.eveille && !o.chargement);
        assert_eq!(o.declencheur.libelle(), "lien");
    }

    #[test]
    fn une_ancienne_session_sans_les_nouveaux_champs_se_lit() {
        // Fichier ecrit par une version anterieure : seuls les champs obligatoires sont la.
        let texte = r#"{
            "onglets": [{"id": 4, "fil": 1, "cree_le": 99}],
            "fils": [{"id": 1, "cree_le": 99}]
        }"#;
        let session: Session = serde_json::from_str(texte).unwrap();
        assert_eq!(session.onglets.len(), 1);
        let o = &session.onglets[0];
        assert_eq!(o.url, "");
        assert_eq!(o.titre, "");
        assert_eq!(o.declencheur, Declencheur::Saisie, "valeur par defaut");
        assert!(!o.ancre);
        assert!(session.actif.is_none());
    }

    #[test]
    fn l_etat_d_execution_ne_se_persiste_pas() {
        let mut o = Onglet::neuf(1, "https://a.fr/".into(), 1, Declencheur::Lien, 0);
        o.eveille = true;
        o.chargement = true;
        let texte = serde_json::to_string(&o).unwrap();
        assert!(!texte.contains("eveille"));
        assert!(!texte.contains("chargement"));
        let relu: Onglet = serde_json::from_str(&texte).unwrap();
        assert!(!relu.eveille && !relu.chargement);
    }

    #[test]
    fn les_regles_de_masquage_par_defaut_sont_utilisables() {
        let regles = regles_par_defaut();
        assert!(regles.len() >= 12, "9 YouTube + 3 generiques");
        for r in &regles {
            assert!(!r.selecteur.trim().is_empty(), "regle sans selecteur");
            assert!(r.actif, "une regle livree coupee ne servirait a rien");
            assert!(!r.libelle.trim().is_empty(), "une regle sans libelle est indechiffrable");
            assert!(r.css().ends_with("display: none !important; }"));
        }
        assert!(regles.iter().any(|r| r.domaine.is_empty()), "il y a des regles globales");
        assert!(regles.iter().any(|r| r.domaine == "youtube.com"));
    }

    #[test]
    fn le_paquet_youtube_sans_shorts_s_installe_une_fois_sans_doublon() {
        let mut regles = regles_par_defaut();
        let avant = regles.len();
        let (ajoutees, traites) = installer_paquets_zap(&mut regles, &[]);
        assert_eq!(traites, vec!["youtube-sans-shorts".to_string()]);
        // Deux regles du paquet existaient deja a l'identique : rattachees, pas dupliquees.
        assert_eq!(regles.len(), avant + ajoutees);
        let du_paquet = regles.iter().filter(|r| r.paquet == "youtube-sans-shorts").count();
        assert_eq!(du_paquet, paquets_zap()[0].regles.len());
        // Deja propose : rien ne bouge (une regle supprimee ne revient pas).
        let (encore, rien) = installer_paquets_zap(&mut regles, &traites);
        assert_eq!((encore, rien.len()), (0, 0));
    }

    #[test]
    fn les_regles_des_paquets_sont_sures_et_la_regle_de_texte_n_a_pas_de_css() {
        for p in paquets_zap() {
            for r in &p.regles {
                assert_eq!(r.selecteur_sur(), r.selecteur, "{} serait abime par le nettoyage", r.selecteur);
                assert!(!r.libelle.is_empty());
                if r.texte.is_empty() {
                    assert!(r.css().ends_with("display: none !important; }"));
                } else {
                    assert!(r.css().is_empty(), "une condition de texte ne passe pas en CSS");
                }
            }
        }
    }

    #[test]
    fn chaque_action_produit_son_css() {
        let avec = |action: &str, valeur: &str, sel: &str| RegleZap {
            action: action.into(),
            valeur: valeur.into(),
            ..RegleZap::nouvelle("a.fr", sel, "")
        };
        assert!(avec("", "", "div.x").css().contains("display: none"));
        assert!(avec("elargir", "", "div.x").css().contains("max-width: none"));
        assert!(avec("replier", "", "div.x").css().starts_with("div.x:not([data-trame-deplie])"));
        assert_eq!(
            avec("garder", "", "main").css(),
            "body :not(:has(main)):not(main):not(main *) { display: none !important; }"
        );
        // `:has` ne s'imbrique pas : « garder » refuse un selecteur qui en contient deja un.
        assert!(avec("garder", "", "div:has(a)").css().is_empty());
        // Le texte reecrit passe par le script, pas par le CSS.
        assert!(avec("texte", "Bonjour", "h1").css().is_empty());
        assert_eq!(avec("style", "font-size: 1.25em", "p").css(), "p { font-size: 1.25em !important; }");
        assert!(avec("inconnue", "", "p").css().is_empty());
        assert_eq!(avec("replier", "", "p").nom_action(), "replié");
    }

    #[test]
    fn un_domaine_saisi_est_ramene_a_la_forme_des_regles() {
        assert_eq!(domaine_saisi("https://www.youtube-nocookie.com/embed/x").as_deref(), Some("youtube-nocookie.com"));
        assert_eq!(domaine_saisi("  Exemple.FR ").as_deref(), Some("exemple.fr"));
        assert_eq!(domaine_saisi("localhost:8080"), None, "il faut un point");
        assert_eq!(domaine_saisi("a b.fr"), None);
        assert_eq!(domaine_saisi("<script>.fr"), None);
    }

    #[test]
    fn copier_les_regles_d_un_site_vers_un_autre_sans_doublon() {
        let mut regles = vec![
            RegleZap { paquet: "p".into(), vu_le: 5, ..RegleZap::nouvelle("a.fr", "div.x", "x") },
            RegleZap { action: "elargir".into(), ..RegleZap::nouvelle("a.fr", "main", "") },
            RegleZap::nouvelle("b.fr", "div.x", "deja la"),
        ];
        assert_eq!(copier_regles_zap(&mut regles, "a.fr", "b.fr"), 1, "div.x existait deja sur b.fr");
        let copie = regles.last().unwrap();
        assert_eq!((copie.domaine.as_str(), copie.action.as_str()), ("b.fr", "elargir"));
        assert_eq!(copier_regles_zap(&mut regles, "a.fr", "b.fr"), 0, "une seconde copie n'ajoute rien");
        let mut regles2 = vec![RegleZap { paquet: "p".into(), vu_le: 5, ..RegleZap::nouvelle("a.fr", "div.x", "") }];
        copier_regles_zap(&mut regles2, "a.fr", "c.fr");
        assert!(regles2[1].paquet.is_empty() && regles2[1].vu_le == 0, "une copie n'appartient a aucun paquet");
    }

    #[test]
    fn importer_des_regles_les_controle() {
        let texte = r#"[
            {"domaine":"https://www.a.fr/x","selecteur":"div.x","action":"bas"},
            {"selecteur":"h1","action":"texte","valeur":"Bonjour"},
            {"domaine":"a.fr","selecteur":"p","action":"pirater"},
            {"domaine":"a.fr","selecteur":"  "},
            {"domaine":"pas un domaine","selecteur":"p"}
        ]"#;
        let r = regles_importees(texte, "courant.fr").unwrap();
        assert_eq!(r.len(), 2, "action inconnue, selecteur vide et domaine invalide sont ecartes");
        assert_eq!((r[0].domaine.as_str(), r[0].action.as_str()), ("a.fr", "bas"));
        assert_eq!(r[1].domaine, "courant.fr", "sans domaine : le site affiche");
        assert!(r[1].actif, "actif par defaut");
        assert!(regles_importees("pas du json", "x.fr").is_err());
        assert!(regles_importees("[]", "x.fr").is_err());
        let un = regles_importees(r#"{"domaine":"a.fr","selecteur":"div"}"#, "x.fr").unwrap();
        assert_eq!(un.len(), 1, "un objet seul est accepte");
    }

    #[test]
    fn un_style_ne_laisse_passer_que_de_l_apparence() {
        assert_eq!(style_sur("color: #60a5fa"), "color: #60a5fa !important");
        assert_eq!(style_sur("opacity: .45; font-weight: 700"), "opacity: .45 !important; font-weight: 700 !important");
        // Hors liste, chargement distant, sortie du bloc, contenu genere : refuses.
        assert_eq!(style_sur("position: fixed"), "");
        assert_eq!(style_sur("background-color: url(https://x)"), "");
        assert_eq!(style_sur("color: red} body {display:none"), "");
        assert_eq!(style_sur("content: 'x'"), "");
        assert_eq!(style_sur("color: red</style><script>"), "");
    }

    #[test]
    fn une_ancienne_regle_sans_les_nouveaux_champs_se_relit() {
        let r: RegleZap = serde_json::from_str(r#"{"domaine":"a.fr","selecteur":"div.x","libelle":"","actif":true}"#).unwrap();
        assert!(r.texte.is_empty() && r.paquet.is_empty() && r.vu_le == 0);
        // Et une regle neuve ne grossit pas zap.json de champs vides.
        let ecrit = serde_json::to_string(&RegleZap::nouvelle("a.fr", "div.x", "")).unwrap();
        assert!(!ecrit.contains("texte") && !ecrit.contains("paquet") && !ecrit.contains("vu_le"));
    }

    #[test]
    fn une_accolade_dans_un_selecteur_ne_casse_pas_le_css() {
        // Sans nettoyage, le bloc se referme au milieu et TOUTES les regles suivantes sautent.
        let r = RegleZap::nouvelle("exemple.fr", "div.ad} body { background: red", "");
        let css = r.css();
        assert_eq!(css.matches('{').count(), 1);
        assert_eq!(css.matches('}').count(), 1);
        assert_eq!(css, "div.ad { display: none !important; }");
    }

    #[test]
    fn un_selecteur_avec_une_balise_ne_peut_pas_fermer_le_style() {
        // Le CSS est injecte entre `<style>` et `</style>` : une balise dans le selecteur
        // sortirait du document.
        let r = RegleZap::nouvelle("", "div</style><script>alert(1)</script>", "");
        let css = r.css();
        assert!(!css.contains('<'), "{css}");
        assert!(!css.contains('>'), "{css}");
        assert_eq!(css.matches('{').count(), 1);
    }

    #[test]
    fn un_selecteur_vide_ne_produit_aucun_css() {
        let r = RegleZap::nouvelle("", "   ", "");
        assert!(r.css().is_empty());
    }

    #[test]
    fn une_capsule_fait_un_aller_retour_json() {
        let capsule = Capsule {
            version: 1,
            nom: "Lecture".into(),
            cree_le: 7,
            onglets: vec![OngletCapsule {
                url: "https://a.fr/".into(),
                titre: "A".into(),
                defilement: 0.25,
                a_lire: true,
                vitalite: 0.5,
            }],
        };
        let texte = serde_json::to_string(&capsule).unwrap();
        let relue: Capsule = serde_json::from_str(&texte).unwrap();
        assert_eq!(relue.nom, "Lecture");
        assert_eq!(relue.onglets.len(), 1);
        assert!(relue.onglets[0].a_lire);
    }

    #[test]
    fn un_manifeste_minimal_prend_les_valeurs_par_defaut() {
        let m: Manifeste = serde_json::from_str(r#"{"name":"Demo"}"#).unwrap();
        assert_eq!(m.name, "Demo");
        assert_eq!(m.run_at, "document_end");
        assert!(m.enabled_by_default, "livree active par defaut");
        assert!(m.matches.is_empty());
        // Une extension qui touche a l'apparence peut se livrer coupee.
        let coupee: Manifeste = serde_json::from_str(
            r#"{"name":"Nuit","enabled_by_default":false,"css":["a.css"]}"#,
        )
        .unwrap();
        assert!(!coupee.enabled_by_default);
        assert_eq!(coupee.css, vec!["a.css".to_string()]);
        // Un manifeste sans nom est refuse : une extension sans nom est inutilisable.
        assert!(serde_json::from_str::<Manifeste>(r#"{"version":"1"}"#).is_err());
    }
}
