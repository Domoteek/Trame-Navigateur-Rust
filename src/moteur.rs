//! Ce que le mandataire attend d'un moteur de tunnel — et rien de plus.
//!
//! Trame a deux moteurs : WireGuard ([`crate::vpn`], `boringtun`) et OpenVPN ([`crate::ovpn`],
//! OpenVPN 3 Core). Ils n'ont rien en commun sur le réseau, mais **la même forme** du côté de la
//! pile TCP/IP : on leur donne des paquets IP, ils rendent des paquets IP. Le mandataire SOCKS5
//! ([`crate::mandataire`]) ne connaît donc que ce trait.
//!
//! Ce qui les distingue est rangé dans [`Parametres`] : l'adresse que la pile porte dans le tunnel,
//! le DNS du fournisseur et le MTU. WireGuard les lit dans le `.conf` ; OpenVPN les **reçoit du
//! serveur** au moment où la session monte.

use std::net::IpAddr;

/// Un moteur de tunnel, vu par la pile.
pub trait Tunnel: Send {
    /// Comment le nommer dans le journal et à l'écran : `193.32.126.68:51820`,
    /// `udp 193.32.126.68:1194`.
    fn etiquette(&self) -> String;

    /// MTU à annoncer à la pile (`None` : celle par défaut du moteur).
    fn mtu(&self) -> Option<usize>;

    /// Fait avancer le moteur : lire ce qui attend sur le réseau, entretenir les minuteries.
    /// OpenVPN n'a rien à faire ici (ses paquets arrivent par un rappel), WireGuard y vide son
    /// socket UDP.
    fn pomper(&mut self) -> Result<(), String>;

    /// Le prochain paquet IP sorti du tunnel, s'il y en a un de prêt.
    fn prendre_paquet(&mut self) -> Option<Vec<u8>>;

    /// Fait entrer dans le tunnel un paquet IP produit par la pile.
    fn envoyer_paquet(&mut self, paquet: &[u8]) -> Result<(), String>;
}

/// Ce qu'il faut à la pile pour travailler : l'adresse qu'elle porte dans le tunnel (avec son
/// préfixe), le résolveur DNS du fournisseur, et le MTU.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parametres {
    pub adresses: Vec<(IpAddr, u8)>,
    pub dns: Option<IpAddr>,
    pub mtu: Option<usize>,
}

impl Parametres {
    /// Les paramètres d'une configuration WireGuard : les adresses du `.conf`, son `DNS =`, son
    /// `MTU =`.
    pub fn depuis_wireguard(config: &crate::tunnel::Config) -> Parametres {
        let adresses = config
            .adresses
            .iter()
            .filter_map(|a| {
                let (ip, prefixe) = match a.split_once('/') {
                    Some((ip, p)) => (ip, p.parse::<u8>().unwrap_or(32)),
                    None => (a.as_str(), 32),
                };
                ip.parse::<IpAddr>().ok().map(|ip| (ip, prefixe))
            })
            .collect();
        Parametres {
            adresses,
            dns: config.dns.first().and_then(|d| d.parse().ok()),
            mtu: config.mtu.map(|m| m as usize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wireguard(adresses: &[&str], dns: &[&str], mtu: Option<u16>) -> crate::tunnel::Config {
        crate::tunnel::Config {
            nom: "essai".into(),
            adresses: adresses.iter().map(|a| a.to_string()).collect(),
            dns: dns.iter().map(|d| d.to_string()).collect(),
            mtu,
            port_ecoute: None,
            endpoint: "193.32.126.68:51820".into(),
            cle_paire: "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=".into(),
            cle_partagee: None,
            allowed_ips: vec!["0.0.0.0/0".into()],
            keepalive: None,
            cle_privee: "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=".into(),
        }
    }

    /// WireGuard : le préfixe vient du `.conf`, IPv4 et IPv6 cohabitent.
    #[test]
    fn les_parametres_viennent_de_la_configuration_wireguard() {
        let p = Parametres::depuis_wireguard(&wireguard(
            &["10.66.163.176/32", "fc00:bbbb:bbbb:bb01::4:300f/128"],
            &["100.64.0.63"],
            Some(1420),
        ));
        assert_eq!(p.adresses.len(), 2);
        assert_eq!(p.adresses[0], ("10.66.163.176".parse().unwrap(), 32));
        assert_eq!(p.adresses[1], ("fc00:bbbb:bbbb:bb01::4:300f".parse().unwrap(), 128));
        assert_eq!(p.dns, Some("100.64.0.63".parse().unwrap()));
        assert_eq!(p.mtu, Some(1420));
    }

    /// Une adresse sans préfixe est traitée comme un hôte seul, et un `.conf` sans DNS ni MTU ne
    /// produit pas de valeur inventée.
    #[test]
    fn une_configuration_sans_prefixe_ni_dns_reste_honnete() {
        let p = Parametres::depuis_wireguard(&wireguard(&["10.5.0.2"], &[], None));
        assert_eq!(p.adresses, vec![("10.5.0.2".parse().unwrap(), 32)]);
        assert_eq!(p.dns, None);
        assert_eq!(p.mtu, None);
    }
}
