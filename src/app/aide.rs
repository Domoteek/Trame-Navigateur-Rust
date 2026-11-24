//! L'aide dans Trame : la Notice (F1), son PDF, et les astuces au bon moment.
//!
//! Choix de Clement du 2026-11-24 sur `docs/maquettes/aide.html` : « les cinq ».
//!  1. la page Notice `trame://localhost/aide.html#<fiche>` — F1 l'ouvre sur la fiche qui va avec
//!     ce qu'on regarde (`chapitre_contexte`) ;
//!  2. l'aide de la Couture (`crate::aide::resultats_couture`, branchee dans la requete) ;
//!  3. les astuces : une a la fois, au plus une par jour, jamais deux fois la meme ;
//!  4. le calque des raccourcis (Ctrl+/, dessine par la barre : `chrome.js`) ;
//!  5. la notice en PDF (`notice.pdf` fabrique a la livraison par `outils/faire-notice.mjs`).
//!
//! Le texte vient TOUJOURS de `ui/notice.js` : rien n'est ecrit ici en double.

use super::*;

pub const PAGE_AIDE: &str = "trame://localhost/aide.html";

/// Deux astuces sont espacees d'au moins un jour.
const ASTUCE_ESPACEMENT_MS: u64 = 24 * 3600 * 1000;
/// Cote a cote : tant d'allers-retours entre deux memes onglets, en si peu de temps.
const ALTERNANCES_COTE: usize = 5;
const ALTERNANCES_FENETRE_MS: u64 = 3 * 60 * 1000;
/// Rouvrir : une page refermee il y a moins que ca, puis retapee.
const ROUVRIR_FENETRE_MS: u64 = 3 * 60 * 1000;

/// Ce que l'aide retient en cours de route (rien n'est ecrit sur le disque, sauf les astuces
/// deja vues, dans les reglages).
#[derive(Default)]
pub(super) struct EtatAide {
    /// Dernieres activations d'onglet (onglet, instant), pour reconnaitre les allers-retours.
    activations: Vec<(TabId, u64)>,
    /// Adresses refermees recemment (adresse, instant).
    fermes_le: Vec<(String, u64)>,
    /// Astuce a montrer des que l'onglet affiche est une page web prete.
    en_attente: Option<String>,
    /// Astuce affichee, dont on attend un bouton. Seule elle ouvre les trois messages
    /// `astuce…` aux pages web (voir l'autorisation dans `traiter_ipc`).
    pub(super) proposee: Option<String>,
}

/// Le cadre de l'astuce, injecte dans la page affichee (comme le popup de mise a jour) : Trame
/// ne cree aucune fenetre a part, et un panneau de la barre se manque.
const ASTUCE_JS: &str = r#"(function () {
  var T = __TRAME_TEINTE__;
  var id = 'trame-astuce';
  var ancien = document.getElementById(id);
  if (ancien) ancien.remove();
  var boite = document.createElement('div');
  boite.id = id;
  boite.setAttribute('style', 'position:fixed;z-index:2147483647;right:22px;bottom:22px;width:360px;padding:16px 18px;border-radius:14px;background:' + T.surface2 + ';color:#e9eefa;font:13.5px/1.5 "Segoe UI",system-ui,sans-serif;box-shadow:0 18px 50px rgba(0,0,0,.6);border:1px solid rgba(' + T.rgb + ',.45)');
  var titre = document.createElement('div');
  titre.textContent = 'Astuce';
  titre.setAttribute('style', 'font-size:11px;letter-spacing:.14em;text-transform:uppercase;color:' + T.clair + ';margin-bottom:5px;font-weight:600');
  var texte = document.createElement('div');
  texte.textContent = __TEXTE__;
  var rangee = document.createElement('div');
  rangee.setAttribute('style', 'display:flex;gap:8px;margin-top:12px;flex-wrap:wrap');
  function bouton(libelle, prim, message) {
    var b = document.createElement('button');
    b.textContent = libelle;
    b.setAttribute('style', 'padding:7px 12px;border-radius:9px;font:600 12.5px "Segoe UI",system-ui,sans-serif;cursor:pointer;' + (prim ? 'background:' + T.accent + ';color:#0b0f17;border:1px solid transparent' : 'background:transparent;color:#c9d3e6;border:1px solid rgba(255,255,255,.18)'));
    b.onclick = function () {
      boite.remove();
      try { window.ipc.postMessage(JSON.stringify({ m: message })); } catch (e) {}
    };
    rangee.appendChild(b);
  }
  bouton(__FAIRE__, true, 'astuceFaire');
  bouton('Compris', false, 'astuceVue');
  bouton("Plus d'astuces", false, 'astucesCouper');
  boite.appendChild(titre);
  boite.appendChild(texte);
  boite.appendChild(rangee);
  (document.body || document.documentElement).appendChild(boite);
  // Partie seule au bout de 40 s : une astuce ne doit pas rester dans le chemin.
  setTimeout(function () { if (boite.isConnected) { boite.remove(); try { window.ipc.postMessage(JSON.stringify({ m: 'astuceVue' })); } catch (e) {} } }, 40000);
})();"#;

fn chaine_js(texte: &str) -> String {
    serde_json::to_string(texte).unwrap_or_else(|_| "\"\"".to_string())
}

/// Sites ou l'on regarde des videos : F1 y ouvre la fiche « video », et quitter l'un d'eux
/// pendant la lecture fait sortir l'astuce « fenetre a part ».
const SITES_VIDEO: [&str; 7] = ["youtube.com", "youtu.be", "twitch.tv", "dailymotion.com", "vimeo.com", "netflix.com", "primevideo.com"];

fn site_video(domaine: &str) -> bool {
    SITES_VIDEO.iter().any(|d| domaine == *d || domaine.ends_with(&format!(".{d}")))
}

/// Meme adresse, a la barre finale pres.
fn meme_adresse(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

impl App {
    /// Ouvre la Notice sur une fiche. Un onglet de la Notice deja ouvert est repris (sa fiche
    /// change) : F1 deux fois n'ouvre pas deux notices.
    pub(super) fn ouvrir_aide(&mut self, chapitre: &str) {
        let chapitre = if crate::aide::chapitre(chapitre).is_some() { chapitre } else { "premiers-pas" };
        let url = format!("{PAGE_AIDE}#{chapitre}");
        let existant = self
            .session
            .onglets
            .iter()
            .find(|o| o.url.split(['#', '?']).next() == Some(PAGE_AIDE))
            .map(|o| o.id);
        info!("aide : fiche {chapitre}");
        match existant {
            Some(id) => {
                self.activer(id);
                if let Some(w) = self.webviews.get(&id) {
                    let _ = w.evaluate_script(&format!("location.hash = {}", chaine_js(chapitre)));
                }
                if let Some(o) = self.onglet_mut(id) {
                    o.url = url;
                }
            }
            None => {
                let parent = self.actif;
                self.ouvrir_onglet(url, Declencheur::NouvelOnglet, parent);
            }
        }
    }

    /// La fiche qui va avec ce que l'on regarde au moment de F1.
    pub(super) fn chapitre_contexte(&self) -> &'static str {
        if self.vue_trame {
            return "trame";
        }
        if self.session.cote.is_some() {
            return "cote-a-cote";
        }
        if self.flottant.is_some() || !self.incrustes.is_empty() {
            return "video";
        }
        let Some(o) = self.actif.and_then(|id| self.trouver(id)) else { return "premiers-pas" };
        if self.session.fils.iter().any(|f| f.id == o.fil && f.prive) {
            return "prive";
        }
        let url = o.url.as_str();
        if url.starts_with(PAGE_AIDE) {
            return "premiers-pas";
        }
        if url.contains("/telechargements.html") || url.contains("/reglages.html") {
            return "sauvegarde";
        }
        if url.contains("/zap.html") {
            return "zappeur";
        }
        if url.starts_with("chrome-extension:") || url.contains("chromewebstore.google.com") {
            return "extensions";
        }
        if site_video(&o.domaine()) {
            return "video";
        }
        "premiers-pas"
    }

    /// Le PDF de la notice : copie dans les telechargements (au nom de la version), puis ouvert.
    /// Sans PDF fabrique (poste de developpement) : la page d'impression, qui propose
    /// « Enregistrer en PDF ».
    pub(super) fn notice_pdf(&mut self) {
        let source = self.racine_ui().join("notice.pdf");
        if !source.is_file() {
            info!("aide : pas de notice.pdf dans {}, page d'impression a la place", source.display());
            self.signaler("Choisis « Enregistrer au format PDF » comme imprimante.");
            self.naviguer_actif(&format!("{PAGE_AIDE}?impression&imprimer"));
            return;
        }
        let dossier = self.dossier_telechargements();
        let cible = dossier.join(format!("Notice de Trame {}.pdf", crate::maj::ACTUELLE));
        let copie = std::fs::create_dir_all(&dossier).and_then(|_| std::fs::copy(&source, &cible));
        match copie {
            Ok(_) => {
                info!("aide : notice copiee dans {}", cible.display());
                if !crate::externe::ouvrir_uri(&cible.to_string_lossy()) {
                    self.signaler(&format!("Notice enregistrée : {}", cible.display()));
                }
            }
            Err(e) => {
                alerte!("aide : copie de la notice impossible ({e})");
                self.signaler("Impossible d'enregistrer la notice dans tes téléchargements.");
            }
        }
    }

    pub(super) fn afficher_calque_raccourcis(&mut self) {
        // Le focus d'abord (Echap doit arriver a la barre), le calque ensuite : donner le focus
        // apres l'ouverture produisait un « blur » qui refermait le calque aussitot.
        if let Some(w) = &self.barre {
            let _ = w.focus();
            let _ = w.evaluate_script("window.trame && window.trame.calqueRaccourcis && window.trame.calqueRaccourcis()");
        }
    }

    /// Messages de la Notice et des astuces. `true` = traite ici.
    pub(super) fn traiter_ipc_aide(&mut self, m: &str, v: &Value) -> bool {
        match m {
            "aideNoticePdf" => self.notice_pdf(),
            "aideOuvrir" => {
                let chapitre = v["chapitre"].as_str().unwrap_or("").to_string();
                self.ouvrir_aide(&chapitre);
            }
            "astuces" => {
                self.reglages.astuces = v["actif"].as_bool().unwrap_or(true);
                self.profil.sauver_reglages(&self.reglages);
                self.signaler(if self.reglages.astuces { "Astuces activées." } else { "Astuces coupées." });
                self.pousser_etat();
            }
            "astuceFaire" => {
                if let Some(id) = self.aide.proposee.take() {
                    if let Some(a) = crate::aide::astuce(&id) {
                        info!("astuce {id} : faite");
                        let faire = a.faire.clone();
                        if !faire.commande.is_empty() {
                            self.commande(&faire.commande);
                        } else {
                            self.ouvrir_aide(&faire.chapitre);
                        }
                    }
                }
            }
            "astuceVue" => {
                if let Some(id) = self.aide.proposee.take() {
                    info!("astuce {id} : vue");
                }
            }
            "astucesCouper" => {
                if self.aide.proposee.take().is_some() {
                    self.reglages.astuces = false;
                    self.profil.sauver_reglages(&self.reglages);
                    self.signaler("Plus d'astuces. Elles se rallument dans la Notice (F1).");
                    self.pousser_etat();
                }
            }
            _ => return false,
        }
        true
    }

    // ------------------------------------------------------------------ astuces

    fn astuce_possible(&self, id: &str, now: u64) -> bool {
        self.reglages.astuces
            && self.aide.proposee.is_none()
            && self.aide.en_attente.is_none()
            && !self.reglages.astuces_vues.iter().any(|v| v == id)
            && now.saturating_sub(self.reglages.astuce_derniere) >= ASTUCE_ESPACEMENT_MS
            && crate::aide::astuce(id).is_some()
    }

    /// Un onglet vient d'etre affiche : allers-retours entre deux onglets (→ cote a cote),
    /// ou une video quittee qui continue de jouer (→ fenetre a part).
    pub(super) fn aide_sur_activation(&mut self, ancien: Option<TabId>, nouveau: TabId) {
        let now = maintenant_ms();
        self.aide.activations.push((nouveau, now));
        let excedent = self.aide.activations.len().saturating_sub(12);
        self.aide.activations.drain(..excedent);

        if self.session.cote.is_none() && self.astuce_possible("cote", now) {
            let recentes: Vec<TabId> = self
                .aide
                .activations
                .iter()
                .rev()
                .take(ALTERNANCES_COTE)
                .filter(|(_, t)| now.saturating_sub(*t) <= ALTERNANCES_FENETRE_MS)
                .map(|(id, _)| *id)
                .collect();
            let alterne = recentes.len() == ALTERNANCES_COTE
                && recentes.windows(2).all(|p| p[0] != p[1])
                && recentes.windows(3).all(|p| p[0] == p[2]);
            if alterne {
                self.aide.en_attente = Some("cote".into());
                return;
            }
        }

        if let Some(ancien) = ancien.filter(|a| *a != nouveau) {
            let joue = !self.incrustes.contains(&ancien)
                && !self.dans_cote_affiche(ancien)
                && self.webviews.get(&ancien).is_some_and(crate::moteur_ext::joue_du_son);
            let video = self.trouver(ancien).is_some_and(|o| site_video(&o.domaine()));
            if joue && video && self.astuce_possible("video", now) {
                self.aide.en_attente = Some("video".into());
            }
        }
    }

    pub(super) fn aide_sur_fermeture(&mut self, url: &str) {
        if url.starts_with("trame:") {
            return;
        }
        let now = maintenant_ms();
        self.aide.fermes_le.retain(|(_, t)| now.saturating_sub(*t) <= ROUVRIR_FENETRE_MS);
        self.aide.fermes_le.push((url.to_string(), now));
    }

    /// Une adresse tapee : est-ce une page refermee a l'instant ? Alors Ctrl+Maj+T merite d'etre connu.
    pub(super) fn aide_sur_saisie(&mut self, url: &str) {
        let now = maintenant_ms();
        let retapee = self
            .aide
            .fermes_le
            .iter()
            .any(|(u, t)| now.saturating_sub(*t) <= ROUVRIR_FENETRE_MS && meme_adresse(u, url));
        if retapee && self.astuce_possible("rouvrir", now) {
            self.aide.en_attente = Some("rouvrir".into());
        }
    }

    /// Au battement : montre l'astuce en attente si l'onglet affiche est une page web prete.
    pub(super) fn aide_tick(&mut self) {
        let Some(id) = self.aide.en_attente.clone() else { return };
        let Some(actif) = self.actif else { return };
        let pret = self.trouver(actif).is_some_and(|o| !o.interne() && !o.chargement && o.url.starts_with("http"));
        if !pret || self.vue_trame {
            return;
        }
        let Some(a) = crate::aide::astuce(&id) else {
            self.aide.en_attente = None;
            return;
        };
        let Some(w) = self.webviews.get(&actif) else { return };
        let script = crate::teinte::injecter(ASTUCE_JS, &self.palette())
            .replace("__TEXTE__", &chaine_js(&a.texte))
            .replace("__FAIRE__", &chaine_js(&a.faire.libelle));
        if w.evaluate_script(&script).is_err() {
            return;
        }
        info!("astuce {id} : montree");
        self.aide.en_attente = None;
        self.aide.proposee = Some(id.clone());
        // Vue des qu'elle est montree : ignoree, elle ne revient pas non plus.
        self.reglages.astuces_vues.push(id);
        self.reglages.astuce_derniere = maintenant_ms();
        self.profil.sauver_reglages(&self.reglages);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_astuce_est_un_script_valide_une_fois_remplie() {
        let s = ASTUCE_JS
            .replace("__TEXTE__", &chaine_js("Un « texte » avec 'apostrophes'\net retour"))
            .replace("__FAIRE__", &chaine_js("Faire"));
        assert!(!s.contains("__TEXTE__") && !s.contains("__FAIRE__"));
        assert!(s.contains("__TRAME_TEINTE__"), "la teinte est posee par teinte::injecter");
    }

    #[test]
    fn meme_adresse_a_la_barre_pres() {
        assert!(meme_adresse("https://a.fr/x/", "https://a.fr/x"));
        assert!(!meme_adresse("https://a.fr/x", "https://a.fr/y"));
    }
}
