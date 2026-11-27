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

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use smoltcp::iface::{Config as ConfIface, Interface, SocketHandle, SocketSet};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{tcp, udp};
use smoltcp::time::Instant as InstantSmol;
use smoltcp::wire::{HardwareAddress, IpAddress, IpCidr, IpEndpoint};


/// Le mandataire tourne tant que ce jeton n'est pas levé.
pub struct Mandataire {
    port: u16,
    arret: Arc<AtomicBool>,
    etat: Arc<EtatTunnel>,
}

/// Ce que le fil du mandataire dit de lui-même à l'application.
#[derive(Default)]
struct EtatTunnel {
    /// Le tunnel tourne et les pages passent.
    en_service: AtomicBool,
    /// Pourquoi il ne tourne plus (pendant une reprise).
    erreur: std::sync::Mutex<Option<String>>,
    /// Le tunnel a tourné au moins une fois (sinon : il n'a jamais pu monter).
    deja_servi: AtomicBool,
}

/// Un port local où personne n'écoute : le proxy du moteur quand le tunnel est exigé mais
/// qu'aucune configuration n'a pu être lue. Toute connexion y est refusée sur-le-champ, donc rien
/// ne sort (ni les pages, ni les extensions, ni le moteur lui-même).
///
/// Choisi UNE fois par session : toutes les webviews d'un même environnement WebView2 doivent
/// recevoir exactement les mêmes arguments, sinon la création échoue (0x8007139F).
pub fn port_ferme() -> u16 {
    static PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
    *PORT.get_or_init(|| {
        TcpListener::bind("127.0.0.1:0")
            .and_then(|e| e.local_addr())
            .map(|a| a.port())
            .unwrap_or(9)
    })
}

impl EtatTunnel {
    fn noter(&self, en_service: bool, erreur: Option<String>) {
        self.en_service.store(en_service, Ordering::Relaxed);
        if en_service {
            self.deja_servi.store(true, Ordering::Relaxed);
        }
        if let Ok(mut e) = self.erreur.lock() {
            *e = erreur;
        }
    }
}

impl Mandataire {
    /// L'adresse à donner à WebView2 : `socks5://127.0.0.1:<port>`.
    pub fn adresse(&self) -> String {
        format!("socks5://127.0.0.1:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Le tunnel tourne-t-il en ce moment ? `false` pendant une reprise après coupure : la
    /// pastille passe alors au rouge (avant la 0.4.94, elle restait verte sur un tunnel mort).
    pub fn en_service(&self) -> bool {
        self.etat.en_service.load(Ordering::Relaxed)
    }

    /// Le tunnel a-t-il déjà tourné ? `false` : il n'a jamais pu monter depuis le démarrage.
    pub fn a_deja_servi(&self) -> bool {
        self.etat.deja_servi.load(Ordering::Relaxed)
    }

    /// La raison de la dernière coupure, tant que le tunnel n'est pas revenu.
    pub fn erreur(&self) -> Option<String> {
        self.etat.erreur.lock().ok().and_then(|e| e.clone())
    }
}

impl Drop for Mandataire {
    fn drop(&mut self) {
        self.arret.store(true, Ordering::Relaxed);
    }
}

/// Démarre le mandataire avec une configuration **WireGuard** — le point d'entrée historique, qui
/// reste tant que l'application ne choisit pas elle-même son moteur (OpenVPN passera par
/// [`demarrer_tunnel`] avec sa propre préparation).
pub fn demarrer(config: crate::tunnel::Config) -> Result<Mandataire, String> {
    demarrer_tunnel(move || {
        let session = crate::vpn::connecter(&config, std::time::Duration::from_secs(10))?;
        let parametres = crate::moteur::Parametres::depuis_wireguard(&config);
        Ok((Box::new(session) as Box<dyn crate::moteur::Tunnel>, parametres))
    })
}

/// Démarre le mandataire : ouverture du tunnel (par `preparer`), pile, écoute SOCKS5.
///
/// Rend la main **une fois le tunnel établi et l'écoute ouverte** : Trame peut alors donner son
/// adresse à WebView2. Une erreur ici veut dire « pas de tunnel », et Trame refuse de naviguer.
///
/// `preparer` est appelé **dans le fil du mandataire** : il peut bloquer (poignée de main
/// WireGuard, négociation TLS d'OpenVPN) sans figer la fenêtre. Il rend le moteur et ce qu'il faut
/// à la pile — l'adresse dans le tunnel, le DNS du fournisseur, le MTU.
pub fn demarrer_tunnel<F>(preparer: F) -> Result<Mandataire, String>
where
    F: Fn() -> Result<(Box<dyn crate::moteur::Tunnel>, crate::moteur::Parametres), String> + Send + 'static,
{
    let ecoute = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("écoute SOCKS5 impossible : {e}"))?;
    let port = ecoute.local_addr().map_err(|e| e.to_string())?.port();
    ecoute.set_nonblocking(true).map_err(|e| e.to_string())?;

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let arret = Arc::new(AtomicBool::new(false));
    let jeton = arret.clone();
    let etat = Arc::new(EtatTunnel::default());
    let etat_fil = etat.clone();

    std::thread::Builder::new()
        .name("trame-mandataire".into())
        .spawn(move || {
            // La préparation rend le détail de l'échec : c'est cette phrase que l'interface
            // affichera quand rien ne monte (0.4.86).
            // Un premier montage raté ne fait plus renoncer (0.4.96) : avant, Trame démarrait alors
            // SANS proxy, et Bitwarden comme le moteur Edge sortaient en direct pendant que les
            // pages restaient bloquées (mesuré au journal réseau du moteur). Désormais l'écoute
            // reste ouverte — le moteur passe par elle, donc rien ne sort — et le fil réessaie.
            let mut courant = match preparer() {
                Ok(pret) => {
                    etat_fil.noter(true, None);
                    Some(pret)
                }
                Err(e) => {
                    crate::alerte!("mandataire : démarrage impossible ({e}), nouvel essai dans 2 s");
                    etat_fil.noter(false, Some(e));
                    None
                }
            };
            let _ = tx.send(Ok(()));
            // Le tunnel peut tomber (réseau perdu, veille, serveur relancé). Avant la 0.4.94, le
            // fil s'arrêtait là et plus aucune page ne sortait jusqu'au redémarrage de Trame. On
            // garde l'écoute — son port est inscrit dans WebView2 — et on remonte le tunnel.
            let mut attente_ms = 2_000u64;
            while !jeton.load(Ordering::Relaxed) {
                if let Some((tunnel, parametres)) = courant.take() {
                    crate::info!("mandataire : tunnel établi avec {}", tunnel.etiquette());
                    etat_fil.noter(true, None);
                    match boucle(tunnel, &ecoute, parametres, &jeton) {
                        Ok(()) => break,
                        Err(e) => {
                            crate::alerte!("mandataire : tunnel coupé ({e}), reprise dans 2 s");
                            etat_fil.noter(false, Some(e));
                            attente_ms = 2_000;
                        }
                    }
                }
                // Pendant la coupure : les connexions du navigateur sont refusées tout de suite
                // (une page dit « impossible » au lieu de tourner dans le vide), rien ne sort.
                let fin = crate::log::maintenant_ms() + attente_ms;
                while crate::log::maintenant_ms() < fin && !jeton.load(Ordering::Relaxed) {
                    while let Ok((flux, _)) = ecoute.accept() {
                        drop(flux);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                if jeton.load(Ordering::Relaxed) {
                    break;
                }
                match preparer() {
                    Ok(pret) => {
                        crate::info!("mandataire : tunnel rétabli");
                        courant = Some(pret);
                    }
                    Err(e) => {
                        crate::info!("mandataire : reprise ratée ({e}), nouvel essai dans {} s", (attente_ms * 2).min(30_000) / 1000);
                        etat_fil.noter(false, Some(e));
                        attente_ms = (attente_ms * 2).min(30_000);
                    }
                }
            }
            etat_fil.noter(false, None);
        })
        .map_err(|e| format!("fil du mandataire impossible : {e}"))?;

    match rx.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(Ok(())) => Ok(Mandataire { port, arret, etat }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("le tunnel n'a pas répondu en 60 s".to_string()),
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

// ------------------------------------------------------------------ ports locaux et identifiants DNS

/// Distribue les ports locaux de la pile, un par socket vivant.
///
/// Défaut corrigé en 0.4.93 : toutes les connexions TCP partaient du port 40000, et les sockets
/// DNS d'un même tour de boucle tombaient sur le même port. Deux connexions vers le même serveur
/// (Google en ouvre plusieurs) avaient alors la même identité : les réponses allaient à la
/// mauvaise, et la page restait en « SynSent » ou sans DNS.
struct Ports {
    suivant: u16,
    pris: HashSet<u16>,
}

const PORT_MIN: u16 = 49152;

impl Ports {
    fn nouveau(graine: u64) -> Ports {
        Ports { suivant: PORT_MIN + (graine % 16000) as u16, pris: HashSet::new() }
    }

    fn prendre(&mut self) -> Option<u16> {
        for _ in 0..16384 {
            let port = self.suivant;
            self.suivant = if self.suivant == u16::MAX { PORT_MIN } else { self.suivant + 1 };
            if self.pris.insert(port) {
                return Some(port);
            }
        }
        None
    }

    fn rendre(&mut self, port: u16) {
        self.pris.remove(&port);
    }
}

/// Ce que tous les clients partagent : ports, identifiants DNS, cache, sockets en fermeture.
struct Commun {
    ports: Ports,
    prochain_id: u16,
    /// Noms déjà résolus, gardés une minute (une page demande souvent le même nom plusieurs fois).
    cache: HashMap<String, (IpAddr, u64)>,
    /// Sockets TCP à qui l'on a demandé de se fermer : retirés une fois fermés (ou après 10 s).
    fermants: Vec<(SocketHandle, u16, u64)>,
}

const DUREE_CACHE_MS: u64 = 60_000;

impl Commun {
    fn identifiant(&mut self) -> u16 {
        self.prochain_id = self.prochain_id.wrapping_add(1).max(1);
        self.prochain_id
    }

    /// Ferme proprement un socket TCP et le range parmi ceux à retirer plus tard.
    fn fermer_tcp(&mut self, sockets: &mut SocketSet<'_>, handle: SocketHandle, port: u16, maintenant: u64, net: bool) {
        let socket = sockets.get_mut::<tcp::Socket>(handle);
        if net {
            socket.abort();
        } else {
            socket.close();
        }
        self.fermants.push((handle, port, maintenant + 10_000));
    }

    fn retirer_udp(&mut self, sockets: &mut SocketSet<'_>, handle: SocketHandle, port: u16) {
        sockets.remove(handle);
        self.ports.rendre(port);
    }

    /// Retire les sockets fermés ; coupe net ceux qui traînent depuis plus de 10 s.
    fn ranger(&mut self, sockets: &mut SocketSet<'_>, maintenant: u64) {
        let mut i = 0;
        while i < self.fermants.len() {
            let (handle, port, limite) = self.fermants[i];
            let socket = sockets.get_mut::<tcp::Socket>(handle);
            match socket.state() {
                tcp::State::Closed | tcp::State::TimeWait => {
                    sockets.remove(handle);
                    self.ports.rendre(port);
                    self.fermants.swap_remove(i);
                    continue;
                }
                _ if maintenant > limite => {
                    // Le RST part au prochain passage de la pile ; le socket sera alors fermé.
                    socket.abort();
                    self.fermants[i].2 = u64::MAX;
                }
                _ => {}
            }
            i += 1;
        }
        if self.cache.len() > 2000 {
            self.cache.retain(|_, (_, expire)| *expire > maintenant);
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

/// Décrit une réponse DNS sans adresse (code, compteurs, troncature) pour le journal.
fn decrire_reponse(reponse: &[u8]) -> String {
    if reponse.len() < 12 {
        return format!("{} octet(s)", reponse.len());
    }
    format!(
        "{} octets, rcode {}, {} réponse(s), tronquée {}",
        reponse.len(),
        reponse[3] & 0x0f,
        u16::from_be_bytes([reponse[6], reponse[7]]),
        reponse[2] & 0x02 != 0
    )
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
    /// On attend la résolution DNS du nom, sur un socket propre à ce client.
    Resolution { socket: SocketHandle, port: u16, id: u16, envoye_le: u64, essais: u8, nom: String },
    Connexion { socket: SocketHandle, port: u16, debut: u64 },
    Relais { socket: SocketHandle, port: u16, fin_envoyee: bool },
    /// Plus rien à faire : le client sera retiré.
    Fini,
}

struct Client {
    flux: TcpStream,
    etat: Etat,
    /// Ce que le navigateur a envoyé et qui doit partir dans le tunnel.
    tampon: Vec<u8>,
    /// Ce que le tunnel a rendu et que le navigateur n'a pas encore pris.
    vers_navigateur: Vec<u8>,
    /// Le navigateur a fermé son côté.
    navigateur_fini: bool,
    hote: Option<(IpAddr, u16)>,
    /// Le nom demandé, pour le journal.
    nom: String,
    /// Dernier instant où des octets ont circulé (sonde des connexions figées).
    dernier_progres: u64,
    /// La sonde a déjà signalé ce client comme figé.
    signale: bool,
    /// Octets reçus du serveur, et comment la connexion s'est terminée (port, raison) — sonde.
    octets_recus: u64,
    fin: Option<(u16, String)>,
    debut: u64,
}

/// Garde les derniers en-têtes TCP de chaque connexion (port local → lignes), pour la sonde
/// `TRAME_SONDE_RELAIS`. Une connexion figée se lit alors paquet par paquet : qui attend qui.
fn tracer(traces: &mut HashMap<u16, VecDeque<String>>, paquet: &[u8], entrant: bool, maintenant: u64) {
    if paquet.len() < 20 || paquet[0] >> 4 != 4 || paquet[9] != 6 {
        return;
    }
    let ihl = ((paquet[0] & 0x0f) as usize) * 4;
    let total = u16::from_be_bytes([paquet[2], paquet[3]]) as usize;
    let frag = u16::from_be_bytes([paquet[6], paquet[7]]);
    if paquet.len() < ihl + 20 {
        return;
    }
    let t = &paquet[ihl..];
    let source = u16::from_be_bytes([t[0], t[1]]);
    let dest = u16::from_be_bytes([t[2], t[3]]);
    let seq = u32::from_be_bytes([t[4], t[5], t[6], t[7]]);
    let ack = u32::from_be_bytes([t[8], t[9], t[10], t[11]]);
    let decalage = ((t[12] >> 4) as usize) * 4;
    let drapeaux = t[13];
    let fenetre = u16::from_be_bytes([t[14], t[15]]);
    let charge = total.saturating_sub(ihl + decalage);
    let local = if entrant { dest } else { source };
    let noms: String = [(0x02, 'S'), (0x10, 'A'), (0x08, 'P'), (0x01, 'F'), (0x04, 'R')]
        .iter()
        .filter(|(b, _)| drapeaux & b != 0)
        .map(|(_, c)| *c)
        .collect();
    let file = traces.entry(local).or_default();
    if !entrant && drapeaux & 0x02 != 0 && drapeaux & 0x10 == 0 {
        file.clear(); // nouveau SYN : le port sert une nouvelle connexion
    }
    let fragment = if frag & 0x3fff != 0 { format!(" FRAG({frag:#06x})") } else { String::new() };
    file.push_back(format!(
        "{} {} seq={seq} ack={ack} {noms} len={charge} win={fenetre} ip={total}{fragment}",
        maintenant % 100_000,
        if entrant { "<-" } else { "->" }
    ));
    while file.len() > 60 {
        file.pop_front();
    }
}

/// Au-delà, on cesse de lire le navigateur tant que le tunnel n'a pas absorbé.
const RETENUE: usize = 256 * 1024;
const SYN_MAX_MS: u64 = 20_000;

const REFUS_HOTE: [u8; 10] = [0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
const REFUS_CONNEXION: [u8; 10] = [0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
const ACCORD: [u8; 10] = [0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0];

fn boucle(
    mut tunnel: Box<dyn crate::moteur::Tunnel>,
    ecoute: &TcpListener,
    parametres: crate::moteur::Parametres,
    arret: &AtomicBool,
) -> Result<(), String> {
    let mtu = tunnel.mtu().or(parametres.mtu).unwrap_or(1420);
    let mut pile = Pile {
        entrants: VecDeque::new(),
        sortants: VecDeque::new(),
        mtu,
    };

    let graine = crate::log::maintenant_ms();
    let mut conf = ConfIface::new(HardwareAddress::Ip);
    conf.random_seed = graine;
    let mut iface = Interface::new(conf, &mut pile, instant(0));
    let v4 = parametres.adresses.iter().find_map(|(ip, prefixe)| match ip {
        IpAddr::V4(a) => Some((*a, *prefixe)),
        IpAddr::V6(_) => None,
    });
    let v6 = parametres.adresses.iter().find_map(|(ip, prefixe)| match ip {
        IpAddr::V6(a) => Some((*a, *prefixe)),
        IpAddr::V4(_) => None,
    });
    iface.update_ip_addrs(|adresses| {
        if let Some((ip, prefixe)) = v4 {
            let _ = adresses.push(IpCidr::new(IpAddress::Ipv4(ip.into()), prefixe));
        }
        if let Some((ip, prefixe)) = v6 {
            let _ = adresses.push(IpCidr::new(IpAddress::Ipv6(ip.into()), prefixe));
        }
    });
    // Les deux moteurs annoncent une route par défaut : c'est le pair qui achemine le reste.
    if let Some((ip, _)) = v4 {
        let _ = iface.routes_mut().add_default_ipv4_route(ip.into());
    }
    if let Some((ip, _)) = v6 {
        let _ = iface.routes_mut().add_default_ipv6_route(ip.into());
    }

    let mut sockets = SocketSet::new(Vec::new());
    let dns: Option<IpAddr> = parametres.dns;
    let mut commun = Commun {
        ports: Ports::nouveau(graine),
        prochain_id: (graine % 60000) as u16,
        cache: HashMap::new(),
        fermants: Vec::new(),
    };

    let mut clients: Vec<Client> = Vec::new();
    crate::info!(
        "mandataire : écoute SOCKS5 sur 127.0.0.1:{} (MTU {}, DNS {})",
        ecoute.local_addr().map(|a| a.port()).unwrap_or(0),
        mtu,
        dns.map(|d| d.to_string()).unwrap_or_else(|| "aucun".into())
    );

    // Sonde de mise au point (jamais en usage normal) : `TRAME_SONDE_COUPURE_S=N` fait tomber le
    // tunnel UNE fois, N secondes après son montage, pour éprouver la reprise
    // (`outils/eprouver-fuites.mjs`).
    static COUPURE_FAITE: AtomicBool = AtomicBool::new(false);
    // Sonde `TRAME_SONDE_RELAIS` : trace des en-têtes TCP et signalement des connexions figées.
    let sonde = std::env::var("TRAME_SONDE_RELAIS").is_ok();
    let mut traces: HashMap<u16, VecDeque<String>> = HashMap::new();
    let coupure_a = std::env::var("TRAME_SONDE_COUPURE_S")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|_| !COUPURE_FAITE.load(Ordering::Relaxed))
        .map(|s| crate::log::maintenant_ms() + s * 1000);

    while !arret.load(Ordering::Relaxed) {
        let maintenant = crate::log::maintenant_ms();
        let temps = instant(maintenant);
        let mut actif = false;
        if coupure_a.is_some_and(|t| maintenant > t) {
            COUPURE_FAITE.store(true, Ordering::Relaxed);
            return Err("coupure simulée (TRAME_SONDE_COUPURE_S)".to_string());
        }

        // 1. Le tunnel : ce qui arrive, ce qui part.
        tunnel.pomper()?;
        while let Some(paquet) = tunnel.prendre_paquet() {
            if sonde {
                tracer(&mut traces, &paquet, true, maintenant);
            }
            pile.entrants.push_back(paquet);
            actif = true;
        }

        // 2. Les connexions du navigateur.
        loop {
            match ecoute.accept() {
                Ok((flux, _)) => {
                    let _ = flux.set_nonblocking(true);
                    let _ = flux.set_nodelay(true);
                    clients.push(Client {
                        flux,
                        etat: Etat::Salutation,
                        tampon: Vec::new(),
                        vers_navigateur: Vec::new(),
                        navigateur_fini: false,
                        hote: None,
                        nom: String::new(),
                        dernier_progres: maintenant,
                        signale: false,
                        octets_recus: 0,
                        fin: None,
                        debut: maintenant,
                    });
                    actif = true;
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }

        // 3. La pile travaille (elle consomme les entrants et produit les sortants).
        iface.poll(temps, &mut pile, &mut sockets);

        // 4. Chaque client avance d'un cran ; ceux qui ont fini rendent leurs sockets.
        {
            let contexte = iface.context();
            for client in clients.iter_mut() {
                if client.avancer(&mut sockets, &mut commun, dns, contexte, maintenant) {
                    actif = true;
                }
            }
        }
        if sonde {
            signaler_figes(&mut clients, &mut sockets, &traces, maintenant);
            for client in clients.iter().filter(|c| matches!(c.etat, Etat::Fini)) {
                if let Some((port, raison)) = &client.fin {
                    crate::info!(
                        "sonde relais : fin « {} » (port {port}) après {} ms, {} o reçus — {raison}, navigateur fini {}",
                        client.nom,
                        maintenant - client.debut,
                        client.octets_recus,
                        client.navigateur_fini
                    );
                    if client.octets_recus < 20_000 {
                        for ligne in traces.get(port).into_iter().flatten().rev().take(14).collect::<Vec<_>>().into_iter().rev() {
                            crate::info!("sonde relais :   {port} {ligne}");
                        }
                    }
                }
            }
        }
        clients.retain(|client| !matches!(client.etat, Etat::Fini));
        commun.ranger(&mut sockets, maintenant);

        // 5. Ce que la pile et les clients ont produit part dans le tunnel.
        iface.poll(temps, &mut pile, &mut sockets);
        while let Some(paquet) = pile.sortants.pop_front() {
            if sonde {
                tracer(&mut traces, &paquet, false, maintenant);
            }
            tunnel.envoyer_paquet(&paquet)?;
            actif = true;
        }

        // Au repos, on cède la main ; en plein transfert, on enchaîne (débit d'une vidéo).
        if actif {
            std::thread::yield_now();
        } else {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    crate::info!("mandataire : arrêt demandé");
    Ok(())
}

/// Sonde : un client en relais sans progrès depuis 8 s est décrit au journal, avec ses paquets.
fn signaler_figes(clients: &mut [Client], sockets: &mut SocketSet<'_>, traces: &HashMap<u16, VecDeque<String>>, maintenant: u64) {
    for client in clients.iter_mut() {
        let Etat::Relais { socket, port, fin_envoyee } = client.etat else { continue };
        let fige = maintenant.saturating_sub(client.dernier_progres) > 8_000;
        if !fige {
            if client.signale {
                crate::info!("sonde relais : « {} » (port {port}) repart", client.nom);
                client.signale = false;
            }
            continue;
        }
        if client.signale {
            continue;
        }
        client.signale = true;
        let s = sockets.get_mut::<tcp::Socket>(socket);
        crate::info!(
            "sonde relais : « {} » (port {port}) FIGÉ depuis {} ms — tcp {:?}, à lire {} o, à envoyer {} o, \
             fenêtre locale {}, vers navigateur {} o, du navigateur {} o, navigateur fini {}, FIN envoyé {fin_envoyee}, \
             may_recv {}, can_recv {}, can_send {}",
            client.nom,
            maintenant - client.dernier_progres,
            s.state(),
            s.recv_queue(),
            s.send_queue(),
            s.recv_capacity() - s.recv_queue(),
            client.vers_navigateur.len(),
            client.tampon.len(),
            client.navigateur_fini,
            s.may_recv(),
            s.can_recv(),
            s.can_send()
        );
        if let Some(lignes) = traces.get(&port) {
            for ligne in lignes {
                crate::info!("sonde relais :   {port} {ligne}");
            }
        }
    }
}

fn instant(ms: u64) -> InstantSmol {
    InstantSmol::from_millis(ms as i64)
}

/// Ce que donne une lecture du navigateur.
enum Lecture {
    Rien,
    Lu,
    Fin,
}

impl Client {
    /// Fait avancer le client. Rend `true` s'il s'est passé quelque chose. En cas d'échec ou de
    /// fin, l'état passe à `Fini` et ses sockets sont rendus à `commun`.
    fn avancer(
        &mut self,
        sockets: &mut SocketSet<'_>,
        commun: &mut Commun,
        serveur_dns: Option<IpAddr>,
        contexte: &mut smoltcp::iface::Context,
        maintenant: u64,
    ) -> bool {
        match self.etape(sockets, commun, serveur_dns, contexte, maintenant) {
            Ok(actif) => actif,
            Err(()) => {
                self.liberer(sockets, commun, maintenant, true);
                true
            }
        }
    }

    /// Rend les sockets du client et le marque fini.
    fn liberer(&mut self, sockets: &mut SocketSet<'_>, commun: &mut Commun, maintenant: u64, net: bool) {
        match std::mem::replace(&mut self.etat, Etat::Fini) {
            Etat::Resolution { socket, port, .. } => commun.retirer_udp(sockets, socket, port),
            Etat::Connexion { socket, port, .. } => {
                self.fin = Some((port, format!("échec en connexion, tcp {:?}", sockets.get::<tcp::Socket>(socket).state())));
                commun.fermer_tcp(sockets, socket, port, maintenant, true)
            }
            Etat::Relais { socket, port, .. } => {
                self.fin = Some((port, format!("relais interrompu, tcp {:?}", sockets.get::<tcp::Socket>(socket).state())));
                commun.fermer_tcp(sockets, socket, port, maintenant, net)
            }
            _ => {}
        }
        let _ = self.flux.shutdown(std::net::Shutdown::Both);
    }

    fn etape(
        &mut self,
        sockets: &mut SocketSet<'_>,
        commun: &mut Commun,
        serveur_dns: Option<IpAddr>,
        contexte: &mut smoltcp::iface::Context,
        maintenant: u64,
    ) -> Result<bool, ()> {
        match self.etat {
            Etat::Fini => Ok(false),
            Etat::Salutation => {
                match self.lire()? {
                    Lecture::Rien => return Ok(false),
                    Lecture::Fin => return Err(()),
                    Lecture::Lu => {}
                }
                if self.tampon.len() < 2 {
                    return Ok(true);
                }
                let attendu = 2 + self.tampon[1] as usize;
                if self.tampon.len() < attendu {
                    return Ok(true);
                }
                self.flux.write_all(&[0x05, 0x00]).map_err(|_| ())?;
                self.tampon.drain(..attendu);
                self.etat = Etat::Requete;
                Ok(true)
            }
            Etat::Requete => {
                if let Lecture::Fin = self.lire()? {
                    return Err(());
                }
                if self.tampon.len() < 5 {
                    return Ok(false);
                }
                let atype = self.tampon[3];
                let taille = match atype {
                    0x01 => 4 + 4 + 2,
                    0x04 => 4 + 16 + 2,
                    0x03 => 5 + self.tampon[4] as usize + 2,
                    _ => return Err(()),
                };
                if self.tampon.len() < taille {
                    return Ok(false);
                }
                let port = u16::from_be_bytes([self.tampon[taille - 2], self.tampon[taille - 1]]);
                match atype {
                    0x01 => {
                        let ip = Ipv4Addr::new(self.tampon[4], self.tampon[5], self.tampon[6], self.tampon[7]);
                        self.hote = Some((IpAddr::V4(ip), port));
                        self.nom = ip.to_string();
                    }
                    0x04 => {
                        let mut octets = [0u8; 16];
                        octets.copy_from_slice(&self.tampon[4..20]);
                        let ip = Ipv6Addr::from(octets);
                        self.hote = Some((IpAddr::V6(ip), port));
                        self.nom = ip.to_string();
                    }
                    _ => {
                        let longueur = self.tampon[4] as usize;
                        let nom = String::from_utf8_lossy(&self.tampon[5..5 + longueur]).to_ascii_lowercase();
                        self.tampon.drain(..taille);
                        self.nom = nom.clone();
                        // Une adresse écrite en chiffres n'a pas besoin du DNS.
                        if let Ok(ip) = nom.parse::<IpAddr>() {
                            self.hote = Some((ip, port));
                            self.connecter(sockets, commun, contexte, maintenant)?;
                            return Ok(true);
                        }
                        if let Some((ip, expire)) = commun.cache.get(&nom) {
                            if *expire > maintenant {
                                self.hote = Some((*ip, port));
                                self.connecter(sockets, commun, contexte, maintenant)?;
                                return Ok(true);
                            }
                        }
                        let Some(serveur) = serveur_dns else {
                            let _ = self.flux.write_all(&REFUS_HOTE);
                            return Err(());
                        };
                        let rx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 4096]);
                        let tx = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 4], vec![0u8; 1024]);
                        let mut socket = udp::Socket::new(rx, tx);
                        let Some(port_local) = commun.ports.prendre() else {
                            let _ = self.flux.write_all(&REFUS_HOTE);
                            return Err(());
                        };
                        if socket.bind(port_local).is_err() {
                            commun.ports.rendre(port_local);
                            let _ = self.flux.write_all(&REFUS_HOTE);
                            return Err(());
                        }
                        let handle = sockets.add(socket);
                        let id = commun.identifiant();
                        let endpoint = IpEndpoint::new(ip_smoltcp(serveur), 53);
                        let envoi = sockets
                            .get_mut::<udp::Socket>(handle)
                            .send_slice(&requete_dns(id, &nom), udp::UdpMetadata::from(endpoint));
                        self.hote = Some((IpAddr::V4(Ipv4Addr::UNSPECIFIED), port));
                        self.etat = Etat::Resolution { socket: handle, port: port_local, id, envoye_le: maintenant, essais: 1, nom };
                        if envoi.is_err() {
                            let _ = self.flux.write_all(&REFUS_HOTE);
                            return Err(());
                        }
                        return Ok(true);
                    }
                }
                self.tampon.drain(..taille);
                self.connecter(sockets, commun, contexte, maintenant)?;
                Ok(true)
            }
            Etat::Resolution { socket, port: port_udp, id, envoye_le, essais, ref nom } => {
                let nom = nom.clone();
                let mut tampon = [0u8; 4096];
                // On lit tout ce qui est arrivé : une réponse d'un autre identifiant (ancienne
                // relance) est ignorée, pas prise pour un échec.
                loop {
                    let recu = sockets
                        .get_mut::<udp::Socket>(socket)
                        .recv_slice(&mut tampon)
                        .ok()
                        .map(|(n, _)| n);
                    let Some(n) = recu else { break };
                    if n < 12 || u16::from_be_bytes([tampon[0], tampon[1]]) != id {
                        continue;
                    }
                    commun.retirer_udp(sockets, socket, port_udp);
                    match extraire_a(&tampon[..n], id) {
                        Some(ip) => {
                            commun.cache.insert(nom.clone(), (ip, maintenant + DUREE_CACHE_MS));
                            if let Some((_, port)) = self.hote {
                                self.hote = Some((ip, port));
                            }
                            self.etat = Etat::Requete; // le socket UDP n'est plus à nous
                            self.connecter(sockets, commun, contexte, maintenant)?;
                            return Ok(true);
                        }
                        None => {
                            crate::info!(
                                "mandataire : « {nom} » introuvable (DNS du tunnel : {})",
                                decrire_reponse(&tampon[..n])
                            );
                            self.etat = Etat::Fini;
                            let _ = self.flux.write_all(&REFUS_HOTE);
                            let _ = self.flux.shutdown(std::net::Shutdown::Both);
                            return Ok(true);
                        }
                    }
                }
                if maintenant.saturating_sub(envoye_le) > 1_500 {
                    if essais >= 4 {
                        crate::info!("mandataire : le DNS du tunnel n'a pas répondu pour « {nom} »");
                        let _ = self.flux.write_all(&REFUS_HOTE);
                        return Err(());
                    }
                    let Some(serveur) = serveur_dns else { return Err(()) };
                    let endpoint = IpEndpoint::new(ip_smoltcp(serveur), 53);
                    let _ = sockets
                        .get_mut::<udp::Socket>(socket)
                        .send_slice(&requete_dns(id, &nom), udp::UdpMetadata::from(endpoint));
                    self.etat = Etat::Resolution { socket, port: port_udp, id, envoye_le: maintenant, essais: essais + 1, nom };
                    return Ok(true);
                }
                Ok(false)
            }
            Etat::Connexion { socket: handle, port, debut } => {
                let etat = sockets.get_mut::<tcp::Socket>(handle).state();
                match etat {
                    tcp::State::Established => {
                        self.flux.write_all(&ACCORD).map_err(|_| ())?;
                        self.etat = Etat::Relais { socket: handle, port, fin_envoyee: false };
                        Ok(true)
                    }
                    tcp::State::Closed => {
                        crate::info!("mandataire : « {} » a refusé la connexion", self.nom);
                        let _ = self.flux.write_all(&REFUS_CONNEXION);
                        Err(())
                    }
                    _ if maintenant.saturating_sub(debut) > SYN_MAX_MS => {
                        crate::info!("mandataire : « {} » ne répond pas (20 s sans poignée de main)", self.nom);
                        let _ = self.flux.write_all(&REFUS_HOTE);
                        Err(())
                    }
                    _ => {
                        // Le navigateur a pu abandonner pendant l'attente.
                        if let Lecture::Fin = self.lire()? {
                            return Err(());
                        }
                        Ok(false)
                    }
                }
            }
            Etat::Relais { socket: handle, port, fin_envoyee } => self.relayer(sockets, commun, handle, port, fin_envoyee, maintenant),
        }
    }

    /// Le relais, dans les deux sens, avec retenue : on ne prend au tunnel que ce que le
    /// navigateur peut absorber, et inversement (sinon des octets se perdaient).
    fn relayer(
        &mut self,
        sockets: &mut SocketSet<'_>,
        commun: &mut Commun,
        handle: SocketHandle,
        port: u16,
        fin_envoyee: bool,
        maintenant: u64,
    ) -> Result<bool, ()> {
        let mut actif = false;

        // Navigateur → tunnel.
        if !self.navigateur_fini && self.tampon.len() < RETENUE {
            match self.lire()? {
                Lecture::Lu => actif = true,
                Lecture::Fin => {
                    self.navigateur_fini = true;
                    actif = true;
                }
                Lecture::Rien => {}
            }
        }
        let socket = sockets.get_mut::<tcp::Socket>(handle);
        if !self.tampon.is_empty() && socket.can_send() {
            if let Ok(n) = socket.send_slice(&self.tampon) {
                if n > 0 {
                    self.tampon.drain(..n);
                    actif = true;
                }
            }
        }
        let mut fin_envoyee = fin_envoyee;
        if self.navigateur_fini && self.tampon.is_empty() && !fin_envoyee {
            socket.close();
            fin_envoyee = true;
        }

        // Tunnel → navigateur.
        loop {
            if !self.vers_navigateur.is_empty() {
                match self.flux.write(&self.vers_navigateur) {
                    Ok(0) => return Err(()),
                    Ok(n) => {
                        self.vers_navigateur.drain(..n);
                        actif = true;
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => return Err(()),
                }
                if !self.vers_navigateur.is_empty() {
                    break;
                }
            }
            if !socket.can_recv() {
                break;
            }
            let mut recu = [0u8; 65536];
            match socket.recv_slice(&mut recu) {
                Ok(n) if n > 0 => {
                    self.vers_navigateur.extend_from_slice(&recu[..n]);
                    self.octets_recus += n as u64;
                    actif = true;
                }
                _ => break,
            }
        }

        // Le serveur a tout envoyé et le navigateur a tout pris : fin propre.
        let serveur_fini = !socket.may_recv() && !matches!(socket.state(), tcp::State::SynSent | tcp::State::SynReceived);
        if serveur_fini && self.vers_navigateur.is_empty() {
            self.fin = Some((port, format!("le serveur a fini, tcp {:?}", socket.state())));
            self.etat = Etat::Fini;
            commun.fermer_tcp(sockets, handle, port, maintenant, false);
            let _ = self.flux.shutdown(std::net::Shutdown::Both);
            return Ok(true);
        }
        self.etat = Etat::Relais { socket: handle, port, fin_envoyee };
        if actif {
            self.dernier_progres = maintenant;
        }
        Ok(actif)
    }

    /// Lit ce que le navigateur a envoyé dans `tampon`.
    fn lire(&mut self) -> Result<Lecture, ()> {
        let mut recu = [0u8; 65536];
        match self.flux.read(&mut recu) {
            Ok(0) => Ok(Lecture::Fin),
            Ok(n) => {
                self.tampon.extend_from_slice(&recu[..n]);
                Ok(Lecture::Lu)
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => Ok(Lecture::Rien),
            Err(_) => Err(()),
        }
    }

    fn connecter(
        &mut self,
        sockets: &mut SocketSet<'_>,
        commun: &mut Commun,
        contexte: &mut smoltcp::iface::Context,
        maintenant: u64,
    ) -> Result<(), ()> {
        let Some((ip, port)) = self.hote else { return Err(()) };
        let Some(port_local) = commun.ports.prendre() else {
            let _ = self.flux.write_all(&REFUS_CONNEXION);
            return Err(());
        };
        let rx = tcp::SocketBuffer::new(vec![0u8; 256 * 1024]);
        let tx = tcp::SocketBuffer::new(vec![0u8; 64 * 1024]);
        let mut socket = tcp::Socket::new(rx, tx);
        socket.set_nagle_enabled(false);
        let endpoint = IpEndpoint::new(ip_smoltcp(ip), port);
        let handle = sockets.add(socket);
        if sockets.get_mut::<tcp::Socket>(handle).connect(contexte, endpoint, port_local).is_err() {
            sockets.remove(handle);
            commun.ports.rendre(port_local);
            crate::info!("mandataire : connexion impossible vers « {} » ({ip})", self.nom);
            let _ = self.flux.write_all(&REFUS_CONNEXION);
            return Err(());
        }
        self.etat = Etat::Connexion { socket: handle, port: port_local, debut: maintenant };
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
