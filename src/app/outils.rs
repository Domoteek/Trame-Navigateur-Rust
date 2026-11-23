//! Les outils du navigateur : favoris, impression, onglets a rendez-vous, cote a cote,
//! ouverture dans le fil courant, pages guettees et memoire de lecture.
//!
//! Tout ici reste leger par construction :
//!   - la memoire de lecture vit sur le disque et n'est relue que pendant une recherche, par un
//!     fil a part ;
//!   - un guet est relu par **une** page cachee a la fois, detruite des la lecture faite, et
//!     jamais quand la memoire libre du PC est sous la reserve ;
//!   - un onglet en rendez-vous est endormi (aucune page vivante) jusqu'a son heure.

use super::*;
use crate::carnet::{self, Acces, Favori, Guet, RAIL_MAX};
use std::sync::atomic::Ordering as Atomique;

/// La page cachee qui relit un guet.
pub(super) struct Sentinelle {
    pub guet: u64,
    pub vue: WebView,
    pub depuis: u64,
    /// Fin du chargement de la page (on laisse ensuite le temps aux scripts du site).
    pub charge_le: Option<u64>,
    /// Lecture demandee, reponse attendue.
    pub en_lecture: bool,
    pub essais: u8,
}

/// Une page doit rester affichee ce temps avant d'entrer dans la memoire de lecture.
const LECTURE_APRES_MS: u64 = 8_000;
/// Abandon d'une lecture de guet (page trop lente, bloquee).
const SENTINELLE_MAX_MS: u64 = 45_000;
/// Attente apres le chargement avant de lire l'element (sites qui s'affichent en JavaScript).
const SENTINELLE_ATTENTE_MS: u64 = 3_000;
/// Un rendez-vous ou une fermeture ne peut pas depasser 60 jours.
const ECHEANCE_MAX_MS: u64 = 60 * 86_400_000;

impl App {
    // ------------------------------------------------------------------ chargement

    pub(super) fn charger_carnet(&mut self) {
        self.favoris = self.profil.charger_liste(&self.profil.fichier_favoris());
        self.guets = self.profil.charger_liste(&self.profil.fichier_guets());
        self.acces_groupes = self.profil.charger_liste(&self.fichier_acces_groupes());
        if self.fichier_acces().exists() {
            self.acces = self.profil.charger_liste(&self.fichier_acces());
        } else if !self.favoris.is_empty() {
            // Avant le 2026-11-22, favoris et acces rapides etaient une seule liste : elle
            // devient les acces rapides, et reste aussi les favoris (rien ne se perd).
            self.acces = self.profil.charger_liste(&self.profil.fichier_favoris());
            self.sauver_acces();
        }
        info!(
            "carnet : {} favori(s), {} accès rapide(s), {} guet(s)",
            self.favoris.len(),
            self.acces.len(),
            self.guets.len()
        );
    }

    fn fichier_acces(&self) -> std::path::PathBuf {
        self.profil.racine().join(crate::carnet::NOM_FICHIER_ACCES)
    }

    fn sauver_acces(&self) {
        self.profil.sauver_liste(&self.fichier_acces(), &self.acces);
    }

    /// L'acces `url` peut-il etre dans la bande du rail : il y est deja, ou il reste une place.
    fn rail_accepte(&self, url: &str) -> bool {
        self.acces.iter().any(|a| a.url == url && a.rail) || self.acces.iter().filter(|a| a.rail).count() < RAIL_MAX
    }

    /// Un onglet ouvert sur le site d'un acces rapide ou d'un favori dont l'icone n'est pas
    /// encore gardee en image : on demande l'image a WebView2.
    pub(super) fn capturer_icone_si_utile(&self, onglet: TabId) {
        let Some(o) = self.trouver(onglet) else { return };
        let h = carnet::hote(&o.url);
        if h.is_empty() {
            return;
        }
        let manque = self.acces.iter().any(|a| carnet::hote(&a.url) == h && !carnet::icone_locale(&a.icone))
            || self.favoris.iter().any(|f| carnet::hote(&f.url) == h && !carnet::icone_locale(&f.icone));
        if let (true, Some(w)) = (manque, self.webviews.get(&onglet)) {
            crate::moteur_ext::lire_icone(w, self.proxy.clone(), onglet);
        }
    }

    /// Les onglets deja ouverts sur les sites des acces rapides et des favoris (apres un ajout).
    fn capturer_icones_ouvertes(&self) {
        let ids: Vec<TabId> = self.webviews.keys().copied().collect();
        for id in ids {
            self.capturer_icone_si_utile(id);
        }
    }

    /// Image de l'icone d'un onglet : elle remplace l'adresse des acces et favoris de son site.
    pub(super) fn icone_capturee(&mut self, onglet: TabId, donnees: String) {
        let Some(h) = self.trouver(onglet).map(|o| carnet::hote(&o.url)) else { return };
        if h.is_empty() {
            return;
        }
        let mut acces = false;
        for a in self.acces.iter_mut().filter(|a| carnet::hote(&a.url) == h && !carnet::icone_locale(&a.icone)) {
            a.icone = Some(donnees.clone());
            acces = true;
        }
        let mut favoris = false;
        for f in self.favoris.iter_mut().filter(|f| carnet::hote(&f.url) == h && !carnet::icone_locale(&f.icone)) {
            f.icone = Some(donnees.clone());
            favoris = true;
        }
        if acces {
            self.sauver_acces();
        }
        if favoris {
            self.sauver_favoris();
        }
        if acces || favoris {
            self.pousser_etat();
        }
    }

    fn signaler_rail_plein(&mut self) {
        self.signaler(&format!("Le rail tient {RAIL_MAX} accès rapides : retires-en un d'abord."));
    }

    fn fichier_acces_groupes(&self) -> std::path::PathBuf {
        self.profil.racine().join("acces-groupes.json")
    }

    fn sauver_favoris(&self) {
        self.profil.sauver_liste(&self.profil.fichier_favoris(), &self.favoris);
    }

    fn sauver_acces_groupes(&self) {
        self.profil.sauver_liste(&self.fichier_acces_groupes(), &self.acces_groupes);
    }

    /// Groupes a afficher : ceux crees, puis ceux qu'un acces porte sans etre dans la liste
    /// (fichier edite a la main, groupe perdu).
    fn groupes_affiches(&self) -> Vec<String> {
        let mut g = self.acces_groupes.clone();
        for f in &self.acces {
            if !f.groupe.is_empty() && !g.contains(&f.groupe) {
                g.push(f.groupe.clone());
            }
        }
        g
    }

    /// Onglet deja ouvert sur le site d'un acces rapide (meme hote), le plus recent d'abord.
    fn onglet_de_l_acces(&self, url: &str) -> Option<TabId> {
        let h = carnet::hote(url);
        if h.is_empty() {
            return None;
        }
        if let Some(id) = self.actif.filter(|id| self.trouver(*id).map(|o| carnet::hote(&o.url) == h).unwrap_or(false)) {
            return Some(id);
        }
        self.session
            .onglets
            .iter()
            .filter(|o| o.rendez_vous.is_none() && carnet::hote(&o.url) == h)
            .max_by_key(|o| o.dernier_focus)
            .map(|o| o.id)
    }

    /// Clic sur un acces rapide : **toujours** un nouvel onglet (rail, clic du milieu), meme si
    /// le site est deja ouvert ailleurs (choix de Clement, 2026-11-22). Depuis la page « Nouvel
    /// onglet », le site se charge a sa place : c'est deja un onglet neuf.
    fn ouvrir_acces(&mut self, url: &str, nouveau: bool) {
        if nouveau {
            self.ouvrir_onglet(url.to_string(), Declencheur::NouvelOnglet, None);
        } else {
            self.naviguer_actif(url);
        }
    }

    /// Icone connue d'un site : celle d'un onglet ouvert sur le meme hote.
    fn icone_connue(&self, url: &str) -> Option<String> {
        let h = carnet::hote(url);
        self.session
            .onglets
            .iter()
            .find(|o| carnet::hote(&o.url) == h && o.favicon.is_some())
            .and_then(|o| o.favicon.clone())
    }

    fn acces_json(&self) -> Value {
        Value::Array(
            self.acces
                .iter()
                .map(|f| {
                    json!({
                        "url": f.url,
                        "titre": if f.titre.is_empty() { urls::affichage_court(&f.url) } else { f.titre.clone() },
                        "domaine": urls::domaine_enregistrable(&f.url),
                        "groupe": f.groupe,
                        "rail": f.rail,
                        "icone": f.icone.clone().or_else(|| self.icone_connue(&f.url)),
                        "ouvert": self.onglet_de_l_acces(&f.url),
                    })
                })
                .collect(),
        )
    }

    fn sauver_guets(&self) {
        self.profil.sauver_liste(&self.profil.fichier_guets(), &self.guets);
    }

    /// Ce que l'interface doit savoir du carnet et des outils.
    pub(super) fn carnet_json(&self) -> Value {
        let actif = self.actif.and_then(|id| self.trouver(id));
        let url = actif.map(|o| o.url.clone()).unwrap_or_default();
        let (g, d) = self.session.cote.unwrap_or((0, 0));
        let precedent = self.onglet_precedent();
        json!({
            "favori": !url.is_empty() && self.favoris.iter().any(|f| f.url == url),
            "favoris": self.favoris.iter().rev().map(|f| json!({
                "url": f.url,
                "titre": if f.titre.is_empty() { urls::affichage_court(&f.url) } else { f.titre.clone() },
                "domaine": urls::domaine_enregistrable(&f.url),
                "icone": f.icone.clone().or_else(|| self.icone_connue(&f.url)),
            })).collect::<Vec<_>>(),
            "acces": self.acces_json(),
            "accesRailMax": RAIL_MAX,
            "accesGroupes": self.groupes_affiches(),
            "cote": self.session.cote.map(|_| json!({
                "gauche": g, "droite": d,
                "affiche": self.paire_affichee().is_some(),
                "lie": self.session.defilement_lie,
            })),
            "precedent": precedent.and_then(|id| self.trouver(id)).map(|o| json!({"id": o.id, "titre": o.libelle()})),
            "guets": self.guets.iter().map(|g| json!({
                "id": g.id,
                "url": g.url,
                "libelle": if g.libelle.is_empty() { urls::affichage_court(&g.url) } else { g.libelle.clone() },
                "texte": g.texte,
                "ancien": g.ancien,
                "change": g.change_le,
                "vu": g.vu,
                "verifie": g.verifie_le,
                "echecs": g.echecs,
                "intervalle": g.intervalle_min,
                "enCours": self.sentinelle.as_ref().map(|s| s.guet == g.id).unwrap_or(false),
            })).collect::<Vec<_>>(),
            "guetsNonVus": self.guets.iter().filter(|g| !g.vu).count(),
            "rendezVous": self.session.onglets.iter().filter(|o| o.rendez_vous.is_some()).count(),
            "pageWeb": actif.map(|o| o.url.starts_with("http://") || o.url.starts_with("https://")).unwrap_or(false),
            "lectureExclu": actif
                .map(|o| !carnet::memorisable(&o.url, &o.domaine(), &self.reglages.lecture_exclus) && o.url.starts_with("http"))
                .unwrap_or(false),
        })
    }

    // ------------------------------------------------------------------ messages

    /// Messages propres aux outils. Rend `true` si le message a ete traite ici.
    pub(super) fn traiter_ipc_outils(&mut self, source: Source, m: &str, v: &Value) -> bool {
        // Les surfaces de Trame seulement (l'autorisation generale est deja passee : un onglet
        // distant n'arrive jamais jusqu'ici avec ces messages).
        match m {
            "naviguer" | "ouvrir" => {
                let mode = v["mode"].as_str().unwrap_or("");
                if mode != "onglet" && mode != "cote" {
                    return false;
                }
                let url = if m == "naviguer" {
                    let saisie = v["url"].as_str().unwrap_or("").trim().to_string();
                    (!saisie.is_empty()).then(|| urls::normaliser_avec(&saisie, &self.reglages.moteur))
                } else {
                    let a = &v["action"];
                    match a["type"].as_str().unwrap_or("") {
                        "navigation" | "historique" | "favori" | "lecture" => a["url"].as_str().map(|s| s.to_string()),
                        "onglet" if mode == "cote" => {
                            if let Some(id) = a["id"].as_u64() {
                                self.cote_avec(id as TabId);
                            }
                            return true;
                        }
                        // Un onglet, un fil, une commande : le mode n'a pas de sens, on ouvre.
                        _ => return false,
                    }
                };
                if let Some(url) = url {
                    self.ouvrir_dans_le_fil(url, mode == "cote");
                }
                true
            }
            "favori" => {
                self.basculer_favori();
                true
            }
            "favoriOuvrir" => {
                // Un favori est une page precise : l'onglet ou elle est deja ouverte (meme adresse),
                // sinon un nouvel onglet — jamais a la place de la page affichee.
                if let Some(url) = v["url"].as_str().filter(|u| self.favoris.iter().any(|f| f.url == *u)) {
                    let url = url.to_string();
                    // Adresses reelles d'abord : un onglet YouTube passe a une autre video sans
                    // recharger, son adresse retenue peut etre en retard.
                    let vivants: Vec<TabId> = self.webviews.keys().copied().collect();
                    for id in vivants {
                        self.actualiser_adresse(id);
                    }
                    let ouvert = self
                        .session
                        .onglets
                        .iter()
                        .filter(|o| o.rendez_vous.is_none() && o.url == url)
                        .max_by_key(|o| o.dernier_focus)
                        .map(|o| o.id);
                    match ouvert {
                        Some(id) if Some(id) != self.actif => self.activer(id),
                        Some(_) => {}
                        None => {
                            self.ouvrir_onglet(url, Declencheur::NouvelOnglet, None);
                        }
                    }
                }
                true
            }
            "favoriRetirer" => {
                let url = v["url"].as_str().unwrap_or("");
                let avant = self.favoris.len();
                self.favoris.retain(|f| f.url != url);
                if self.favoris.len() != avant {
                    self.sauver_favoris();
                    self.pousser_etat();
                }
                true
            }
            // ---- acces rapides (page « Nouvel onglet » et bande du rail)
            "accesRetirer" => {
                let url = v["url"].as_str().unwrap_or("");
                let avant = self.acces.len();
                self.acces.retain(|f| f.url != url);
                if self.acces.len() != avant {
                    self.sauver_acces();
                    self.pousser_etat();
                }
                true
            }
            "accesOuvrir" => {
                if let Some(url) = v["url"].as_str().filter(|u| self.acces.iter().any(|f| f.url == *u)) {
                    let url = url.to_string();
                    self.ouvrir_acces(&url, v["nouveau"].as_bool().unwrap_or(false));
                }
                true
            }
            "accesEnregistrer" => {
                // Ajout (pas d'`ancienne`) ou modification d'un acces existant.
                let saisie = v["url"].as_str().unwrap_or("").trim();
                if saisie.is_empty() {
                    return true;
                }
                let url = urls::normaliser_avec(saisie, &self.reglages.moteur);
                if !carnet::favorisable(&url) {
                    self.signaler("Un accès rapide doit être une adresse web.");
                    return true;
                }
                let titre = v["titre"].as_str().unwrap_or("").trim().to_string();
                let groupe = v["groupe"].as_str().unwrap_or("").trim().to_string();
                let ancienne = v["ancienne"].as_str().unwrap_or("");
                let mut rail = v["rail"].as_bool().unwrap_or(false);
                if rail && !self.rail_accepte(ancienne) {
                    self.signaler_rail_plein();
                    rail = false;
                }
                if !groupe.is_empty() && !self.acces_groupes.contains(&groupe) {
                    self.acces_groupes.push(groupe.clone());
                    self.sauver_acces_groupes();
                }
                let pos = self
                    .acces
                    .iter()
                    .position(|f| !ancienne.is_empty() && f.url == ancienne)
                    .or_else(|| self.acces.iter().position(|f| f.url == url));
                let icone_onglet = self.icone_connue(&url);
                match pos {
                    Some(i) => {
                        let f = &mut self.acces[i];
                        if f.url != url {
                            f.icone = None;
                        }
                        f.url = url;
                        f.titre = titre;
                        f.groupe = groupe;
                        f.rail = rail;
                        if f.icone.is_none() {
                            f.icone = icone_onglet;
                        }
                    }
                    None => {
                        let titre = if titre.is_empty() { carnet::nom_du_site(&url) } else { titre };
                        self.acces.push(Acces {
                            url,
                            titre,
                            ajoute_le: maintenant_ms(),
                            groupe,
                            rail,
                            icone: icone_onglet,
                        });
                    }
                }
                // Une adresse modifiee peut rejoindre celle d'un autre acces : un seul garde.
                let mut vues = std::collections::HashSet::new();
                self.acces.retain(|f| vues.insert(f.url.clone()));
                self.sauver_acces();
                self.pousser_etat();
                self.capturer_icones_ouvertes();
                true
            }
            "accesDepuisOnglet" => {
                // Un onglet glisse sur la bande du rail ou sur les acces rapides du Nouvel onglet,
                // ou le « + » de la bande (`actif`) : l'acces mene au SITE de la page, pas a la
                // page elle-meme (une page precise, c'est un favori).
                let id = v["onglet"].as_u64().map(|n| n as TabId).or_else(|| v["actif"].as_bool().filter(|b| *b).and(self.actif));
                let Some(o) = id.and_then(|id| self.trouver(id)).cloned() else {
                    return true;
                };
                if !o.url.starts_with("http://") && !o.url.starts_with("https://") {
                    self.signaler("Les accès rapides s'appliquent aux sites web.");
                    return true;
                }
                let url = carnet::racine_du_site(&o.url);
                let mut rail = v["rail"].as_bool().unwrap_or(false);
                let groupe = v["groupe"].as_str().unwrap_or("").trim().to_string();
                if rail && !self.rail_accepte(&url) {
                    self.signaler_rail_plein();
                    rail = false;
                }
                if let Some(f) = self.acces.iter_mut().find(|f| f.url == url) {
                    f.rail |= rail;
                    if f.icone.is_none() {
                        f.icone = o.favicon.clone();
                    }
                } else {
                    self.acces.push(Acces {
                        url,
                        titre: carnet::nom_du_site(&o.url),
                        ajoute_le: maintenant_ms(),
                        groupe,
                        rail,
                        icone: o.favicon.clone(),
                    });
                }
                self.signaler(if rail { "Ajouté aux accès rapides du rail." } else { "Ajouté aux accès rapides." });
                self.sauver_acces();
                self.pousser_etat();
                self.capturer_icones_ouvertes();
                true
            }
            "accesRail" => {
                if let Some(url) = v["url"].as_str() {
                    if let Some(i) = self.acces.iter().position(|f| f.url == url) {
                        let voulu = v["rail"].as_bool().unwrap_or(!self.acces[i].rail);
                        if voulu && !self.rail_accepte(url) {
                            self.signaler_rail_plein();
                            return true;
                        }
                        self.acces[i].rail = voulu;
                        self.sauver_acces();
                        self.pousser_etat();
                    }
                }
                true
            }
            "accesGroupeDe" => {
                if let Some(url) = v["url"].as_str() {
                    let groupe = v["groupe"].as_str().unwrap_or("").trim().to_string();
                    if let Some(f) = self.acces.iter_mut().find(|f| f.url == url) {
                        f.groupe = groupe;
                        self.sauver_acces();
                        self.pousser_etat();
                    }
                }
                true
            }
            "accesOrdre" => {
                // Nouvel ordre complet (glisser-deposer) ; les adresses inconnues sont ignorees,
                // les oubliees gardent leur place relative a la fin.
                if let Some(liste) = v["urls"].as_array() {
                    let mut neuf: Vec<Acces> = Vec::with_capacity(self.acces.len());
                    for u in liste.iter().filter_map(|u| u.as_str()) {
                        if let Some(i) = self.acces.iter().position(|f| f.url == u) {
                            neuf.push(self.acces.remove(i));
                        }
                    }
                    neuf.append(&mut self.acces);
                    self.acces = neuf;
                    self.sauver_acces();
                    self.pousser_etat();
                }
                true
            }
            "accesGroupeAjouter" => {
                let nom = v["nom"].as_str().unwrap_or("").trim().to_string();
                if !nom.is_empty() && !self.acces_groupes.contains(&nom) {
                    self.acces_groupes = self.groupes_affiches();
                    self.acces_groupes.push(nom);
                    self.sauver_acces_groupes();
                    self.pousser_etat();
                }
                true
            }
            "accesGroupeRenommer" | "accesGroupeRetirer" => {
                let nom = v["nom"].as_str().unwrap_or("").trim().to_string();
                let nouveau = if m == "accesGroupeRenommer" {
                    v["nouveau"].as_str().unwrap_or("").trim().to_string()
                } else {
                    String::new()
                };
                if nom.is_empty() || (m == "accesGroupeRenommer" && nouveau.is_empty()) {
                    return true;
                }
                self.acces_groupes = self.groupes_affiches();
                if nouveau.is_empty() || self.acces_groupes.contains(&nouveau) {
                    self.acces_groupes.retain(|g| *g != nom);
                } else if let Some(g) = self.acces_groupes.iter_mut().find(|g| **g == nom) {
                    *g = nouveau.clone();
                }
                // Les acces du groupe retire restent, sans groupe.
                for f in self.acces.iter_mut().filter(|f| f.groupe == nom) {
                    f.groupe = nouveau.clone();
                }
                self.sauver_acces_groupes();
                self.sauver_acces();
                self.pousser_etat();
                self.capturer_icones_ouvertes();
                true
            }
            "imprimer" => {
                self.imprimer();
                true
            }
            "chercherPage" => {
                self.chercher_dans_page();
                true
            }
            "rendezVous" | "fermeture" => {
                let id = v["id"].as_u64().map(|n| n as TabId).or(self.actif);
                let quand = v["quand"].as_u64();
                if let Some(id) = id {
                    if m == "rendezVous" {
                        self.programmer_rendez_vous(id, quand, v["libelle"].as_str().unwrap_or(""));
                    } else {
                        self.programmer_fermeture(id, quand, v["libelle"].as_str().unwrap_or(""));
                    }
                }
                true
            }
            "rendezVousMaintenant" => {
                if let Some(id) = v["id"].as_u64() {
                    self.activer(id as TabId);
                }
                true
            }
            "coteAvec" => {
                if let Some(id) = v["id"].as_u64() {
                    self.cote_avec(id as TabId);
                }
                true
            }
            "coteFermer" => {
                self.cote_fermer();
                true
            }
            "coteInverser" => {
                if let Some((g, d)) = self.session.cote {
                    self.session.cote = Some((d, g));
                    self.appliquer_geometrie();
                    self.marquer_modifie();
                    self.pousser_etat();
                }
                true
            }
            "coteLie" => {
                self.session.defilement_lie = v["actif"].as_bool().unwrap_or(!self.session.defilement_lie);
                self.marquer_modifie();
                self.pousser_etat();
                true
            }
            "menuPage" => {
                // Clic droit sur un onglet du rail : son menu s'ouvre SANS l'afficher (on peut
                // l'endormir, le reveiller, le deplacer… sans quitter la page lue). `cible` dit au
                // menu de quel onglet il s'agit.
                let cible = v["id"].as_u64();
                // `y` (hauteur du clic dans le rail, meme repere que la barre) : le menu se pose a
                // cote de l'onglet. `h` : hauteur de la fenetre, pour qu'il n'en deborde pas.
                let ancre = v["y"].as_f64().map(|y| {
                    let echelle = self.window.scale_factor();
                    let h = self.window.inner_size().height as f64 / echelle;
                    serde_json::json!({ "y": y, "h": h.round(), "cible": cible })
                });
                let ancre = ancre.map(|a| a.to_string()).unwrap_or_default();
                if let Some(b) = &self.barre {
                    let _ = b.evaluate_script(&format!(
                        "window.trame && window.trame.menuPage && window.trame.menuPage({ancre})"
                    ));
                }
                true
            }
            "bulle" => {
                // Infobulle du rail replie : 64 px de large, il ne peut pas la dessiner. La barre
                // la pose en carte a droite de l'icone (`ui/bulles.js`, `carte`), dans la colonne
                // de son panneau. `y` : meme repere que la barre. `h` : hauteur de la fenetre, pour
                // que la carte d'une icone basse remonte au lieu de deborder. Sans `texte` : la cacher.
                let echelle = self.window.scale_factor();
                let h = (self.window.inner_size().height as f64 / echelle).round();
                let b = serde_json::json!({
                    "texte": v["texte"].as_str().unwrap_or(""),
                    "y": v["y"].as_f64().unwrap_or(0.0),
                    "h": h,
                });
                if let Some(barre) = &self.barre {
                    let _ = barre.evaluate_script(&format!(
                        "window.trame && window.trame.bulle && window.trame.bulle({b})"
                    ));
                }
                true
            }
            "menuCommandes" => {
                // Clic droit sur le logo du rail : la liste des commandes. Rail deplie (`ici`) :
                // le rail la dessine sous le logo. Rail replie (64 px) : la barre la dessine.
                // Dans la barre, elle tombait loin du logo (retour de Clement, 2026-11-22).
                let liste: Vec<serde_json::Value> = crate::search::commandes(">")
                    .iter()
                    .map(|r| serde_json::json!({"id": r.id, "titre": r.titre, "raccourci": r.sous_titre}))
                    .collect();
                let js = format!(
                    "window.trame && window.trame.menuCommandes && window.trame.menuCommandes({})",
                    serde_json::Value::Array(liste)
                );
                if v["ici"].as_bool().unwrap_or(false) {
                    self.eval_sur(source, &js);
                } else if let Some(b) = &self.barre {
                    let _ = b.evaluate_script(&js);
                }
                true
            }
            "guetPoser" => {
                self.demarrer_guet();
                true
            }
            "guetRetirer" => {
                let id = v["id"].as_u64().unwrap_or(0);
                self.guets.retain(|g| g.id != id);
                if self.sentinelle.as_ref().map(|s| s.guet == id).unwrap_or(false) {
                    self.sentinelle = None;
                }
                self.sauver_guets();
                self.pousser_etat();
                true
            }
            "guetOuvrir" => {
                let id = v["id"].as_u64().unwrap_or(0);
                self.ouvrir_guet(id);
                true
            }
            "guetRelire" => {
                let id = v["id"].as_u64().unwrap_or(0);
                if let Some(g) = self.guets.iter_mut().find(|g| g.id == id) {
                    g.verifie_le = 0;
                    g.echecs = 0;
                }
                self.dernier_guet = 0;
                self.signaler("Le guet sera relu dans quelques secondes.");
                self.pousser_etat();
                true
            }
            "guetIntervalle" => {
                let id = v["id"].as_u64().unwrap_or(0);
                let minutes = v["minutes"].as_u64().unwrap_or(30).clamp(5, 24 * 60) as u32;
                if let Some(g) = self.guets.iter_mut().find(|g| g.id == id) {
                    g.intervalle_min = minutes;
                }
                self.sauver_guets();
                self.pousser_etat();
                true
            }
            "memoireLecture" => {
                self.reglages.memoire_lecture = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                self.pousser_etat();
                true
            }
            "lectureExclure" => {
                // Le site de l'onglet actif (ou celui donne) ne sera plus garde, et ce qui en
                // avait ete garde est efface.
                let domaine = v["domaine"]
                    .as_str()
                    .map(|s| s.trim().to_lowercase())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| self.domaine_actif());
                if domaine.is_empty() {
                    return true;
                }
                if !self.reglages.lecture_exclus.contains(&domaine) {
                    self.reglages.lecture_exclus.push(domaine.clone());
                    self.profil.sauver_reglages(&self.reglages);
                }
                let exclus = self.reglages.lecture_exclus.clone();
                let chemin = self.profil.fichier_lectures();
                std::thread::spawn(move || {
                    let _ = carnet::compacter(&chemin, usize::MAX, &|l| {
                        carnet::memorisable(&l.url, &urls::domaine_enregistrable(&l.url), &exclus)
                    });
                });
                self.signaler(&format!("{domaine} ne sera plus gardé dans la mémoire de lecture (et ce qui l'était est effacé)."));
                self.pousser_etat();
                true
            }
            "lectureInclure" => {
                let domaine = v["domaine"].as_str().unwrap_or("").to_string();
                self.reglages.lecture_exclus.retain(|d| *d != domaine);
                self.profil.sauver_reglages(&self.reglages);
                self.pousser_etat();
                true
            }
            "lecturesEffacer" => {
                let chemin = self.profil.fichier_lectures();
                let _ = std::fs::remove_file(&chemin);
                self.lectures_faites.clear();
                self.signaler("Mémoire de lecture effacée.");
                self.pousser_etat();
                true
            }
            "lecturesCompter" => {
                let chemin = self.profil.fichier_lectures();
                let proxy = self.proxy.clone();
                std::thread::spawn(move || {
                    let n = carnet::compter_lectures(&chemin);
                    let octets = std::fs::metadata(&chemin).map(|m| m.len()).unwrap_or(0);
                    let corps = json!({"n": n, "mo": (octets as f64 / 1048576.0 * 10.0).round() / 10.0});
                    let js = format!("window.trame && window.trame.lecturesCompte && window.trame.lecturesCompte({corps})");
                    let _ = proxy.send_event(UserEvent::Script { source, js });
                });
                true
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------------ ouvrir dans le fil

    /// Ouvre une adresse dans un **nouvel onglet du fil courant**, juste apres la page en
    /// cours (Alt+Entree dans la barre). Avec `cote`, la nouvelle page s'affiche a cote de
    /// celle d'ou l'on vient (Maj+Entree).
    pub(super) fn ouvrir_dans_le_fil(&mut self, url: String, cote: bool) {
        let avant = self.actif;
        let fil = avant.and_then(|id| self.trouver(id)).map(|o| o.fil);
        let id = self.ouvrir_onglet(url, Declencheur::Saisie, avant);
        if let (Some(fil), Some(avant)) = (fil, avant) {
            let ancien_fil = self.trouver(id).map(|o| o.fil);
            if let Some(o) = self.onglet_mut(id) {
                o.fil = fil;
            }
            // Le fil cree pour l'occasion, s'il est reste vide, disparait.
            if let Some(ancien) = ancien_fil.filter(|f| *f != fil) {
                if !self.session.onglets.iter().any(|o| o.fil == ancien) {
                    self.session.fils.retain(|f| f.id != ancien);
                }
            }
            // Juste apres la page d'ou l'on vient.
            if let Some(pos) = self.session.onglets.iter().position(|o| o.id == id) {
                let onglet = self.session.onglets.remove(pos);
                let apres = self
                    .session
                    .onglets
                    .iter()
                    .position(|o| o.id == avant)
                    .map(|p| p + 1)
                    .unwrap_or(self.session.onglets.len());
                self.session.onglets.insert(apres, onglet);
            }
            self.renommer_fils_auto();
            if cote {
                self.session.cote = Some((avant, id));
                self.preparer_cote();
                info!("cote a cote : onglets {avant} et {id}");
            }
        }
        self.appliquer_geometrie();
        self.marquer_modifie();
        self.pousser_etat();
    }

    // ------------------------------------------------------------------ favoris, impression

    pub(super) fn basculer_favori(&mut self) {
        // L'adresse vraie de la page, pas celle du dernier chargement (YouTube change de video
        // sans recharger : le favori gardait youtube.com, constate le 2026-11-22).
        if let Some(id) = self.actif {
            self.actualiser_adresse(id);
        }
        let Some(o) = self.actif.and_then(|id| self.trouver(id)).cloned() else { return };
        if !carnet::favorisable(&o.url) {
            self.signaler("Les favoris s'appliquent aux pages web.");
            return;
        }
        if let Some(pos) = self.favoris.iter().position(|f| f.url == o.url) {
            self.favoris.remove(pos);
            self.signaler("Retiré des favoris.");
        } else {
            self.favoris.push(Favori {
                url: o.url.clone(),
                titre: o.libelle(),
                ajoute_le: maintenant_ms(),
                icone: o.favicon.clone(),
            });
            self.signaler("Page ajoutée aux favoris (Ctrl+D pour la retirer).");
        }
        self.sauver_favoris();
        self.pousser_etat();
        self.capturer_icones_ouvertes();
    }

    /// Favoris qui repondent a une saisie (pour la barre). Vide : les plus recents.
    pub(super) fn resultats_favoris(&self, requete: &str) -> Vec<search::Resultat> {
        let q = requete.trim();
        if q.starts_with('>') || q.starts_with('?') {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (rang, f) in self.favoris.iter().rev().enumerate() {
            let titre = if f.titre.is_empty() { urls::affichage_court(&f.url) } else { f.titre.clone() };
            let p = if q.is_empty() {
                if rang >= 6 {
                    continue;
                }
                0.5
            } else {
                search::pertinence(q, &titre).max(search::pertinence(q, &f.url) * 0.9)
            };
            if p <= 0.3 {
                continue;
            }
            out.push(search::Resultat {
                genre: "favori".into(),
                id: f.url.clone(),
                titre,
                sous_titre: urls::affichage_court(&f.url),
                score: 0.72 * p + 0.28,
                action: json!({"type": "favori", "url": f.url}),
            });
        }
        out
    }

    pub(super) fn imprimer(&mut self) {
        let Some(id) = self.actif else { return };
        let interne = self.trouver(id).map(|o| o.interne()).unwrap_or(true);
        if interne || self.vue_trame {
            self.signaler("L'impression s'applique aux pages web.");
            return;
        }
        match self.webviews.get(&id) {
            // La boite de dialogue de Chromium : imprimante, ou « Enregistrer au format PDF ».
            Some(w) => {
                if let Err(e) = w.print() {
                    alerte!("impression impossible : {e}");
                    self.signaler("Impression impossible sur cette page.");
                }
            }
            None => self.signaler("Cet onglet est en veille : un clic le réveille d'abord."),
        }
    }

    // ------------------------------------------------------------------ rendez-vous

    /// Met un onglet de cote jusqu'a `quand` (heure locale calculee par l'interface, en ms).
    fn programmer_rendez_vous(&mut self, id: TabId, quand: Option<u64>, libelle: &str) {
        let now = maintenant_ms();
        let quand = match quand {
            Some(q) if q > now && q - now <= ECHEANCE_MAX_MS => q,
            None => {
                if let Some(o) = self.onglet_mut(id) {
                    o.rendez_vous = None;
                }
                self.pousser_etat();
                return;
            }
            _ => return,
        };
        let Some(o) = self.trouver(id).cloned() else { return };
        if let Some(o) = self.onglet_mut(id) {
            o.rendez_vous = Some(quand);
            o.revenu = false;
        }
        // L'onglet quitte l'ecran : on passe a un voisin, puis on l'endort.
        if self.actif == Some(id) {
            let suivant = self
                .session
                .onglets
                .iter()
                .filter(|x| x.id != id && x.rendez_vous.is_none())
                .max_by_key(|x| x.dernier_focus)
                .map(|x| x.id);
            match suivant {
                Some(s) => self.activer(s),
                None => {
                    let neuf = self.ouvrir_onglet(urls::NOUVEL_ONGLET.to_string(), Declencheur::NouvelOnglet, None);
                    self.activer(neuf);
                }
            }
        }
        if matches!(self.session.cote, Some((g, d)) if g == id || d == id) {
            self.session.cote = None;
            self.appliquer_geometrie();
        }
        self.endormir(id);
        let quand_txt = if libelle.is_empty() { "plus tard".to_string() } else { libelle.to_string() };
        self.signaler(&format!("« {} » revient {quand_txt}. D'ici là il dort, hors du rail.", o.libelle()));
        self.marquer_modifie();
        self.pousser_etat();
    }

    fn programmer_fermeture(&mut self, id: TabId, quand: Option<u64>, libelle: &str) {
        let now = maintenant_ms();
        let valeur = match quand {
            Some(q) if q > now && q - now <= ECHEANCE_MAX_MS => Some(q),
            None => None,
            _ => return,
        };
        if let Some(o) = self.onglet_mut(id) {
            o.fermeture = valeur;
        }
        match valeur {
            Some(_) => self.signaler(&format!("Cet onglet se fermera seul {libelle} (Ctrl+Maj+T le rouvre).")),
            None => self.signaler("Fermeture programmée annulée."),
        }
        self.marquer_modifie();
        self.pousser_etat();
    }

    /// Rendez-vous echus et fermetures programmees.
    fn echeances(&mut self, now: u64) {
        let revenus: Vec<TabId> = self
            .session
            .onglets
            .iter()
            .filter(|o| o.rendez_vous.map(|q| q <= now).unwrap_or(false))
            .map(|o| o.id)
            .collect();
        for id in &revenus {
            let titre = self.trouver(*id).map(|o| o.libelle()).unwrap_or_default();
            if let Some(o) = self.onglet_mut(*id) {
                o.rendez_vous = None;
                o.revenu = true;
                o.vitalite = 1.0;
            }
            info!("rendez-vous : onglet {id} revient");
            self.signaler(&format!("Rendez-vous : « {titre} » est de retour dans le rail."));
        }
        if !revenus.is_empty() {
            self.window
                .request_user_attention(Some(tao::window::UserAttentionType::Informational));
            self.marquer_modifie();
            self.pousser_etat();
        }
        let a_fermer: Vec<TabId> = self
            .session
            .onglets
            .iter()
            .filter(|o| o.fermeture.map(|q| q <= now).unwrap_or(false))
            .map(|o| o.id)
            .collect();
        for id in a_fermer {
            info!("fermeture programmee : onglet {id}");
            if let Some(o) = self.onglet_mut(id) {
                o.fermeture = None;
            }
            self.fermer_onglet(id);
        }
    }

    // ------------------------------------------------------------------ cote a cote

    fn ratio_cote(&self) -> f64 {
        let r = self.session.ratio_cote;
        if (0.2..=0.8).contains(&r) {
            r
        } else {
            0.5
        }
    }

    /// La paire affichee maintenant : l'un des deux est l'onglet actif, les deux existent.
    pub(super) fn paire_affichee(&self) -> Option<(TabId, TabId)> {
        let (g, d) = self.session.cote?;
        let actif = self.actif?;
        if self.vue_trame || (actif != g && actif != d) || g == d {
            return None;
        }
        (self.trouver(g).is_some() && self.trouver(d).is_some()).then_some((g, d))
    }

    pub(super) fn dans_cote_affiche(&self, id: TabId) -> bool {
        self.paire_affichee().map(|(g, d)| id == g || id == d).unwrap_or(false)
    }

    /// Les deux rectangles du cote a cote et la separation (x, y, largeur, hauteur physiques).
    pub(super) fn rects_cote(&self, geo: &Geometrie) -> Option<(TabId, TabId, wry::Rect, wry::Rect, (i32, i32, i32, i32))> {
        let (g, d) = self.paire_affichee()?;
        if !self.webviews.contains_key(&g) || !self.webviews.contains_key(&d) {
            return None;
        }
        let (cx, cy, cl, ch) = rect_physique(geo.contenu);
        let echelle = self.window.scale_factor();
        let jour = ((6.0 * echelle).round() as i32).max(2);
        let lg = (((cl - jour) as f64) * self.ratio_cote()).round() as i32;
        let ld = (cl - jour - lg).max(1);
        let rect = |x: i32, l: i32| wry::Rect {
            position: tao::dpi::PhysicalPosition::new(x, cy).into(),
            size: tao::dpi::PhysicalSize::new(l.max(1) as u32, ch.max(1) as u32).into(),
        };
        Some((g, d, rect(cx, lg), rect(cx + lg + jour, ld), (cx + lg, cy, jour, ch)))
    }

    /// Reveille l'autre page de la paire si elle dort.
    pub(super) fn preparer_cote(&mut self) {
        if let Some((g, d)) = self.paire_affichee() {
            for id in [g, d] {
                if !self.webviews.contains_key(&id) {
                    self.reveiller(id);
                }
            }
        }
    }

    /// L'onglet le plus recemment regarde, hors l'actif (et hors rendez-vous).
    pub(super) fn onglet_precedent(&self) -> Option<TabId> {
        let actif = self.actif?;
        self.session
            .onglets
            .iter()
            .filter(|o| o.id != actif && o.rendez_vous.is_none())
            .max_by_key(|o| o.dernier_focus)
            .map(|o| o.id)
    }

    /// Affiche `id` a cote de l'onglet actif (a droite). Sans onglet actif, rien.
    pub(super) fn cote_avec(&mut self, id: TabId) {
        let Some(actif) = self.actif else { return };
        let autre = if id == actif { self.onglet_precedent() } else { Some(id) };
        let Some(autre) = autre else {
            self.signaler("Il faut un deuxième onglet pour le côte à côte.");
            return;
        };
        if let Some(o) = self.onglet_mut(autre) {
            o.rendez_vous = None;
        }
        self.session.cote = Some((actif, autre));
        self.preparer_cote();
        self.appliquer_geometrie();
        self.marquer_modifie();
        self.pousser_etat();
    }

    pub(super) fn cote_fermer(&mut self) {
        if self.session.cote.take().is_some() {
            self.cote_glisse = false;
            self.window.set_cursor_icon(tao::window::CursorIcon::Default);
            self.appliquer_geometrie();
            self.marquer_modifie();
            self.pousser_etat();
        }
    }

    /// Le defilement d'une page du cote a cote entraine l'autre, quand le lien est mis.
    pub(super) fn lier_defilement(&mut self, id: TabId, y: f64) {
        if !self.session.defilement_lie {
            return;
        }
        let Some((g, d)) = self.paire_affichee() else { return };
        if id != g && id != d {
            return;
        }
        // L'autre page signale le defilement qu'on vient de lui imposer : ne pas le renvoyer.
        if let Some((cible, valeur)) = self.echo_defilement {
            if cible == id && (valeur - y).abs() < 4.0 {
                return;
            }
        }
        let autre = if id == g { d } else { g };
        self.echo_defilement = Some((autre, y));
        self.eval_sur(Source::Onglet(autre), &format!("window.scrollTo(0, {});", y.max(0.0)));
    }

    // ---- souris sur la separation

    fn sur_separation(&self, x: f64, y: f64) -> bool {
        let geo = self.geometrie_courante();
        match self.rects_cote(&geo) {
            Some((_, _, _, _, (sx, sy, sl, sh))) => {
                let marge = 3.0;
                x >= sx as f64 - marge && x <= (sx + sl) as f64 + marge && y >= sy as f64 && y <= (sy + sh) as f64
            }
            None => false,
        }
    }

    pub fn souris_bouge(&mut self, x: f64, y: f64) {
        if self.cote_glisse {
            let geo = self.geometrie_courante();
            let (cx, _, cl, _) = rect_physique(geo.contenu);
            if cl > 0 {
                self.session.ratio_cote = ((x - cx as f64) / cl as f64).clamp(0.2, 0.8);
                self.appliquer_geometrie();
            }
            return;
        }
        let dessus = self.sur_separation(x, y);
        if dessus != self.cote_survol {
            self.cote_survol = dessus;
            self.window.set_cursor_icon(if dessus {
                tao::window::CursorIcon::ColResize
            } else {
                tao::window::CursorIcon::Default
            });
        }
    }

    pub fn souris_sortie(&mut self) {
        if !self.cote_glisse && self.cote_survol {
            self.cote_survol = false;
            self.window.set_cursor_icon(tao::window::CursorIcon::Default);
        }
    }

    pub fn souris_bouton(&mut self, appuye: bool) {
        if appuye && self.cote_survol {
            self.cote_glisse = true;
        } else if !appuye && self.cote_glisse {
            self.cote_glisse = false;
            self.marquer_modifie();
        }
    }

    // ------------------------------------------------------------------ battement

    pub(super) fn outils_tick(&mut self, now: u64) {
        // Sonde `TRAME_IPC` (diagnostic) : rejoue des messages de la barre, un par battement, a
        // partir de 4 s apres l'ouverture des onglets. Sert a eprouver les outils sans souris.
        if self.restaure && now.saturating_sub(self.demarrage_ms) > 7_000 {
            if let Some(corps) = self.sonde_ipc.pop_front() {
                info!("sonde ipc : {corps}");
                self.traiter_ipc(Source::Barre, &corps);
            }
        }
        self.echeances(now);
        self.surveiller_lecture(now);
        self.guets_tick(now);
    }

    // ------------------------------------------------------------------ memoire de lecture

    /// Garde le texte de la page affichee depuis `LECTURE_APRES_MS`, une fois par adresse et
    /// par onglet. Le texte est lu par Trame (pas envoye par la page).
    fn surveiller_lecture(&mut self, now: u64) {
        if !self.reglages.memoire_lecture || self.vue_trame || self.window.is_minimized() {
            self.lecture_vue = None;
            return;
        }
        let Some(o) = self.actif.and_then(|id| self.trouver(id)) else { return };
        let (id, url, chargement, domaine) = (o.id, o.url.clone(), o.chargement, o.domaine());
        match &self.lecture_vue {
            Some((t, u, _)) if *t == id && *u == url => {}
            _ => {
                self.lecture_vue = Some((id, url.clone(), now));
                return;
            }
        }
        let depuis = self.lecture_vue.as_ref().map(|l| l.2).unwrap_or(now);
        if chargement || now.saturating_sub(depuis) < LECTURE_APRES_MS {
            return;
        }
        if self.lectures_faites.get(&id) == Some(&url) {
            return;
        }
        self.lectures_faites.insert(id, url.clone());
        if !carnet::memorisable(&url, &domaine, &self.reglages.lecture_exclus) {
            return;
        }
        let Some(w) = self.webviews.get(&id) else { return };
        let proxy = self.proxy.clone();
        let _ = w.evaluate_script_with_callback(carnet::SCRIPT_LECTURE, move |valeur| {
            let _ = proxy.send_event(UserEvent::Lecture { onglet: id, url: url.clone(), valeur });
        });
    }

    pub(super) fn lecture_recue(&mut self, onglet: TabId, url: String, valeur: String) {
        let Ok(v) = serde_json::from_str::<Value>(&valeur) else { return };
        let Some(texte) = v["texte"].as_str() else { return };
        // L'onglet a pu changer de page entre la demande et la reponse.
        if self.trouver(onglet).map(|o| o.url != url).unwrap_or(true) {
            return;
        }
        let lecture = carnet::Lecture {
            url: url.clone(),
            titre: v["titre"].as_str().unwrap_or("").to_string(),
            ts: maintenant_ms(),
            texte: carnet::condenser(texte),
        };
        let chemin = self.profil.fichier_lectures();
        std::thread::spawn(move || {
            if let Err(e) = carnet::ajouter_lecture(&chemin, &lecture) {
                alerte!("memoire de lecture : ecriture impossible ({e})");
            }
        });
        info!("memoire de lecture : {} gardee", urls::affichage_court(&url));
    }

    /// Lance la recherche dans la memoire de lecture (fil a part). Les resultats arrivent par
    /// `UserEvent::LecturesTrouvees` et s'ajoutent a ceux de la barre.
    pub(super) fn chercher_dans_lectures(&mut self, rid: u64, requete: &str, seules: bool) {
        let generation = self.recherche_lecture.fetch_add(1, Atomique::SeqCst) + 1;
        if !self.reglages.memoire_lecture || requete.chars().count() < 3 || carnet::mots(requete).is_empty() {
            if seules {
                let items = json!([]);
                self.lectures_trouvees(rid, items);
            }
            return;
        }
        let chemin = self.profil.fichier_lectures();
        let compteur = self.recherche_lecture.clone();
        let proxy = self.proxy.clone();
        let q = requete.to_string();
        let limite = if seules { 12 } else { 4 };
        std::thread::spawn(move || {
            let abandon = || compteur.load(Atomique::SeqCst) != generation;
            let trouvees = carnet::chercher_lectures(&chemin, &q, limite, maintenant_ms(), &abandon);
            if abandon() {
                return;
            }
            let now = maintenant_ms();
            let items: Vec<Value> = trouvees
                .into_iter()
                .map(|t| {
                    let jours = now.saturating_sub(t.ts) / 86_400_000;
                    let quand = match jours {
                        0 => "lu aujourd'hui".to_string(),
                        1 => "lu hier".to_string(),
                        n => format!("lu il y a {n} j"),
                    };
                    json!({
                        "genre": "lecture",
                        "id": t.url,
                        "titre": if t.titre.is_empty() { urls::affichage_court(&t.url) } else { t.titre.clone() },
                        "sousTitre": format!("{} · {} · {quand}", t.extrait, urls::domaine_enregistrable(&t.url)),
                        "action": {"type": "lecture", "url": t.url},
                    })
                })
                .collect();
            let _ = proxy.send_event(UserEvent::LecturesTrouvees { rid, items: Value::Array(items) });
        });
    }

    pub(super) fn lectures_trouvees(&mut self, rid: u64, items: Value) {
        info!("memoire de lecture : {} page(s) retrouvee(s) (requete {rid})", items.as_array().map(|a| a.len()).unwrap_or(0));
        if let Some(b) = &self.barre {
            let js = format!("window.trame && window.trame.resultatsPlus && window.trame.resultatsPlus({})", json!({"rid": rid, "items": items}));
            let _ = b.evaluate_script(&js);
        }
    }

    // ------------------------------------------------------------------ guets

    fn demarrer_guet(&mut self) {
        let Some(id) = self.actif else { return };
        let web = self.trouver(id).map(|o| o.url.starts_with("http")).unwrap_or(false);
        if !web || !self.webviews.contains_key(&id) {
            self.signaler("Le guet s'applique à une page web ouverte.");
            return;
        }
        self.selection_guet = true;
        let selecteur = crate::teinte::injecter(SELECTEUR_JS, &self.palette());
        self.eval_sur(Source::Onglet(id), &selecteur);
        self.signaler("Cliquez l'élément à guetter (un prix, un stock, un résultat) — Échap annule.");
    }

    pub(super) fn lire_element_pour_guet(&mut self, onglet: TabId, selecteur: String) {
        if selecteur.is_empty() {
            return;
        }
        let Some(w) = self.webviews.get(&onglet) else { return };
        let proxy = self.proxy.clone();
        let sel = selecteur.clone();
        let _ = w.evaluate_script_with_callback(&carnet::script_guet(&selecteur), move |valeur| {
            let texte = serde_json::from_str::<Option<String>>(&valeur).ok().flatten();
            let _ = proxy.send_event(UserEvent::GuetPose { onglet, selecteur: sel.clone(), texte });
        });
    }

    pub(super) fn guet_pose(&mut self, onglet: TabId, selecteur: String, texte: Option<String>) {
        let Some(texte) = texte.filter(|t| !t.trim().is_empty()) else {
            self.signaler("Cet élément est vide : choisissez-en un qui contient du texte (un prix, un nombre…).");
            return;
        };
        let Some(o) = self.trouver(onglet).cloned() else { return };
        let now = maintenant_ms();
        let id = self.guets.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        let apercu: String = carnet::condenser(&texte).chars().take(60).collect();
        self.guets.push(Guet {
            id,
            url: o.url.clone(),
            selecteur,
            libelle: o.libelle(),
            texte: carnet::condenser(&texte),
            ancien: String::new(),
            cree_le: now,
            verifie_le: now,
            change_le: None,
            vu: true,
            echecs: 0,
            intervalle_min: 30,
        });
        self.sauver_guets();
        self.signaler(&format!("Guet posé sur « {apercu} » : Trame relit cette page toutes les 30 min et vous prévient si ça change."));
        self.pousser_etat();
    }

    fn ouvrir_guet(&mut self, id: u64) {
        let Some(g) = self.guets.iter_mut().find(|g| g.id == id) else { return };
        g.vu = true;
        let url = g.url.clone();
        self.sauver_guets();
        let existant = self.session.onglets.iter().find(|o| o.url == url).map(|o| o.id);
        match existant {
            Some(t) => {
                self.activer(t);
                // La page ouverte peut dater : on la recharge pour voir la valeur du guet.
                if let Some(w) = self.webviews.get(&t) {
                    let _ = w.reload();
                }
            }
            None => {
                let t = self.ouvrir_onglet(url, Declencheur::Saisie, None);
                self.activer(t);
            }
        }
        self.pousser_etat();
    }

    /// Une lecture de guet a la fois, dans une page cachee detruite aussitot apres.
    fn guets_tick(&mut self, now: u64) {
        if let Some(s) = &self.sentinelle {
            let (id, depuis, charge, en_lecture) = (s.guet, s.depuis, s.charge_le, s.en_lecture);
            if now.saturating_sub(depuis) > SENTINELLE_MAX_MS {
                info!("guet {id} : page trop lente, lecture abandonnee");
                self.guet_lu(id, None);
                return;
            }
            if let (Some(c), false) = (charge, en_lecture) {
                if now.saturating_sub(c) >= SENTINELLE_ATTENTE_MS {
                    let selecteur = self.guets.iter().find(|g| g.id == id).map(|g| g.selecteur.clone());
                    let Some(selecteur) = selecteur else {
                        self.sentinelle = None;
                        return;
                    };
                    let proxy = self.proxy.clone();
                    if let Some(s) = self.sentinelle.as_mut() {
                        s.en_lecture = true;
                        let _ = s.vue.evaluate_script_with_callback(&carnet::script_guet(&selecteur), move |valeur| {
                            let texte = serde_json::from_str::<Option<String>>(&valeur).ok().flatten();
                            let _ = proxy.send_event(UserEvent::GuetLu { id, texte });
                        });
                    }
                }
            }
            return;
        }
        if now.saturating_sub(self.dernier_guet) < 20_000 || !self.restaure {
            return;
        }
        self.dernier_guet = now;
        // Jamais quand le PC manque de memoire : le guet attendra.
        let dispo = self.derniere_mesure.dispo_mo;
        if dispo > 0.0 && dispo < RESERVE_MEMOIRE_MO {
            return;
        }
        let Some(g) = self.guets.iter().filter(|g| g.du(now)).min_by_key(|g| g.verifie_le).cloned() else {
            return;
        };
        self.lancer_sentinelle(&g, now);
    }

    fn lancer_sentinelle(&mut self, g: &Guet, now: u64) {
        let geo = self.geometrie_courante();
        let proxy = self.proxy.clone();
        let id = g.id;
        let garde = self.garde_navigation();
        let mut contexte = verrou_contexte();
        let resultat = self
            .habiller(WebViewBuilder::new_with_web_context(&mut contexte))
            .with_url("about:blank")
            .with_bounds(geo.contenu)
            .with_visible(false)
            .with_focused(false)
            .with_navigation_handler(garde)
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_on_page_load_handler(move |evenement, _| {
                if matches!(evenement, wry::PageLoadEvent::Finished) {
                    let _ = proxy.send_event(UserEvent::GuetCharge { id });
                }
            })
            .build_as_child(&self.window);
        drop(contexte);
        match resultat {
            Ok(vue) => {
                crate::moteur_ext::rendre_muette(&vue);
                let url = self.adresse_propre(g.url.clone());
                if let Err(e) = vue.load_url(&url) {
                    alerte!("guet {id} : chargement impossible ({e})");
                    return;
                }
                info!("guet {id} : relecture de {}", urls::affichage_court(&g.url));
                self.sentinelle = Some(Sentinelle { guet: id, vue, depuis: now, charge_le: None, en_lecture: false, essais: 0 });
                let geo = self.geometrie_courante();
                self.superposer(&geo);
            }
            Err(e) => alerte!("guet {id} : page de surveillance impossible ({e})"),
        }
    }

    pub(super) fn guet_charge(&mut self, id: u64) {
        if let Some(s) = self.sentinelle.as_mut() {
            // « about:blank » finit aussi de charger : on ne compte que la vraie page.
            let url = s.vue.url().unwrap_or_default();
            if s.guet == id && url.starts_with("http") && s.charge_le.is_none() {
                s.charge_le = Some(maintenant_ms());
            }
        }
    }

    pub(super) fn guet_lu(&mut self, id: u64, texte: Option<String>) {
        // Element introuvable : le site l'affiche peut-etre plus tard (on reessaie deux fois).
        if texte.is_none() {
            if let Some(s) = self.sentinelle.as_mut() {
                if s.guet == id && s.essais < 2 && maintenant_ms().saturating_sub(s.depuis) < SENTINELLE_MAX_MS - 5_000 {
                    s.essais += 1;
                    s.en_lecture = false;
                    s.charge_le = Some(maintenant_ms().saturating_sub(SENTINELLE_ATTENTE_MS - 2_000));
                    return;
                }
            }
        }
        if self.sentinelle.as_ref().map(|s| s.guet == id).unwrap_or(false) {
            self.sentinelle = None; // la page cachee est detruite ici
        }
        let now = maintenant_ms();
        let Some(g) = self.guets.iter_mut().find(|g| g.id == id) else { return };
        g.verifie_le = now;
        let mut annonce = None;
        match texte.map(|t| carnet::condenser(&t)).filter(|t| !t.is_empty()) {
            Some(t) => {
                g.echecs = 0;
                if carnet::texte_change(&g.texte, &t) {
                    g.ancien = std::mem::replace(&mut g.texte, t.clone());
                    g.change_le = Some(now);
                    g.vu = false;
                    let court = |s: &str| s.chars().take(40).collect::<String>();
                    annonce = Some(format!("Guet « {} » : {} → {}", court(&g.libelle), court(&g.ancien), court(&t)));
                }
            }
            None => {
                g.echecs += 1;
                info!("guet {id} : element introuvable ({} echec(s))", g.echecs);
            }
        }
        self.sauver_guets();
        if let Some(message) = annonce {
            info!("{message}");
            self.signaler(&message);
            self.window
                .request_user_attention(Some(tao::window::UserAttentionType::Informational));
        }
        self.pousser_etat();
    }
}
