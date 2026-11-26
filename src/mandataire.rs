//! Le **mandataire local** : un serveur SOCKS5 qui fait passer les pages **dans le tunnel**.
//!
//! C'est la seconde moitié du VPN intégré (`src/vpn.rs` tient la session WireGuard). Trame
//! n'installe aucun pilote : les pages passent par ce mandataire, que WebView2 traverse
//! (`--proxy-server=socks5://127.0.0.1:PORT`, mesuré fonctionnel le 2026-11-27). Conséquence
//! heureuse : si le tunnel tombe, le mandataire ne répond plus et **plus rien ne sort**.
//!
//! Trois pièces, une seule boucle :
//!   - une **pile TCP/IP en espace utilisateur** (`smoltcp`) branchée sur les paquets du tunnel ;
//!   - un **résolveur DNS dans le tunnel** (le DNS du fournisseur, jamais celui de la box) ;
//!   - le **SOCKS5** lui-même, qui relie une connexion locale à un socket de la pile.

use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use smoltcp::iface::{Config as ConfIface, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{tcp, udp};
use smoltcp::time::Instant as InstantSmol;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, IpEndpoint, Ipv4Address, Ipv6Address};

use crate::vpn::Session;

/// Le mandataire tourne tant que ce jeton n'est pas levé.
pub struct Mandataire {
    port: u16,
    arret: Arc<AtomicBool>,
}

impl Mandataire {
    /// L'adresse à donner à WebView2 : `socks5://127.0.0.1:<port>`.
    pub fn adresse(&self) -> String {
        format!("socks5://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for Mandataire {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
    }
}

/// Démarre le mandataire : poignée de main du tunnel, pile, écoute SOCKS5.
///
/// Rend la main **une fois le tunnel établi et l'écoute ouverte** : Trame peut alors donner son
/// adresse à WebView2. Une erreur ici veut dire « pas de tunnel », et Trame refuse de naviguer.
pub fn demarrer(config: crate::tunnel::Config) -> Result<Mandataire, String> {
    let ecoute = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("écoute SOCKS5 impossible : {e}"))?;
    let port = ecoute.local_addr().map_err(|e| e.to_string())?.port();
    ecoute.set_nonblocking(true).map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let arret = Arc::new(AtomicBool::new(false));
    let jeton = arret.clone();

    std::thread::Builder::new()
        .name("trame-mandataire".into())
        .spawn(move || {
            let mut session = match Session::ouvrir(&config) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            match session.attendre_handshake(std::time::Duration::from_secs(10)) {
                Ok(duree) => {
                    crate::info!("mandataire : tunnel établi en {} ms", duree.as_millis());
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            }
            if let Err(e) = boucle(session, ecoute, &config, &jeton) {
                crate::alerte!("mandataire : {e}");
            }
        })
        .map_err(|e| format!("fil du mandataire impossible : {e}"))?;

    match rx.recv_timeout(std::time::Duration::from_secs(12)) {
        Ok(Ok(())) => Ok(Mandataire { port, arret }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("le tunnel n'a pas répondu en 12 s".to_string()),
    }
}

// ------------------------------------------------------------------ la pile : deux files de paquets

struct Pile {
    /// Paquets venus du tunnel, à donner à smoltcp.
    entrants: VecDeque<Vec<u8>>,
    /// Paquets produits par smoltcp, à chiffrer et envoyer.
    sortants: VecDeque<Vec<u8>>,
    /// MTU à annoncer (1420 par défaut, la valeur de WireGuard).
    mtu: usize,
}

struct JetonRx(Vec<u8>);

impl RxToken for JetonRx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}

struct JetonTx<'a>(&'a mut VecDeque<Vec<u8>>);

impl TxToken for JetonTx<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, len: usize, f: F) -> R {
        let mut tampon = vec![0u8; len];
        let resultat = f(&mut tampon);
        self.0.push_back(tampon);
        resultat
    }
}

impl Device for Pile {
    type RxToken<'b>
        = JetonRx
    where
        Self: 'b;
    type TxToken<'b>
        = JetonTx<'b>
    where
        Self: 'b;

    fn receive(&mut self, _maintenant: InstantSmol) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let paquet = self.entrants.pop_front()?;
        Some((JetonRx(paquet), JetonTx(&mut self.sortants)))
    }

    fn transmit(&mut self, _maintenant: InstantSmol) -> Option<Self::TxToken<'_>> {
        Some(JetonTx(&mut self.sortants))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut capacites = DeviceCapabilities::default();
        capacites.medium = Medium::Ip;
        capacites.max_transmission_unit = self.mtu;
        capacites
    }
}

// ------------------------------------------------------------------ le résolveur (DNS dans le tunnel)

struct Resolveur {
    serveur: IpEndpoint,
    socket: SocketHandle,
    /// Identifiant de la demande en cours (`0` : aucune).
    en_cours: u16,
    nom: String,
    envoye_le: u64,
    essais: u8,
    /// La réponse, gardée jusqu'à ce que le client qui l'attend la prenne.
    resultat: Option<Result<IpAddr, String>>,
}

impl Resolveur {
    fn nouveau(adresse: IpAddr, sockets: &mut SocketSet<'_>) -> Resolveur {
        let rx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 2048]);
        let tx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 2048]);
        let mut socket = udp::Socket::new(rx, tx);
        let _ = socket.bind(40000u16);
        Resolveur {
            serveur: IpEndpoint::new(ip_smoltcp(adresse), 53),
            socket: sockets.add(socket),
            en_cours: 0,
            nom: String::new(),
            envoye_le: 0,
            essais: 0,
            resultat: None,
        }
    }

    /// Envoie une requête A pour `nom`. Rend l'identifiant à attendre.
    fn demander(&mut self, sockets: &mut SocketSet<'_>, nom: &str, maintenant: u64) -> Option<u16> {
        self.en_cours = (self.en_cours % 60000) + 1;
        self.nom = nom.to_string();
        self.envoye_le = maintenant;
        self.essais = 1;
        self.resultat = None;
        self.envoyer(sockets)?;
        Some(self.en_cours)
    }

    fn envoyer(&mut self, sockets: &mut SocketSet<'_>) -> Option<()> {
        let requete = requete_dns(self.en_cours, &self.nom);
        let socket = sockets.get_mut::<udp::Socket>(self.socket);
        socket
            .send_slice(&requete, udp::UdpMetadata::from(self.serveur))
            .ok()?;
        Some(())
    }

    /// Fait avancer le résolveur : lit la réponse et la GARDE, relance au besoin.
    ///
    /// C'est la boucle qui l'appelle, jamais le client : deux lectures du même socket UDP se
    /// voleraient la réponse (mesure du 2026-11-28 : le client recevait un EOF au lieu de sa
    /// réponse SOCKS5, parce que la boucle avait déjà consommé le datagramme).
    fn entretenir(&mut self, sockets: &mut SocketSet<'_>, maintenant: u64) {
        if self.en_cours == 0 || self.resultat.is_some() {
            return;
        }
        let mut tampon = [0u8; 2048];
        let socket = sockets.get_mut::<udp::Socket>(self.socket);
        if let Ok((n, _)) = socket.recv_slice(&mut tampon) {
            self.resultat = Some(match extraire_a(&tampon[..n], self.en_cours) {
                Some(ip) => Ok(ip),
                None => Err(format!("« {} » introuvable (DNS du tunnel)", self.nom)),
            });
            return;
        }
        if maintenant.saturating_sub(self.envoye_le) > 2_000 {
            if self.essais >= 3 {
                self.resultat = Some(Err(format!(
                    "le DNS du tunnel n'a pas répondu pour « {} »",
                    self.nom
                )));
                return;
            }
            self.essais += 1;
            self.envoye_le = maintenant;
            self.envoyer(sockets);
        }
    }
}

fn ip_smoltcp(ip: IpAddr) -> IpAddress {
    match ip {
        IpAddr::V4(v4) => IpAddress::Ipv4(v4.into()),
        IpAddr::V6(v6) => IpAddress::Ipv6(v6.into()),
    }
}

/// Une requête DNS « A » minimale (une question, récursion demandée).
pub fn requete_dns(identifiant: u16, nom: &str) -> Vec<u8> {
    let mut requete = Vec::with_capacity(64 + nom.len());
    requete.extend_from_slice(&identifiant.to_be_bytes());
    requete.extend_from_slice(&[0x01, 0x00]); // requête standard, récursion voulue
    requete.extend_from_slice(&[0x00, 0x01]); // 1 question
    requete.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
    for partie in nom.trim_end_matches('.').split('.') {
        requete.push(partie.len().min(63) as u8);
        requete.extend_from_slice(partie.as_bytes());
    }
    requete.push(0);
    requete.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // A, IN
    requete
}

/// Extrait la première adresse IPv4 d'une réponse DNS (question déjà connue).
///
/// Parseur volontairement minimal : on saute l'en-tête et la question, puis on parcourt les
/// réponses. Un nom compressé (`0xC0`) ne fait que deux octets — c'est le seul cas tordu.
pub fn extraire_a(reponse: &[u8], identifiant: u16) -> Option<IpAddr> {
    if reponse.len() < 12 {
        return None;
    }
    if u16::from_be_bytes([reponse[0], reponse[1]]) != identifiant {
        return None;
    }
    // RCODE 3 = nom inexistant, 2 = échec serveur.
    if reponse[3] & 0x0f != 0 {
        return None;
    }
    let questions = u16::from_be_bytes([reponse[4], reponse[5]]) as usize;
    let reponses = u16::from_be_bytes([reponse[6], reponse[7]]) as usize;
    let mut i = 12;
    // Sauter la question : nom, puis type (2) et classe (2).
    for _ in 0..questions {
        i = sauter_nom(reponse, i)?;
        i += 4;
    }
    for _ in 0..reponses {
        i = sauter_nom(reponse, i)?;
        if i + 10 > reponse.len() {
            return None;
        }
        let typee = u16::from_be_bytes([reponse[i], reponse[i + 1]]);
        let longueur = u16::from_be_bytes([reponse[i + 8], reponse[i + 9]]) as usize;
        i += 10;
        if i + longueur > reponse.len() {
            return None;
        }
        if typee == 1 && longueur == 4 {
            return Some(IpAddr::V4(Ipv4Addr::new(
                reponse[i],
                reponse[i + 1],
                reponse[i + 2],
                reponse[i + 3],
            )));
        }
        i += longueur;
    }
    None
}

/// Rend l'index qui suit le nom à `i` (gère la compression `0xC0`).
fn sauter_nom(reponse: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let octet = *reponse.get(i)?;
        if octet == 0 {
            return Some(i + 1);
        }
        if octet & 0xc0 == 0xc0 {
            return Some(i + 2);
        }
        i += 1 + octet as usize;
    }
}

// ------------------------------------------------------------------ SOCKS5

enum Etat {
    Salutation,
    Requete,
    /// On attend la résolution DNS du nom, sur CE client (socket, identifiant, envoi, essais).
    Resolution { socket: SocketHandle, id: u16, envoye_le: u64, essais: u8, nom: String },
    Connexion(SocketHandle),
    Relais(SocketHandle),
}

struct Client {
    flux: TcpStream,
    etat: Etat,
    tampon: Vec<u8>,
    /// Ce que le navigateur a déjà envoyé et qui doit partir dans le tunnel.
    a_pousser: Vec<u8>,
    hote: Option<(IpAddr, u16)>,
}

fn boucle(
    mut session: Session,
    ecoute: TcpListener,
    config: &crate::tunnel::Config,
    arret: &AtomicBool,
) -> Result<(), String> {
    let mtu = session.mtu().unwrap_or(1420);
    let mut pile = Pile {
        entrants: VecDeque::new(),
        sortants: VecDeque::new(),
        mtu,
    };

    let mut conf = ConfIface::new(HardwareAddress::Ip);
    conf.random_seed = crate::log::maintenant_ms();
    let mut iface = Interface::new(conf, &mut pile, instant(0));
    let v4 = config.adresses.iter().find_map(|a| a.split('/').next().and_then(|x| x.parse::<Ipv4Addr>().ok()));
    let v6 = config.adresses.iter().find_map(|a| a.split('/').next().and_then(|x| x.parse::<Ipv6Addr>().ok()));
    iface.update_ip_addrs(|adresses| {
        if let Some(ip) = v4 {
            let _ = adresses.push(IpCidr::new(IpAddress::Ipv4(ip.into()), 32));
        }
        if let Some(ip) = v6 {
            let _ = adresses.push(IpCidr::new(IpAddress::Ipv6(ip.into()), 128));
        }
    });
    // WireGuard achemine par `AllowedIPs` : une route par défaut vers le pair suffit.
    if let Some(ip) = v4 {
        let _ = iface.routes_mut().add_default_ipv4_route(ip.into());
    }
    if let Some(ip) = v6 {
        let _ = iface.routes_mut().add_default_ipv6_route(ip.into());
    }

    let mut sockets = SocketSet::new(Vec::new());
    let dns: Option<IpAddr> = config.dns.first().and_then(|d| d.parse().ok());

    let mut clients: Vec<Client> = Vec::new();
    crate::info!(
        "mandataire : écoute SOCKS5 sur 127.0.0.1:{} (MTU {}, DNS {})",
        ecoute.local_addr().map(|a| a.port()).unwrap_or(0),
        session.mtu().unwrap_or(1420),
        dns.map(|d| d.to_string()).unwrap_or_else(|| "aucun".into())
    );

    while !arret.load(Ordering::Relaxed) {
        let maintenant = crate::log::maintenant_ms();
        let temps = instant(maintenant);

        // 1. Le tunnel : ce qui arrive, ce qui part.
        session.pomper()?;
        while let Some(paquet) = session.prendre_paquet() {
            pile.entrants.push_back(paquet);
        }

        // 2. Les connexions du navigateur.
        loop {
            match ecoute.accept() {
                Ok((flux, _)) => {
                    let _ = flux.set_nonblocking(true);
                    clients.push(Client {
                        flux,
                        etat: Etat::Salutation,
                        tampon: Vec::new(),
                        a_pousser: Vec::new(),
                        hote: None,
                    });
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // 3. La pile travaille (elle consomme les entrants et produit les sortants).
        iface.poll(temps, &mut pile, &mut sockets);

        // 4. Le résolveur, s'il a une demande en cours.
        // 5. Chaque client avance d'un cran.
        let mut a_oublier = Vec::new();
        {
            let contexte = iface.context();
            for (index, client) in clients.iter_mut().enumerate() {
                if client.avancer(&mut sockets, dns, contexte, maintenant).is_err() {
                    a_oublier.push(index);
                }
            }
        }
        for index in a_oublier.into_iter().rev() {
            clients.swap_remove(index);
        }

        // 6. Ce que la pile et les clients ont produit part dans le tunnel.
        iface.poll(temps, &mut pile, &mut sockets);
        while let Some(paquet) = pile.sortants.pop_front() {
            session.envoyer_paquet(&paquet)?;
        }

        std::thread::sleep(std::time::Duration::from_millis(3));
    }
    crate::info!("mandataire : arrêt demandé");
    Ok(())
}

fn instant(ms: u64) -> InstantSmol {
    InstantSmol::from_millis(ms as i64)
}

impl Client {
    fn avancer(
        &mut self,
        sockets: &mut SocketSet<'_>,
        serveur_dns: Option<IpAddr>,
        contexte: &mut smoltcp::iface::Context,
        maintenant: u64,
    ) -> Result<(), ()> {
        match self.etat {
            Etat::Salutation => {
                if !self.lire()? {
                    return Ok(());
                }
                if self.tampon.len() < 2 {
                    return Ok(());
                }
                let attendu = 2 + self.tampon[1] as usize;
                if self.tampon.len() < attendu {
                    return Ok(());
                }
                if self.flux.write_all(&[0x05, 0x00]).is_err() {
                    return Err(());
                }
                self.tampon.drain(..attendu);
                self.etat = Etat::Requete;
            }
            Etat::Requete => {
                if !self.lire()? {
                    return Ok(());
                }
                if self.tampon.len() < 5 {
                    return Ok(());
                }
                let atype = self.tampon[3];
                let taille = match atype {
                    0x01 => 4 + 4 + 2,
                    0x04 => 4 + 16 + 2,
                    0x03 => {
                        let longueur = self.tampon[4] as usize;
                        5 + longueur + 2
                    }
                    _ => return Err(()),
                };
                if self.tampon.len() < taille {
                    return Ok(());
                }
                let port = u16::from_be_bytes([self.tampon[taille - 2], self.tampon[taille - 1]]);
                match atype {
                    0x01 => {
                        let ip = Ipv4Addr::new(self.tampon[4], self.tampon[5], self.tampon[6], self.tampon[7]);
                        self.hote = Some((IpAddr::V4(ip), port));
                    }
                    0x04 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(&self.tampon[4..20]);
                        self.hote = Some((IpAddr::V6(Ipv6Addr::from(octets)), port));
                    }
                    0x03 => {
                        let longueur = self.tampon[4] as usize;
                        let nom = String::from_utf8_lossy(&self.tampon[5..5 + longueur]).to_string();
                        self.tampon.drain(..taille);
                        let Some(serveur) = serveur_dns else {
                            let _ = self.flux.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                            return Err(());
                        };
                        // Un socket DNS PAR CLIENT : deux onglets qui résolvent deux noms en même
                        // temps ne se volent plus la réponse (défaut constaté le 2026-11-28 :
                        // beaucoup d'onglets ne se chargeaient pas).
                        let rx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 2048]);
                        let tx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 2048]);
                        let mut socket = udp::Socket::new(rx, tx);
                        // Un port local DISTINCT : un socket lie au port 0 ne recoit pas sa reponse
                        // (smoltcp remet le datagramme au socket lie au port de destination).
                        let port_local = 40000u16 + (maintenant % 10000) as u16;
                        if socket.bind(port_local).is_err() {
                            let _ = self.flux.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                            return Err(());
                        }
                        let handle = sockets.add(socket);
                        let identifiant = (maintenant % 60000) as u16 + 1;
                        let requete = requete_dns(identifiant, &nom);
                        let endpoint = IpEndpoint::new(ip_smoltcp(serveur), 53);
                        let socket = sockets.get_mut::<udp::Socket>(handle);
                        if socket.send_slice(&requete, udp::UdpMetadata::from(endpoint)).is_err() {
                            let _ = self.flux.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                            return Err(());
                        }
                        self.etat = Etat::Resolution { socket: handle, id: identifiant, envoye_le: maintenant, essais: 1, nom };
                        self.hote = Some((IpAddr::V4(Ipv4Addr::UNSPECIFIED), port));
                        return Ok(());
                    }
                    _ => return Err(()),
                }
                self.tampon.drain(..taille);
                self.connecter(sockets, contexte)?;
            }
            Etat::Resolution { socket, id, envoye_le, essais, ref nom } => {
                let nom = nom.clone();
                let mut tampon = [0u8; 2048];
                let recu = {
                    let socket = sockets.get_mut::<udp::Socket>(socket);
                    socket.recv_slice(&mut tampon).ok().map(|(n, _)| n)
                };
                if let Some(n) = recu {
                    match extraire_a(&tampon[..n], id) {
                        Some(ip) => {
                            if let Some((_, port)) = self.hote {
                                self.hote = Some((ip, port));
                            }
                            self.connecter(sockets, contexte)?;
                        }
                        None => {
                            crate::info!("mandataire : « {nom} » introuvable (DNS du tunnel)");
                            let _ = self.flux.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                            return Err(());
                        }
                    }
                } else if maintenant.saturating_sub(envoye_le) > 2_000 {
                    if essais >= 3 {
                        crate::info!("mandataire : le DNS du tunnel n'a pas répondu pour « {nom} »");
                        let _ = self.flux.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                        return Err(());
                    }
                    let Some(serveur) = serveur_dns else { return Err(()) };
                    let requete = requete_dns(id, &nom);
                    let endpoint = IpEndpoint::new(ip_smoltcp(serveur), 53);
                    let handle = socket;
                    let socket = sockets.get_mut::<udp::Socket>(handle);
                    let _ = socket.send_slice(&requete, udp::UdpMetadata::from(endpoint));
                    self.etat = Etat::Resolution {
                        socket: handle,
                        id,
                        envoye_le: maintenant,
                        essais: essais + 1,
                        nom,
                    };
                }
            }
            Etat::Connexion(handle) => {
                let socket = sockets.get_mut::<tcp::Socket>(handle);
                let etat = socket.state();
                if etat != tcp::State::SynSent || maintenant % 4000 < 4 {
                    crate::info!("mandataire : TCP {etat:?}");
                }
                if etat == tcp::State::Established {
                    if self.flux.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).is_err() {
                        return Err(());
                    }
                    self.etat = Etat::Relais(handle);
                } else if socket.state() == tcp::State::Closed {
                    let _ = self.flux.write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
                    return Err(());
                }
            }
            Etat::Relais(handle) => {
                let mut recu = [0u8; 8192];
                if !self.lire()? {
                    // Rien de nouveau côté navigateur : on laisse la place aux autres clients.
                }
                let a_envoyer = std::mem::take(&mut self.tampon);
                let socket = sockets.get_mut::<tcp::Socket>(handle);
                if !a_envoyer.is_empty() && socket.may_send() {
                    // `send_slice` en prend ce qu'il peut : le reste repartira au tour suivant.
                    // Jeter le reliquat tronquait les requêtes (défaut constaté le 2026-11-28).
                    match socket.send_slice(&a_envoyer) {
                        Ok(n) if n < a_envoyer.len() => self.tampon.extend_from_slice(&a_envoyer[n..]),
                        Err(_) => self.tampon.extend_from_slice(&a_envoyer),
                        _ => {}
                    }
                }
                if socket.may_recv() {
                    if let Ok(n) = socket.recv_slice(&mut recu) {
                        if n > 0 && self.flux.write_all(&recu[..n]).is_err() {
                            return Err(());
                        }
                    }
                }
                if socket.state() == tcp::State::Closed {
                    return Err(());
                }
            }
        }
        Ok(())
    }

    /// Lit ce que le navigateur a envoyé dans `tampon`. Rend `true` s'il y a de la lecture.
    fn lire(&mut self) -> Result<bool, ()> {
        let mut recu = [0u8; 8192];
        match self.flux.read(&mut recu) {
            Ok(0) => Err(()),
            Ok(n) => {
                self.tampon.extend_from_slice(&recu[..n]);
                Ok(true)
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(false),
            Err(_) => Err(()),
        }
    }

    fn connecter(&mut self, sockets: &mut SocketSet<'_>, contexte: &mut smoltcp::iface::Context) -> Result<(), ()> {
        let Some((ip, port)) = self.hote else { return Err(()) };
        let rx = tcp::SocketBuffer::new(vec![0u8; 32768]);
        let tx = tcp::SocketBuffer::new(vec![0u8; 32768]);
        let mut socket = tcp::Socket::new(rx, tx);
        socket.set_nagle_enabled(false);
        let endpoint = IpEndpoint::new(ip_smoltcp(ip), port);
        let handle = sockets.add(socket);
        let socket = sockets.get_mut::<tcp::Socket>(handle);
        if socket.connect(contexte, endpoint, 40000u16).is_err() {
            let _ = self.flux.write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            return Err(());
        }
        self.etat = Etat::Connexion(handle);
        Ok(())
    }
}


/// Sonde de bout en bout : démarre le mandataire, puis demande une page **à travers** lui.
///
/// Jamais appelée en usage normal : `main` ne la déclenche que sur `TRAME_TUNNEL_ESSAI=2`. Elle
/// mesure la chaîne entière — tunnel, pile, DNS dans le tunnel, SOCKS5 — sans WebView2.
pub fn essai(racine: &std::path::Path) -> Result<String, String> {
    use std::net::ToSocketAddrs;
    use std::time::Duration;

    let config = crate::tunnel::charger(racine).ok_or("aucune configuration de tunnel importée")?;
    let mandataire = demarrer(config)?;
    let adresse = ("127.0.0.1", mandataire.port())
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("adresse du mandataire introuvable")?;
    let mut flux = TcpStream::connect(adresse).map_err(|e| format!("connexion au mandataire : {e}"))?;
    flux.set_read_timeout(Some(Duration::from_secs(25))).map_err(|e| e.to_string())?;
    flux.set_write_timeout(Some(Duration::from_secs(25))).map_err(|e| e.to_string())?;

    // 1. Poignée de main SOCKS5 (sans authentification).
    flux.write_all(&[0x05, 0x01, 0x00]).map_err(|e| e.to_string())?;
    let mut reponse = [0u8; 2];
    flux.read_exact(&mut reponse).map_err(|e| format!("salutation SOCKS5 : {e}"))?;
    if reponse[0] != 0x05 {
        return Err(format!("le mandataire ne parle pas SOCKS5 ({reponse:?})"));
    }

    // 2. CONNECT par NOM : c'est le DNS du tunnel qui doit résoudre, pas la box.
    let nom = "example.com";
    let mut requete = vec![0x05, 0x01, 0x00, 0x03, nom.len() as u8];
    requete.extend_from_slice(nom.as_bytes());
    requete.extend_from_slice(&80u16.to_be_bytes());
    flux.write_all(&requete).map_err(|e| e.to_string())?;
    let mut rep = [0u8; 10];
    flux.read_exact(&mut rep).map_err(|e| format!("réponse SOCKS5 : {e}"))?;
    if rep[1] != 0 {
        return Err(format!("le mandataire refuse la connexion (code {})", rep[1]));
    }

    // 3. Une vraie requête HTTP, à travers le tunnel.
    flux.write_all(b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n")
        .map_err(|e| e.to_string())?;
    let mut page = Vec::new();
    let _ = flux.read_to_end(&mut page);
    let premiere = String::from_utf8_lossy(&page).lines().next().unwrap_or("").to_string();
    if page.is_empty() {
        return Err("aucune réponse de la page à travers le tunnel".to_string());
    }
    Ok(format!(
        "SOCKS5 → tunnel : « {premiere} » ({} octets reçus)",
        page.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La requête DNS doit être un vrai message : identifiant, une question, A/IN.
    #[test]
    fn la_requete_dns_est_bien_formee() {
        let requete = requete_dns(0x1234, "example.com");
        assert_eq!(&requete[0..2], &[0x12, 0x34]);
        assert_eq!(&requete[2..4], &[0x01, 0x00], "récursion demandée");
        assert_eq!(&requete[4..6], &[0x00, 0x01], "une question");
        assert_eq!(requete[12], 7, "longueur de « example »");
        assert_eq!(&requete[requete.len() - 4..], &[0x00, 0x01, 0x00, 0x01], "A/IN");
    }

    /// Réponse réelle de forme classique : l'adresse est extraite.
    #[test]
    fn une_reponse_dns_donne_l_adresse() {
        let mut reponse = Vec::new();
        reponse.extend_from_slice(&[0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]);
        // question : example.com A IN
        reponse.push(7);
        reponse.extend_from_slice(b"example");
        reponse.push(3);
        reponse.extend_from_slice(b"com");
        reponse.push(0);
        reponse.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
        // réponse : nom compressé, A, IN, ttl, 4 octets
        reponse.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x04]);
        reponse.extend_from_slice(&[93, 184, 216, 34]);
        assert_eq!(
            extraire_a(&reponse, 0x1234),
            Some(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)))
        );
    }

    /// Un nom inexistant (RCODE 3) ne rend pas d'adresse.
    #[test]
    fn un_nom_inexistant_ne_donne_rien() {
        let mut reponse = vec![0x12, 0x34, 0x81, 0x83, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        reponse.push(7);
        reponse.extend_from_slice(b"nexiste");
        reponse.push(3);
        reponse.extend_from_slice(b"com");
        reponse.push(0);
        reponse.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
        assert_eq!(extraire_a(&reponse, 0x1234), None);
    }

    /// L'identifiant doit correspondre : sinon on lirait la réponse d'une autre demande.
    #[test]
    fn une_reponse_d_un_autre_identifiant_est_ignoree() {
        let mut reponse = vec![0x12, 0x34, 0x81, 0x80, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 0];
        reponse.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 0x3c, 0x00, 0x04, 1, 2, 3, 4]);
        assert_eq!(extraire_a(&reponse, 0x9999), None);
    }
}
