//! Le réseau tel que Windows le voit : y a-t-il un **tunnel** (VPN) en marche ?
//!
//! Sert au portier « tunnel obligatoire » (réglage) : quand il est allumé et qu'aucun tunnel n'est
//! monté, Trame **refuse** la navigation vers un site et affiche une page qui explique, plutôt que
//! de laisser passer le trafic en clair. Choix de Clément, 2026-11-27.
//!
//! Deux étages, volontairement séparés :
//!   - `tunnel_parmi` est une fonction **pure**, testée : c'est elle qui décide ce qu'est un tunnel ;
//!   - `cartes` parle à Windows (`GetAdaptersAddresses`) et ne fait que décrire.

/// Ce qu'on retient d'une carte réseau pour décider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Carte {
    pub description: String,
    /// `IfType` de Windows : 131 = tunnel, 23 = PPP (VPN RAS, mais aussi PPPoE).
    pub type_if: u32,
    pub monte: bool,
}

/// `IF_TYPE_TUNNEL` : le type que Windows donne aux cartes de tunnel (WireGuard, TAP, Wintun…).
const TYPE_TUNNEL: u32 = 131;

/// Mots des tunnels connus. On ne se fie pas au seul type : des pilotes de tunnel s'annoncent
/// parfois en « Ethernet » (constaté avec certains clients VPN), alors que leur description, elle,
/// ne trompe pas.
const NOMS: &[&str] = &[
    "wireguard",
    "wintun",
    "tap-windows",
    "tap adapter",
    "openvpn",
    "tailscale",
    "zerotier",
    "nordlynx",
    "proton",
    "vpn",
];

/// Le tunnel **monté**, s'il y en a un : sa description. `None` : aucun tunnel.
///
/// Fonction pure, testée : c'est ici, et nulle part ailleurs, qu'on décide ce qui compte comme
/// tunnel. Être strict est le bon sens de l'erreur — un faux négatif bloque la navigation (visible,
/// corrigeable), un faux positif laisserait passer le trafic (silencieux).
pub fn tunnel_parmi(cartes: &[Carte]) -> Option<String> {
    cartes
        .iter()
        .find(|c| {
            c.monte
                && (c.type_if == TYPE_TUNNEL || {
                    let description = c.description.to_lowercase();
                    NOMS.iter().any(|nom| description.contains(nom))
                })
        })
        .map(|c| c.description.clone())
}

/// Les cartes réseau, telles que Windows les décrit (montées ou non).
#[cfg(windows)]
pub fn cartes() -> Vec<Carte> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_INCLUDE_ALL_INTERFACES, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Networking::WinSock::AF_UNSPEC;

    // ERROR_BUFFER_OVERFLOW : le tampon était trop petit, `utilise` dit la taille voulue.
    const TROP_PETIT: u32 = 111;

    let mut taille: u32 = 16 * 1024;
    let mut tampon: Vec<u8> = Vec::new();
    for _ in 0..4 {
        tampon = vec![0u8; taille as usize];
        let mut utilise = taille;
        let code = unsafe {
            GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                GAA_FLAG_INCLUDE_ALL_INTERFACES,
                None,
                Some(tampon.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut utilise,
            )
        };
        if code == TROP_PETIT {
            taille = utilise.max(1024);
            continue;
        }
        if code != 0 {
            crate::alerte!("reseau : enumeration des cartes impossible (code {code})");
            return Vec::new();
        }
        break;
    }

    let mut sortie = Vec::new();
    let mut courant = tampon.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !courant.is_null() {
        // Le pointeur vient de Windows et la liste est valide tant que `tampon` vit.
        let carte = unsafe { &*courant };
        sortie.push(Carte {
            description: texte(carte.Description),
            type_if: carte.IfType,
            monte: carte.OperStatus == IfOperStatusUp,
        });
        courant = carte.Next;
    }
    sortie
}

#[cfg(not(windows))]
pub fn cartes() -> Vec<Carte> {
    Vec::new()
}

/// La description d'une carte (chaîne large Windows, terminée par zéro).
#[cfg(windows)]
fn texte(brut: windows::core::PWSTR) -> String {
    if brut.is_null() {
        return String::new();
    }
    let mut longueur = 0usize;
    // Borné : une description de carte ne fait pas 4 000 caractères.
    while longueur < 4096 && unsafe { *brut.0.add(longueur) } != 0 {
        longueur += 1;
    }
    unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(brut.0, longueur)) }
}

/// Le tunnel monté, s'il y en a un.
pub fn tunnel_actif() -> Option<String> {
    tunnel_parmi(&cartes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carte(description: &str, type_if: u32, monte: bool) -> Carte {
        Carte { description: description.into(), type_if, monte }
    }

    /// Le cas de tous les jours : une connexion ordinaire n'est pas un tunnel.
    #[test]
    fn une_connexion_ordinaire_n_est_pas_un_tunnel() {
        assert_eq!(tunnel_parmi(&[]), None);
        assert_eq!(tunnel_parmi(&[carte("Realtek PCIe GbE Family Controller", 6, true)]), None);
        assert_eq!(tunnel_parmi(&[carte("Intel(R) Wi-Fi 6 AX201", 71, true)]), None);
    }

    /// Un tunnel monté est reconnu, par son type ou par son nom.
    #[test]
    fn un_tunnel_monte_est_reconnu() {
        let par_type = carte("Une carte quelconque", TYPE_TUNNEL, true);
        assert_eq!(tunnel_parmi(&[par_type.clone()]), Some("Une carte quelconque".into()));

        for nom in ["WireGuard Tunnel: wg0", "Wintun Userspace Tunnel", "TAP-Windows Adapter V9", "OpenVPN Data Channel Offload", "VPN Maison"] {
            assert!(tunnel_parmi(&[carte(nom, 6, true)]).is_some(), "« {nom} » doit compter comme un tunnel");
        }
    }

    /// Un tunnel qui n'est PAS monté ne protège rien : il ne compte pas.
    #[test]
    fn un_tunnel_eteint_ne_compte_pas() {
        assert_eq!(tunnel_parmi(&[carte("WireGuard Tunnel: wg0", TYPE_TUNNEL, false)]), None);
    }

    /// Le PPP seul ne suffit pas : une connexion PPPoE (fibre de l'opérateur) s'annonce en PPP et
    /// n'est pas un tunnel. En revanche une carte PPP dont le nom dit « VPN » en est un.
    #[test]
    fn le_ppp_seul_ne_fait_pas_un_tunnel() {
        assert_eq!(tunnel_parmi(&[carte("WAN Miniport (PPPOE)", 23, true)]), None);
        assert!(tunnel_parmi(&[carte("VPN de la maison", 23, true)]).is_some());
    }
}
