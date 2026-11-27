//! Moteur OpenVPN, en espace utilisateur.
//!
//! Demande de Clément (2026-11-28) : « il faut que ça marche avec OpenVPN ». WireGuard et OpenVPN
//! n'ont rien en commun : WireGuard échange une clé en un paquet, OpenVPN monte un canal de contrôle
//! **TLS** puis un canal de données chiffré, authentifié par certificat ou mot de passe. Le moteur
//! `boringtun` de [`crate::vpn`] ne peut donc pas lire un `.ovpn` : il faut un second moteur.
//!
//! Celui-ci est **OpenVPN 3 Core**, la bibliothèque d'OpenVPN Connect, liée en C++ (crate
//! `openvpn-rust-bindings`). Le point qui compte : elle tourne **dans notre processus** avec un
//! tunnel en **espace utilisateur** — elle ne crée aucun adaptateur réseau, donc **aucun pilote et
//! aucun droit administrateur**, ce qui est la contrainte de Trame depuis le début. Les paquets IP
//! qu'un adaptateur aurait portés sont remis à notre code, et notre pile `smoltcp`
//! ([`crate::mandataire`]) en fait des connexions SOCKS5, exactement comme avec WireGuard.
//!
//! Compilation : OpenSSL et LZ4 sont compilés depuis les sources au premier `cargo build` (10 min
//! environ, une fois). Sur Windows il faut **Strawberry Perl** (le Perl livré avec Git ne convient
//! pas) : `OPENSSL_SRC_PERL` pointe alors vers son `perl.exe`. Voir `docs/OPENVPN.md`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use openvpn_rust_bindings::{
    Compression, CoreEvent, Credentials, Engine, EngineConfig, EngineEvents, SignRequest, TunSettings, TlsVersionMin,
};

/// Ce que le fichier `.ovpn` a de lisible avant toute connexion : de quoi savoir s'il faut des
/// identifiants, et vers quel serveur on va.
#[derive(Debug, Clone, PartialEq)]
pub struct Resume {
    /// Le profil porte-t-il de quoi se connecter sans rien demander (`true`) ou faut-il un
    /// utilisateur et un mot de passe (`auth-user-pass`) ?
    pub autologin: bool,
    /// La clé privée du client est-elle hors du profil (elle doit alors être fournie à part) ?
    pub external_pki: bool,
    pub serveur: String,
    pub port: String,
    pub protocole: String,
}

impl Resume {
    /// Une ligne lisible pour l'interface : `udp 193.32.126.68:1194`.
    pub fn resume(&self) -> String {
        format!("{} {}:{}", self.protocole, self.serveur, self.port)
    }
}

/// Lit un profil `.ovpn` avec le moteur, **sans se connecter**. C'est ce qui décide si le fichier
/// est un profil OpenVPN exploitable et ce qu'il faudra demander à l'utilisateur.
pub fn valider(profil: &str) -> Result<Resume, String> {
    let moteur = Engine::new(Arc::new(Silencieux)).map_err(|e| format!("moteur OpenVPN indisponible : {e}"))?;
    let evalue = moteur
        .eval(&EngineConfig {
            profile: profil_adapte(profil),
            gui_version: "trame".to_string(),
            conn_timeout_secs: 15,
            ..Default::default()
        })
        .map_err(|e| e.message)?;
    Ok(Resume {
        autologin: evalue.autologin,
        external_pki: evalue.external_pki,
        serveur: evalue.remote_host,
        port: evalue.remote_port,
        protocole: evalue.remote_proto,
    })
}

/// Le profil ressemble-t-il à un fichier OpenVPN ? (Avant d'appeler le moteur, pour choisir le bon
/// lecteur : WireGuard refuse une ligne qui n'est pas une affectation, OpenVPN commence par des
/// directives nues comme `client`.)
pub fn ressemble_a_un_profil(texte: &str) -> bool {
    texte.lines().any(|l| {
        let l = l.trim();
        l.eq_ignore_ascii_case("client") || l.eq_ignore_ascii_case("dev tun") || l.starts_with("remote ")
    }) && !texte.contains("[Interface]")
}

/// Les événements du moteur. Ici ils ne servent qu'à la validation : rien n'est journalisé, rien
/// n'est renvoyé.
struct Silencieux;

impl EngineEvents for Silencieux {
    fn event(&self, _event: CoreEvent) {}
    fn log(&self, _ligne: &str) {}
    fn sign(&self, _demande: SignRequest) -> Result<String, String> {
        Err("clé du client hors du profil".to_string())
    }
    fn tun_up(&self, _reglages: Result<TunSettings, String>) {}
    fn packet(&self, _paquet: &[u8]) -> bool {
        false
    }
    fn tun_down(&self) {}
}

/// Le profil tel qu'il faut le donner à Core.
///
/// **Pourquoi cette ligne.** OpenVPN 3 exige un certificat client **même quand le profil n'en a
/// pas** : il réclame un « External PKI alias » et refuse de démarrer (`Missing External PKI
/// alias`, mesuré le 2026-11-28). L'option de profil `setenv CLIENT_CERT 0` lui dit qu'il n'y a
/// aucun certificat à présenter — c'est exactement le cas des profils **NordVPN**, qui
/// authentifient par **utilisateur / mot de passe** (`auth-user-pass`) et ne portent que `<ca>` et
/// `<tls-crypt>`. Sans elle, un profil Nord valide était refusé avant même d'essayer.
fn profil_adapte(profil: &str) -> String {
    let mut tete = String::new();
    if !profil.contains("<cert>") && !profil.contains("CLIENT_CERT") {
        tete.push_str("setenv CLIENT_CERT 0\n");
    }
    // Core refuse de démarrer sur une option OpenVPN 2.x qu'il ne connaît pas (`UNKNOWN/UNSUPPORTED
    // OPTIONS`) : les profils de fournisseurs en portent plusieurs. `ignore-unknown-option` les
    // ramène au rang d'avertissement, ce qui est sans risque — ces options ne servent qu'au client
    // 2.x (relance sur battement de cœur).
    if !profil.contains("ignore-unknown-option") {
        tete.push_str("ignore-unknown-option ping-timer-rem ping-exit\n");
    }
    if tete.is_empty() {
        return profil.to_string();
    }
    let mut sortie = String::with_capacity(profil.len() + tete.len());
    sortie.push_str(&tete);
    sortie.push_str(profil);
    sortie
}

/// Les réglages passés au moteur. `identifiants` est fourni quand le profil les demande
/// (`auth-user-pass` sans section `<auth-user-pass>` dans le fichier).
fn config_de_connexion(profil: &str, identifiants: Option<(String, String)>) -> (EngineConfig, Option<Credentials>) {
    let config = EngineConfig {
        profile: profil_adapte(profil),
        gui_version: "trame".to_string(),
        conn_timeout_secs: 30,
        // VORACLE : la compression sur un tunnel chiffré fuit ce qu'il contient. Coupée, sauf si
        // un serveur l'exige — c'est alors un choix explicite.
        compression: Compression::Off,
        // Les fournisseurs (Nord, Proton…) servent du AES-256-CBC : sans cette option le canal de
        // données serait limité à AES-GCM et ChaCha20-Poly1305.
        allow_non_preferred_ciphers: true,
        tls_version_min: TlsVersionMin::Tls12,
        ..Default::default()
    };
    let identifiants = identifiants.map(|(utilisateur, mot_de_passe)| Credentials {
        username: utilisateur,
        password: mot_de_passe.into(),
        ..Default::default()
    });
    (config, identifiants)
}

/// La file des paquets reçus du tunnel, remplie par le moteur (fil de Core) et vidée par la pile
/// TCP/IP. C'est la seule chose partagée entre les deux.
#[derive(Clone)]
pub struct Boite(Arc<Mutex<VecDeque<Vec<u8>>>>);

impl Boite {
    pub fn nouvelle() -> Boite {
        Boite(Arc::new(Mutex::new(VecDeque::new())))
    }

    fn poser(&self, paquet: Vec<u8>) {
        if let Ok(mut file) = self.0.lock() {
            file.push_back(paquet);
        }
    }

    pub fn prendre(&self) -> Option<Vec<u8>> {
        self.0.lock().ok()?.pop_front()
    }
}

impl Default for Boite {
    fn default() -> Self {
        Boite::nouvelle()
    }
}

/// Une session OpenVPN ouverte : le moteur tourne sur son propre fil, les paquets passent par la
/// boîte. Le MTU et l'adresse viennent de ce que le serveur a poussé (`push`), pas d'un fichier.
pub struct Session {
    moteur: Arc<Engine>,
    boite: Boite,
    reglages: TunSettings,
    resume: Resume,
    /// Les derniers messages du moteur, gardés courts : c'est ce qu'on lit quand ça ne monte pas,
    /// ou quand la session tombe en cours de route.
    journal: Arc<Mutex<VecDeque<String>>>,
    /// Levé par le moteur sur DISCONNECTED : la session est finie, le mandataire doit le savoir.
    arret: Arc<AtomicBool>,
    /// La raison donnée par `connect` quand il rend la main en erreur.
    fin: Arc<Mutex<Option<String>>>,
}

/// Les événements du moteur pendant une vraie connexion : il pousse les paquets dans la boîte,
/// signale le montage du tunnel, et garde les dernières lignes pour le diagnostic.
struct Pont {
    boite: Boite,
    monte: Arc<Mutex<Option<Result<TunSettings, String>>>>,
    journal: Arc<Mutex<VecDeque<String>>>,
    arret: Arc<AtomicBool>,
}

impl Pont {
    fn noter(&self, ligne: String) {
        crate::info!("openvpn : {ligne}");
        if let Ok(mut j) = self.journal.lock() {
            if j.len() >= 40 {
                j.pop_front();
            }
            j.push_back(ligne);
        }
    }
}

impl EngineEvents for Pont {
    fn event(&self, event: CoreEvent) {
        self.noter(format!("{} — {}", event.name, event.info));
        // Le serveur a fermé, ou la session s'arrête d'elle-même : on le saura en attendant.
        if event.name.to_ascii_uppercase().contains("DISCONNECT") {
            self.arret.store(true, Ordering::Relaxed);
        }
    }
    fn log(&self, ligne: &str) {
        self.noter(ligne.to_string());
    }
    fn sign(&self, _demande: SignRequest) -> Result<String, String> {
        Err("clé du client hors du profil : non géré par Trame".to_string())
    }
    fn tun_up(&self, reglages: Result<TunSettings, String>) {
        // Le tunnel n'est monté que si le serveur a **poussé une adresse**. Core appelle aussi
        // `tun_up` pour annoncer des réglages vides au démarrage : les prendre pour un montage
        // ferait croire à un tunnel qui n'existe pas (mesuré le 2026-11-28 sur le banc).
        let utile = match &reglages {
            Ok(t) => t.addr4.is_some() || t.addr6.is_some(),
            Err(_) => true,
        };
        if utile {
            if let Ok(mut emplacement) = self.monte.lock() {
                *emplacement = Some(reglages);
            }
        }
    }
    fn packet(&self, paquet: &[u8]) -> bool {
        self.boite.poser(paquet.to_vec());
        true
    }
    fn tun_down(&self) {}
}

/// Attend qu'un tunnel soit monté, au plus cette durée.
pub const DELAI_MONTAGE: Duration = Duration::from_secs(30);

impl Session {
    /// Ouvre une session OpenVPN : profil, identifiants éventuels, et **attente du montage**.
    ///
    /// Bloque jusqu'à ce que le serveur ait poussé l'adresse, les routes et le DNS (`tun_up`), ou
    /// jusqu'au délai. C'est cette attente que le fil du mandataire absorbe — pas la fenêtre.
    pub fn connecter(
        profil: &str,
        identifiants: Option<(String, String)>,
        delai: Duration,
    ) -> Result<Session, String> {
        let resume = valider(profil)?;
        let boite = Boite::nouvelle();
        let monte = Arc::new(Mutex::new(None));
        let journal = Arc::new(Mutex::new(VecDeque::new()));
        let arret = Arc::new(AtomicBool::new(false));
        let fin = Arc::new(Mutex::new(None::<String>));
        let pont = Arc::new(Pont {
            boite: boite.clone(),
            monte: monte.clone(),
            journal: journal.clone(),
            arret: arret.clone(),
        });
        let moteur = Arc::new(Engine::new(pont).map_err(|e| format!("moteur OpenVPN indisponible : {e}"))?);
        let (config, identifiants) = config_de_connexion(profil, identifiants);
        moteur.eval(&config).map_err(|e| e.message)?;
        if let Some(identifiants) = identifiants {
            moteur
                .provide_credentials(&identifiants)
                .map_err(|e| format!("identifiants refusés : {}", e.message))?;
        } else if !resume.autologin {
            return Err("ce profil demande un utilisateur et un mot de passe".to_string());
        }

        // `connect` bloque jusqu'à la fin de la session : il vit sur son fil.
        let pour_fil = moteur.clone();
        let fin_du_fil = fin.clone();
        std::thread::Builder::new()
            .name("trame-openvpn".into())
            .spawn(move || {
                if let Err(e) = pour_fil.connect() {
                    crate::alerte!("openvpn : session terminée ({})", e.message);
                    if let Ok(mut raison) = fin_du_fil.lock() {
                        *raison = Some(e.message);
                    }
                }
            })
            .map_err(|e| format!("fil OpenVPN impossible : {e}"))?;

        // L'attente du montage : `tun_up` arrive du fil de Core.
        let debut = std::time::Instant::now();
        loop {
            if let Ok(mut emplacement) = monte.lock() {
                if let Some(resultat) = emplacement.take() {
                    let reglages = resultat?;
                    crate::info!(
                        "openvpn : tunnel monté — {} {:?}, MTU {:?}, DNS {:?}",
                        resume.resume(),
                        reglages.addr4,
                        reglages.mtu,
                        reglages.dns
                    );
                    return Ok(Session {
                        moteur,
                        boite,
                        reglages,
                        resume,
                        journal,
                        arret,
                        fin,
                    });
                }
            }
            if arret.load(Ordering::Relaxed) {
                return Err(self::echec(&journal, "la session s'est arrêtée avant le montage"));
            }
            // Le fil de connexion a rendu la main en erreur : sa raison vaut mieux qu'un délai.
            if let Some(raison) = fin.lock().ok().and_then(|mut r| r.take()) {
                return Err(self::echec(&journal, &format!("session terminée ({raison})")));
            }
            if debut.elapsed() >= delai {
                return Err(self::echec(
                    &journal,
                    &format!("le tunnel n'a pas monté en {} s", delai.as_secs()),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Ce qu'il faut à la pile : l'adresse poussée par le serveur, son DNS, son MTU.
    pub fn parametres(&self) -> crate::moteur::Parametres {
        let mut adresses = Vec::new();
        if let Some((ip, prefixe)) = self.reglages.addr4 {
            adresses.push((std::net::IpAddr::V4(ip), prefixe));
        }
        if let Some((ip, prefixe)) = self.reglages.addr6 {
            adresses.push((std::net::IpAddr::V6(ip), prefixe));
        }
        crate::moteur::Parametres {
            adresses,
            dns: self.reglages.dns.first().map(|a| a.ip()),
            mtu: self.reglages.mtu.map(|m| m as usize),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.moteur.stop();
    }
}

impl crate::moteur::Tunnel for Session {
    fn etiquette(&self) -> String {
        self.resume.resume()
    }

    fn mtu(&self) -> Option<usize> {
        self.reglages.mtu.map(|m| m as usize)
    }

    /// Les paquets arrivent par le rappel `packet` (ils sont déjà dans la boîte) : il reste à
    /// dire si la session vit. Une session que le serveur a fermée (DISCONNECTED) rend une
    /// erreur tout de suite, avec sa raison : le mandataire la remonte sans attendre qu'une page
    /// essaie d'envoyer un paquet (avant la 0.4.96, une session morte au repos passait inaperçue).
    fn pomper(&mut self) -> Result<(), String> {
        if !self.arret.load(Ordering::Relaxed) {
            return Ok(());
        }
        let raison = self.fin.lock().ok().and_then(|f| f.clone());
        let quoi = match raison {
            Some(r) => format!("session OpenVPN terminée ({r})"),
            None => "session OpenVPN terminée".to_string(),
        };
        Err(echec(&self.journal, &quoi))
    }

    fn prendre_paquet(&mut self) -> Option<Vec<u8>> {
        self.boite.prendre()
    }

    fn envoyer_paquet(&mut self, paquet: &[u8]) -> Result<(), String> {
        if self.moteur.send_packet(paquet) {
            Ok(())
        } else {
            Err(echec(&self.journal, "le moteur OpenVPN n'accepte plus de paquet"))
        }
    }
}

/// Sonde de mise au point : ouvre une session OpenVPN avec un profil et des identifiants passés
/// par l'environnement, et rend le détail de ce que le serveur a poussé.
///
/// Jamais appelée en usage normal : `main` ne la déclenche que sur `TRAME_TUNNEL_ESSAI=3`. Elle
/// existe pour éprouver un vrai profil (`TRAME_OVPN_PROFIL`) sans toucher au profil de tous les
/// jours ni au rangement des configurations.
pub fn essai() -> Result<String, String> {
    let chemin = std::env::var("TRAME_OVPN_PROFIL")
        .map_err(|_| "TRAME_OVPN_PROFIL n'est pas posé (chemin du fichier .ovpn)".to_string())?;
    let profil = std::fs::read_to_string(&chemin).map_err(|e| format!("lecture de {chemin} : {e}"))?;
    let identifiants = match (std::env::var("TRAME_OVPN_USER"), std::env::var("TRAME_OVPN_PASS")) {
        (Ok(u), Ok(p)) if !u.is_empty() && !p.is_empty() => Some((u, p)),
        _ => None,
    };
    let session = Session::connecter(&profil, identifiants, DELAI_MONTAGE)?;
    let p = session.parametres();
    let etiquette = crate::moteur::Tunnel::etiquette(&session);
    Ok(format!(
        "openvpn : {etiquette} monté — adresse {} (préfixe {:?}), MTU {:?}, DNS {:?}, {} route(s)",
        p.adresses
            .iter()
            .map(|(ip, _)| ip.to_string())
            .collect::<Vec<_>>()
            .join(" / "),
        p.adresses.iter().map(|(_, x)| *x).collect::<Vec<_>>(),
        p.mtu,
        p.dns.map(|d| d.to_string()).unwrap_or_else(|| "aucun".into()),
        session.reglages.routes4.len() + session.reglages.routes6.len()
    ))
}

/// Une phrase d'échec qui emporte les dernières lignes du moteur : sans elles, « ça ne monte pas »
/// ne dit rien.
fn echec(journal: &Arc<Mutex<VecDeque<String>>>, quoi: &str) -> String {
    let dernieres = journal
        .lock()
        .map(|j| j.iter().rev().take(4).cloned().collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>())
        .unwrap_or_default();
    if dernieres.is_empty() {
        format!("{quoi} (le moteur n'a rien dit)")
    } else {
        format!("{quoi} — {}", dernieres.join(" | "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFIL: &str = "client
dev tun
proto udp
remote 193.32.126.68 1194
cipher AES-256-CBC
auth SHA512
auth-user-pass
<ca>
CERTIFICAT
</ca>
";

    /// Un `.ovpn` se reconnaît à ses directives nues, pas à une section WireGuard.
    #[test]
    fn un_profil_openvpn_se_reconnait() {
        assert!(ressemble_a_un_profil(PROFIL));
        assert!(!ressemble_a_un_profil("[Interface]\nPrivateKey = x\n\n[Peer]\nPublicKey = y\n"));
        assert!(!ressemble_a_un_profil("remote oui\n[Interface]\n"));
    }

    /// Le moteur lit le profil et dit ce qu'il faudra demander : ici des identifiants, et un
    /// serveur précis.
    #[test]
    fn le_moteur_lit_un_profil_et_annonce_le_serveur() {
        let resume = valider(PROFIL).expect("profil lisible par le moteur OpenVPN");
        assert!(!resume.autologin, "auth-user-pass : identifiants nécessaires");
        assert_eq!(resume.serveur, "193.32.126.68");
        assert_eq!(resume.port, "1194");
        assert_eq!(resume.protocole, "udp");
        assert_eq!(resume.resume(), "udp 193.32.126.68:1194");
    }

    /// Un fichier qui n'est pas un profil est refusé, avec le message du moteur.
    #[test]
    fn un_fichier_qui_n_est_pas_un_profil_est_refuse() {
        assert!(valider("bonjour\n").is_err());
    }

    /// Un profil **sans certificat client** (cas des profils NordVPN, qui authentifient par
    /// utilisateur / mot de passe) est **adapté, pas refusé** : `setenv CLIENT_CERT 0` dit à Core
    /// qu'il n'y a aucun certificat à présenter. Sans cette ligne, il répondrait
    /// `Missing External PKI alias` et rien ne monterait.
    #[test]
    fn un_profil_sans_certificat_client_est_adapte() {
        let adapte = profil_adapte(PROFIL);
        assert!(
            adapte.starts_with("setenv CLIENT_CERT 0"),
            "la ligne doit être posée avant le profil : {adapte:?}"
        );
        assert!(adapte.contains("auth-user-pass"), "le reste du profil est intact");
        // Un profil qui porte son certificat ne reçoit pas la ligne CLIENT_CERT — mais garde la
        // tolérance aux options OpenVPN 2.x, qui ne coûte rien.
        let avec_cert = "client\n<cert>\nX\n</cert>\n";
        let adapte_cert = profil_adapte(avec_cert);
        assert!(!adapte_cert.contains("CLIENT_CERT"), "pas de CLIENT_CERT quand le certificat est là");
        assert!(adapte_cert.contains("<cert>"));
        assert!(adapte_cert.starts_with("ignore-unknown-option"));
        // Un profil qui dit déjà ce qu'il veut n'est pas doublé.
        let deja = "setenv CLIENT_CERT 0\nignore-unknown-option ping-timer-rem\nclient\n";
        assert_eq!(profil_adapte(deja), deja);
        // Et le moteur, une fois adapté, ne réclame plus de PKI externe.
        let resume = valider(PROFIL).expect("profil lisible");
        assert!(!resume.external_pki, "après adaptation, plus de PKI externe");
        assert!(!resume.autologin, "auth-user-pass : identifiants nécessaires");
    }
}
