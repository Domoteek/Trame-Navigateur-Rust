//! Profil sur disque : session, historique, capsules, stockage des extensions.
//!
//! Toutes les ecritures sont atomiques (fichier temporaire puis renommage) : une coupure
//! de courant ne doit jamais laisser un `session.json` tronque, sinon la session est perdue
//! exactement au moment ou on en a besoin.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::{Capsule, EntreeHistorique, RegleZap, Reglages, Session};
use crate::{alerte, info};

/// Lit un fichier texte du profil en retirant un eventuel BOM UTF-8.
///
/// Un fichier retouche a la main (Bloc-notes, `Set-Content -Encoding utf8` de PowerShell 5.1)
/// commence par un BOM, que `serde_json` refuse : constate le 2026-11-21, `reglages.json` etait
/// alors juge illisible et remplace par les valeurs par defaut.
fn lire_texte(chemin: impl AsRef<Path>) -> std::io::Result<String> {
    let texte = fs::read_to_string(chemin)?;
    Ok(match texte.strip_prefix('\u{feff}') {
        Some(reste) => reste.to_string(),
        None => texte,
    })
}

pub struct Profil {
    racine: PathBuf,
}

/// Extensions livrees avant que le profil ne tienne le registre `extensions-livrees.json` :
/// un profil existant sans registre les a deja recues (et a pu en supprimer).
const LIVREES_D_ORIGINE: &[&str] = &["compteur-visites", "nuit-douce", "temps-lecture"];

/// Verrou de profil : un seul Trame par dossier.
///
/// Sans lui, deux instances ouvrees sur le meme profil ecrivent chacune `session.json` a leur
/// arret et la derniere ecrase le travail de l'autre — constate en test. WebView2, lui, refuse
/// deja le second dossier de donnees : la deuxieme instance serait de toute facon cassee, mais
/// de facon illisible.
pub struct Verrou {
    chemin: PathBuf,
}

impl Drop for Verrou {
    fn drop(&mut self) {
        // Le fichier disparait a l'arret normal. Apres un arret force il reste, avec son PID :
        // `prendre_verrou` reconnait alors un verrou perime et le reprend.
        let _ = fs::remove_file(&self.chemin);
    }
}

impl Profil {
    /// Ouvre (et cree au besoin) le profil. `TRAME_PROFIL` permet d'isoler un profil de test.
    ///
    /// Le profil vit dans **`%LOCALAPPDATA%\Trame`** : c'est une donnee de machine, pas de
    /// compte. Le mettre dans Roaming ferait suivre 300 Mo de caches et de cookies d'un poste a
    /// l'autre a chaque ouverture de session — ce qu'aucun navigateur ne fait.
    pub fn ouvrir() -> Self {
        if let Some(chemin) = std::env::var_os("TRAME_PROFIL") {
            return Self::avec_racine(PathBuf::from(chemin));
        }
        let racine = racine_locale();
        rapatrier_depuis_roaming(&racine);
        Self::avec_racine(racine)
    }

    /// Ouvre un profil a un emplacement donne.
    ///
    /// `ouvrir` s'en sert, et les tests aussi : sans ce constructeur, un test devrait passer
    /// par `TRAME_PROFIL`, une variable **globale au processus** que les tests paralleles se
    /// disputeraient.
    pub fn avec_racine(racine: PathBuf) -> Self {
        for sous in ["", "capsules", "extensions"] {
            let p = if sous.is_empty() { racine.clone() } else { racine.join(sous) };
            if let Err(e) = fs::create_dir_all(&p) {
                alerte!("profil : impossible de creer {} : {e}", p.display());
            }
        }
        info!("profil : {}", racine.display());
        Profil { racine }
    }

    pub fn racine(&self) -> &Path {
        &self.racine
    }
    pub fn fichier_session(&self) -> PathBuf {
        self.racine.join("session.json")
    }
    pub fn fichier_historique(&self) -> PathBuf {
        self.racine.join("history.jsonl")
    }
    pub fn fichier_favoris(&self) -> PathBuf {
        self.racine.join("favoris.json")
    }
    pub fn fichier_guets(&self) -> PathBuf {
        self.racine.join("guets.json")
    }
    pub fn fichier_lectures(&self) -> PathBuf {
        self.racine.join("lectures.jsonl")
    }

    /// Lit une liste JSON du profil (favoris, guets). Un fichier absent donne une liste vide ;
    /// un fichier illisible est mis de cote (jamais detruit) et la liste repart de zero.
    pub fn charger_liste<T: serde::de::DeserializeOwned>(&self, chemin: &Path) -> Vec<T> {
        if !chemin.exists() {
            return Vec::new();
        }
        let Ok(texte) = lire_texte(chemin) else {
            alerte!("{} : lecture impossible", chemin.display());
            return Vec::new();
        };
        match serde_json::from_str::<Vec<T>>(&texte) {
            Ok(v) => v,
            Err(e) => {
                let copie = ecarter(chemin);
                alerte!(
                    "{} illisible ({e}) : liste remise a zero{}",
                    chemin.display(),
                    copie.map(|p| format!(", ancien fichier garde en {}", p.display())).unwrap_or_default()
                );
                Vec::new()
            }
        }
    }

    pub fn sauver_liste<T: serde::Serialize>(&self, chemin: &Path, liste: &[T]) {
        match serde_json::to_string_pretty(liste) {
            Ok(texte) => {
                if let Err(e) = ecrire_atomique(chemin, &texte) {
                    alerte!("{} : ecriture impossible ({e})", chemin.display());
                }
            }
            Err(e) => alerte!("{} : serialisation impossible ({e})", chemin.display()),
        }
    }

    pub fn dossier_capsules(&self) -> PathBuf {
        self.racine.join("capsules")
    }
    pub fn dossier_extensions(&self) -> PathBuf {
        self.racine.join("extensions")
    }
    pub fn fichier_ext_storage(&self) -> PathBuf {
        self.racine.join("ext-storage.json")
    }
    pub fn fichier_journal(&self) -> PathBuf {
        self.racine.join("trame.log")
    }
    pub fn fichier_reglages(&self) -> PathBuf {
        self.racine.join("reglages.json")
    }
    pub fn fichier_zap(&self) -> PathBuf {
        self.racine.join("zap.json")
    }

    pub fn fichier_verrou(&self) -> PathBuf {
        self.racine.join("trame.lock")
    }

    /// Liste des telechargements (page « Téléchargements »).
    pub fn charger_telechargements(&self) -> Vec<crate::telechargements::Telechargement> {
        lire_texte(self.racine.join("telechargements.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Ecrit la liste (les 300 plus recents seulement).
    pub fn sauver_telechargements(&self, liste: &[crate::telechargements::Telechargement]) {
        let debut = liste.len().saturating_sub(300);
        if let Ok(texte) = serde_json::to_string_pretty(&liste[debut..]) {
            let _ = fs::write(self.racine.join("telechargements.json"), texte);
        }
    }

    /// Ancien dossier des telechargements, dans le profil (avant le 2026-11-22).
    pub fn dossier_telechargements(&self) -> PathBuf {
        self.racine.join("telechargements")
    }

    /// Profil WebView2 : cookies, connexions aux extensions, service workers, caches.
    ///
    /// Il vit **dans** le profil, et non a cote de l'executable. Le defaut de wry
    /// (`<exe>.WebView2`) placait ces donnees dans `target\release` : un `cargo clean` effacait
    /// la connexion a Bitwarden, et un binaire deplace repartait de zero.
    pub fn dossier_webview2(&self) -> PathBuf {
        self.racine.join("webview2")
    }

    /// Rapatrie le profil WebView2 laisse a cote de l'executable, une seule fois.
    ///
    /// Rend le dossier a utiliser : le nouveau s'il est deja en place ou si le rapatriement a
    /// reussi, l'ancien si la copie echoue (on ne perd pas une session pour un demenagement).
    pub fn rapatrier_webview2(&self) -> PathBuf {
        self.rapatrier_depuis(ancien_dossier_webview2())
    }

    /// Le rapatriement, avec l'ancien emplacement en parametre : c'est ce qui le rend testable
    /// sans dependre de l'endroit ou se trouve l'executable pendant les tests.
    pub fn rapatrier_depuis(&self, ancien: Option<PathBuf>) -> PathBuf {
        let cible = self.dossier_webview2();
        if cible.join("EBWebView").is_dir() {
            return cible;
        }
        let Some(ancien) = ancien else {
            return cible;
        };
        if !ancien.join("EBWebView").is_dir() {
            return cible;
        }
        let octets = crate::ressources::taille(&ancien);
        // 1. deplacement : instantane quand c'est le meme volume.
        if std::fs::rename(&ancien, &cible).is_ok() {
            info!(
                "profil WebView2 deplace dans le profil ({:.1} Mo) : {}",
                octets as f64 / 1_048_576.0,
                cible.display()
            );
            return cible;
        }
        // 2. copie : volumes differents. Assez rapide (quelques centaines de Mo), et l'ancien
        //    dossier est garde sous un autre nom au lieu d'etre supprime.
        match crate::ressources::copier_arbre(&ancien, &cible) {
            Ok(()) => {
                let garde = ancien.with_file_name(format!(
                    "{}.migre",
                    ancien.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                ));
                let _ = std::fs::rename(&ancien, &garde);
                info!(
                    "profil WebView2 copie dans le profil ({:.1} Mo), ancien dossier garde en {}",
                    octets as f64 / 1_048_576.0,
                    garde.display()
                );
                cible
            }
            Err(e) => {
                alerte!(
                    "profil WebView2 : copie impossible ({e}) — on garde l'ancien emplacement {}",
                    ancien.display()
                );
                ancien
            }
        }
    }

    /// Prend le verrou du profil.
    ///
    /// `Err(pid)` = un autre Trame **vivant** tient deja ce dossier. Un verrou laisse par un
    /// processus mort (arret force, coupure) est reconnu comme perime et repris.
    pub fn prendre_verrou(&self) -> Result<Verrou, u32> {
        let chemin = self.fichier_verrou();
        for _ in 0..2 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&chemin)
            {
                Ok(mut fichier) => {
                    let _ = writeln!(fichier, "{}", std::process::id());
                    return Ok(Verrou { chemin });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let pid = fs::read_to_string(&chemin)
                        .ok()
                        .and_then(|t| t.trim().parse::<u32>().ok())
                        .unwrap_or(0);
                    if pid != 0 && processus_vivant(pid) {
                        return Err(pid);
                    }
                    info!("verrou de profil perime (pid {pid}), il est repris");
                    let _ = fs::remove_file(&chemin);
                }
                Err(e) => {
                    // Un profil en lecture seule ne doit pas empecher de demarrer : on continue
                    // sans verrou, en le disant.
                    alerte!("verrou de profil impossible ({e}) : demarrage sans verrou");
                    return Ok(Verrou { chemin });
                }
            }
        }
        Err(0)
    }

    /// Regles de masquage. Au premier lancement, le fichier est cree avec un jeu par defaut.
    pub fn charger_zap(&self) -> Vec<RegleZap> {
        let chemin = self.fichier_zap();
        if !chemin.exists() {
            let defaut = crate::model::regles_par_defaut();
            self.sauver_zap(&defaut);
            return defaut;
        }
        let Ok(texte) = lire_texte(&chemin) else {
            alerte!("zap : lecture impossible ({})", chemin.display());
            return crate::model::regles_par_defaut();
        };
        match serde_json::from_str::<Vec<RegleZap>>(&texte) {
            Ok(regles) => regles,
            Err(e) => {
                // Avant, un fichier casse rendait une liste vide, sans un mot : les regles
                // semblaient avoir disparu. On ecarte le fichier (sans le detruire) et on
                // remet le jeu par defaut.
                let copie = ecarter(&chemin);
                alerte!(
                    "zap illisible ({e}) : regles par defaut remises{}",
                    copie
                        .map(|p| format!(", ancien fichier garde en {}", p.display()))
                        .unwrap_or_default()
                );
                let defaut = crate::model::regles_par_defaut();
                self.sauver_zap(&defaut);
                defaut
            }
        }
    }

    pub fn sauver_zap(&self, regles: &[RegleZap]) {
        match serde_json::to_string_pretty(regles) {
            Ok(texte) => {
                if let Err(e) = ecrire_atomique(&self.fichier_zap(), &texte) {
                    alerte!("zap : ecriture impossible ({e})");
                }
            }
            Err(e) => alerte!("zap : serialisation impossible ({e})"),
        }
    }

    pub fn charger_reglages(&self) -> Reglages {
        let chemin = self.fichier_reglages();
        let Ok(texte) = lire_texte(&chemin) else {
            return Reglages::default();
        };
        match serde_json::from_str::<Reglages>(&texte) {
            Ok(mut r) => {
                // Un profil ecrit par une version precedente garde ses choix : l'ancien champ
                // (liste des extensions coupees) est repris dans la carte des choix explicites.
                let reprises = r.migrer_extensions();
                if reprises > 0 {
                    info!("reglages : {reprises} choix d'extension repris de l'ancien format");
                }
                r
            }
            Err(e) => {
                // Un fichier casse ne doit pas emporter les reglages en silence : on le garde
                // a cote, on le dit, et on repart des valeurs par defaut.
                let copie = ecarter(&chemin);
                alerte!(
                    "reglages illibles ({e}) : valeurs par defaut{}",
                    copie
                        .map(|p| format!(", ancien fichier garde en {}", p.display()))
                        .unwrap_or_default()
                );
                Reglages::default()
            }
        }
    }

    pub fn sauver_reglages(&self, reglages: &Reglages) {
        match serde_json::to_string_pretty(reglages) {
            Ok(texte) => {
                if let Err(e) = ecrire_atomique(&self.fichier_reglages(), &texte) {
                    alerte!("reglages : ecriture impossible ({e})");
                }
            }
            Err(e) => alerte!("reglages : serialisation impossible ({e})"),
        }
    }

    pub fn charger_session(&self) -> Session {
        let chemin = self.fichier_session();
        match lire_texte(&chemin) {
            Ok(texte) => match serde_json::from_str::<Session>(&texte) {
                Ok(mut s) => {
                    // Coherence minimale : un id de fil orphelin casserait le rail.
                    let ids: Vec<u32> = s.fils.iter().map(|f| f.id).collect();
                    if ids.is_empty() {
                        s.onglets.clear();
                        return Session::default();
                    }
                    s.onglets.retain(|o| ids.contains(&o.fil));
                    for o in s.onglets.iter_mut() {
                        o.eveille = false;
                        o.chargement = false;
                    }
                    info!("session : {} onglet(s), {} fil(s)", s.onglets.len(), s.fils.len());
                    s
                }
                Err(e) => {
                    // Sans cela, un fichier casse etait **ecrase** a la sortie suivante :
                    // la session etait perdue sans laisser de quoi la relire a la main.
                    let copie = ecarter(&chemin);
                    alerte!(
                        "session illisible ({e}) : demarrage a vide{}",
                        copie
                            .map(|p| format!(", ancien fichier garde en {}", p.display()))
                            .unwrap_or_default()
                    );
                    Session::default()
                }
            },
            Err(_) => Session::default(),
        }
    }

    pub fn sauver_session(&self, session: &Session) {
        let valeur = match serde_json::to_value(session) {
            Ok(v) => v,
            Err(e) => {
                alerte!("session : serialisation impossible ({e})");
                return;
            }
        };
        if let Err(e) = ecrire_atomique(&self.fichier_session(), &valeur.to_string()) {
            alerte!("session : ecriture impossible ({e})");
        }
    }

    pub fn charger_historique(&self) -> Vec<EntreeHistorique> {
        let Ok(texte) = fs::read_to_string(self.fichier_historique()) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut ignorees = 0usize;
        for ligne in texte.lines() {
            if ligne.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<EntreeHistorique>(ligne) {
                Ok(e) => out.push(e),
                Err(_) => ignorees += 1,
            }
        }
        if ignorees > 0 {
            info!("historique : {ignorees} ligne(s) illisible(s) ignoree(s)");
        }
        // On garde les 5000 plus recentes en memoire.
        if out.len() > 5000 {
            out.drain(0..out.len() - 5000);
        }
        out
    }

    pub fn ajouter_historique(&self, e: &EntreeHistorique) {
        let Ok(ligne) = serde_json::to_string(e) else { return };
        let resultat = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.fichier_historique())
            .and_then(|mut f| writeln!(f, "{ligne}"));
        if let Err(erreur) = resultat {
            alerte!("historique : ajout impossible ({erreur})");
        }
    }

    pub fn reecrire_historique(&self, entrees: &[EntreeHistorique]) {
        let mut texte = String::new();
        for e in entrees {
            if let Ok(l) = serde_json::to_string(e) {
                texte.push_str(&l);
                texte.push('\n');
            }
        }
        if let Err(e) = ecrire_atomique_brut(&self.fichier_historique(), &texte) {
            alerte!("historique : reecriture impossible ({e})");
        }
    }

    pub fn lister_capsules(&self) -> Vec<(PathBuf, String)> {
        let mut out = Vec::new();
        let Ok(lecture) = fs::read_dir(self.dossier_capsules()) else {
            return out;
        };
        for entree in lecture.flatten() {
            let chemin = entree.path();
            if chemin.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let nom = lire_texte(&chemin)
                .ok()
                .and_then(|t| serde_json::from_str::<Capsule>(&t).ok())
                .map(|c| c.nom)
                .unwrap_or_else(|| {
                    chemin
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default()
                });
            out.push((chemin, nom));
        }
        out.sort();
        out
    }

    pub fn sauver_capsule(&self, capsule: &Capsule) -> std::io::Result<PathBuf> {
        let base = format!(
            "{}-{}",
            slug(&capsule.nom),
            capsule.cree_le / 1000
        );
        let chemin = self.dossier_capsules().join(format!("{base}.json"));
        let texte = serde_json::to_string_pretty(capsule)?;
        ecrire_atomique_brut(&chemin, &texte)?;
        info!("capsule ecrite : {}", chemin.display());
        Ok(chemin)
    }

    pub fn charger_capsule(&self, chemin: &Path) -> Option<Capsule> {
        let texte = lire_texte(chemin).ok()?;
        serde_json::from_str::<Capsule>(&texte).ok()
    }

    /// Copie dans le profil les extensions livrees avec l'application, chacune UNE fois.
    ///
    /// Avant le 2026-11-22, la copie n'avait lieu qu'au premier lancement (profil vide) : une
    /// extension livree plus tard (le saute-pub YouTube) n'arrivait jamais chez un utilisateur
    /// existant. Desormais `extensions-livrees.json` retient celles deja proposees : une nouvelle
    /// est copiee, une extension que l'utilisateur a supprimee ne revient pas.
    pub fn amorcer_extensions(&self, source: &Path) {
        let cible = self.dossier_extensions();
        let registre = self.racine.join("extensions-livrees.json");
        let mut proposees: Vec<String> = match lire_texte(&registre) {
            Ok(t) => serde_json::from_str(&t).unwrap_or_default(),
            // Pas encore de registre : un profil existant a deja recu les livrees d'origine
            // (et a pu en supprimer) ; un profil vide n'a rien recu.
            Err(_) => {
                let deja_rempli = fs::read_dir(&cible)
                    .map(|r| r.flatten().any(|e| e.path().is_dir()))
                    .unwrap_or(false);
                if deja_rempli {
                    LIVREES_D_ORIGINE.iter().map(|s| s.to_string()).collect()
                } else {
                    Vec::new()
                }
            }
        };
        let Ok(lecture) = fs::read_dir(source) else {
            return;
        };
        let mut copies = Vec::new();
        for entree in lecture.flatten() {
            let chemin = entree.path();
            if !chemin.is_dir() {
                continue;
            }
            let Some(nom) = chemin.file_name().map(|n| n.to_string_lossy().to_string()) else {
                continue;
            };
            if proposees.contains(&nom) {
                // Deja proposee : si elle est encore la et que la livree est plus recente, on la
                // remplace, en gardant l'ancienne de cote (l'utilisateur a pu la retoucher).
                let destination = cible.join(&nom);
                let (Some(neuve), Some(ancienne)) =
                    (version_manifeste(&chemin), version_manifeste(&destination))
                else {
                    continue;
                };
                if comparer_versions(&neuve, &ancienne) != std::cmp::Ordering::Greater {
                    continue;
                }
                if self.remplacer_par_livree(&nom, &chemin, &destination, &ancienne) {
                    info!("extension livree {nom} mise a jour : {ancienne} -> {neuve}");
                    copies.push(format!("{nom} {neuve}"));
                }
                continue;
            }
            let destination = cible.join(&nom);
            if destination.exists() || copier_dossier(&chemin, &destination).is_ok() {
                copies.push(nom.clone());
            }
            proposees.push(nom);
        }
        if let Ok(texte) = serde_json::to_string_pretty(&proposees) {
            let _ = fs::write(&registre, texte);
        }
        if !copies.is_empty() {
            info!("extensions livrees copiees dans le profil : {}", copies.join(", "));
        }
    }

    /// Remplace la copie du profil d'une extension livree par la livree, l'ancienne gardee dans
    /// `extensions-anciennes\` (l'utilisateur a pu la retoucher). En cas d'echec, l'ancienne est
    /// remise : jamais de trou.
    fn remplacer_par_livree(&self, nom: &str, livree: &Path, destination: &Path, etiquette: &str) -> bool {
        let garde = self
            .racine
            .join("extensions-anciennes")
            .join(format!("{nom}-{etiquette}-{}", crate::log::maintenant_ms()));
        let deplacee = garde
            .parent()
            .map(|p| fs::create_dir_all(p).is_ok())
            .unwrap_or(false)
            && fs::rename(destination, &garde).is_ok();
        if deplacee && copier_dossier(livree, destination).is_ok() {
            info!("extension livree {nom} remise a neuf (ancienne gardee dans {})", garde.display());
            return true;
        }
        if deplacee {
            let _ = fs::remove_dir_all(destination);
            let _ = fs::rename(&garde, destination);
            alerte!("extension livree {nom} : remplacement impossible, ancienne remise");
        }
        false
    }

    /// Verification d'apres mise a jour (et bouton « Réparer Trame ») : chaque extension livree
    /// encore presente dans le profil est comparee **par son contenu** a la livree, pas seulement
    /// par son numero de version — une copie retouchee ou laissee par une tres vieille version
    /// survivait sinon a toutes les mises a jour. Une extension que l'utilisateur a supprimee ne
    /// revient pas. `toutes` : remplace aussi les identiques. Rend les noms remis a neuf.
    pub fn verifier_extensions_livrees(&self, source: &Path, toutes: bool) -> Vec<String> {
        let cible = self.dossier_extensions();
        let mut reparees = Vec::new();
        let Ok(lecture) = fs::read_dir(source) else {
            return reparees;
        };
        for entree in lecture.flatten() {
            let livree = entree.path();
            if !livree.is_dir() {
                continue;
            }
            let nom = livree.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let destination = cible.join(&nom);
            if nom.is_empty() || !destination.is_dir() {
                continue;
            }
            if !toutes && dossiers_identiques(&livree, &destination) {
                continue;
            }
            let etiquette = version_manifeste(&destination).unwrap_or_else(|| "?".into());
            if self.remplacer_par_livree(&nom, &livree, &destination, &etiquette) {
                reparees.push(nom);
            }
        }
        reparees
    }

    pub fn charger_ext_storage(&self) -> Value {
        lire_texte(self.fichier_ext_storage())
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()))
    }

    pub fn sauver_ext_storage(&self, valeur: &Value) {
        if let Err(e) = ecrire_atomique(&self.fichier_ext_storage(), &valeur.to_string()) {
            alerte!("stockage extensions : ecriture impossible ({e})");
        }
    }
}

/// Racine du profil : `%LOCALAPPDATA%\Trame`.
pub fn racine_locale() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Trame")
}

/// Ancienne racine (versions precedentes) : `%APPDATA%\Trame`.
pub fn racine_roaming() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Trame"))
}

/// Deplace un profil d'un dossier vers un autre, une seule fois.
///
/// Appelee au demarrage avant toute creation de webview : le profil n'est donc pas ouvert.
/// Si la destination contient deja une session, on ne touche a rien. En cas d'echec du
/// deplacement (fichier verrouille), on **garde tout en place** : mieux vaut un profil au
/// mauvais endroit qu'un profil perdu.
pub fn rapatrier_racine(ancienne: &Path, nouvelle: &Path) -> bool {
    if !ancienne.is_dir() {
        return false;
    }
    if nouvelle.join("session.json").is_file() {
        return false;
    }
    // Le dossier parent doit exister pour que le deplacement echoue proprement : sans lui,
    // \`rename\` echoue et on tombe dans la copie (plus lente) sans raison.
    if let Some(parent) = nouvelle.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if !nouvelle.exists() && std::fs::rename(ancienne, nouvelle).is_ok() {
        info!(
            "profil deplace : {} -> {} (donnee de machine, plus de roaming)",
            ancienne.display(),
            nouvelle.display()
        );
        return true;
    }
    // Volumes differents ou fichier verrouille : copie dossier par dossier.
    let mut copies = 0usize;
    if let Ok(lecture) = std::fs::read_dir(ancienne) {
        for entree in lecture.flatten() {
            let source = entree.path();
            let nom = entree.file_name();
            let cible = nouvelle.join(&nom);
            let resultat = if source.is_dir() {
                copier_dossier(&source, &cible)
            } else {
                std::fs::create_dir_all(nouvelle)
                    .and_then(|_| std::fs::copy(&source, &cible).map(|_| ()))
            };
            if resultat.is_ok() {
                copies += 1;
            }
        }
    }
    if copies > 0 {
        alerte!(
            "profil copie depuis {} ({} element(s)) : l'ancien dossier est garde, il peut etre \
             supprime une fois la nouvelle installation verifiee",
            ancienne.display(),
            copies
        );
        return true;
    }
    alerte!(
        "profil : impossible de deplacer {} — on garde l'ancien emplacement",
        ancienne.display()
    );
    false
}

fn rapatrier_depuis_roaming(nouvelle: &Path) {
    let Some(ancienne) = racine_roaming() else {
        return;
    };
    if ancienne == nouvelle {
        return;
    }
    rapatrier_racine(&ancienne, nouvelle);
}

/// Emplacement par defaut de WebView2 : `<exe>.WebView2`, a cote de l'executable.
fn ancien_dossier_webview2() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let nom = exe.file_name()?.to_string_lossy().to_string();
    Some(exe.parent()?.join(format!("{nom}.WebView2")))
}

/// Ce PID est-il encore un Trame vivant ?///
/// On verifie aussi le nom du processus : Windows recycle les PID, et un numero reutilise par
/// un autre programme ferait croire a un profil ouvert alors qu'il ne l'est plus.
pub fn processus_vivant(pid: u32) -> bool {
    let mut systeme = sysinfo::System::new();
    systeme.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    systeme
        .process(sysinfo::Pid::from_u32(pid))
        .map(|p| {
            let nom = p.name().to_string_lossy().to_ascii_lowercase();
            nom.contains("trame")
        })
        .unwrap_or(false)
}

/// Ecarte un fichier illisible **sans le detruire** : il est renomme a cote.
///
/// Un profil casse est un fait rare et penible : le garder permet de retrouver ses onglets a
/// la main, ou de comprendre ce qui l'a casse. On ne supprime jamais rien. Le suffixe
/// numerote evite qu'une seconde corruption n'ecrase la premiere.
fn ecarter(chemin: &Path) -> Option<PathBuf> {
    let nom = chemin.file_name()?.to_string_lossy().to_string();
    for essai in 1..100 {
        let suffixe = if essai == 1 {
            String::new()
        } else {
            format!("-{essai}")
        };
        let cible = chemin.with_file_name(format!("{nom}.casse{suffixe}"));
        if cible.exists() {
            continue;
        }
        return match fs::rename(chemin, &cible) {
            Ok(()) => Some(cible),
            Err(e) => {
                alerte!("profil : impossible d'ecarter {} ({e})", chemin.display());
                None
            }
        };
    }
    alerte!("profil : trop de copies cassees autour de {}", chemin.display());
    None
}

/// Copie recursive (extensions livrees -> profil).
/// Version lue dans le `manifest.json` d'un dossier d'extension.
pub(crate) fn version_manifeste(dossier: &Path) -> Option<String> {
    let texte = fs::read_to_string(dossier.join("manifest.json")).ok()?;
    let v: Value = serde_json::from_str(&texte).ok()?;
    v.get("version")?.as_str().map(str::to_string)
}

/// Compare deux versions `1.2.10` nombre par nombre (une partie absente vaut 0).
fn comparer_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parties = |s: &str| -> Vec<u64> {
        s.split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0)).collect()
    };
    let (pa, pb) = (parties(a), parties(b));
    for i in 0..pa.len().max(pb.len()) {
        let o = pa.get(i).unwrap_or(&0).cmp(pb.get(i).unwrap_or(&0));
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    std::cmp::Ordering::Equal
}

/// Les fichiers d'un dossier, chemins relatifs tries (pour comparer deux arbres).
fn fichiers_relatifs(racine: &Path, ici: &Path, sortie: &mut Vec<PathBuf>) {
    let Ok(lecture) = fs::read_dir(ici) else { return };
    for entree in lecture.flatten() {
        let chemin = entree.path();
        if chemin.is_dir() {
            fichiers_relatifs(racine, &chemin, sortie);
        } else if let Ok(relatif) = chemin.strip_prefix(racine) {
            sortie.push(relatif.to_path_buf());
        }
    }
}

/// Deux dossiers ont-ils exactement les memes fichiers, octet pour octet ?
pub(crate) fn dossiers_identiques(a: &Path, b: &Path) -> bool {
    let (mut fa, mut fb) = (Vec::new(), Vec::new());
    fichiers_relatifs(a, a, &mut fa);
    fichiers_relatifs(b, b, &mut fb);
    fa.sort();
    fb.sort();
    fa == fb && fa.iter().all(|r| matches!((fs::read(a.join(r)), fs::read(b.join(r))), (Ok(x), Ok(y)) if x == y))
}

fn copier_dossier(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::create_dir_all(destination)?;
    for entree in fs::read_dir(source)?.flatten() {
        let chemin = entree.path();
        let nom = chemin.file_name().unwrap_or_default().to_os_string();
        let cible = destination.join(nom);
        if chemin.is_dir() {
            copier_dossier(&chemin, &cible)?;
        } else {
            fs::copy(&chemin, &cible)?;
        }
    }
    Ok(())
}

/// Ecrit un fichier par remplacement atomique.
pub(crate) fn ecrire_atomique(chemin: &Path, contenu: &str) -> std::io::Result<()> {
    ecrire_atomique_brut(chemin, contenu)
}

fn ecrire_atomique_brut(chemin: &Path, contenu: &str) -> std::io::Result<()> {
    if let Some(parent) = chemin.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporaire = chemin.with_extension("tmp");
    {
        let mut f = fs::File::create(&temporaire)?;
        f.write_all(contenu.as_bytes())?;
        f.sync_all()?;
    }
    // `rename` remplace l'existant sous Windows depuis Rust 1.x (MoveFileEx REPLACE_EXISTING).
    match fs::rename(&temporaire, chemin) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&temporaire);
            Err(e)
        }
    }
}

fn slug(nom: &str) -> String {
    let mut s = String::new();
    for c in nom.chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            if !s.ends_with('-') {
                s.push('-');
            }
        }
        if s.len() >= 40 {
            break;
        }
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "capsule".to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fil, Onglet, OngletCapsule, Declencheur};

    /// Un profil jetable, unique par test : les tests tournent en parallele.
    fn profil(nom: &str) -> Profil {
        let racine = std::env::temp_dir().join(format!(
            "trame-test-{}-{nom}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&racine);
        Profil::avec_racine(racine)
    }

    fn session_exemple() -> Session {
        let mut s = Session::default();
        s.fils.push(Fil::neuf(1, "Bitwarden".into(), 1_000));
        s.fils.push(Fil::neuf(2, "Home lab".into(), 2_000));
        let mut a = Onglet::neuf(10, "https://exemple.fr/a".into(), 1, Declencheur::Saisie, 1_000);
        a.titre = "A".into();
        let mut b = Onglet::neuf(11, "https://exemple.fr/b".into(), 2, Declencheur::Lien, 2_000);
        b.titre = "B".into();
        b.parent = Some(10);
        s.onglets.push(a);
        s.onglets.push(b);
        s.actif = Some(11);
        s.prochain_onglet = 12;
        s.prochain_fil = 3;
        s
    }

    #[test]
    fn une_session_ecrite_se_relit_a_l_identique() {
        let p = profil("session-aller-retour");
        let session = session_exemple();
        p.sauver_session(&session);
        let relue = p.charger_session();
        assert_eq!(relue.onglets.len(), 2);
        assert_eq!(relue.fils.len(), 2);
        assert_eq!(relue.actif, Some(11));
        assert_eq!(relue.onglets[1].parent, Some(10));
        assert_eq!(relue.prochain_onglet, 12);
        // L'etat d'execution ne se persiste pas : un onglet relu n'est jamais « eveille ».
        assert!(relue.onglets.iter().all(|o| !o.eveille && !o.chargement));
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn des_reglages_avec_bom_se_lisent() {
        let p = profil("reglages-bom");
        fs::write(
            p.fichier_reglages(),
            "\u{feff}{\"moteur\":\"qwant\",\"extensions_epinglees\":[\"abc\"]}",
        )
        .unwrap();
        let r = p.charger_reglages();
        assert_eq!(r.moteur, "qwant");
        assert_eq!(r.extensions_epinglees, vec!["abc".to_string()]);
        assert!(!p.racine().join("reglages.json.casse").exists(), "rien n'est ecarte");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn une_session_illisible_est_ecartee_sans_etre_detruite() {
        let p = profil("session-cassee");
        fs::write(p.fichier_session(), "{ ceci n'est pas du json").unwrap();
        let relue = p.charger_session();
        assert!(relue.onglets.is_empty() && relue.fils.is_empty());
        // Le fichier est renomme, pas supprime : on doit pouvoir le relire a la main.
        let garde = p.racine().join("session.json.casse");
        assert!(garde.is_file(), "le fichier casse doit etre garde");
        assert_eq!(
            fs::read_to_string(&garde).unwrap(),
            "{ ceci n'est pas du json"
        );
        assert!(!p.fichier_session().exists());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn deux_corruptions_ne_s_ecrasent_pas() {
        let p = profil("session-deux-cassees");
        fs::write(p.fichier_session(), "premier").unwrap();
        p.charger_session();
        fs::write(p.fichier_session(), "deuxieme").unwrap();
        p.charger_session();
        assert_eq!(
            fs::read_to_string(p.racine().join("session.json.casse")).unwrap(),
            "premier"
        );
        assert_eq!(
            fs::read_to_string(p.racine().join("session.json.casse-2")).unwrap(),
            "deuxieme"
        );
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_onglet_sans_fil_est_ecarte() {
        let p = profil("session-orphelin");
        let mut session = session_exemple();
        // Un onglet qui pointe vers un fil disparu casserait le rail.
        session.onglets[0].fil = 99;
        p.sauver_session(&session);
        let relue = p.charger_session();
        assert_eq!(relue.onglets.len(), 1);
        assert_eq!(relue.onglets[0].id, 11);
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn une_session_sans_fil_du_tout_repart_a_zero() {
        let p = profil("session-sans-fil");
        let mut session = session_exemple();
        session.fils.clear();
        p.sauver_session(&session);
        let relue = p.charger_session();
        assert!(relue.onglets.is_empty());
        assert!(relue.fils.is_empty());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn les_reglages_se_relisent() {
        let p = profil("reglages");
        let mut r = Reglages::default();
        r.moteur = "qwant".into();
        r.budget = 3;
        r.barre_extensions = false;
        r.extensions_epinglees.push("abc".into());
        // Le choix d'activation doit traverser le disque : c'est ce que l'utilisateur attend
        // d'une extension qu'il a allumee ou coupee.
        r.memoriser_extension("nuit-douce", true);
        r.memoriser_extension("temps-lecture", false);
        p.sauver_reglages(&r);
        let relu = p.charger_reglages();
        assert_eq!(relu.moteur, "qwant");
        assert_eq!(relu.budget, 3);
        assert!(!relu.barre_extensions);
        assert_eq!(relu.extensions_epinglees, vec!["abc".to_string()]);
        assert!(relu.extension_active("nuit-douce", false), "allumee, elle le reste");
        assert!(!relu.extension_active("temps-lecture", true), "coupee, elle le reste");
        assert!(relu.extension_active("jamais-touchee", true), "sans choix : le manifeste");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_ancien_profil_garde_ses_extensions_coupees() {
        // `extensions_desactivees` est l'ancien emplacement du choix : le relire en silence
        // comme « aucune extension coupee » aurait rallume ce que l'utilisateur avait eteint.
        let p = profil("reglages-ancien-etat");
        fs::write(
            p.fichier_reglages(),
            r#"{"moteur":"brave","extensions_desactivees":["nuit-douce"]}"#,
        )
        .unwrap();
        let r = p.charger_reglages();
        assert_eq!(r.moteur, "brave");
        assert!(!r.extension_active("nuit-douce", true), "elle doit rester coupee");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn des_reglages_illisibles_reviennent_aux_valeurs_par_defaut() {
        let p = profil("reglages-casses");
        fs::write(p.fichier_reglages(), "[[[").unwrap();
        let r = p.charger_reglages();
        assert_eq!(r.moteur, crate::urls::MOTEUR_DEFAUT);
        assert_eq!(r.budget, 6);
        assert!(p.racine().join("reglages.json.casse").is_file());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_ancien_fichier_de_reglages_sans_les_nouveaux_champs_reste_lisible() {
        // Compatibilite ascendante : un profil ecrit avant l'epinglage doit s'ouvrir.
        let p = profil("reglages-anciens");
        fs::write(
            p.fichier_reglages(),
            r#"{"moteur":"google","budget":4}"#,
        )
        .unwrap();
        let r = p.charger_reglages();
        assert_eq!(r.moteur, "google");
        assert_eq!(r.budget, 4);
        assert!(r.extensions_epinglees.is_empty());
        assert!(r.extensions_etat.is_empty());
        assert!(r.extensions_desactivees.is_empty());
        assert!(r.barre_extensions, "absent = valeur par defaut (true)");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn le_zap_par_defaut_est_ecrit_au_premier_lancement() {
        let p = profil("zap-defaut");
        assert!(!p.fichier_zap().exists());
        let regles = p.charger_zap();
        assert_eq!(regles.len(), crate::model::regles_par_defaut().len());
        assert!(p.fichier_zap().is_file(), "le jeu par defaut doit etre ecrit");
        // Relu dans la foulee, sans doublon.
        assert_eq!(p.charger_zap().len(), regles.len());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_zap_illisible_remet_les_regles_par_defaut() {
        let p = profil("zap-casse");
        fs::write(p.fichier_zap(), "pas du json").unwrap();
        let regles = p.charger_zap();
        assert_eq!(regles.len(), crate::model::regles_par_defaut().len());
        assert!(p.racine().join("zap.json.casse").is_file());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn l_historique_ignore_les_lignes_illisibles_et_garde_les_bonnes() {
        let p = profil("historique");
        p.ajouter_historique(&EntreeHistorique {
            url: "https://a.fr/".into(),
            titre: "A".into(),
            ts: 10,
        });
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(p.fichier_historique())
            .unwrap();
        writeln!(f, "ligne cassee").unwrap();
        writeln!(f).unwrap();
        drop(f);
        p.ajouter_historique(&EntreeHistorique {
            url: "https://b.fr/".into(),
            titre: "B".into(),
            ts: 20,
        });
        let relu = p.charger_historique();
        assert_eq!(relu.len(), 2);
        assert_eq!(relu[0].url, "https://a.fr/");
        assert_eq!(relu[1].url, "https://b.fr/");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn l_historique_ne_depasse_pas_5000_entrees_en_memoire() {
        let p = profil("historique-borne");
        let entrees: Vec<EntreeHistorique> = (0..5200)
            .map(|i| EntreeHistorique {
                url: format!("https://exemple.fr/{i}"),
                titre: String::new(),
                ts: i as u64,
            })
            .collect();
        p.reecrire_historique(&entrees);
        let relu = p.charger_historique();
        assert_eq!(relu.len(), 5000);
        // Ce sont les plus recentes qui restent.
        assert_eq!(relu[4999].url, "https://exemple.fr/5199");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn une_capsule_ecrite_se_relit_et_se_liste() {
        let p = profil("capsule");
        let capsule = Capsule {
            version: 1,
            nom: "Veille techno".into(),
            cree_le: 1_700_000_000_000,
            onglets: vec![OngletCapsule {
                url: "https://exemple.fr/x".into(),
                titre: "X".into(),
                defilement: 0.42,
                a_lire: true,
                vitalite: 0.9,
            }],
        };
        let chemin = p.sauver_capsule(&capsule).unwrap();
        assert_eq!(
            chemin.file_name().unwrap().to_string_lossy(),
            "veille-techno-1700000000.json"
        );
        let relue = p.charger_capsule(&chemin).unwrap();
        assert_eq!(relue.nom, "Veille techno");
        assert_eq!(relue.onglets.len(), 1);
        assert!((relue.onglets[0].defilement - 0.42).abs() < 1e-9);
        assert!(relue.onglets[0].a_lire);
        let liste = p.lister_capsules();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].1, "Veille techno");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn une_capsule_illisible_prend_le_nom_de_son_fichier() {
        let p = profil("capsule-cassee");
        fs::write(p.dossier_capsules().join("vrac.json"), "n'importe quoi").unwrap();
        let liste = p.lister_capsules();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].1, "vrac");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn le_stockage_des_extensions_survit_et_reste_un_objet() {
        let p = profil("ext-storage");
        let mut valeur = Value::Object(serde_json::Map::new());
        valeur["nuit-douce"] = serde_json::json!({"actif": false});
        p.sauver_ext_storage(&valeur);
        assert_eq!(p.charger_ext_storage(), valeur);
        // Un fichier qui ne contient pas un objet est refuse : les extensions attendent un
        // dictionnaire, pas une liste.
        fs::write(p.fichier_ext_storage(), "[1,2,3]").unwrap();
        assert!(p.charger_ext_storage().is_object());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn les_extensions_livrees_sont_copiees_une_seule_fois() {
        let p = profil("amorce-extensions");
        let source = p.racine().join("source");
        fs::create_dir_all(source.join("demo")).unwrap();
        fs::write(source.join("demo").join("manifest.json"), "{}").unwrap();
        p.amorcer_extensions(&source);
        assert!(p.dossier_extensions().join("demo").join("manifest.json").is_file());
        // Deuxieme appel : on n'ecrase pas (l'utilisateur a pu modifier son extension).
        fs::write(p.dossier_extensions().join("demo").join("manifest.json"), "modifie").unwrap();
        p.amorcer_extensions(&source);
        assert_eq!(
            fs::read_to_string(p.dossier_extensions().join("demo").join("manifest.json")).unwrap(),
            "modifie"
        );
        // Une extension livree plus tard arrive dans un profil deja rempli…
        fs::create_dir_all(source.join("nouvelle")).unwrap();
        fs::write(source.join("nouvelle").join("manifest.json"), "{}").unwrap();
        p.amorcer_extensions(&source);
        assert!(p.dossier_extensions().join("nouvelle").join("manifest.json").is_file());
        // … et une extension supprimee par l'utilisateur ne revient pas.
        fs::remove_dir_all(p.dossier_extensions().join("nouvelle")).unwrap();
        p.amorcer_extensions(&source);
        assert!(!p.dossier_extensions().join("nouvelle").exists());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn la_verification_d_apres_mise_a_jour_compare_le_contenu() {
        let p = profil("verif-livrees");
        let source = p.racine().join("source");
        fs::create_dir_all(source.join("demo")).unwrap();
        fs::write(source.join("demo").join("manifest.json"), r#"{"version":"1.0.0"}"#).unwrap();
        fs::write(source.join("demo").join("contenu.js"), "// neuf").unwrap();
        fs::create_dir_all(source.join("supprimee")).unwrap();
        fs::write(source.join("supprimee").join("manifest.json"), "{}").unwrap();
        p.amorcer_extensions(&source);
        fs::remove_dir_all(p.dossier_extensions().join("supprimee")).unwrap();
        // Identique : rien a faire.
        assert!(p.verifier_extensions_livrees(&source, false).is_empty());
        // Meme numero, contenu perime (une copie d'une tres vieille version) : remise a neuf.
        fs::write(p.dossier_extensions().join("demo").join("contenu.js"), "// vieux").unwrap();
        fs::write(p.dossier_extensions().join("demo").join("reste.js"), "x").unwrap();
        assert_eq!(p.verifier_extensions_livrees(&source, false), vec!["demo".to_string()]);
        assert_eq!(fs::read_to_string(p.dossier_extensions().join("demo").join("contenu.js")).unwrap(), "// neuf");
        assert!(!p.dossier_extensions().join("demo").join("reste.js").exists());
        assert!(dossiers_identiques(&source.join("demo"), &p.dossier_extensions().join("demo")));
        // Supprimee par l'utilisateur : elle ne revient pas, meme forcee.
        assert_eq!(p.verifier_extensions_livrees(&source, true), vec!["demo".to_string()]);
        assert!(!p.dossier_extensions().join("supprimee").exists());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn une_extension_livree_plus_recente_remplace_l_ancienne_et_la_garde() {
        let p = profil("maj-livree");
        let source = p.racine().join("source");
        fs::create_dir_all(source.join("demo")).unwrap();
        fs::write(source.join("demo").join("manifest.json"), r#"{"version":"1.0.0"}"#).unwrap();
        p.amorcer_extensions(&source);
        // Meme version : rien ne bouge, meme retouchee.
        fs::write(p.dossier_extensions().join("demo").join("retouche.js"), "x").unwrap();
        p.amorcer_extensions(&source);
        assert!(p.dossier_extensions().join("demo").join("retouche.js").is_file());
        // Version 1.10.0 livree : remplacee, l'ancienne gardee de cote.
        fs::write(source.join("demo").join("manifest.json"), r#"{"version":"1.10.0"}"#).unwrap();
        p.amorcer_extensions(&source);
        assert_eq!(version_manifeste(&p.dossier_extensions().join("demo")).as_deref(), Some("1.10.0"));
        assert!(!p.dossier_extensions().join("demo").join("retouche.js").exists());
        let gardees: Vec<_> = fs::read_dir(p.racine().join("extensions-anciennes")).unwrap().flatten().collect();
        assert_eq!(gardees.len(), 1);
        assert!(gardees[0].path().join("retouche.js").is_file());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn les_versions_se_comparent_nombre_par_nombre() {
        use std::cmp::Ordering::*;
        assert_eq!(comparer_versions("2.0.0", "1.0.0"), Greater);
        assert_eq!(comparer_versions("1.10.0", "1.9"), Greater);
        assert_eq!(comparer_versions("1.0", "1.0.0"), Equal);
        assert_eq!(comparer_versions("0.9", "1"), Less);
    }

    #[test]
    fn un_profil_d_avant_le_registre_recoit_seulement_les_nouvelles() {
        let p = Profil::avec_racine(std::env::temp_dir().join(format!("trame-livrees-{}", std::process::id())));
        let source = p.racine().join("source");
        for nom in ["compteur-visites", "saute-pub"] {
            fs::create_dir_all(source.join(nom)).unwrap();
            fs::write(source.join(nom).join("manifest.json"), "{}").unwrap();
        }
        // Profil existant, sans registre, ou l'utilisateur avait supprime compteur-visites.
        fs::create_dir_all(p.dossier_extensions().join("nuit-douce")).unwrap();
        p.amorcer_extensions(&source);
        assert!(p.dossier_extensions().join("saute-pub").is_dir(), "la nouvelle arrive");
        assert!(!p.dossier_extensions().join("compteur-visites").exists(), "la supprimee ne revient pas");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_nom_de_capsule_vide_donne_un_slug_utilisable() {
        assert_eq!(slug(""), "capsule");
        assert_eq!(slug("///"), "capsule");
        assert_eq!(slug("Veille techno"), "veille-techno");
        assert!(slug(&"a".repeat(200)).len() <= 40);
    }

    #[test]
    fn le_profil_webview2_est_rapatrie_dans_le_profil() {
        let p = profil("webview2-rapatriement");
        // Ancien emplacement : a cote de l'executable, comme le faisait wry par defaut.
        let ancien = p.racine().join("ancien.WebView2");
        fs::create_dir_all(ancien.join("EBWebView").join("Default")).unwrap();
        fs::write(ancien.join("EBWebView").join("Default").join("Cookies"), b"cookies").unwrap();

        let dossier = p.rapatrier_depuis(Some(ancien.clone()));
        assert_eq!(dossier, p.dossier_webview2(), "le nouveau dossier doit etre utilise");
        assert!(dossier.join("EBWebView").join("Default").join("Cookies").is_file());
        assert!(
            !ancien.is_dir(),
            "l'ancien dossier est deplace, pas laisse en double"
        );
        // Une seconde fois : deja en place, rien ne bouge.
        let encore = p.rapatrier_depuis(Some(ancien));
        assert_eq!(encore, dossier);
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn le_profil_roaming_est_deplace_vers_local() {
        let atelier = std::env::temp_dir().join(format!("trame-roaming-{}", std::process::id()));
        let _ = fs::remove_dir_all(&atelier);
        let ancienne = atelier.join("Roaming").join("Trame");
        let nouvelle = atelier.join("Local").join("Trame");
        fs::create_dir_all(ancienne.join("capsules")).unwrap();
        fs::write(ancienne.join("session.json"), "{\"onglets\":[],\"fils\":[]}").unwrap();
        fs::write(ancienne.join("capsules").join("un.json"), "{}").unwrap();
        fs::create_dir_all(ancienne.join("webview2").join("EBWebView")).unwrap();

        assert!(rapatrier_racine(&ancienne, &nouvelle), "le deplacement doit avoir lieu");
        assert!(nouvelle.join("session.json").is_file(), "session deplacee");
        assert!(nouvelle.join("capsules").join("un.json").is_file(), "sous-dossier deplace");
        assert!(nouvelle.join("webview2").join("EBWebView").is_dir(), "profil WebView2 deplace");
        assert!(!ancienne.exists(), "l'ancien dossier a ete deplace, pas duplique");
        // Deuxieme appel : rien a faire, et surtout on n'ecrase pas la nouvelle session.
        assert!(!rapatrier_racine(&ancienne, &nouvelle));
        let _ = fs::remove_dir_all(&atelier);
    }

    #[test]
    fn une_nouvelle_racine_deja_remplie_n_est_jamais_ecrasee() {
        let atelier = std::env::temp_dir().join(format!("trame-deja-{}", std::process::id()));
        let _ = fs::remove_dir_all(&atelier);
        let ancienne = atelier.join("ancienne");
        let nouvelle = atelier.join("nouvelle");
        fs::create_dir_all(&ancienne).unwrap();
        fs::write(ancienne.join("session.json"), "ancienne").unwrap();
        fs::create_dir_all(&nouvelle).unwrap();
        fs::write(nouvelle.join("session.json"), "nouvelle").unwrap();
        assert!(!rapatrier_racine(&ancienne, &nouvelle), "aucun deplacement");
        assert_eq!(fs::read_to_string(nouvelle.join("session.json")).unwrap(), "nouvelle");
        assert!(ancienne.is_dir(), "l'ancien dossier reste en place");
        let _ = fs::remove_dir_all(&atelier);
    }

    #[test]
    fn une_ancienne_racine_absente_ne_fait_rien() {
        let atelier = std::env::temp_dir().join(format!("trame-absente-{}", std::process::id()));
        let _ = fs::remove_dir_all(&atelier);
        let nouvelle = atelier.join("nouvelle");
        assert!(!rapatrier_racine(&atelier.join("inexistante"), &nouvelle));
        assert!(!nouvelle.exists(), "aucun dossier ne doit etre cree a vide");
    }

    #[test]
    fn la_racine_par_defaut_est_locale() {
        let racine = racine_locale();
        assert!(racine.ends_with("Trame"), "{:?}", racine);
        let local = dirs::data_local_dir().expect("LOCALAPPDATA existe sur Windows");
        assert!(
            racine.starts_with(&local),
            "le profil doit vivre sous LOCALAPPDATA, pas {:?}",
            racine
        );
        if let Some(roaming) = racine_roaming() {
            assert_ne!(racine, roaming, "roaming ne doit plus etre la racine");
        }
    }

    #[test]
    fn un_profil_webview2_deja_en_place_n_est_pas_touche() {
        let p = profil("webview2-stable");
        fs::create_dir_all(p.dossier_webview2().join("EBWebView")).unwrap();
        fs::write(p.dossier_webview2().join("EBWebView").join("marqueur"), b"x").unwrap();
        let dossier = p.rapatrier_depuis(Some(p.racine().join("inexistant.WebView2")));
        assert_eq!(dossier, p.dossier_webview2());
        assert!(dossier.join("EBWebView").join("marqueur").is_file());
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_ancien_emplacement_absent_ne_cree_rien() {
        let p = profil("webview2-absent");
        let dossier = p.rapatrier_depuis(None);
        assert_eq!(dossier, p.dossier_webview2());
        assert!(!dossier.exists(), "aucun dossier ne doit etre cree a vide");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_seul_verrou_par_profil() {
        let p = profil("verrou");
        let premier = p.prendre_verrou().expect("le premier verrou doit passer");
        let second = p.prendre_verrou();
        assert!(second.is_err(), "le second doit etre refuse");
        // Le PID rendu est celui du processus courant : il est vivant et s'appelle trame.
        assert_eq!(second.err(), Some(std::process::id()));
        drop(premier);
        assert!(!p.fichier_verrou().exists(), "le verrou disparait a l'arret");
        assert!(p.prendre_verrou().is_ok(), "il peut etre repris ensuite");
        let _ = fs::remove_dir_all(p.racine());
    }

    #[test]
    fn un_verrou_laisse_par_un_processus_mort_est_repris() {
        let p = profil("verrou-perime");
        // PID 0 : jamais vivant, donc verrou perime.
        fs::write(p.fichier_verrou(), "0").unwrap();
        assert!(p.prendre_verrou().is_ok(), "un verrou perime ne bloque pas");
        let _ = fs::remove_dir_all(p.racine());
    }
}
