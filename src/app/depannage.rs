//! Le dépannage : vérification d'après mise à jour, « Réparer Trame », « Rapport pour Claude ».
//!
//! Demande de Clément du 2026-11-24 (« les trois ») : chez un de ses fils, le compteur du lecteur
//! YouTube ne se rafraîchissait plus jusqu'à une désinstallation complète ; chez l'autre, un PC
//! qui avait connu les toutes premières versions. Trois réponses :
//!  1. au premier démarrage d'une nouvelle version, les extensions livrées sont comparées par leur
//!     CONTENU à celles du profil (l'interface l'est déjà à chaque démarrage, `ressources.rs`) ;
//!  2. « Réparer Trame » remet à zéro ce que la désinstallation remettait à zéro — caches du
//!     moteur, service workers, extensions livrées, réglages techniques — sans toucher aux fils,
//!     aux favoris ni aux connexions ;
//!  3. « Rapport pour Claude » écrit un fichier lisible pour trouver la vraie cause, sans adresse
//!     ni titre de page (le PC d'un enfant).

use super::*;

/// Lignes de journal gardées dans le rapport.
const JOURNAL_LIGNES: usize = 250;

/// Ne garde d'une adresse que son site : `https://www.youtube.com/watch?v=…` → `https://www.youtube.com/…`.
/// Le rapport part du PC d'un enfant : on dit OÙ, jamais QUOI.
pub(super) fn masquer_adresses(ligne: &str) -> String {
    let mut sortie = String::with_capacity(ligne.len());
    let mut reste = ligne;
    loop {
        let debut = ["https://", "http://", "chrome-extension://"]
            .iter()
            .filter_map(|s| reste.find(s).map(|i| (i, s.len())))
            .min_by_key(|(i, _)| *i);
        let Some((i, schema)) = debut else {
            sortie.push_str(reste);
            return sortie;
        };
        sortie.push_str(&reste[..i + schema]);
        let apres = &reste[i + schema..];
        let fin_hote = apres.find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace() || c == '"' || c == ')').unwrap_or(apres.len());
        sortie.push_str(&apres[..fin_hote]);
        let fin = apres[fin_hote..].find(|c: char| c.is_whitespace() || c == '"' || c == ')').map(|j| fin_hote + j).unwrap_or(apres.len());
        if fin > fin_hote {
            sortie.push_str("/…");
        }
        reste = &apres[fin..];
    }
}

impl App {
    /// Au démarrage : si la version a changé depuis le dernier lancement, vérifie le contenu des
    /// extensions livrées et prépare un message pour dire ce qui a été remis à neuf.
    pub(super) fn verifier_apres_mise_a_jour(&mut self) {
        let version = crate::maj::ACTUELLE;
        if self.reglages.version_vue == version {
            return;
        }
        let avant = std::mem::replace(&mut self.reglages.version_vue, version.to_string());
        let reparees = self.profil.verifier_extensions_livrees(&self.racine_extensions_livrees(), false);
        self.profil.sauver_reglages(&self.reglages);
        info!(
            "verification d'apres mise a jour ({} -> {version}) : {}",
            if avant.is_empty() { "?" } else { &avant },
            if reparees.is_empty() { "tout est conforme".to_string() } else { format!("remis a neuf : {}", reparees.join(", ")) }
        );
        if !reparees.is_empty() {
            self.message_demarrage = Some(format!(
                "Trame {version} : {} extension(s) remise(s) à neuf ({}).",
                reparees.len(),
                reparees.join(", ")
            ));
        }
    }

    /// Au battement : dit, une fois l'interface prête, ce que la vérification a réparé.
    pub(super) fn depannage_tick(&mut self, now: u64) {
        if now.saturating_sub(self.demarrage_ms) < 4_000 {
            return;
        }
        if let Some(m) = self.message_demarrage.take() {
            self.signaler(&m);
        }
    }

    /// « Réparer Trame » : ce que remettait à zéro une désinstallation complète, sans rien perdre
    /// de ce qui compte (fils, favoris, accès, connexions, mots de passe des extensions).
    pub(super) fn reparer_trame(&mut self) {
        info!("reparation demandee");
        // 1. Caches du moteur, pour chaque profil en service (principal, fils cloisonnes).
        let mut faits = 0usize;
        if let Some(w) = &self.barre {
            crate::moteur_ext::vider_caches(w, self.proxy.clone(), "profil principal".into());
            faits += 1;
        }
        for (id, w) in &self.webviews {
            crate::moteur_ext::vider_caches(w, self.proxy.clone(), format!("l'onglet {id}"));
            faits += 1;
        }
        // 2. Extensions livrées remises à neuf (les anciennes gardées de côté), puis rechargées.
        let reparees = self.profil.verifier_extensions_livrees(&self.racine_extensions_livrees(), true);
        self.extensions = extensions::charger(&self.profil.dossier_extensions());
        self.appliquer_etat_extensions();
        // 3. Extensions Chrome : réadaptées et rechargées une fois au prochain démarrage.
        let chrome = extensions_chrome::oublier_rechargements(&self.dossiers_extensions_chrome());
        // 4. Réglages techniques remis par défaut (veille, nombre d'onglets éveillés, mémoire, pause).
        let defaut = Reglages::default();
        self.reglages.veille_auto = defaut.veille_auto;
        self.reglages.budget = defaut.budget;
        self.reglages.budget_auto = defaut.budget_auto;
        self.reglages.memoire_basse = defaut.memoire_basse;
        self.reglages.pause_onglets = defaut.pause_onglets;
        self.profil.sauver_reglages(&self.reglages);
        info!(
            "reparation : caches demandes pour {faits} vue(s), extensions livrees {} ; extensions Chrome {} ; reglages techniques par defaut",
            if reparees.is_empty() { "aucune".into() } else { reparees.join(", ") },
            if chrome { "a readapter" } else { "deja neuves" }
        );
        self.signaler("Trame réparé : caches vidés, extensions remises à neuf. Redémarre Trame pour finir.");
        self.pousser_etat();
    }

    /// « Rapport pour Claude » : un fichier texte dans les téléchargements, montré dans
    /// l'Explorateur. Rien de ce que l'on a lu : ni adresse complète, ni titre de page.
    pub(super) fn rapport_pour_claude(&mut self) {
        let (lisible, fichier) = crate::sauvegarde::date_locale();
        let version = crate::maj::ACTUELLE;
        let mut r = String::new();
        let mut ligne = |t: String| {
            r.push_str(&t);
            r.push_str("\r\n");
        };
        ligne(format!("Rapport de Trame pour Claude — {lisible}"));
        ligne(String::new());
        ligne(format!("Trame            : {version} ({})", std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default()));
        ligne(format!("Windows          : {}", version_windows()));
        ligne(format!("Moteur WebView2  : {}", wry::webview_version().unwrap_or_else(|_| "?".into())));
        let echelle = self.window.scale_factor();
        let taille = self.window.inner_size();
        ligne(format!(
            "Fenêtre          : {} x {} (échelle {:.2}){}",
            (taille.width as f64 / echelle).round(),
            (taille.height as f64 / echelle).round(),
            echelle,
            if self.window.is_maximized() { ", agrandie" } else { "" }
        ));
        ligne(format!("Profil           : {}", self.profil.racine().display()));
        ligne(format!(
            "Onglets          : {} ({} éveillés), {} fils, rail {}",
            self.session.onglets.len(),
            self.webviews.len(),
            self.session.fils.len(),
            if self.rail_replie { "replié" } else { "déplié" }
        ));
        ligne(String::new());
        ligne("Extensions de Trame (profil / livrée) :".into());
        let livrees = self.racine_extensions_livrees();
        if let Ok(lecture) = std::fs::read_dir(self.profil.dossier_extensions()) {
            let mut noms: Vec<PathBuf> = lecture.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            noms.sort();
            for d in noms {
                let nom = d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let livree = livrees.join(&nom);
                let etat = if !livree.is_dir() {
                    "pas livrée avec Trame".to_string()
                } else if crate::profile::dossiers_identiques(&livree, &d) {
                    "identique à la livrée".to_string()
                } else {
                    format!("DIFFÉRENTE de la livrée ({})", crate::profile::version_manifeste(&livree).unwrap_or_default())
                };
                ligne(format!("  - {nom} {} : {etat}", crate::profile::version_manifeste(&d).unwrap_or_else(|| "?".into())));
            }
        }
        // Les compteurs des extensions (nombres seulement) : pubs sautées, voiles gris retirés…
        // C'est par eux qu'on sait si le saute-pub a croisé la fenêtre « bloqueurs » de YouTube.
        ligne("Compteurs des extensions :".into());
        if let Some(racine) = self.ext_storage.as_object() {
            for (ext, valeurs) in racine {
                let nombres: Vec<String> = valeurs
                    .as_object()
                    .map(|o| o.iter().filter(|(_, v)| v.is_number()).map(|(k, v)| format!("{k} = {v}")).collect())
                    .unwrap_or_default();
                if !nombres.is_empty() {
                    ligne(format!("  - {ext} : {}", nombres.join(", ")));
                }
            }
        }
        ligne("Extensions Chrome :".into());
        for e in self.liste_extensions_chrome() {
            ligne(format!(
                "  - {} {} [{}] {}",
                e["nom"].as_str().unwrap_or("?"),
                e["version"].as_str().unwrap_or("?"),
                e["id"].as_str().unwrap_or("?"),
                if e["actif"].as_bool().unwrap_or(true) { "active" } else { "désactivée" }
            ));
        }
        ligne(String::new());
        ligne("Réglages :".into());
        if let Ok(mut v) = serde_json::to_value(&self.reglages) {
            if let Some(o) = v.as_object_mut() {
                // Ce qui ne sert pas au dépannage et dit quelque chose de la personne.
                for cle in ["lecture_exclus", "dossier_telechargements", "dossier_sauvegardes", "extensions_cout"] {
                    o.remove(cle);
                }
            }
            for l in serde_json::to_string_pretty(&v).unwrap_or_default().lines() {
                ligne(format!("  {l}"));
            }
        }
        ligne(String::new());
        ligne(format!("Journal (les {JOURNAL_LIGNES} dernières lignes, adresses réduites à leur site) :"));
        let journal = std::fs::read(self.profil.fichier_journal()).unwrap_or_default();
        let debut = journal.len().saturating_sub(400_000);
        let texte = String::from_utf8_lossy(&journal[debut..]);
        let lignes: Vec<&str> = texte.lines().collect();
        for l in &lignes[lignes.len().saturating_sub(JOURNAL_LIGNES)..] {
            ligne(format!("  {}", masquer_adresses(l)));
        }

        let dossier = self.dossier_telechargements();
        let chemin = dossier.join(format!("Rapport Trame {version} {fichier}.txt"));
        match std::fs::create_dir_all(&dossier).and_then(|_| std::fs::write(&chemin, r.as_bytes())) {
            Ok(()) => {
                info!("rapport ecrit : {}", chemin.display());
                // Trame cache (bancs d'essai) : pas d'Explorateur, il volerait le focus.
                if std::env::var("TRAME_CACHEE").is_err() {
                    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", chemin.display())).spawn();
                }
                self.signaler("Rapport écrit dans tes téléchargements : envoie-le à Claude.");
            }
            Err(e) => {
                alerte!("rapport impossible : {e}");
                self.signaler("Impossible d'écrire le rapport dans tes téléchargements.");
            }
        }
    }

    /// Messages de la page des réglages. `true` = traité ici.
    pub(super) fn traiter_ipc_depannage(&mut self, m: &str) -> bool {
        match m {
            "trameReparer" => self.reparer_trame(),
            "trameRapport" => self.rapport_pour_claude(),
            _ => return false,
        }
        true
    }
}

/// « Windows 11 Pro 26300 » : nom et numéro de version, lus au registre.
fn version_windows() -> String {
    let lire = |valeur: &str| -> Option<String> {
        let sortie = std::process::Command::new("reg")
            .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/v", valeur])
            .creation_flags_sans_fenetre()
            .output()
            .ok()?;
        let texte = String::from_utf8_lossy(&sortie.stdout).to_string();
        texte.lines().find(|l| l.contains(valeur)).and_then(|l| l.split("REG_SZ").nth(1)).map(|v| v.trim().to_string())
    };
    let nom = lire("ProductName").unwrap_or_else(|| "Windows".into());
    let version = lire("CurrentBuild").unwrap_or_default();
    // Le registre dit encore « Windows 10 » sur Windows 11 : le numéro (≥ 22000) tranche.
    let nom = if version.parse::<u32>().unwrap_or(0) >= 22000 { nom.replace("Windows 10", "Windows 11") } else { nom };
    format!("{nom} {version}")
}

trait SansFenetre {
    fn creation_flags_sans_fenetre(&mut self) -> &mut Self;
}

impl SansFenetre for std::process::Command {
    fn creation_flags_sans_fenetre(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW : pas de console qui clignote.
        self.creation_flags(0x0800_0000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_rapport_ne_garde_que_le_site_des_adresses() {
        assert_eq!(
            masquer_adresses("page https://www.youtube.com/watch?v=abc chargee"),
            "page https://www.youtube.com/… chargee"
        );
        assert_eq!(masquer_adresses("site http://exemple.fr fin"), "site http://exemple.fr fin");
        assert_eq!(
            masquer_adresses("(\"https://a.fr/x\") et chrome-extension://abcd/popup.html"),
            "(\"https://a.fr/…\") et chrome-extension://abcd/…"
        );
        assert_eq!(masquer_adresses("rien a cacher"), "rien a cacher");
    }
}
