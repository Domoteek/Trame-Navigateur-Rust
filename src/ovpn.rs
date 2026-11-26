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
            profile: profil.to_string(),
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

// Les variantes du crate qui nous serviront à la connexion réelle (étape suivante) sont importées
// ici pour que les choix par défaut soient documentés à un seul endroit.
#[allow(dead_code)]
fn config_de_connexion(profil: &str, identifiants: Option<(String, String)>) -> (EngineConfig, Option<Credentials>) {
    let config = EngineConfig {
        profile: profil.to_string(),
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
///
/// Branchée au mandataire à l'étape suivante (celui-ci tire aujourd'hui ses paquets de
/// [`crate::vpn::Session`], qui interroge son propre socket UDP).
#[allow(dead_code)]
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
/// boîte. Le MTU vient de ce que le serveur a poussé (`push`), sinon 1500.
#[allow(dead_code)]
pub struct SessionOuvrable {
    pub moteur: Arc<Engine>,
    pub boite: Boite,
    pub mtu: Option<usize>,
    pub resume: Resume,
}

/// Attend qu'un tunnel soit monté, au plus `delai`.
#[allow(dead_code)]
pub const DELAI_MONTAGE: Duration = Duration::from_secs(30);

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
}
