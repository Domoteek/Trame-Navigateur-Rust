//! Le tunnel WireGuard de Trame — **le moteur** (étape 2).
//!
//! Trame s'installe par utilisateur : pas de pilote, pas de droits administrateur. Le tunnel vit
//! donc **dans le processus** (`boringtun`, l'implémentation WireGuard en Rust) et parle UDP
//! lui-même. Les pages, elles, passeront par un **proxy local** (SOCKS5) que Trame traverse :
//! c'est ce qui remplace la carte réseau virtuelle — et c'est ce qui fait qu'un tunnel tombé
//! n'ouvre aucune fuite, puisque plus rien ne répond.
//!
//! Ici : la session (clés, socket UDP, minuteries, poignée de main). La pile TCP/IP et le proxy
//! viennent se brancher sur `pomper`.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use boringtun::noise::errors::WireGuardError;
use boringtun::noise::{Tunn, TunnResult};

/// Taille maximale d'un paquet IP qui sort du tunnel (`AllowedIPs` peut inclure l'IPv6).
const MAX_PAQUET: usize = 65_535;

pub struct Session {
    tunnel: Tunn,
    udp: UdpSocket,
    /// Adresse du pair (l'`Endpoint` de la configuration).
    pair: SocketAddr,
    /// Tampon d'envoi vers le réseau.
    sortie: Vec<u8>,
    /// Tampon de réception du réseau.
    entree: Vec<u8>,
    /// Tampon de travail pour `decapsulate`.
    travail: Vec<u8>,
    /// Combien de datagrammes sont partis vers le pair, et combien en sont revenus. Un tunnel qui
    /// envoie et ne reçoit **rien** est le symptôme d'une clé inconnue du pair (WireGuard se tait)
    /// ou d'un réseau qui bloque l'UDP.
    pub envoyes: u64,
    pub recus: u64,
    /// Datagrammes que le moteur a refusés (paquet en retard, rejoué, d'une session déjà
    /// remplacée…). WireGuard les jette sans rien dire ; les compter suffit.
    pub refuses: u64,
    /// Dernière fois qu'un refus est parti au journal (un par minute au plus).
    refus_note_le: u64,
    /// Paquets IP sortis du tunnel, en attente de la pile TCP/IP (`src/mandataire.rs`).
    recueillis: std::collections::VecDeque<Vec<u8>>,
    /// MTU annoncée par la configuration (`None` : 1420, la valeur de WireGuard).
    mtu: Option<usize>,
}

impl Session {
    /// Ouvre la session vers une adresse **précise** (une des adresses du nom, voir `connecter`).
    pub fn ouvrir_a(config: &crate::tunnel::Config, pair: SocketAddr) -> Result<Session, String> {
        let secret = crate::tunnel::octets_32(&config.cle_privee)
            .ok_or("clé privée illisible (32 octets en base64 attendus)")?;
        let publique = crate::tunnel::octets_32(&config.cle_paire)
            .ok_or("clé publique du pair illisible (32 octets en base64 attendus)")?;
        let partagee = match &config.cle_partagee {
            Some(cle) => Some(crate::tunnel::octets_32(cle).ok_or("clé partagée illisible")?),
            None => None,
        };

        let udp = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("socket UDP impossible : {e}"))?;
        udp.connect(pair).map_err(|e| format!("connexion UDP vers {pair} impossible : {e}"))?;
        udp.set_nonblocking(true).map_err(|e| format!("socket UDP non bloquant impossible : {e}"))?;

        // Un index de session tiré au hasard, comme le fait WireGuard : il ne doit pas être 0.
        let index = (crate::log::maintenant_ms() as u32 & 0x00ff_ffff).max(1);
        let tunnel = Tunn::new(
            x25519_dalek::StaticSecret::from(secret),
            x25519_dalek::PublicKey::from(publique),
            partagee,
            // Sans keepalive, un pair derrière une box perd la session au bout de quelques minutes
            // de silence. 25 s est la valeur recommandée par WireGuard.
            Some(config.keepalive.unwrap_or(25)),
            index,
            None,
        )
        .map_err(|e| format!("session WireGuard impossible : {e}"))?;

        Ok(Session {
            tunnel,
            udp,
            pair,
            sortie: vec![0u8; MAX_PAQUET],
            entree: vec![0u8; MAX_PAQUET],
            travail: vec![0u8; MAX_PAQUET],
            envoyes: 0,
            recus: 0,
            refuses: 0,
            refus_note_le: 0,
            recueillis: std::collections::VecDeque::new(),
            mtu: config.mtu.map(|m| m as usize),
        })
    }

    pub fn pair(&self) -> SocketAddr {
        self.pair
    }

    /// La poignée de main a-t-elle abouti ? (`Some(x)` = secondes depuis la dernière poignée.)
    pub fn etabli(&self) -> bool {
        self.tunnel.stats().0.is_some()
    }

    /// Secondes depuis la dernière poignée de main, si elle a eu lieu.
    pub fn age_handshake(&self) -> Option<std::time::Duration> {
        self.tunnel.stats().0
    }

    /// Lance une poignée de main vers le pair. WireGuard en envoie une de lui-même dès qu'il a des
    /// données à faire passer ; ici, tant que rien ne circule, c'est nous qui ouvrons le bal.
    pub fn demarrer(&mut self) -> Result<(), String> {
        let mut copie = std::mem::take(&mut self.sortie);
        let resultat = self.tunnel.format_handshake_initiation(&mut copie, false);
        let a_envoyer = match resultat {
            TunnResult::WriteToNetwork(paquet) => Some(paquet.to_vec()),
            TunnResult::Done => None,
            TunnResult::Err(e) => {
                self.sortie = copie;
                return Err(format!("poignée de main impossible : {e:?}"));
            }
            _ => None,
        };
        self.sortie = copie;
        if let Some(paquet) = a_envoyer {
            if self.udp.send(&paquet).is_ok() {
                self.envoyes += 1;
            }
        }
        Ok(())
    }

    /// Envoie au pair ce que le tunnel a à dire (poignée de main, keepalive, accusés).
    fn vider_minuteries(&mut self) -> Result<(), String> {
        loop {
            // `update_timers` écrit dans `sortie` et ne rend un paquet que s'il a quelque chose.
            let mut copie = std::mem::take(&mut self.sortie);
            let resultat = self.tunnel.update_timers(&mut copie);
            let a_envoyer = match resultat {
                TunnResult::WriteToNetwork(paquet) => {
                    let paquet = paquet.to_vec();
                    if self.udp.send(&paquet).is_ok() {
                        self.envoyes += 1;
                    }
                    true
                }
                TunnResult::Done => false,
                // Session expirée (aucune poignée de main réussie depuis trop longtemps, après une
                // veille par exemple) : on en relance une au lieu d'abandonner le tunnel.
                TunnResult::Err(WireGuardError::ConnectionExpired) => {
                    self.sortie = copie;
                    crate::info!("tunnel : session expirée, nouvelle poignée de main");
                    return self.demarrer();
                }
                TunnResult::Err(e) => {
                    self.sortie = copie;
                    self.noter_refus(&format!("minuterie : {e:?}"));
                    return Ok(());
                }
                _ => false,
            };
            self.sortie = copie;
            if !a_envoyer {
                return Ok(());
            }
        }
    }

    /// Une passe : ce qui doit partir, puis ce qui est arrivé. À appeler souvent (toutes les
    /// quelques millisecondes) : c'est la boucle du tunnel.
    pub fn pomper(&mut self) -> Result<(), String> {
        self.vider_minuteries()?;

        // Ce qui arrive du réseau. Le socket est non bloquant : on vide jusqu'à `WouldBlock`.
        loop {
            let mut recu = [0u8; 2_048];
            let n = match self.udp.recv(&mut recu) {
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                // Windows rend une erreur sur un socket UDP « connecté » quand un ICMP « port
                // injoignable » est revenu (WSAECONNRESET) : ce n'est pas la fin du tunnel.
                Err(e) if matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionRefused) => {
                    self.noter_refus(&format!("réseau : {e}"));
                    continue;
                }
                Err(e) => return Err(format!("lecture du tunnel impossible : {e}")),
            };
            self.recus += 1;
            let datagramme = recu[..n].to_vec();
            // `decapsulate` rend un paquet à envoyer, ou un paquet IP à remettre à la pile, et il
            // faut le rappeler avec un datagramme vide tant qu'il a quelque chose à donner.
            let mut courant = std::mem::take(&mut self.travail);
            let mut resultat = self.tunnel.decapsulate(Some(self.pair.ip()), &datagramme, &mut courant);
            loop {
                match resultat {
                    TunnResult::WriteToNetwork(paquet) => {
                        let paquet = paquet.to_vec();
                        if self.udp.send(&paquet).is_ok() {
                            self.envoyes += 1;
                        }
                        resultat = self.tunnel.decapsulate(None, &[], &mut courant);
                    }
                    // Les paquets venus du pair : ils partent à la pile TCP/IP, qui les rend en
                    // connexions locales (voir `prendre_paquet`).
                    TunnResult::WriteToTunnelV4(paquet, _) | TunnResult::WriteToTunnelV6(paquet, _) => {
                        self.recueillis.push_back(paquet.to_vec());
                        resultat = self.tunnel.decapsulate(None, &[], &mut courant);
                    }
                    TunnResult::Done => break,
                    // Un datagramme refusé (`NoCurrentSession` au changement de clé, compteur
                    // rejoué, MAC invalide…) se JETTE, comme le fait WireGuard. Avant la 0.4.94,
                    // un seul suffisait à arrêter le tunnel (trois fois le 2026-11-26).
                    TunnResult::Err(e) => {
                        self.noter_refus(&format!("{e:?}"));
                        break;
                    }
                }
            }
            self.travail = courant;
        }
    }

    /// Attelle à la pile : la poignée de main aboutit-elle, et en combien de temps ?
    /// Sert à la sonde de mise au point (`TRAME_TUNNEL_ESSAI`).
    pub fn attendre_handshake(&mut self, delai: std::time::Duration) -> Result<std::time::Duration, String> {
        let debut = std::time::Instant::now();
        let mut dernier_envoi = std::time::Instant::now() - std::time::Duration::from_secs(10);
        while debut.elapsed() < delai {
            // Toutes les deux secondes, on relance une poignée de main : un premier paquet perdu
            // ne doit pas laisser le tunnel muet.
            if !self.etabli() && dernier_envoi.elapsed() >= std::time::Duration::from_secs(2) {
                self.demarrer()?;
                dernier_envoi = std::time::Instant::now();
            }
            self.pomper()?;
            if self.etabli() {
                return Ok(debut.elapsed());
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        Err(format!(
            "aucune poignée de main après {} s (le pair {} ne répond pas)",
            delai.as_secs(),
            self.pair
        ))
    }
}

/// **Toutes** les adresses d'un endpoint, IPv4 d'abord et sans doublon.
///
/// Un nom d'hôte de fournisseur rend plusieurs adresses, qui tournent (mesuré le 2026-11-28 sur
/// `paris.fr.wg.nordhold.net` : TTL 30 s, une adresse différente à chaque requête, dont des
/// injoignables). Prendre la **première** seulement laissait le tunnel muet pour une raison
/// invisible. Et l'ordre compte : une IPv6 en tête, sur une machine sans IPv6, donne une poignée
/// de main silencieuse — alors que Trame sort en IPv4.
pub fn resoudre_toutes(endpoint: &str) -> Result<Vec<SocketAddr>, String> {
    let adresses: Vec<SocketAddr> = endpoint
        .to_socket_addrs()
        .map_err(|e| format!("endpoint « {endpoint} » introuvable : {e}"))?
        .collect();
    let triees = ordonner(adresses);
    if triees.is_empty() {
        return Err(format!("endpoint « {endpoint} » : aucune adresse"));
    }
    Ok(triees)
}

/// IPv4 avant IPv6, doublons retirés. L'ordre d'arrivée est conservé à l'intérieur de chaque
/// famille : c'est celui du résolveur, qui sait ce qui est proche.
fn ordonner(adresses: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let mut triees: Vec<SocketAddr> = Vec::new();
    for adresse in adresses {
        if triees.contains(&adresse) {
            continue;
        }
        if adresse.is_ipv6() {
            triees.push(adresse);
        } else {
            // Insertion stable : chaque IPv4 se glisse devant la première IPv6 déjà rangée.
            let avant_ipv6 = triees.iter().position(|a| a.is_ipv6()).unwrap_or(triees.len());
            triees.insert(avant_ipv6, adresse);
        }
    }
    triees
}

impl crate::moteur::Tunnel for Session {
    fn etiquette(&self) -> String {
        self.pair.to_string()
    }

    fn mtu(&self) -> Option<usize> {
        self.mtu
    }

    fn pomper(&mut self) -> Result<(), String> {
        Session::pomper(self)
    }

    fn prendre_paquet(&mut self) -> Option<Vec<u8>> {
        Session::prendre_paquet(self)
    }

    fn envoyer_paquet(&mut self, paquet: &[u8]) -> Result<(), String> {
        Session::envoyer_paquet(self, paquet)
    }
}

/// Ouvre la session et **attend la poignée de main**, en essayant les adresses du nom l'une après
/// l'autre (trois au plus), avec un budget total inchangé : il est réparti entre elles.
///
/// Mesure du 2026-11-28 : une configuration Mullvad perce en 20 ms, une configuration Nord ne
/// répond sur **sept** adresses essayées à la main. Le silence de WireGuard ne dit pas pourquoi
/// (clé inconnue du serveur, port filtré, serveur hors service) ; ce que Trame peut faire, c'est
/// essayer les autres adresses du même nom avant de renoncer, et dire lesquelles ont été tentées.
pub fn connecter(config: &crate::tunnel::Config, delai: std::time::Duration) -> Result<Session, String> {
    let adresses = resoudre_toutes(&config.endpoint)?;
    let essayees: Vec<SocketAddr> = adresses.iter().copied().take(3).collect();
    // Au moins deux secondes par adresse : en dessous, un pair lointain n'a pas le temps de
    // répondre, et on condamnerait une configuration valide.
    let part = (delai / essayees.len() as u32).max(std::time::Duration::from_secs(2));
    let mut echecs: Vec<String> = Vec::new();
    for adresse in &essayees {
        let mut session = match Session::ouvrir_a(config, *adresse) {
            Ok(s) => s,
            Err(e) => {
                echecs.push(format!("{adresse} ({e})"));
                continue;
            }
        };
        let debut = std::time::Instant::now();
        match session.attendre_handshake(part) {
            Ok(_) => {
                crate::info!(
                    "tunnel : poignée de main en {} ms avec {adresse} ({} adresse(s) essayée(s))",
                    debut.elapsed().as_millis(),
                    essayees.len()
                );
                return Ok(session);
            }
            Err(e) => echecs.push(format!("{adresse} ({e})")),
        }
    }
    Err(format!(
        "aucune poignée de main — le pair ne répond sur aucune adresse ({}). Un serveur WireGuard se tait quand il ne reconnaît pas la clé : configuration non validée côté fournisseur, ou port 51820 filtré sur le réseau. Adresses essayées : {}",
        config.endpoint,
        echecs.join(" ; ")
    ))
}

/// Sonde de mise au point : monte le tunnel configuré, tente la poignée de main, rend le détail.
///
/// Jamais appelée en usage normal : `main` ne la déclenche que sur `TRAME_TUNNEL_ESSAI`.
pub fn essai(racine: &std::path::Path) -> Result<String, String> {
    let config = crate::tunnel::charger(racine).ok_or("aucune configuration de tunnel importée")?;
    let debut = std::time::Instant::now();
    let session = connecter(&config, std::time::Duration::from_secs(10))?;
    let duree = debut.elapsed();
    let (_, tx, rx, perte, rtt) = session.stats();
    Ok(format!(
        "poignée de main avec {} en {} ms (tx {tx} o, rx {rx} o, perte {perte:.1} %, rtt {:?}, pair {}, {} datagrammes envoyés / {} reçus)",
        config.endpoint,
        duree.as_millis(),
        rtt,
        session.pair(),
        session.envoyes,
        session.recus
    ))
}

impl Session {
    /// Compte un datagramme refusé ; le journal en garde un par minute, avec le total.
    fn noter_refus(&mut self, raison: &str) {
        self.refuses += 1;
        let maintenant = crate::log::maintenant_ms();
        if maintenant.saturating_sub(self.refus_note_le) > 60_000 {
            self.refus_note_le = maintenant;
            crate::info!("tunnel : datagramme jeté ({raison}), {} depuis le début", self.refuses);
        }
    }

    /// Le prochain paquet IP sorti du tunnel, pour la pile (`src/mandataire.rs`).
    pub fn prendre_paquet(&mut self) -> Option<Vec<u8>> {
        self.recueillis.pop_front()
    }

    /// Chiffre un paquet IP de la pile et l'envoie au pair.
    pub fn envoyer_paquet(&mut self, paquet: &[u8]) -> Result<(), String> {
        let mut copie = std::mem::take(&mut self.sortie);
        let resultat = self.tunnel.encapsulate(paquet, &mut copie);
        let a_envoyer = match resultat {
            TunnResult::WriteToNetwork(sortant) => Some(sortant.to_vec()),
            TunnResult::Done => None,
            // Paquet jeté : la pile TCP le renverra.
            TunnResult::Err(e) => {
                self.sortie = copie;
                self.noter_refus(&format!("chiffrement : {e:?}"));
                return Ok(());
            }
            _ => None,
        };
        self.sortie = copie;
        if let Some(sortant) = a_envoyer {
            if self.udp.send(&sortant).is_ok() {
                self.envoyes += 1;
            }
        }
        Ok(())
    }

    /// Les compteurs du tunnel (secondes depuis la poignée, octets, perte, rtt).
    pub fn stats(&self) -> (Option<std::time::Duration>, usize, usize, f32, Option<u32>) {
        self.tunnel.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deux clés de test (aucune valeur réelle).
    const CLE_PRIVEE: &str = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
    const CLE_PAIRE: &str = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";

    fn config(endpoint: &str) -> crate::tunnel::Config {
        crate::tunnel::lire(&format!(
            "[Interface]\nPrivateKey = {CLE_PRIVEE}\nAddress = 10.0.0.2/32\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = {endpoint}\n"
        ))
        .expect("configuration valide")
    }

    /// Ouvre une session de test vers la première adresse de l'endpoint (toujours locale ici).
    fn ouvrir(config: &crate::tunnel::Config) -> Session {
        let pair = resoudre_toutes(&config.endpoint).expect("endpoint resolu")[0];
        Session::ouvrir_a(config, pair).expect("session")
    }

    /// La session s'ouvre sans toucher au réseau, et son endpoint est bien résolu.
    #[test]
    fn une_session_s_ouvre_sans_reseau() {
        let session = ouvrir(&config("127.0.0.1:51820"));
        assert_eq!(session.pair().port(), 51820);
        assert!(!session.etabli(), "aucune poignée de main avant le premier paquet");
        assert_eq!(session.age_handshake(), None);
    }

    /// Un nom d'hôte rend plusieurs adresses : IPv4 d'abord, sans doublon. L'ordre décide —
    /// une IPv6 en tête, sur une machine sans IPv6, donnait une poignée de main silencieuse.
    #[test]
    fn les_adresses_sont_ordonnees_ipv4_d_abord_et_sans_doublon() {
        use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
        let v6a = SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 51820, 0, 0));
        let v4a = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(193, 32, 126, 68), 51820));
        let v4b = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(193, 32, 126, 69), 51820));
        let triees = ordonner(vec![v6a, v4a, v4b, v4a]);
        assert_eq!(triees, vec![v4a, v4b, v6a], "IPv4 dans l'ordre du résolveur, puis IPv6");
    }

    /// Un endpoint en clair (une seule adresse) reste une adresse : la résolution ne doit pas
    /// perdre le cas simple.
    #[test]
    fn un_endpoint_numerique_rend_une_seule_adresse() {
        let adresses = resoudre_toutes("127.0.0.1:51820").expect("adresse");
        assert_eq!(adresses.len(), 1);
        assert_eq!(adresses[0].port(), 51820);
    }

    /// Des datagrammes que le moteur refuse (données d'une session inconnue — le cas
    /// `NoCurrentSession` du 2026-11-26 —, paquet tronqué) sont JETÉS : le tunnel continue.
    #[test]
    fn un_datagramme_refuse_ne_fait_pas_tomber_le_tunnel() {
        let pair = UdpSocket::bind("127.0.0.1:0").expect("banc : faux pair");
        let mut client = ouvrir(&config(&pair.local_addr().unwrap().to_string()));
        let adresse_client = client.udp.local_addr().unwrap();
        // Type 4 (données) vers un index que le client n'a jamais ouvert, puis un débris.
        let mut donnees = vec![4u8, 0, 0, 0, 0x78, 0x56, 0x34, 0x12];
        donnees.extend_from_slice(&[0u8; 8 + 32]);
        pair.send_to(&donnees, adresse_client).unwrap();
        pair.send_to(&[1u8, 2, 3], adresse_client).unwrap();
        let debut = std::time::Instant::now();
        while client.recus < 2 && debut.elapsed() < std::time::Duration::from_secs(3) {
            client.pomper().expect("le tunnel survit aux datagrammes refusés");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(client.recus, 2, "les deux datagrammes sont arrivés");
        assert_eq!(client.refuses, 2, "et ont été jetés, comptés");
        client.pomper().expect("la passe suivante marche toujours");
    }

    /// Deux pairs locaux : la poignée de main aboutit, et les compteurs le disent. C'est le banc
    /// qui prouve que le moteur fonctionne sans dépendre d'un fournisseur.
    #[test]
    fn deux_pairs_locaux_etablissent_la_session() {
        // Un « serveur » : il écoute sur un port, et son Tunn est l'autre moitié.
        let ecoute = UdpSocket::bind("127.0.0.1:0").expect("banc : socket serveur");
        let adresse = ecoute.local_addr().expect("adresse du serveur");
        ecoute.set_nonblocking(true).expect("non bloquant");

        let secret_client = x25519_dalek::StaticSecret::from(crate::tunnel::octets_32(CLE_PRIVEE).unwrap());
        let public_client = x25519_dalek::PublicKey::from(&secret_client);
        let secret_serveur = x25519_dalek::StaticSecret::from(crate::tunnel::octets_32(CLE_PAIRE).unwrap());
        let public_serveur = x25519_dalek::PublicKey::from(&secret_serveur);

        let mut serveur = Tunn::new(secret_serveur, public_client, None, None, 7, None).expect("tunn serveur");
        let mut client = ouvrir(&config(&adresse.to_string()));
        // Le client doit connaître la clé publique du serveur : on refait sa configuration.
        client.tunnel = Tunn::new(secret_client, public_serveur, None, Some(25), 9, None).expect("tunn client");

        let delai = std::time::Duration::from_secs(5);
        let debut = std::time::Instant::now();
        let mut etabli = false;
        client.demarrer().expect("poignée de main cliente");
        while debut.elapsed() < delai {
            client.pomper().expect("pompe cliente");
            // Le serveur lit ce que le client a envoyé et répond.
            let mut recu = [0u8; 2048];
            while let Ok((n, de)) = ecoute.recv_from(&mut recu) {
                let datagramme = recu[..n].to_vec();
                let mut sortie = [0u8; 2048];
                let mut resultat = serveur.decapsulate(None, &datagramme, &mut sortie);
                loop {
                    match resultat {
                        TunnResult::WriteToNetwork(paquet) => {
                            let paquet = paquet.to_vec();
                            // On répond à l'EXPÉDITEUR (`de`), pas au port du serveur : le client
                            // écoute sur un port éphémère.
                            let _ = ecoute.send_to(&paquet, de);
                            resultat = serveur.decapsulate(None, &[], &mut sortie);
                        }
                        TunnResult::Done => break,
                        TunnResult::Err(_) => break,
                        _ => break,
                    }
                }
            }
            if client.etabli() {
                etabli = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(etabli, "la poignée de main entre deux pairs locaux doit aboutir");
    }
}
