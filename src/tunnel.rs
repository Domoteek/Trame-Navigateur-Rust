//! Le tunnel WireGuard de Trame — **la configuration**, et la clé privée protégée.
//!
//! Décision de Clément (2026-11-27) : le VPN doit être **intégré à Trame**, et Trame s'installe
//! **par utilisateur** (aucun droit administrateur). D'où l'architecture retenue, en deux temps :
//!
//!   1. **ce module** : lire une configuration WireGuard (`.conf`), la valider, et garder la **clé
//!      privée chiffrée par Windows** (DPAPI, compte de l'utilisateur) — jamais en clair dans un
//!      fichier de réglages ;
//!   2. le tunnel lui-même (WireGuard en espace utilisateur, puis un proxy local que Trame
//!      traverse) : il viendra ensuite, et c'est ce qui évite d'installer un pilote.
//!
//! Rien ici ne touche au réseau : ce module ne fait que lire et vérifier.

use std::path::{Path, PathBuf};

/// Une configuration WireGuard (`[Interface]` + un `[Peer]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Nom donné par l'utilisateur (le nom du fichier importé, par défaut).
    pub nom: String,
    /// Adresses de l'interface, avec leur longueur de préfixe (`10.2.0.2/32`).
    pub adresses: Vec<String>,
    /// Serveurs DNS du tunnel.
    pub dns: Vec<String>,
    /// MTU (`None` : la valeur de WireGuard, 1420).
    pub mtu: Option<u16>,
    /// Port d'écoute local (`ListenPort`), facultatif.
    pub port_ecoute: Option<u16>,
    /// `hote:port` du pair.
    pub endpoint: String,
    /// Clé publique du pair (base64).
    pub cle_paire: String,
    /// Clé partagée (`PresharedKey`), facultative.
    pub cle_partagee: Option<String>,
    /// Ce qui passe dans le tunnel (`AllowedIPs`).
    pub allowed_ips: Vec<String>,
    /// `PersistentKeepalive`, en secondes.
    pub keepalive: Option<u16>,
    /// La clé privée, **gardée en mémoire seulement** (elle part dans `tunnel.cle`, chiffrée).
    pub cle_privee: String,
}

impl Config {
    /// Ce qu'on montre sans jamais dévoiler la clé privée : l'essentiel pour reconnaître la
    /// configuration.
    pub fn resume(&self) -> String {
        let adresses = if self.adresses.is_empty() {
            "sans adresse".to_string()
        } else {
            self.adresses.join(", ")
        };
        format!("{adresses} → {} (clé {}…)", self.endpoint, empreinte(&self.cle_paire))
    }

    /// Le JSON public (tout sauf la clé privée).
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

    /// Relit un JSON écrit par `json_public` (la clé privée vient d'ailleurs).
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

/// Les 32 octets d'une clé WireGuard (base64), pour le moteur du tunnel.
pub fn octets_32(cle: &str) -> Option<[u8; 32]> {
    let octets = decoder_base64(cle.trim())?;
    if octets.len() != 32 {
        return None;
    }
    let mut sortie = [0u8; 32];
    sortie.copy_from_slice(&octets);
    Some(sortie)
}

/// Les 8 premiers caractères d'une clé, pour la reconnaître à l'écran sans la dévoiler.
pub fn empreinte(cle: &str) -> String {
    cle.chars().take(8).collect()
}

/// Une clé WireGuard : 32 octets en base64, et pas 32 zéros (une clé nulle est un refus explicite
/// de chiffrer, pas une clé).
fn cle_valide(valeur: &str) -> bool {
    match decoder_base64(valeur.trim()) {
        Some(octets) => octets.len() == 32 && octets.iter().any(|o| *o != 0),
        None => false,
    }
}

/// Décode du base64 standard (avec ou sans remplissage `=`).
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
        // Un reste de 1 caractère n'est pas du base64.
        _ => return None,
    }
    Some(sortie)
}

/// `hote:port` (l'hôte peut être un nom ou une IP ; un `[v6]:port` est accepté).
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

/// `10.2.0.2/32`, `fd00::2/128`, ou une adresse seule (préfixe complet supposé).
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
/// affichera, et elle ne contient jamais la clé privée.
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
            return Err(format!("ligne {} : « {ligne} » n'est pas une affectation", numero + 1));
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
                mtu = Some(valeur.parse::<u16>().map_err(|_| format!("MTU invalide : « {valeur} »"))?)
            }
            ("interface", "listenport") => {
                port_ecoute = Some(valeur.parse::<u16>().map_err(|_| format!("ListenPort invalide : « {valeur} »"))?)
            }
            ("peer", "publickey") => cle_paire = Some(valeur.to_string()),
            ("peer", "presharedkey") => cle_partagee = Some(valeur.to_string()),
            ("peer", "endpoint") => endpoint = Some(valeur.to_string()),
            ("peer", "allowedips") => allowed.extend(liste()),
            ("peer", "persistentkeepalive") => {
                keepalive = Some(
                    valeur
                        .parse::<u16>()
                        .map_err(|_| format!("PersistentKeepalive invalide : « {valeur} »"))?,
                )
            }
            // Tout le reste est ignoré, comme le fait WireGuard (options futures).
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

    let cle_privee = cle_privee.ok_or("il manque « PrivateKey » dans [Interface]")?;
    if !cle_valide(&cle_privee) {
        return Err("la clé privée n'est pas une clé WireGuard (32 octets en base64)".into());
    }
    let cle_paire = cle_paire.ok_or("il manque « PublicKey » dans [Peer]")?;
    if !cle_valide(&cle_paire) {
        return Err("la clé publique du pair n'est pas une clé WireGuard (32 octets en base64)".into());
    }
    if let Some(partagee) = &cle_partagee {
        if !cle_valide(partagee) {
            return Err("la clé partagée n'est pas une clé WireGuard (32 octets en base64)".into());
        }
    }
    let endpoint = endpoint.ok_or("il manque « Endpoint » dans [Peer]")?;
    if !endpoint_valide(&endpoint) {
        return Err(format!("« {endpoint} » n'est pas un endpoint « hote:port »"));
    }
    for a in &adresses {
        if !adresse_plausible(a) {
            return Err(format!("adresse d'interface invalide : « {a} »"));
        }
    }
    for a in &allowed {
        if !adresse_plausible(a) {
            return Err(format!("AllowedIPs invalide : « {a} »"));
        }
    }
    // Sans AllowedIPs, un client WireGuard ne route rien : on prend tout, c'est ce que veulent les
    // configurations de fournisseur (Mullvad, ProtonVPN…).
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

// ------------------------------------------------------------------ stockage de la clé privée

/// Chiffre la clé privée avec **DPAPI**, au compte de l'utilisateur : le fichier n'est lisible que
/// par lui, sur cette machine. (Le même mécanisme que le coffre de secrets du hub, côté outillage.)
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

/// Déchiffre ce que `proteger` a écrit.
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

// ------------------------------------------------------------------ plusieurs configurations
//
// Demande de Clément (2026-11-28) : « pouvoir intégrer plusieurs fichiers .conf en fonction du pays
// où je veux me situer et switcher rapidement entre l'un et l'autre, même s'il faut redémarrer ».
// Chaque configuration vit donc dans **son dossier** (`tunnels/<id>/`), et un fichier `actif.txt`
// dit laquelle se monte au démarrage. Rien n'est partagé entre deux configurations : supprimer l'une
// ne touche pas aux autres, et chacune garde sa clé chiffrée séparément.
//
// L'ancien emplacement — deux fichiers à la racine du profil, une seule configuration possible —
// est repris une fois par `migrer` : une mise à jour ne perd pas la configuration déjà importée.

/// Le genre d'une configuration rangée : les deux moteurs de Trame ne lisent pas les mêmes fichiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Genre {
    WireGuard,
    OpenVpn,
}

impl Genre {
    pub fn json(&self) -> &'static str {
        match self {
            Genre::WireGuard => "wireguard",
            Genre::OpenVpn => "openvpn",
        }
    }
}

/// Une configuration rangée dans le profil, telle que l'interface la liste (sans la clé).
#[derive(Clone, Debug, PartialEq)]
pub struct Entree {
    pub id: String,
    pub genre: Genre,
    pub nom: String,
    pub endpoint: String,
    pub adresses: Vec<String>,
    /// Pour OpenVPN : le protocole (`udp`, `tcp`), vide pour WireGuard.
    pub protocole: String,
}

impl Entree {
    /// Le résumé montré à l'écran : où l'on sort, et par où.
    pub fn resume(&self) -> String {
        match self.genre {
            // WireGuard : l'adresse que la pile porte, puis le pair.
            Genre::WireGuard => {
                let adresses = if self.adresses.is_empty() {
                    "(adresse non précisée)".to_string()
                } else {
                    self.adresses.join(", ")
                };
                format!("{adresses} → {}", self.endpoint)
            }
            // OpenVPN : le protocole, puis le serveur (l'adresse n'est connue qu'après le montage).
            Genre::OpenVpn => format!("openvpn {} {}", self.protocole, self.endpoint),
        }
    }
}

/// Une configuration **chargée**, prête à donner au bon moteur.
#[derive(Clone, Debug)]
pub enum Configuration {
    /// WireGuard : le `.conf` déjà analysé.
    WireGuard(Config),
    /// OpenVPN : le profil tel quel (il porte ses certificats), et les identifiants s'il en faut.
    OpenVpn {
        nom: String,
        profil: String,
        identifiants: Option<(String, String)>,
    },
}

/// Dossier qui range toutes les configurations.
pub fn dossier(racine: &Path) -> PathBuf {
    racine.join("tunnels")
}

/// Identifiant lisible tiré du nom : `NordVPN_France_1130` → `nordvpn-france-1130`.
pub fn identifiant(nom: &str) -> String {
    let mut sortie = String::new();
    let mut tiret = true;
    for c in nom.chars() {
        if c.is_ascii_alphanumeric() {
            sortie.push(c.to_ascii_lowercase());
            tiret = false;
        } else if !tiret {
            sortie.push('-');
            tiret = true;
        }
    }
    let propre = sortie.trim_matches('-').to_string();
    if propre.is_empty() {
        "tunnel".to_string()
    } else {
        propre
    }
}

fn fichiers_de(racine: &Path, id: &str) -> (PathBuf, PathBuf) {
    let d = dossier(racine).join(id);
    (d.join("tunnel.json"), d.join("tunnel.cle"))
}

/// Écrit (ou réécrit) une configuration sous un identifiant donné.
pub fn ecrire(racine: &Path, id: &str, config: &Config) -> Result<(), String> {
    let (public, cle) = fichiers_de(racine, id);
    if let Some(d) = public.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("dossier impossible : {e}"))?;
    }
    std::fs::write(&public, config.json_public()).map_err(|e| format!("écriture impossible : {e}"))?;
    let chiffre = proteger(config.cle_privee.as_bytes())?;
    std::fs::write(&cle, chiffre).map_err(|e| format!("écriture impossible : {e}"))?;
    Ok(())
}

/// Range une configuration **de plus** sous un identifiant unique, et la rend active.
pub fn enregistrer(racine: &Path, config: &Config) -> Result<String, String> {
    let base = identifiant(&config.nom);
    let mut id = base.clone();
    let mut n = 2;
    while dossier(racine).join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    ecrire(racine, &id, config)?;
    definir_actif(racine, &id)?;
    Ok(id)
}

/// Relit une configuration précise (`None` : identifiant inconnu, clé illisible).
pub fn charger_id(racine: &Path, id: &str) -> Option<Config> {
    let (public, cle) = fichiers_de(racine, id);
    let texte = std::fs::read_to_string(&public).ok()?;
    let chiffre = std::fs::read(&cle).ok()?;
    let clair = deproteger(&chiffre).ok()?;
    let cle_privee = String::from_utf8(clair).ok()?;
    Config::depuis_json(&texte, cle_privee).ok()
}

/// L'identifiant de la configuration active. Si `actif.txt` désigne une configuration disparue, on
/// retombe sur la première rangée : mieux vaut un tunnel choisi qu'aucun tunnel.
pub fn actif(racine: &Path) -> Option<String> {
    let liste = lister(racine);
    if liste.is_empty() {
        return None;
    }
    let note = std::fs::read_to_string(dossier(racine).join("actif.txt"))
        .ok()
        .map(|t| t.trim().to_string())
        .unwrap_or_default();
    if !note.is_empty() && liste.iter().any(|e| e.id == note) {
        return Some(note);
    }
    liste.first().map(|e| e.id.clone())
}

/// Rend une configuration active (c'est elle qui se montera au prochain démarrage).
pub fn definir_actif(racine: &Path, id: &str) -> Result<(), String> {
    if !dossier(racine).join(id).is_dir() {
        return Err(format!("configuration inconnue : {id}"));
    }
    std::fs::write(dossier(racine).join("actif.txt"), id)
        .map_err(|e| format!("écriture impossible : {e}"))
}

/// Relit la configuration **active** (`None` : aucune).
pub fn charger(racine: &Path) -> Option<Config> {
    actif(racine).and_then(|id| charger_id(racine, &id))
}

/// Toutes les configurations rangées, triées par identifiant.
pub fn lister(racine: &Path) -> Vec<Entree> {
    let Ok(lecture) = std::fs::read_dir(dossier(racine)) else {
        return Vec::new();
    };
    let mut entrees: Vec<Entree> = lecture
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let id = e.file_name().to_string_lossy().to_string();
            // OpenVPN d'abord : son `openvpn.json` porte le genre. WireGuard n'en a pas besoin —
            // son `tunnel.json` date de la 0.4.85 et se lit comme avant.
            if let Ok(texte) = std::fs::read_to_string(e.path().join("openvpn.json")) {
                let v: serde_json::Value = serde_json::from_str(&texte).ok()?;
                return Some(Entree {
                    nom: v["nom"].as_str().unwrap_or(&id).to_string(),
                    endpoint: v["serveur"].as_str().unwrap_or("").to_string(),
                    adresses: Vec::new(),
                    protocole: v["protocole"].as_str().unwrap_or("").to_string(),
                    genre: Genre::OpenVpn,
                    id,
                });
            }
            let texte = std::fs::read_to_string(e.path().join("tunnel.json")).ok()?;
            let v: serde_json::Value = serde_json::from_str(&texte).ok()?;
            let adresses = v["adresses"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            Some(Entree {
                nom: v["nom"].as_str().unwrap_or(&id).to_string(),
                endpoint: v["endpoint"].as_str().unwrap_or("").to_string(),
                adresses,
                protocole: String::new(),
                genre: Genre::WireGuard,
                id,
            })
        })
        .collect();
    entrees.sort_by(|a, b| a.id.cmp(&b.id));
    entrees
}

/// Le genre d'une configuration rangée (`None` : cet identifiant n'existe pas).
pub fn genre(racine: &Path, id: &str) -> Option<Genre> {
    let d = dossier(racine).join(id);
    if d.join("openvpn.json").is_file() {
        Some(Genre::OpenVpn)
    } else if d.join("tunnel.json").is_file() {
        Some(Genre::WireGuard)
    } else {
        None
    }
}

/// Range un profil OpenVPN : le profil **entier chiffré** (il peut porter la clé privée du client),
/// les identifiants s'il en faut, et un résumé public pour l'interface.
///
/// Le profil est validé par le moteur avant d'être écrit : un fichier qui n'est pas un `.ovpn`
/// exploitable est refusé avant de toucher au disque.
pub fn enregistrer_ovpn(
    racine: &Path,
    nom: &str,
    profil: &str,
    identifiants: Option<(String, String)>,
) -> Result<String, String> {
    let resume = crate::ovpn::valider(profil)?;
    let base = identifiant(nom);
    // Réimporter le **même** profil met à jour son entrée au lieu d'en créer une seconde : c'est ce
    // qui permet d'ajouter les identifiants de service après coup, sans se retrouver avec `nord-fr`
    // puis `nord-fr-2` dans la liste.
    let mut id = base.clone();
    let mut n = 2;
    while dossier(racine).join(&id).exists() && genre(racine, &id) != Some(Genre::OpenVpn) {
        id = format!("{base}-{n}");
        n += 1;
    }
    let d = dossier(racine).join(&id);
    std::fs::create_dir_all(&d).map_err(|e| format!("dossier impossible : {e}"))?;

    let public = serde_json::json!({
        "nom": nom,
        "genre": Genre::OpenVpn.json(),
        "serveur": resume.serveur,
        "port": resume.port,
        "protocole": resume.protocole,
        "autologin": resume.autologin,
    });
    std::fs::write(d.join("openvpn.json"), public.to_string())
        .map_err(|e| format!("écriture impossible : {e}"))?;
    let chiffre = proteger(profil.as_bytes())?;
    std::fs::write(d.join("profil.cle"), chiffre).map_err(|e| format!("écriture impossible : {e}"))?;
    if let Some((utilisateur, mot_de_passe)) = identifiants {
        let paquet = format!("{utilisateur}\n{mot_de_passe}");
        let chiffre = proteger(paquet.as_bytes())?;
        std::fs::write(d.join("identifiants.cle"), chiffre).map_err(|e| format!("écriture impossible : {e}"))?;
    }
    definir_actif(racine, &id)?;
    Ok(id)
}

/// Relit un profil OpenVPN rangé : le texte du `.ovpn`, et les identifiants s'ils ont été gardés.
pub fn charger_ovpn(racine: &Path, id: &str) -> Option<(String, Option<(String, String)>)> {
    let d = dossier(racine).join(id);
    let chiffre = std::fs::read(d.join("profil.cle")).ok()?;
    let profil = String::from_utf8(deproteger(&chiffre).ok()?).ok()?;
    let identifiants = std::fs::read(d.join("identifiants.cle"))
        .ok()
        .and_then(|c| deproteger(&c).ok())
        .and_then(|clair| String::from_utf8(clair).ok())
        .and_then(|paquet| {
            paquet
                .split_once('\n')
                .map(|(u, p)| (u.to_string(), p.to_string()))
        });
    Some((profil, identifiants))
}

/// Relit une configuration **quel que soit son genre** : c'est ce que l'application donne au bon
/// moteur au démarrage.
pub fn charger_configuration(racine: &Path, id: &str) -> Option<Configuration> {
    match genre(racine, id)? {
        Genre::WireGuard => charger_id(racine, id).map(Configuration::WireGuard),
        Genre::OpenVpn => {
            let (profil, identifiants) = charger_ovpn(racine, id)?;
            let nom = lister(racine)
                .into_iter()
                .find(|e| e.id == id)
                .map(|e| e.nom)
                .unwrap_or_else(|| id.to_string());
            Some(Configuration::OpenVpn {
                nom,
                profil,
                identifiants,
            })
        }
    }
}

/// La configuration **active**, quel que soit son genre.
pub fn charger_active(racine: &Path) -> Option<Configuration> {
    actif(racine).and_then(|id| charger_configuration(racine, &id))
}

/// Supprime une configuration — la clé d'abord. Si c'était l'active, une autre prend sa place.
pub fn supprimer(racine: &Path, id: &str) -> Result<(), String> {
    let (public, cle) = fichiers_de(racine, id);
    let _ = std::fs::remove_file(&cle);
    let _ = std::fs::remove_file(&public);
    // Les fichiers propres à OpenVPN : le profil chiffré et les identifiants.
    let d = dossier(racine).join(id);
    let _ = std::fs::remove_file(d.join("profil.cle"));
    let _ = std::fs::remove_file(d.join("identifiants.cle"));
    let _ = std::fs::remove_file(d.join("openvpn.json"));
    std::fs::remove_dir_all(&d).map_err(|e| format!("suppression impossible : {e}"))?;
    if actif_note(racine).as_deref() == Some(id) {
        let _ = std::fs::remove_file(dossier(racine).join("actif.txt"));
        if let Some(premiere) = lister(racine).first() {
            let _ = definir_actif(racine, &premiere.id.clone());
        }
    }
    Ok(())
}

/// Ce que `actif.txt` dit, sans repli (sert à savoir si l'entrée supprimée était l'active).
fn actif_note(racine: &Path) -> Option<String> {
    std::fs::read_to_string(dossier(racine).join("actif.txt"))
        .ok()
        .map(|t| t.trim().to_string())
}

/// Oublie **toutes** les configurations (la clé d'abord, puis le dossier). L'interface supprime
/// une configuration à la fois (`supprimer`) ; celle-ci reste le geste de remise à zéro, éprouvé
/// par les tests et prêt pour un futur « tout oublier ».
#[cfg(test)]
pub fn oublier(racine: &Path) -> usize {
    let mut n = 0;
    for e in lister(racine) {
        let (public, cle) = fichiers_de(racine, &e.id);
        let _ = std::fs::remove_file(&cle);
        let _ = std::fs::remove_file(&public);
        if std::fs::remove_dir_all(dossier(racine).join(&e.id)).is_ok() {
            n += 1;
        }
    }
    let _ = std::fs::remove_file(dossier(racine).join("actif.txt"));
    n
}

/// Reprend l'ancien emplacement (deux fichiers à la racine du profil) dans le nouveau rangement.
/// Rend l'identifiant créé, ou `None` s'il n'y avait rien à reprendre. **Copie puis vérifie avant
/// de supprimer** : une panne au milieu ne doit jamais coûter la clé.
pub fn migrer(racine: &Path) -> Option<String> {
    let ancien_public = racine.join("tunnel.json");
    let ancienne_cle = racine.join("tunnel.cle");
    if !ancien_public.is_file() || !ancienne_cle.is_file() {
        return None;
    }
    let texte = std::fs::read_to_string(&ancien_public).ok()?;
    let nom = serde_json::from_str::<serde_json::Value>(&texte)
        .ok()
        .and_then(|v| v["nom"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "tunnel".to_string());
    let base = identifiant(&nom);
    let mut id = base.clone();
    let mut n = 2;
    while dossier(racine).join(&id).exists() {
        id = format!("{base}-{n}");
        n += 1;
    }
    let (public, cle) = fichiers_de(racine, &id);
    let dossier_cible = dossier(racine).join(&id);
    std::fs::create_dir_all(&dossier_cible).ok()?;
    std::fs::copy(&ancien_public, &public).ok()?;
    std::fs::copy(&ancienne_cle, &cle).ok()?;
    // La relecture est la preuve que la copie est complète : sans elle, on ne supprime rien.
    let publique_ok = std::fs::read_to_string(&public).ok()? == texte;
    let cle_ok = std::fs::read(&cle).map(|a| !a.is_empty()).unwrap_or(false);
    if !publique_ok || !cle_ok {
        let _ = std::fs::remove_dir_all(&dossier_cible);
        return None;
    }
    let _ = definir_actif(racine, &id);
    let _ = std::fs::remove_file(&ancien_public);
    let _ = std::fs::remove_file(&ancienne_cle);
    Some(id)
}


#[cfg(test)]
mod tests {
    use super::*;

    // Deux clés de test (32 octets en base64, aucune valeur réelle).
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
    /// fournisseur, et sans cette règle la configuration importée ne servirait à rien.
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

    /// Ce qui doit être refusé, avec un message qui dit quoi corriger (et jamais la clé privée).
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
                "clé privée",
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
                Ok(_) => panic!("cette configuration aurait dû être refusée : {texte}"),
                Err(e) => {
                    assert!(e.contains(attendu), "message « {e} » ne parle pas de « {attendu} »");
                    assert!(!e.contains(CLE_PRIVEE), "le message ne doit jamais contenir la clé privée");
                }
            }
        }
    }

    /// Une clé nulle (32 zéros) est un refus explicite de chiffrer, pas une clé.
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
        assert!(!json.contains(CLE_PRIVEE), "la clé privée ne doit jamais être écrite");
        let relu = Config::depuis_json(&json, c.cle_privee.clone()).unwrap();
        assert_eq!(relu.endpoint, c.endpoint);
        assert_eq!(relu.adresses, c.adresses);
        assert_eq!(relu.cle_privee, c.cle_privee);
    }

    /// DPAPI : ce qui est chiffré se relit, et le chiffré ne contient pas le clair.
    #[test]
    fn la_cle_privee_est_chiffree_et_se_relit() {
        let clair = CLE_PRIVEE.as_bytes();
        let chiffre = proteger(clair).expect("DPAPI disponible");
        assert_ne!(chiffre, clair, "le chiffré ne doit pas être le clair");
        assert!(
            !chiffre.windows(clair.len()).any(|f| f == clair),
            "la clé ne doit pas apparaître telle quelle dans le fichier"
        );
        let relu = deproteger(&chiffre).expect("déchiffrement");
        assert_eq!(relu, clair);
    }

    /// Le résumé ne dévoile rien : il sert à reconnaître la configuration à l'écran.
    #[test]
    fn le_resume_ne_devoile_pas_la_cle_privee() {
        let c = lire(&conf()).unwrap();
        let resume = c.resume();
        assert!(resume.contains("193.32.126.5:51820"));
        assert!(!resume.contains(CLE_PRIVEE));
    }

    // ------------------------------------------------------- plusieurs configurations

    fn racine_essai(nom: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("trame-tunnel-test-{nom}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dossier d'essai");
        d
    }

    #[test]
    fn les_identifiants_sont_lisibles_et_uniques() {
        assert_eq!(identifiant("NordVPN_France_1130"), "nordvpn-france-1130");
        assert_eq!(identifiant("fr-par-wg-003.conf"), "fr-par-wg-003-conf");
        assert_eq!(identifiant("   "), "tunnel");
        assert_eq!(identifiant("____"), "tunnel");
    }

    /// La demande de Clément : plusieurs configurations rangées côte à côte, et une active.
    #[test]
    fn plusieurs_configurations_se_rangent_et_se_choisissent() {
        let racine = racine_essai("multi");
        let mut mullvad = lire(&conf()).unwrap();
        mullvad.nom = "mullvad-fr".into();
        let mut nord = lire(&conf()).unwrap();
        nord.nom = "nord-fr".into();
        nord.endpoint = "185.61.156.20:51820".into();

        assert_eq!(enregistrer(&racine, &mullvad).unwrap(), "mullvad-fr");
        assert_eq!(enregistrer(&racine, &nord).unwrap(), "nord-fr");
        assert_eq!(lister(&racine).len(), 2, "les deux sont rangées");
        assert_eq!(actif(&racine).as_deref(), Some("nord-fr"), "la dernière importée devient active");
        assert_eq!(charger(&racine).unwrap().endpoint, "185.61.156.20:51820");

        // Basculer d'une configuration à l'autre, c'est ce que fait « Utiliser ».
        definir_actif(&racine, "mullvad-fr").unwrap();
        assert_eq!(charger(&racine).unwrap().endpoint, "193.32.126.5:51820");

        // Deux fichiers peuvent porter le même nom : le second ne doit pas écraser le premier.
        let mut doublon = mullvad.clone();
        doublon.nom = "mullvad-fr".into();
        assert_eq!(enregistrer(&racine, &doublon).unwrap(), "mullvad-fr-2");
        assert_eq!(lister(&racine).len(), 3);

        // Supprimer l'active laisse une autre prendre sa place.
        supprimer(&racine, "mullvad-fr-2").unwrap();
        assert_eq!(lister(&racine).len(), 2);
        assert!(charger(&racine).is_some(), "une configuration reste active");
        assert_eq!(oublier(&racine), 2, "oublier tout rend le nombre effacé");
        assert!(lister(&racine).is_empty());
    }

    /// L'ancien rangement (deux fichiers à la racine) est repris sans perdre la clé : c'est ce qui
    /// protège une mise à jour depuis la 0.4.84.
    #[test]
    fn l_ancien_emplacement_est_repris_sans_perdre_la_cle() {
        let racine = racine_essai("migre");
        let mut config = lire(&conf()).unwrap();
        config.nom = "mullvad-fr-par".into();
        std::fs::write(racine.join("tunnel.json"), config.json_public()).unwrap();
        std::fs::write(racine.join("tunnel.cle"), proteger(config.cle_privee.as_bytes()).unwrap()).unwrap();

        assert_eq!(migrer(&racine).as_deref(), Some("mullvad-fr-par"));
        assert!(!racine.join("tunnel.json").exists(), "l'ancien fichier disparaît");
        assert!(!racine.join("tunnel.cle").exists(), "l'ancienne clé aussi");
        let relu = charger(&racine).expect("configuration rechargée après reprise");
        assert_eq!(relu.cle_privee, config.cle_privee, "la clé survit à la reprise");
        assert_eq!(relu.endpoint, config.endpoint);
        assert_eq!(migrer(&racine), None, "il n'y a plus rien à reprendre");
    }

    // ------------------------------------------------------------- OpenVPN

    const PROFIL_OVPN: &str = "client
dev tun
proto udp
remote 193.32.126.68 1194
auth-user-pass
<ca>
CERTIFICAT
</ca>
";

    /// Un profil OpenVPN se range **à côté** des configurations WireGuard : même dossier, autre
    /// genre. Le profil lui-même est chiffré (il peut porter la clé privée du client).
    #[test]
    fn un_profil_openvpn_se_range_a_cote_des_configurations_wireguard() {
        let racine = racine_essai("ovpn");
        let mut wg = lire(&conf()).unwrap();
        wg.nom = "mullvad-fr".into();
        enregistrer(&racine, &wg).unwrap();
        let id = enregistrer_ovpn(
            &racine,
            "nord-fr",
            PROFIL_OVPN,
            Some(("utilisateur-distinctif".into(), "motdepasse-distinctif".into())),
        )
        .unwrap();
        assert_eq!(id, "nord-fr");

        let liste = lister(&racine);
        assert_eq!(liste.len(), 2, "les deux genres cohabitent");
        let nord = liste.iter().find(|e| e.id == "nord-fr").expect("nord listé");
        assert_eq!(nord.genre, Genre::OpenVpn);
        assert_eq!(nord.resume(), "openvpn udp 193.32.126.68");
        let mullvad = liste.iter().find(|e| e.id == "mullvad-fr").expect("mullvad listé");
        assert_eq!(mullvad.genre, Genre::WireGuard);

        // Le profil se relit à l'identique, identifiants compris…
        let (profil, identifiants) = charger_ovpn(&racine, "nord-fr").expect("profil relu");
        assert_eq!(profil, PROFIL_OVPN);
        assert_eq!(
            identifiants,
            Some(("utilisateur-distinctif".to_string(), "motdepasse-distinctif".to_string()))
        );
        // …et il n'est **pas** en clair sur le disque.
        let brut = std::fs::read(racine.join("tunnels").join("nord-fr").join("profil.cle")).unwrap();
        assert!(
            !brut.windows(7).any(|f| f == b"remote "),
            "le profil chiffré ne doit pas contenir le texte du .ovpn"
        );
        let identifiants_bruts = std::fs::read(racine.join("tunnels").join("nord-fr").join("identifiants.cle")).unwrap();
        let secret = b"motdepasse-distinctif";
        assert!(
            !identifiants_bruts.windows(secret.len()).any(|f| f == secret),
            "les identifiants ne doivent pas être en clair dans le fichier chiffré"
        );

        // Le chargeur rend le bon genre : c'est lui qui décidera du moteur au démarrage.
        match charger_configuration(&racine, "nord-fr") {
            Some(Configuration::OpenVpn { profil, identifiants, nom }) => {
                assert_eq!(nom, "nord-fr");
                assert_eq!(profil, PROFIL_OVPN);
                assert!(identifiants.is_some());
            }
            autre => panic!("attendu une configuration OpenVPN, reçu {autre:?}"),
        }
        assert!(matches!(
            charger_configuration(&racine, "mullvad-fr"),
            Some(Configuration::WireGuard(_))
        ));
        // La dernière importée est active : ici le profil OpenVPN.
        assert!(matches!(charger_active(&racine), Some(Configuration::OpenVpn { .. })));
    }

    /// Un fichier qui n'est pas un profil OpenVPN exploitable est refusé **avant** d'écrire quoi que
    /// ce soit, et ne touche pas aux configurations déjà rangées.
    #[test]
    fn un_faux_profil_openvpn_est_refuse_sans_rien_ecrire() {
        let racine = racine_essai("ovpn-refus");
        let mut wg = lire(&conf()).unwrap();
        wg.nom = "mullvad-fr".into();
        enregistrer(&racine, &wg).unwrap();
        assert!(enregistrer_ovpn(&racine, "casse", "bonjour\n", None).is_err());
        let liste = lister(&racine);
        assert_eq!(liste.len(), 1, "rien n'a été ajouté");
        assert_eq!(liste[0].id, "mullvad-fr");
    }

    /// Supprimer un profil OpenVPN efface son profil chiffré, ses identifiants et sa fiche — et rien
    /// d'autre.
    #[test]
    fn supprimer_un_profil_openvpn_n_efface_que_lui() {
        let racine = racine_essai("ovpn-suppr");
        let mut wg = lire(&conf()).unwrap();
        wg.nom = "mullvad-fr".into();
        enregistrer(&racine, &wg).unwrap();
        let id = enregistrer_ovpn(&racine, "nord-fr", PROFIL_OVPN, None).unwrap();
        supprimer(&racine, &id).unwrap();
        assert!(!racine.join("tunnels").join("nord-fr").exists(), "son dossier a disparu");
        assert_eq!(genre(&racine, &id), None);
        assert_eq!(genre(&racine, "mullvad-fr"), Some(Genre::WireGuard));
        assert!(charger_id(&racine, "mullvad-fr").is_some(), "l'autre configuration est intacte");
    }
}
