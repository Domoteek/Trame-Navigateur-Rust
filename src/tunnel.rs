//! Le tunnel WireGuard de Trame â€” **la configuration**, et la clÃ© privÃ©e protÃ©gÃ©e.
//!
//! DÃ©cision de ClÃ©ment (2026-11-27) : le VPN doit Ãªtre **intÃ©grÃ© Ã  Trame**, et Trame s'installe
//! **par utilisateur** (aucun droit administrateur). D'oÃ¹ l'architecture retenue, en deux temps :
//!
//!   1. **ce module** : lire une configuration WireGuard (`.conf`), la valider, et garder la **clÃ©
//!      privÃ©e chiffrÃ©e par Windows** (DPAPI, compte de l'utilisateur) â€” jamais en clair dans un
//!      fichier de rÃ©glages ;
//!   2. le tunnel lui-mÃªme (WireGuard en espace utilisateur, puis un proxy local que Trame
//!      traverse) : il viendra ensuite, et c'est ce qui Ã©vite d'installer un pilote.
//!
//! Rien ici ne touche au rÃ©seau : ce module ne fait que lire et vÃ©rifier.

use std::path::{Path, PathBuf};

/// Une configuration WireGuard (`[Interface]` + un `[Peer]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Nom donnÃ© par l'utilisateur (le nom du fichier importÃ©, par dÃ©faut).
    pub nom: String,
    /// Adresses de l'interface, avec leur longueur de prÃ©fixe (`10.2.0.2/32`).
    pub adresses: Vec<String>,
    /// Serveurs DNS du tunnel.
    pub dns: Vec<String>,
    /// MTU (`None` : la valeur de WireGuard, 1420).
    pub mtu: Option<u16>,
    /// Port d'Ã©coute local (`ListenPort`), facultatif.
    pub port_ecoute: Option<u16>,
    /// `hote:port` du pair.
    pub endpoint: String,
    /// ClÃ© publique du pair (base64).
    pub cle_paire: String,
    /// ClÃ© partagÃ©e (`PresharedKey`), facultative.
    pub cle_partagee: Option<String>,
    /// Ce qui passe dans le tunnel (`AllowedIPs`).
    pub allowed_ips: Vec<String>,
    /// `PersistentKeepalive`, en secondes.
    pub keepalive: Option<u16>,
    /// La clÃ© privÃ©e, **gardÃ©e en mÃ©moire seulement** (elle part dans `tunnel.cle`, chiffrÃ©e).
    pub cle_privee: String,
}

impl Config {
    /// Ce qu'on montre sans jamais dÃ©voiler la clÃ© privÃ©e : l'essentiel pour reconnaÃ®tre la
    /// configuration.
    pub fn resume(&self) -> String {
        let adresses = if self.adresses.is_empty() {
            "sans adresse".to_string()
        } else {
            self.adresses.join(", ")
        };
        format!("{adresses} â†’ {} (clÃ© {}â€¦)", self.endpoint, empreinte(&self.cle_paire))
    }

    /// Le JSON public (tout sauf la clÃ© privÃ©e).
    pub fn json_public(&self) -> String {
        serde_json::json!({
            "nom": self.nom,
            "adresses": self.adresses,
            "dns": self.dns,
            "mtu": self.mtu,
            "portEcoute": self.port_ecoute,
            "endpoint": self.endpoint,
            "clePaire": self.cle_paire,
            "clePartagee": self.cle_partagee.is_some(),
            "allowedIps": self.allowed_ips,
            "keepalive": self.keepalive,
        })
        .to_string()
    }

    /// Relit un JSON Ã©crit par `json_public` (la clÃ© privÃ©e vient d'ailleurs).
    pub fn depuis_json(texte: &str, cle_privee: String) -> Result<Config, String> {
        let v: serde_json::Value = serde_json::from_str(texte).map_err(|e| format!("JSON illisible : {e}"))?;
        let chaines = |cle: &str| -> Vec<String> {
            v[cle]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default()
        };
        Ok(Config {
            nom: v["nom"].as_str().unwrap_or("Tunnel").to_string(),
            adresses: chaines("adresses"),
            dns: chaines("dns"),
            mtu: v["mtu"].as_u64().map(|n| n as u16),
            port_ecoute: v["portEcoute"].as_u64().map(|n| n as u16),
            endpoint: v["endpoint"].as_str().unwrap_or("").to_string(),
            cle_paire: v["clePaire"].as_str().unwrap_or("").to_string(),
            cle_partagee: chaines("clePartagee").first().cloned().or_else(|| None),
            allowed_ips: chaines("allowedIps"),
            keepalive: v["keepalive"].as_u64().map(|n| n as u16),
            cle_privee,
        })
    }
}

/// Les 32 octets d'une clÃ© WireGuard (base64), pour le moteur du tunnel.
pub fn octets_32(cle: &str) -> Option<[u8; 32]> {
    let octets = decoder_base64(cle.trim())?;
    if octets.len() != 32 {
        return None;
    }
    let mut sortie = [0u8; 32];
    sortie.copy_from_slice(&octets);
    Some(sortie)
}

/// Les 8 premiers caractÃ¨res d'une clÃ©, pour la reconnaÃ®tre Ã  l'Ã©cran sans la dÃ©voiler.
pub fn empreinte(cle: &str) -> String {
    cle.chars().take(8).collect()
}

/// Une clÃ© WireGuard : 32 octets en base64, et pas 32 zÃ©ros (une clÃ© nulle est un refus explicite
/// de chiffrer, pas une clÃ©).
fn cle_valide(valeur: &str) -> bool {
    match decoder_base64(valeur.trim()) {
        Some(octets) => octets.len() == 32 && octets.iter().any(|o| *o != 0),
        None => false,
    }
}

/// DÃ©code du base64 standard (avec ou sans remplissage `=`).
fn decoder_base64(texte: &str) -> Option<Vec<u8>> {
    let mut sortie = Vec::new();
    let mut quartet = [0u8; 4];
    let mut n = 0;
    for c in texte.chars() {
        if c.is_whitespace() {
            continue;
        }
        let valeur = match c {
            'A'..='Z' => c as u8 - b'A',
            'a'..='z' => c as u8 - b'a' + 26,
            '0'..='9' => c as u8 - b'0' + 52,
            '+' => 62,
            '/' => 63,
            '=' => break,
            _ => return None,
        };
        quartet[n] = valeur;
        n += 1;
        if n == 4 {
            sortie.push((quartet[0] << 2) | (quartet[1] >> 4));
            sortie.push((quartet[1] << 4) | (quartet[2] >> 2));
            sortie.push((quartet[2] << 6) | quartet[3]);
            n = 0;
        }
    }
    match n {
        0 => {}
        2 => sortie.push((quartet[0] << 2) | (quartet[1] >> 4)),
        3 => {
            sortie.push((quartet[0] << 2) | (quartet[1] >> 4));
            sortie.push((quartet[1] << 4) | (quartet[2] >> 2));
        }
        // Un reste de 1 caractÃ¨re n'est pas du base64.
        _ => return None,
    }
    Some(sortie)
}

/// `hote:port` (l'hÃ´te peut Ãªtre un nom ou une IP ; un `[v6]:port` est acceptÃ©).
fn endpoint_valide(valeur: &str) -> bool {
    let (hote, port) = match valeur.rsplit_once(':') {
        Some(p) => p,
        None => return false,
    };
    if hote.trim().is_empty() {
        return false;
    }
    matches!(port.trim().parse::<u16>(), Ok(p) if p > 0)
}

/// `10.2.0.2/32`, `fd00::2/128`, ou une adresse seule (prÃ©fixe complet supposÃ©).
fn adresse_plausible(valeur: &str) -> bool {
    let (adresse, prefixe) = match valeur.split_once('/') {
        Some((a, p)) => (a, Some(p)),
        None => (valeur, None),
    };
    let ip = adresse.trim();
    if ip.parse::<std::net::Ipv4Addr>().is_err() && ip.parse::<std::net::Ipv6Addr>().is_err() {
        return false;
    }
    match prefixe {
        Some(p) => matches!(p.trim().parse::<u8>(), Ok(n) if n <= 128),
        None => true,
    }
}

/// Lit une configuration WireGuard. Rend une erreur **lisible** : c'est elle que l'interface
/// affichera, et elle ne contient jamais la clÃ© privÃ©e.
pub fn lire(texte: &str) -> Result<Config, String> {
    let mut section = String::new();
    let mut cle_privee: Option<String> = None;
    let mut cle_paire: Option<String> = None;
    let mut cle_partagee: Option<String> = None;
    let mut endpoint: Option<String> = None;
    let mut adresses: Vec<String> = Vec::new();
    let mut dns: Vec<String> = Vec::new();
    let mut allowed: Vec<String> = Vec::new();
    let mut mtu: Option<u16> = None;
    let mut port_ecoute: Option<u16> = None;
    let mut keepalive: Option<u16> = None;

    for (numero, brute) in texte.lines().enumerate() {
        let ligne = brute.split('#').next().unwrap_or("").trim();
        if ligne.is_empty() {
            continue;
        }
        if ligne.starts_with('[') && ligne.ends_with(']') {
            section = ligne[1..ligne.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let Some((nom, valeur)) = ligne.split_once('=') else {
            return Err(format!("ligne {} : Â« {ligne} Â» n'est pas une affectation", numero + 1));
        };
        let nom = nom.trim().to_ascii_lowercase();
        let valeur = valeur.trim();
        let liste = || -> Vec<String> {
            valeur
                .split(',')
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        };
        match (section.as_str(), nom.as_str()) {
            ("interface", "privatekey") => cle_privee = Some(valeur.to_string()),
            ("interface", "address") => adresses.extend(liste()),
            ("interface", "dns") => dns.extend(liste()),
            ("interface", "mtu") => {
                mtu = Some(valeur.parse::<u16>().map_err(|_| format!("MTU invalide : Â« {valeur} Â»"))?)
            }
            ("interface", "listenport") => {
                port_ecoute = Some(valeur.parse::<u16>().map_err(|_| format!("ListenPort invalide : Â« {valeur} Â»"))?)
            }
            ("peer", "publickey") => cle_paire = Some(valeur.to_string()),
            ("peer", "presharedkey") => cle_partagee = Some(valeur.to_string()),
            ("peer", "endpoint") => endpoint = Some(valeur.to_string()),
            ("peer", "allowedips") => allowed.extend(liste()),
            ("peer", "persistentkeepalive") => {
                keepalive = Some(
                    valeur
                        .parse::<u16>()
                        .map_err(|_| format!("PersistentKeepalive invalide : Â« {valeur} Â»"))?,
                )
            }
            // Tout le reste est ignorÃ©, comme le fait WireGuard (options futures).
            _ => {
                if section != "interface" && section != "peer" {
                    return Err(format!(
                        "ligne {} : hors d'une section [Interface] ou [Peer]",
                        numero + 1
                    ));
                }
            }
        }
    }

    let cle_privee = cle_privee.ok_or("il manque Â« PrivateKey Â» dans [Interface]")?;
    if !cle_valide(&cle_privee) {
        return Err("la clÃ© privÃ©e n'est pas une clÃ© WireGuard (32 octets en base64)".into());
    }
    let cle_paire = cle_paire.ok_or("il manque Â« PublicKey Â» dans [Peer]")?;
    if !cle_valide(&cle_paire) {
        return Err("la clÃ© publique du pair n'est pas une clÃ© WireGuard (32 octets en base64)".into());
    }
    if let Some(partagee) = &cle_partagee {
        if !cle_valide(partagee) {
            return Err("la clÃ© partagÃ©e n'est pas une clÃ© WireGuard (32 octets en base64)".into());
        }
    }
    let endpoint = endpoint.ok_or("il manque Â« Endpoint Â» dans [Peer]")?;
    if !endpoint_valide(&endpoint) {
        return Err(format!("Â« {endpoint} Â» n'est pas un endpoint Â« hote:port Â»"));
    }
    for a in &adresses {
        if !adresse_plausible(a) {
            return Err(format!("adresse d'interface invalide : Â« {a} Â»"));
        }
    }
    for a in &allowed {
        if !adresse_plausible(a) {
            return Err(format!("AllowedIPs invalide : Â« {a} Â»"));
        }
    }
    // Sans AllowedIPs, un client WireGuard ne route rien : on prend tout, c'est ce que veulent les
    // configurations de fournisseur (Mullvad, ProtonVPNâ€¦).
    if allowed.is_empty() {
        allowed.push("0.0.0.0/0".into());
        allowed.push("::/0".into());
    }

    Ok(Config {
        nom: "Tunnel".to_string(),
        adresses,
        dns,
        mtu,
        port_ecoute,
        endpoint,
        cle_paire,
        cle_partagee,
        allowed_ips: allowed,
        keepalive,
        cle_privee,
    })
}

/// Lit un fichier `.conf` et rend la configuration, nommÃ©e d'aprÃ¨s le fichier.
pub fn lire_fichier(chemin: &Path) -> Result<Config, String> {
    // Un fichier de configuration WireGuard est en UTF-8 ; on refuse le binaire franc.
    let octets = std::fs::read(chemin).map_err(|e| format!("lecture impossible : {e}"))?;
    let texte = String::from_utf8(octets).map_err(|_| "le fichier n'est pas du texte UTF-8".to_string())?;
    let mut config = lire(&texte)?;
    config.nom = chemin
        .file_stem()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Tunnel".to_string());
    Ok(config)
}

// ------------------------------------------------------------------ stockage de la clÃ© privÃ©e

/// Chiffre la clÃ© privÃ©e avec **DPAPI**, au compte de l'utilisateur : le fichier n'est lisible que
/// par lui, sur cette machine. (Le mÃªme mÃ©canisme que le coffre de secrets du hub, cÃ´tÃ© outillage.)
#[cfg(windows)]
pub fn proteger(secret: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let entree = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut sortie = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&entree, None, None, None, None, 0, &mut sortie).map_err(|e| e.message())?;
        let octets = std::slice::from_raw_parts(sortie.pbData, sortie.cbData as usize).to_vec();
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(sortie.pbData as *mut _)));
        Ok(octets)
    }
}

/// DÃ©chiffre ce que `proteger` a Ã©crit.
#[cfg(windows)]
pub fn deproteger(secret: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let entree = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let mut sortie = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&entree, None, None, None, None, 0, &mut sortie).map_err(|e| e.message())?;
        let octets = std::slice::from_raw_parts(sortie.pbData, sortie.cbData as usize).to_vec();
        let _ = windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(sortie.pbData as *mut _)));
        Ok(octets)
    }
}

#[cfg(not(windows))]
pub fn proteger(secret: &[u8]) -> Result<Vec<u8>, String> {
    Ok(secret.to_vec())
}

#[cfg(not(windows))]
pub fn deproteger(secret: &[u8]) -> Result<Vec<u8>, String> {
    Ok(secret.to_vec())
}

/// Enregistre la configuration : la partie publique dans `tunnel.json`, la clÃ© privÃ©e chiffrÃ©e dans
/// `tunnel.cle`. Un fichier de rÃ©glages ne contient donc **jamais** la clÃ©.
pub fn enregistrer(racine: &Path, config: &Config) -> Result<(), String> {
    std::fs::write(racine.join("tunnel.json"), config.json_public()).map_err(|e| format!("Ã©criture impossible : {e}"))?;
    let chiffre = proteger(config.cle_privee.as_bytes())?;
    std::fs::write(racine.join("tunnel.cle"), chiffre).map_err(|e| format!("Ã©criture impossible : {e}"))?;
    Ok(())
}

/// Relit la configuration enregistrÃ©e (`None` : aucune).
pub fn charger(racine: &Path) -> Option<Config> {
    let public = std::fs::read_to_string(racine.join("tunnel.json")).ok()?;
    let chiffre = std::fs::read(racine.join("tunnel.cle")).ok()?;
    let clair = deproteger(&chiffre).ok()?;
    let cle_privee = String::from_utf8(clair).ok()?;
    Config::depuis_json(&public, cle_privee).ok()
}

/// Oublie la configuration : les deux fichiers disparaissent (la clÃ© d'abord).
pub fn oublier(racine: &Path) {
    let _ = std::fs::remove_file(racine.join("tunnel.cle"));
    let _ = std::fs::remove_file(racine.join("tunnel.json"));
}

/// Les fichiers d'une configuration, pour les sauvegardes et les messages.
pub fn fichiers(racine: &Path) -> (PathBuf, PathBuf) {
    (racine.join("tunnel.json"), racine.join("tunnel.cle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deux clÃ©s de test (32 octets en base64, aucune valeur rÃ©elle).
    const CLE_PRIVEE: &str = "yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=";
    const CLE_PAIRE: &str = "xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=";

    fn conf() -> String {
        format!(
            "[Interface]\nPrivateKey = {CLE_PRIVEE}\nAddress = 10.66.0.2/32\nDNS = 10.64.0.1\nMTU = 1380\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = 193.32.126.5:51820\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n"
        )
    }

    #[test]
    fn une_configuration_complete_se_lit() {
        let c = lire(&conf()).expect("config valide");
        assert_eq!(c.adresses, vec!["10.66.0.2/32"]);
        assert_eq!(c.dns, vec!["10.64.0.1"]);
        assert_eq!(c.mtu, Some(1380));
        assert_eq!(c.endpoint, "193.32.126.5:51820");
        assert_eq!(c.keepalive, Some(25));
        assert_eq!(c.allowed_ips.len(), 2);
        assert_eq!(c.cle_privee, CLE_PRIVEE);
    }

    /// Sans `AllowedIPs`, un client route tout : c'est ce que font les configurations de
    /// fournisseur, et sans cette rÃ¨gle la configuration importÃ©e ne servirait Ã  rien.
    #[test]
    fn sans_allowed_ips_tout_passe_dans_le_tunnel() {
        let c = lire(&format!(
            "[Interface]\nPrivateKey = {CLE_PRIVEE}\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = vpn.exemple.fr:51820\n"
        ))
        .expect("config valide");
        assert_eq!(c.allowed_ips, vec!["0.0.0.0/0", "::/0"]);
    }

    #[test]
    fn les_commentaires_et_les_vides_ne_derangent_pas() {
        let texte = format!(
            "# mon tunnel\n\n[Interface]\nPrivateKey = {CLE_PRIVEE}  # la mienne\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = 1.2.3.4:1\n"
        );
        assert!(lire(&texte).is_ok());
    }

    /// Ce qui doit Ãªtre refusÃ©, avec un message qui dit quoi corriger (et jamais la clÃ© privÃ©e).
    #[test]
    fn les_configurations_invalides_sont_refusees_avec_un_message_clair() {
        let cas: Vec<(String, &str)> = vec![
            (
                format!("[Interface]\nPrivateKey = {CLE_PRIVEE}\n\n[Peer]\nEndpoint = 1.2.3.4:51820\n"),
                "PublicKey",
            ),
            (
                format!("[Interface]\nPrivateKey = {CLE_PRIVEE}\n\n[Peer]\nPublicKey = {CLE_PAIRE}\n"),
                "Endpoint",
            ),
            (
                "[Interface]\n\n[Peer]\nPublicKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nEndpoint = 1.2.3.4:5\n".to_string(),
                "PrivateKey",
            ),
            (
                format!("[Interface]\nPrivateKey = trop-court\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = 1.2.3.4:5\n"),
                "clÃ© privÃ©e",
            ),
            (
                format!("[Interface]\nPrivateKey = {CLE_PRIVEE}\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = sans-port\n"),
                "hote:port",
            ),
            (
                format!("[Interface]\nPrivateKey = {CLE_PRIVEE}\nAddress = 10.0.0.300/32\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = 1.2.3.4:5\n"),
                "adresse d'interface",
            ),
            (
                "PrivateKey = quelquechose\n".to_string(),
                "hors d'une section",
            ),
        ];
        for (texte, attendu) in cas {
            match lire(&texte) {
                Ok(_) => panic!("cette configuration aurait dÃ» Ãªtre refusÃ©e : {texte}"),
                Err(e) => {
                    assert!(e.contains(attendu), "message Â« {e} Â» ne parle pas de Â« {attendu} Â»");
                    assert!(!e.contains(CLE_PRIVEE), "le message ne doit jamais contenir la clÃ© privÃ©e");
                }
            }
        }
    }

    /// Une clÃ© nulle (32 zÃ©ros) est un refus explicite de chiffrer, pas une clÃ©.
    #[test]
    fn une_cle_nulle_est_refusee() {
        let nulle = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
        let texte = format!("[Interface]\nPrivateKey = {nulle}\n\n[Peer]\nPublicKey = {CLE_PAIRE}\nEndpoint = 1.2.3.4:5\n");
        assert!(lire(&texte).is_err());
    }

    #[test]
    fn le_json_public_ne_contient_pas_la_cle_privee() {
        let c = lire(&conf()).unwrap();
        let json = c.json_public();
        assert!(!json.contains(CLE_PRIVEE), "la clÃ© privÃ©e ne doit jamais Ãªtre Ã©crite");
        let relu = Config::depuis_json(&json, c.cle_privee.clone()).unwrap();
        assert_eq!(relu.endpoint, c.endpoint);
        assert_eq!(relu.adresses, c.adresses);
        assert_eq!(relu.cle_privee, c.cle_privee);
    }

    /// DPAPI : ce qui est chiffrÃ© se relit, et le chiffrÃ© ne contient pas le clair.
    #[test]
    fn la_cle_privee_est_chiffree_et_se_relit() {
        let clair = CLE_PRIVEE.as_bytes();
        let chiffre = proteger(clair).expect("DPAPI disponible");
        assert_ne!(chiffre, clair, "le chiffrÃ© ne doit pas Ãªtre le clair");
        assert!(
            !chiffre.windows(clair.len()).any(|f| f == clair),
            "la clÃ© ne doit pas apparaÃ®tre telle quelle dans le fichier"
        );
        let relu = deproteger(&chiffre).expect("dÃ©chiffrement");
        assert_eq!(relu, clair);
    }

    /// Le rÃ©sumÃ© ne dÃ©voile rien : il sert Ã  reconnaÃ®tre la configuration Ã  l'Ã©cran.
    #[test]
    fn le_resume_ne_devoile_pas_la_cle_privee() {
        let c = lire(&conf()).unwrap();
        let resume = c.resume();
        assert!(resume.contains("193.32.126.5:51820"));
        assert!(!resume.contains(CLE_PRIVEE));
    }
}
