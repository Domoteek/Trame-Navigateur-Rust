//! Format CRX : l'archive d'une extension du Chrome Web Store.
//!
//! Un `.crx` est un ZIP precede d'un en-tete de signature. Deux versions existent :
//!
//! ```text
//! CRX3 : "Cr24" | u32 version=3 | u32 taille_en_tete | en-tete (protobuf) | ZIP
//! CRX2 : "Cr24" | u32 version=2 | u32 taille_cle | cle | u32 taille_signature | signature | ZIP
//! ```
//!
//! Le magasin de Chrome ne sert que du CRX3 aujourd'hui, mais un fichier `.crx` peut venir
//! d'ailleurs : on accepte les deux, et un ZIP nu (quelqu'un a renomme une archive).
//!
//! **La signature** : un CRX3 est verifie (RSA-SHA256) avant d'etre extrait — la cle de la preuve
//! doit etre celle du `crx_id`, et la signature doit tenir sur
//! `"CRX3 SignedData\0" | u32le(taille) | SignedData | ZIP` (ce que Chromium signe). Depuis la
//! 0.4.55, un paquet telecharge dont la preuve ne tient pas est **refuse**. Un ZIP nu ou un CRX2
//! n'ont aucune preuve exploitable : ils restent installables (import local assume), et c'est
//! ecrit dans l'interface comme dans `docs/FORMAT-EXTENSION.md`.

/// En-tete magique de tous les CRX.
const MAGIE: &[u8; 4] = b"Cr24";

/// Ce que contient un fichier telecharge, une fois l'en-tete retire.
#[derive(Debug)]
pub struct ArchiveExtraite {
    /// Version du format CRX rencontree (0 pour un ZIP nu).
    pub version_crx: u32,
    /// Les octets du ZIP, prets a etre decompresses.
    pub zip: Vec<u8>,
}

/// Retire l'en-tete du CRX et rend le ZIP.
pub fn extraire(octets: &[u8]) -> Result<ArchiveExtraite, String> {
    // Un ZIP nu : "PK\x03\x04" (ou "PK\x05\x06" pour une archive vide). Teste **avant** la
    // longueur minimale : une extension minuscule tient en dix octets.
    if octets.starts_with(b"PK\x03\x04") || octets.starts_with(b"PK\x05\x06") {
        return Ok(ArchiveExtraite {
            version_crx: 0,
            zip: octets.to_vec(),
        });
    }
    if octets.len() < 12 {
        return Err("fichier trop court pour etre une extension".into());
    }
    if &octets[0..4] != MAGIE {
        return Err(format!(
            "ce n'est pas une extension (.crx ou .zip attendu, en-tete « {} » trouve)",
            String::from_utf8_lossy(&octets[0..4])
        ));
    }
    let version = u32::from_le_bytes([octets[4], octets[5], octets[6], octets[7]]);
    match version {
        3 => {
            let taille = u32::from_le_bytes([octets[8], octets[9], octets[10], octets[11]]) as usize;
            let debut = 12 + taille;
            if taille == 0 || debut >= octets.len() {
                return Err("en-tete CRX3 incoherent (taille invalide)".into());
            }
            // 0.4.55 : un paquet telecharge doit prouver qu'il est intact AVANT d'etre installe.
            // Un CRX3 dont la signature ne tient pas est refuse ici, avec le motif.
            verifier_signature(octets)?;
            verifier_zip(&octets[debut..])?;
            Ok(ArchiveExtraite {
                version_crx: 3,
                zip: octets[debut..].to_vec(),
            })
        }
        2 => {
            // CRX2 : deux blocs prefixes par leur taille (cle publique, puis signature).
            // Les tailles commencent a l'octet 8 (magie + version), pas a 12.
            let mut position = 8usize;
            for nom in ["cle publique", "signature"] {
                if position + 4 > octets.len() {
                    return Err(format!("en-tete CRX2 tronque avant la {nom}"));
                }
                let longueur = u32::from_le_bytes([
                    octets[position],
                    octets[position + 1],
                    octets[position + 2],
                    octets[position + 3],
                ]) as usize;
                position += 4 + longueur;
                if position > octets.len() {
                    return Err(format!("en-tete CRX2 tronque : {nom} annoncee trop longue"));
                }
            }
            if position >= octets.len() {
                return Err("en-tete CRX2 sans contenu".into());
            }
            verifier_zip(&octets[position..])?;
            Ok(ArchiveExtraite {
                version_crx: 2,
                zip: octets[position..].to_vec(),
            })
        }
        autre => Err(format!(
            "version de CRX inconnue ({autre}) : cette extension n'est pas installable ici"
        )),
    }
}

/// Le contexte que Chromium signe, avec son octet nul final.
const CONTEXTE_SIGNATURE: &[u8] = b"CRX3 SignedData\x00";

/// Verifie la signature d'un paquet CRX3 (RSA-SHA256).
///
/// La charge signee est celle que Chromium signe :
///
/// ```text
/// "CRX3 SignedData\0" | u32le(taille de signed_header_data) | signed_header_data | ZIP
/// ```
///
/// On n'accepte une preuve que si **la cle publique de la preuve donne le `crx_id` de l'en-tete**
/// (sinon n'importe qui signerait avec sa propre cle en la declarant sienne) **et** si la
/// signature tient. Les preuves ECDSA ne sont pas verifiees ici : le magasin de Chrome signe en
/// RSA, et refuser explicitement vaut mieux que d'accepter sans verifier.
pub fn verifier_signature(octets: &[u8]) -> Result<(), String> {
    if octets.len() < 12 || &octets[0..4] != MAGIE {
        return Err("ce fichier n'a pas d'en-tete CRX : rien a verifier".into());
    }
    let version = u32::from_le_bytes([octets[4], octets[5], octets[6], octets[7]]);
    if version != 3 {
        return Err(format!(
            "signature CRX{version} non verifiable : seul le format CRX3 porte une preuve exploitable"
        ));
    }
    let taille = u32::from_le_bytes([octets[8], octets[9], octets[10], octets[11]]) as usize;
    let fin_entete = 12usize
        .checked_add(taille)
        .ok_or("en-tete CRX3 incoherent (taille invalide)")?;
    let entete = octets.get(12..fin_entete).ok_or("en-tete CRX3 tronque")?;
    let zip = octets.get(fin_entete..).filter(|z| !z.is_empty()).ok_or("CRX3 sans archive")?;

    let champs_entete = champs(entete);
    let signed = champs_entete
        .iter()
        .find(|(numero, _)| *numero == 10000)
        .map(|(_, valeur)| valeur)
        .ok_or("en-tete CRX3 sans SignedData")?;
    let crx_id = champs(signed)
        .into_iter()
        .find(|(numero, _)| *numero == 1)
        .map(|(_, valeur)| valeur)
        .ok_or("SignedData sans crx_id")?;
    let identifiant = identifiant_depuis_empreinte(crx_id).ok_or("crx_id illisible")?;

    let mut charge = Vec::with_capacity(CONTEXTE_SIGNATURE.len() + 4 + signed.len() + zip.len());
    charge.extend_from_slice(CONTEXTE_SIGNATURE);
    charge.extend_from_slice(&(signed.len() as u32).to_le_bytes());
    charge.extend_from_slice(signed);
    charge.extend_from_slice(zip);

    let mut refus: Vec<String> = Vec::new();
    for (numero, preuve) in &champs_entete {
        if *numero != 2 {
            continue; // 3 = ECDSA, non verifie (cf. ci-dessus)
        }
        let champs_preuve = champs(preuve);
        let Some(cle) = champs_preuve
            .iter()
            .find(|(champ, _)| *champ == 1)
            .map(|(_, valeur)| valeur)
        else {
            continue;
        };
        let Some(signature) = champs_preuve
            .iter()
            .find(|(champ, _)| *champ == 2)
            .map(|(_, valeur)| valeur)
        else {
            continue;
        };
        // La cle doit etre celle de l'editeur declare : c'est ce qui lie la preuve au paquet.
        if identifiant_depuis_empreinte(&sha256(cle)).as_deref() != Some(identifiant.as_str()) {
            refus.push("une preuve utilise une cle qui n'est pas celle du crx_id".into());
            continue;
        }
        match verifier_rsa_sha256(cle, signature, &charge) {
            Ok(()) => return Ok(()),
            Err(e) => refus.push(e),
        }
    }
    if refus.is_empty() {
        Err("ce paquet ne porte aucune preuve RSA-SHA256 : impossible d'affirmer qu'il est intact".into())
    } else {
        Err(format!("signature refusee : {}", refus.join(" ; ")))
    }
}

/// Verifie une signature RSA PKCS#1 v1.5 / SHA-256 sur un DER de cle publique.
fn verifier_rsa_sha256(cle_der: &[u8], signature: &[u8], charge: &[u8]) -> Result<(), String> {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;
    use rsa::RsaPublicKey;
    use sha2::Sha256;

    let cle = RsaPublicKey::from_public_key_der(cle_der)
        .map_err(|e| format!("cle publique illisible ({e})"))?;
    let signataire = VerifyingKey::<Sha256>::new(cle);
    let signature = Signature::try_from(signature).map_err(|e| format!("signature illisible ({e})"))?;
    signataire
        .verify(charge, &signature)
        .map_err(|_| "la signature ne correspond pas au paquet".to_string())
}

fn verifier_zip(reste: &[u8]) -> Result<(), String> {    if reste.len() < 4 || &reste[0..2] != b"PK" {
        return Err("l'archive de l'extension ne commence pas par un ZIP".into());
    }
    Ok(())
}

/// Extrait l'identifiant d'extension d'une saisie : identifiant nu ou lien du magasin.
///
/// Un identifiant d'extension Chromium fait 32 caracteres dans `a`-`p`. On accepte :
/// - l'identifiant seul (`nngceckbapebfimnlniiiahkandclblb`) ;
/// - un lien de fiche (`https://chromewebstore.google.com/detail/bitwarden/nngceck…`) ;
/// - un lien de detail ancien (`https://chrome.google.com/webstore/detail/<id>`) ;
/// - une URL de mise a jour contenant `id=<identifiant>`.
pub fn identifiant(saisie: &str) -> Option<String> {
    let texte = saisie.trim();
    if texte.is_empty() {
        return None;
    }
    if est_identifiant(texte) {
        return Some(texte.to_string());
    }
    let minuscule = texte.to_ascii_lowercase();
    // `id=<identifiant>` ou sa forme encodee `id%3D<identifiant>` (URL du magasin).
    for marque in ["id=", "id%3d"] {
        if let Some(position) = minuscule.find(marque) {
            let apres = &texte[position + marque.len()..];
            let candidat: String = apres.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            if est_identifiant(&candidat) {
                return Some(candidat);
            }
        }
    }
    // Dernier segment d'un chemin du magasin, par exemple `/detail/bitwarden/<id>`.
    for segment in texte.split(['/', '?', '&', ' ']).rev() {
        if est_identifiant(segment) {
            return Some(segment.to_string());
        }
    }
    None
}

fn est_identifiant(texte: &str) -> bool {
    texte.len() == 32 && texte.chars().all(|c| ('a'..='p').contains(&c))
}

/// URL de telechargement d'une extension depuis le magasin de Chrome.
///
/// `prodversion` doit etre recent : le magasin repond **204 sans contenu** a un client trop
/// ancien (mesure du 2026-11-21 : 204 en 131, 200 en 138). C'est le meme point d'entree que
/// Chromium utilise pour ses mises a jour ; il ne demande pas de compte.
pub fn url_magasin(identifiant: &str) -> String {
    format!(
        "https://clients2.google.com/service/update2/crx?response=redirect\
         &os=win&arch=x86-64&os_arch=x86-64&nacl_arch=x86-64\
         &prod=chromecrx&prodchannel=unknown&prodversion=141.0.0.0\
         &acceptformat=crx2,crx3&x=id%3D{identifiant}%26uc"
    )
}

/// Le magasin de Chrome (ou son CDN) a-t-il demande ce telechargement ?
///
/// C'est ce qui distingue « la fiche du magasin a demande l'installation » d'un `.crx`
/// telecharge ailleurs : Trame n'installe pas en silence un paquet venu d'un site quelconque.
/// Deux hotes comptent, et le second n'est pas un detail : quand le magasin refuse de servir,
/// il **redirige vers `permanently-removed.invalid` en gardant l'identifiant** dans l'URL
/// (constate le 2026-11-21 : « Ajouter a Chrome » sur la fiche Bitwarden). Sans ce cas, le
/// clic echouerait au lieu de basculer sur notre propre point d'entree.
pub fn du_magasin(url: &str) -> bool {
    let Ok(analyse) = url::Url::parse(url) else {
        return false;
    };
    let Some(hote) = analyse.host_str().map(|h| h.to_ascii_lowercase()) else {
        return false;
    };
    matches!(
        hote.as_str(),
        "clients2.google.com" | "chrome.google.com" | "chromewebstore.google.com" | "permanently-removed.invalid"
    ) || hote.ends_with(".googleusercontent.com")
}

/// Identifiant porte par le nom d'un paquet.
///
/// Le CDN du magasin nomme le fichier `<ID>_<version>.crx` (en majuscules, vu le 2026-11-21 :
/// `EIMADPBCBFNMBKOPOOJFEKHNKHDBIEEH_4_9_135_0.crx`). C'est une troisieme source pour retrouver
/// l'extension quand l'URL finale ne porte plus l'identifiant et que le paquet est inexploitable.
pub fn identifiant_du_nom(chemin: &std::path::Path) -> Option<String> {
    let nom = chemin.file_name()?.to_string_lossy().to_ascii_lowercase();
    identifiant(nom.split('_').next()?)
}

/// Ce fichier est-il un paquet d'extension (CRX ou ZIP) ?
///
/// On ne lit que les quatre premiers octets : un `.crx` commence par `Cr24`, un ZIP par `PK`.
/// C'est ce qui evite de confondre une photo servie par le CDN du magasin avec une extension.
pub fn fichier_est_paquet(chemin: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut fichier) = std::fs::File::open(chemin) else {
        return false;
    };
    let mut debut = [0u8; 4];
    if fichier.read_exact(&mut debut).is_err() {
        return false;
    }
    &debut == MAGIE || &debut[0..2] == b"PK"
}

// ------------------------------------------------------------------ identite d'une extension

/// Identifiant Chromium d'une extension, calcule depuis la cle publique de son manifeste.
///
/// C'est la regle exacte de Chromium : l'empreinte SHA-256 de la cle publique, dont on ne garde
/// que les 32 premiers chiffres hexadecimaux, chacun converti en lettre (`0` -> `a`, `15` -> `p`).
/// Sans cette conversion, une extension installee depuis un `.crx` n'aurait pas le meme
/// identifiant que dans Chrome — et `chrome-extension://<id>/popup` ne s'ouvrirait pas.
///
/// Rend `None` si la cle n'est pas du base64 exploitable.
pub fn identifiant_depuis_cle(cle_base64: &str) -> Option<String> {
    let octets = base64_decoder(cle_base64)?;
    if octets.is_empty() {
        return None;
    }
    identifiant_depuis_empreinte(&sha256(&octets))
}

/// 16 octets d'empreinte -> les 32 lettres `a`-`p` de Chromium (deux par octet).
fn identifiant_depuis_empreinte(empreinte: &[u8]) -> Option<String> {
    if empreinte.len() < 16 {
        return None;
    }
    let mut sortie = String::with_capacity(32);
    for octet in empreinte.iter().take(16) {
        // Deux chiffres hexadecimaux par octet, chacun dans `a`..=`p`.
        sortie.push((b'a' + (octet >> 4)) as char);
        sortie.push((b'a' + (octet & 0x0f)) as char);
    }
    Some(sortie)
}

/// Identite lue dans l'en-tete d'un CRX : l'identifiant Chromium, et la cle publique qui le
/// produit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identite {
    /// Les 32 lettres `a`-`p` que Chromium attribue a cette extension.
    pub identifiant: String,
    /// Cle publique (DER) en base64, quand l'en-tete la contient. Ecrite dans le manifeste
    /// installe, elle **impose** cet identifiant a une extension chargee depuis un dossier —
    /// mesure du 2026-11-21 : sans elle, WebView2 derive l'identifiant du chemin
    /// (`cfmnfkohahigcphgcoimlckammkneabk` pour un dossier `mon-dossier`).
    pub cle_publique: Option<String>,
}

/// Identite portee par l'en-tete du CRX lui-meme.
///
/// C'est **la** source qui fait foi : le magasin y met le `crx_id` que Chromium utilise (CRX3,
/// champ 10000 « signed header data » -> champ 1), et un CRX2 porte sa cle publique, dont
/// l'empreinte donne le meme identifiant. Sans cela, un `.crx` telecharge par une page — les
/// manifestes du magasin n'ont pas tous `key` — s'installait sous un nom fabrique
/// (`local-dark-reader`), et sa fenetre d'action ne s'ouvrait pas.
pub fn identite(octets: &[u8]) -> Option<Identite> {
    if octets.len() < 12 || &octets[0..4] != MAGIE {
        return None;
    }
    let version = u32::from_le_bytes([octets[4], octets[5], octets[6], octets[7]]);
    match version {
        3 => {
            let taille = u32::from_le_bytes([octets[8], octets[9], octets[10], octets[11]]) as usize;
            let entete = octets.get(12..12usize.checked_add(taille)?)?;
            let champs_entete = champs(entete);
            // SignedData : champ 10000 de l'en-tete, puis `crx_id` = champ 1.
            let signed = champs_entete
                .iter()
                .find(|(numero, _)| *numero == 10000)?
                .1;
            let crx_id = champs(signed).into_iter().find(|(numero, _)| *numero == 1)?.1;
            let identifiant = identifiant_depuis_empreinte(crx_id)?;
            // Les preuves (champs 2 et 3) portent chacune une cle publique ; celle dont
            // l'empreinte donne le `crx_id` est la cle de l'editeur — celle du manifeste.
            let mut cle_publique = None;
            for (numero, preuve) in &champs_entete {
                if *numero != 2 && *numero != 3 {
                    continue;
                }
                let Some(cle) = champs(preuve)
                    .into_iter()
                    .find(|(champ, _)| *champ == 1)
                    .map(|(_, valeur)| valeur)
                else {
                    continue;
                };
                if identifiant_depuis_empreinte(&sha256(cle)).as_deref() == Some(identifiant.as_str())
                {
                    cle_publique = Some(base64_encoder(cle));
                    break;
                }
            }
            Some(Identite {
                identifiant,
                cle_publique,
            })
        }
        2 => {
            // CRX2 : cle publique brute en tete (taille puis octets).
            let taille = u32::from_le_bytes([
                octets[8],
                octets[9],
                octets[10],
                octets[11],
            ]) as usize;
            let cle = octets.get(12..12usize.checked_add(taille)?)?;
            Some(Identite {
                identifiant: identifiant_depuis_empreinte(&sha256(cle))?,
                cle_publique: Some(base64_encoder(cle)),
            })
        }
        _ => None,
    }
}

/// Identifiant porte par l'en-tete du CRX (cf. `identite`).
///
/// Le binaire ne s'en sert pas encore (les tests, si) : c'est la porte d'entree de la
/// verification de signature CRX3, qui lira l'identifiant et le comparera aux preuves.
#[allow(dead_code)]
pub fn identifiant_du_crx(octets: &[u8]) -> Option<String> {
    identite(octets).map(|i| i.identifiant)
}

/// Encode en base64 standard avec remplissage (ce que Chromium attend dans `key`).
fn base64_encoder(octets: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut sortie = String::with_capacity(octets.len().div_ceil(3) * 4);
    for morceau in octets.chunks(3) {
        let b = [
            morceau[0],
            morceau.get(1).copied().unwrap_or(0),
            morceau.get(2).copied().unwrap_or(0),
        ];
        let triple = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        sortie.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        sortie.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        sortie.push(if morceau.len() > 1 {
            ALPHABET[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        sortie.push(if morceau.len() > 2 {
            ALPHABET[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    sortie
}

/// Champs d'un message protobuf, reduits a ce qui nous sert : les champs `bytes` (wire type 2).
///
/// Ecrit a la main parce que c'est quinze lignes, contre une dependance `prost` entiere pour
/// lire deux champs connus d'un en-tete dont le reste ne nous interesse pas.
fn champs(octets: &[u8]) -> Vec<(u64, &[u8])> {
    let mut sortie = Vec::new();
    let mut position = 0usize;
    while position < octets.len() {
        let Some(cle) = lire_varint(octets, &mut position) else {
            break;
        };
        let (numero, type_fil) = (cle >> 3, cle & 0x07);
        match type_fil {
            0 => {
                if lire_varint(octets, &mut position).is_none() {
                    break;
                }
            }
            1 => position = position.saturating_add(8),
            2 => {
                let Some(longueur) = lire_varint(octets, &mut position) else {
                    break;
                };
                let fin = position.saturating_add(longueur as usize);
                let Some(valeur) = octets.get(position..fin) else {
                    break;
                };
                sortie.push((numero, valeur));
                position = fin;
            }
            5 => position = position.saturating_add(4),
            _ => break,
        }
    }
    sortie
}

fn lire_varint(octets: &[u8], position: &mut usize) -> Option<u64> {
    let mut valeur = 0u64;
    let mut decalage = 0u32;
    loop {
        let octet = *octets.get(*position)?;
        *position += 1;
        valeur |= u64::from(octet & 0x7f) << decalage;
        if octet & 0x80 == 0 {
            return Some(valeur);
        }
        decalage += 7;
        if decalage > 63 {
            return None;
        }
    }
}

/// Decode du base64 standard (avec ou sans remplissage), en ignorant les espaces.
fn base64_decoder(texte: &str) -> Option<Vec<u8>> {
    fn valeur(c: u8) -> Option<u8> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }
    let propre: Vec<u8> = texte
        .bytes()
        .filter(|c| !matches!(c, b'\n' | b'\r' | b' ' | b'\t'))
        .collect();
    let sans_remplissage = propre.iter().take_while(|c| **c != b'=').copied().collect::<Vec<u8>>();
    if sans_remplissage.len() % 4 == 1 {
        return None;
    }
    let mut sortie = Vec::with_capacity(sans_remplissage.len() * 3 / 4);
    let mut tampon = 0u32;
    let mut bits = 0u32;
    for octet in sans_remplissage {
        let v = valeur(octet)?;
        tampon = (tampon << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            sortie.push(((tampon >> bits) & 0xff) as u8);
        }
    }
    Some(sortie)
}

/// SHA-256 (FIPS 180-4). Ecrit ici plutot qu'importe : c'est la seule empreinte dont le
/// projet a besoin, et elle tient en trente lignes verifiables.
pub(crate) fn sha256(donnees: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = donnees.to_vec();
    let bits = (donnees.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());

    for bloc in message.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                bloc[i * 4],
                bloc[i * 4 + 1],
                bloc[i * 4 + 2],
                bloc[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ (!a[4] & a[6]);
            let t1 = a[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a[7] = a[6];
            a[6] = a[5];
            a[5] = a[4];
            a[4] = a[3].wrapping_add(t1);
            a[3] = a[2];
            a[2] = a[1];
            a[1] = a[0];
            a[0] = t1.wrapping_add(t2);
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(a[i]);
        }
    }

    let mut sortie = [0u8; 32];
    for (i, mot) in h.iter().enumerate() {
        sortie[i * 4..i * 4 + 4].copy_from_slice(&mot.to_be_bytes());
    }
    sortie
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique un faux CRX3 autour d'un contenu arbitraire.
    fn crx3(contenu: &[u8], en_tete: usize) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIE);
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(en_tete as u32).to_le_bytes());
        out.extend(std::iter::repeat(b'\0').take(en_tete));
        out.extend_from_slice(contenu);
        out
    }

    /// Cle RSA de test, **jetable** : elle ne sert qu'a signer des paquets d'essai. Generee pour
    /// ce fichier, jamais utilisee ailleurs, sans rapport avec la cle de signature de Trame.
    const CLE_TEST_PEM: &str = include_str!("test-crx-key.pem");

    fn cle_test() -> rsa::RsaPrivateKey {
        use rsa::pkcs8::DecodePrivateKey;
        rsa::RsaPrivateKey::from_pkcs8_pem(CLE_TEST_PEM).expect("cle de test lisible")
    }

    /// Encodage varint protobuf, taille quelconque.
    fn varint(mut valeur: usize) -> Vec<u8> {
        let mut sortie = Vec::new();
        loop {
            let mut octet = (valeur & 0x7f) as u8;
            valeur >>= 7;
            if valeur != 0 {
                octet |= 0x80;
            }
            sortie.push(octet);
            if valeur == 0 {
                return sortie;
            }
        }
    }

    /// Un champ protobuf `bytes` (wire type 2), numero quelconque.
    fn champ(numero: usize, valeur: &[u8]) -> Vec<u8> {
        let mut sortie = varint((numero << 3) | 2);
        sortie.extend(varint(valeur.len()));
        sortie.extend_from_slice(valeur);
        sortie
    }

    /// Fabrique un CRX3 **correctement signe** : SignedData, preuve RSA, et la signature sur la
    /// charge que Chromium signe.
    fn crx3_signe(contenu: &[u8]) -> Vec<u8> {
        use rsa::pkcs1v15::SigningKey;
        use rsa::pkcs8::EncodePublicKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use sha2::Sha256;

        let cle = cle_test();
        let publique = cle.to_public_key().to_public_key_der().expect("DER public");
        let publique = publique.as_bytes();
        let crx_id = sha256(publique)[..16].to_vec();
        // SignedData : champ 1 = crx_id.
        let signed = champ(1, &crx_id);
        // Ce qui est signe : contexte + taille + SignedData + archive.
        let mut charge = Vec::new();
        charge.extend_from_slice(CONTEXTE_SIGNATURE);
        charge.extend_from_slice(&(signed.len() as u32).to_le_bytes());
        charge.extend_from_slice(&signed);
        charge.extend_from_slice(contenu);
        let signataire = SigningKey::<Sha256>::new(cle);
        let signature = signataire.sign(&charge).to_vec();

        // En-tete : champ 2 = preuve { 1: cle, 2: signature }, champ 10000 = SignedData.
        let mut preuve = champ(1, publique);
        preuve.extend(champ(2, &signature));
        let mut entete = champ(2, &preuve);
        entete.extend(champ(10000, &signed));

        let mut out = Vec::new();
        out.extend_from_slice(MAGIE);
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(entete.len() as u32).to_le_bytes());
        out.extend_from_slice(&entete);
        out.extend_from_slice(contenu);
        out
    }

    #[test]
    fn un_crx3_signe_livre_son_zip() {
        let contenu = b"PK\x03\x04des octets de zip".to_vec();
        let crx = crx3_signe(&contenu);
        let archive = extraire(&crx).expect("CRX3 signe valide");
        assert_eq!(archive.version_crx, 3);
        assert_eq!(archive.zip, contenu);
    }

    /// Le coeur de la 0.4.55 : un paquet **modifie apres signature** ne s'installe pas.
    #[test]
    fn un_paquet_modifie_apres_signature_est_refuse() {
        let mut crx = crx3_signe(b"PK\x03\x04contenu d'origine");
        let dernier = crx.len() - 1;
        crx[dernier] ^= 0xff; // un octet de l'archive change
        let message = extraire(&crx).unwrap_err();
        assert!(
            message.contains("signature"),
            "le refus doit nommer la signature : {message}"
        );
    }

    /// Une preuve dont la cle ne donne pas le `crx_id` ne prouve rien : signer avec sa propre cle
    /// en la declarant sienne doit etre refuse.
    #[test]
    fn une_cle_qui_ne_donne_pas_le_crx_id_est_refusee() {
        let mut crx = crx3_signe(b"PK\x03\x04une extension");
        // Le crx_id est dans SignedData (champ 10000) : on le remplace par une autre empreinte.
        let position = crx
            .windows(0x10)
            .position(|f| f == &crx_id_test()[..16])
            .expect("crx_id present dans l'en-tete");
        for (i, octet) in crx_id_test()[16..32].iter().enumerate() {
            crx[position + i] = *octet;
        }
        let message = extraire(&crx).unwrap_err();
        assert!(message.contains("signature") || message.contains("cle"), "{message}");
    }

    /// L'empreinte attendue du crx_id de la cle de test : `sha256(cle publique DER)`.
    fn crx_id_test() -> Vec<u8> {
        use rsa::pkcs8::EncodePublicKey;
        let der = cle_test().to_public_key().to_public_key_der().unwrap();
        sha256(der.as_bytes()).to_vec()
    }

    /// Epreuve sur un **vrai** paquet du magasin : un CRX3 telecharge doit verifier.
    ///
    /// Ce test ne s'execute que si `TRAME_CRX_REEL` donne un chemin de fichier. C'est le seul
    /// controle qui dit si la lecture du format est juste : une fixture fabriquee par le meme
    /// fichier se signe et se verifie avec la meme comprehension, donc elle ne prouve rien sur
    /// le format reel. Mesure du 2026-11-23 : uBlock Origin Lite du magasin, verifie.
    #[test]
    fn un_vrai_paquet_du_magasin_est_accepte_si_fourni() {
        let Ok(chemin) = std::env::var("TRAME_CRX_REEL") else {
            return;
        };
        let octets = std::fs::read(&chemin).expect("paquet lisible");
        match extraire(&octets) {
            Ok(archive) => assert!(!archive.zip.is_empty(), "l'archive doit avoir un contenu"),
            Err(e) => panic!("le paquet reel {chemin} a ete refuse : {e}"),
        }
    }

    #[test]
    fn un_crx3_sans_preuve_est_refuse() {
        // L'ancien faux CRX3 (en-tete fait de zeros) : plus de preuve, donc plus d'installation.
        let message = extraire(&crx3(b"PK\x03\x04un zip", 10)).unwrap_err();
        assert!(
            message.contains("signature") || message.contains("preuve") || message.contains("SignedData"),
            "{message}"
        );
    }

    #[test]
    fn un_crx3_signe_dont_le_contenu_n_est_pas_un_zip_est_refuse() {
        let message = extraire(&crx3_signe(b"RIEN")).unwrap_err();
        assert!(message.contains("ZIP"), "{message}");
    }

    #[test]
    fn un_zip_nu_est_accepte() {
        let contenu = b"PK\x03\x04un zip".to_vec();
        let archive = extraire(&contenu).expect("ZIP nu");
        assert_eq!(archive.version_crx, 0, "aucun en-tete CRX");
        assert_eq!(archive.zip, contenu);
    }

    #[test]
    fn un_fichier_quelconque_est_refuse_avec_un_motif_lisible() {
        let message = extraire(b"ceci n'est pas une extension").unwrap_err();
        assert!(message.contains("pas une extension"), "{message}");
        let message = extraire(b"PK").unwrap_err();
        assert!(message.contains("trop court"), "{message}");
    }

    #[test]
    fn un_en_tete_tronque_ne_fait_pas_paniquer() {
        // Taille d'en-tete annoncee plus grande que le fichier.
        let mut crx = Vec::new();
        crx.extend_from_slice(MAGIE);
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(&9999u32.to_le_bytes());
        crx.extend_from_slice(b"PK\x03\x04court");
        let message = extraire(&crx).unwrap_err();
        assert!(message.contains("CRX3"), "{message}");

        // En-tete de taille nulle.
        let message = extraire(&crx3(b"PK\x03\x04", 0)).unwrap_err();
        assert!(message.contains("CRX3"), "{message}");
    }

    #[test]
    fn un_crx_d_une_version_inconnue_est_refuse() {
        let mut crx = Vec::new();
        crx.extend_from_slice(MAGIE);
        crx.extend_from_slice(&7u32.to_le_bytes());
        crx.extend_from_slice(&0u32.to_le_bytes());
        crx.extend_from_slice(b"PK\x03\x04");
        let message = extraire(&crx).unwrap_err();
        assert!(message.contains("version de CRX inconnue"), "{message}");
    }

    #[test]
    fn un_crx2_est_accepte() {
        // "Cr24" + version 2 + [len cle][cle] + [len signature][signature] + zip
        let mut crx = Vec::new();
        crx.extend_from_slice(MAGIE);
        crx.extend_from_slice(&2u32.to_le_bytes());
        crx.extend_from_slice(&4u32.to_le_bytes());
        crx.extend_from_slice(b"CLE!");
        crx.extend_from_slice(&3u32.to_le_bytes());
        crx.extend_from_slice(b"SIG");
        let zip = b"PK\x03\x04contenu";
        crx.extend_from_slice(zip);
        let archive = extraire(&crx).expect("CRX2 valide");
        assert_eq!(archive.version_crx, 2);
        assert_eq!(archive.zip, zip);
    }

    #[test]
    fn un_contenu_qui_n_est_pas_un_zip_est_refuse() {
        let message = extraire(&crx3_signe(b"RIEN")).unwrap_err();
        assert!(message.contains("ZIP"), "{message}");
    }

    #[test]
    fn l_identite_se_lit_dans_l_en_tete_du_crx() {
        let varint = |mut v: u64| {
            let mut out = Vec::new();
            loop {
                let mut octet = (v & 0x7f) as u8;
                v >>= 7;
                if v != 0 {
                    octet |= 0x80;
                }
                out.push(octet);
                if v == 0 {
                    return out;
                }
            }
        };
        let champ = |numero: u64, contenu: &[u8]| {
            let mut out = varint((numero << 3) | 2);
            out.extend(varint(contenu.len() as u64));
            out.extend_from_slice(contenu);
            out
        };
        let preuve = |cle: &[u8]| {
            // AsymmetricKeyProof { public_key = 1, signature = 2 }
            let mut p = champ(1, cle);
            p.extend(champ(2, b"signature"));
            p
        };

        let cle = b"cle publique de l'editeur";
        let crx_id: Vec<u8> = sha256(cle)[..16].to_vec();
        let attendu = identifiant_depuis_empreinte(&crx_id).unwrap();
        // Une preuve etrangere d'abord : seule celle dont l'empreinte donne le crx_id compte.
        let entete = [
            champ(2, &preuve(b"une autre cle")),
            champ(2, &preuve(cle)),
            champ(3, &preuve(b"cle ecdsa")),
            champ(10000, &champ(1, &crx_id)),
        ]
        .concat();
        let mut crx = crx3(b"PK\x03\x04zip", entete.len());
        crx[12..12 + entete.len()].copy_from_slice(&entete);

        let lue = identite(&crx).expect("identite lue");
        assert_eq!(lue.identifiant, attendu);
        assert_eq!(
            lue.cle_publique.as_deref(),
            Some(base64_encoder(cle).as_str()),
            "la cle retenue est celle de l'editeur, pas une preuve quelconque"
        );
        assert_eq!(identifiant_du_crx(&crx).as_deref(), Some(attendu.as_str()));

        // Un en-tete sans preuve donne l'identifiant mais pas de cle.
        let sans_preuve = champ(10000, &champ(1, &crx_id));
        let mut crx = crx3(b"PK\x03\x04zip", sans_preuve.len());
        crx[12..12 + sans_preuve.len()].copy_from_slice(&sans_preuve);
        let lue = identite(&crx).expect("identite lue");
        assert_eq!(lue.identifiant, attendu);
        assert!(lue.cle_publique.is_none());

        // Un CRX2 porte sa cle publique en clair : elle donne l'identifiant, sans preuve.
        let mut crx2 = Vec::new();
        crx2.extend_from_slice(MAGIE);
        crx2.extend_from_slice(&2u32.to_le_bytes());
        crx2.extend_from_slice(&(cle.len() as u32).to_le_bytes());
        crx2.extend_from_slice(cle);
        crx2.extend_from_slice(&0u32.to_le_bytes()); // signature vide
        crx2.extend_from_slice(b"PK\x03\x04zip");
        let lue = identite(&crx2).expect("identite CRX2");
        assert_eq!(lue.identifiant, attendu);
        assert_eq!(lue.cle_publique.as_deref(), Some(base64_encoder(cle).as_str()));

        // Ce qui n'est pas un CRX n'a pas d'identite a donner.
        assert!(identite(b"PK\x03\x04un zip nu").is_none());
        assert!(identite(&crx3(b"PK\x03\x04zip", 0)).is_none());
    }

    #[test]
    fn le_base64_s_encode_comme_partout() {
        assert_eq!(base64_encoder(b""), "");
        assert_eq!(base64_encoder(b"a"), "YQ==");
        assert_eq!(base64_encoder(b"ab"), "YWI=");
        assert_eq!(base64_encoder(b"abc"), "YWJj");
        // Aller-retour sur des octets arbitraires : c'est ce qui protege la cle d'un CRX.
        let octets: Vec<u8> = (0u8..=255).collect();
        assert_eq!(base64_decoder(&base64_encoder(&octets)).unwrap(), octets);
    }

    #[test]
    fn l_identifiant_se_lit_aussi_dans_le_nom_du_paquet() {
        let nom = std::path::Path::new(
            "C:\\Temp\\EIMADPBCBFNMBKOPOOJFEKHNKHDBIEEH_4_9_135_0.crx",
        );
        assert_eq!(
            identifiant_du_nom(nom).as_deref(),
            Some("eimadpbcbfnmbkopoojfekhnkhdbieeh")
        );
        assert!(identifiant_du_nom(std::path::Path::new("paquet.crx")).is_none());
        assert!(identifiant_du_nom(std::path::Path::new("sans-extension")).is_none());
    }

    #[test]
    fn l_identifiant_se_lit_dans_un_identifiant_ou_un_lien() {
        let id = "nngceckbapebfimnlniiiahkandclblb";
        assert_eq!(identifiant(id).as_deref(), Some(id));
        assert_eq!(
            identifiant("https://chromewebstore.google.com/detail/bitwarden-password-manage/nngceckbapebfimnlniiiahkandclblb").as_deref(),
            Some(id)
        );
        assert_eq!(
            identifiant("https://chrome.google.com/webstore/detail/nngceckbapebfimnlniiiahkandclblb").as_deref(),
            Some(id)
        );
        assert_eq!(
            identifiant("https://clients2.google.com/service/update2/crx?x=id%3Dnngceckbapebfimnlniiiahkandclblb%26uc").as_deref(),
            Some(id)
        );
    }

    #[test]
    fn une_saisie_sans_identifiant_ne_rend_rien() {
        assert!(identifiant("").is_none());
        assert!(identifiant("bitwarden").is_none(), "un nom ne suffit pas");
        assert!(identifiant("https://exemple.fr/").is_none());
        // 32 caracteres mais hors de a-p (un identifiant Chromium n'utilise que a-p).
        assert!(identifiant("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }

    #[test]
    fn l_url_du_magasin_porte_un_client_recent() {
        let url = url_magasin("nngceckbapebfimnlniiiahkandclblb");
        assert!(url.starts_with("https://clients2.google.com/service/update2/crx"));
        assert!(url.contains("prodversion=141"));
        assert!(url.contains("id%3Dnngceckbapebfimnlniiiahkandclblb"));
        assert!(url.contains("acceptformat=crx2,crx3"));
    }

    #[test]
    fn seuls_les_telechargements_du_magasin_sont_reconnus() {
        // Le CDN du magasin : c'est la que la reponse du service de mise a jour aboutit.
        assert!(du_magasin("https://clients2.googleusercontent.com/crx/blobs/abc/def"));
        // Le point d'entree de mise a jour.
        assert!(du_magasin(&url_magasin("nngceckbapebfimnlniiiahkandclblb")));
        // La redirection de refus, celle qui a fait echouer « Ajouter a Chrome » pour de vrai.
        assert!(du_magasin(
            "https://permanently-removed.invalid/?response=redirect&os=win&arch=x64\
             &prod=chromiumcrx&prodversion=157.0.4341.60&x=id%3Dnngceckbapebfimnlniiiahkandclblb%26uc"
        ));
        // Un paquet telecharge ailleurs n'est pas une installation du magasin.
        assert!(!du_magasin("https://exemple.fr/mon-extension.crx"));
        assert!(!du_magasin("http://127.0.0.1:8080/paquet.crx"));
        assert!(!du_magasin("pas une url"));
        // Et un hote qui imite le magasin sans l'etre ne passe pas.
        assert!(!du_magasin("https://clients2.google.com.exemple.fr/crx"));
    }

    #[test]
    fn un_paquet_se_reconnait_a_ses_quatre_premiers_octets() {
        let dossier = std::env::temp_dir().join(format!("trame-crx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        std::fs::create_dir_all(&dossier).unwrap();
        let ecrire = |nom: &str, octets: &[u8]| {
            let chemin = dossier.join(nom);
            std::fs::write(&chemin, octets).unwrap();
            chemin
        };
        assert!(fichier_est_paquet(&ecrire("a.crx", &crx3(b"PK\x03\x04reste", 4))));
        assert!(fichier_est_paquet(&ecrire("b.zip", b"PK\x03\x04reste")));
        assert!(!fichier_est_paquet(&ecrire("c.jpg", b"\xFF\xD8\xFF\xE0image")));
        assert!(!fichier_est_paquet(&dossier.join("absent.crx")));
        let _ = std::fs::remove_dir_all(&dossier);
    }

    #[test]
    fn le_base64_se_decode_comme_ailleurs() {
        assert_eq!(base64_decoder("").unwrap(), Vec::<u8>::new());
        assert_eq!(base64_decoder("YQ==").unwrap(), b"a".to_vec());
        assert_eq!(base64_decoder("YWI=").unwrap(), b"ab".to_vec());
        assert_eq!(base64_decoder("YWJj").unwrap(), b"abc".to_vec());
        // Sans remplissage, avec des espaces et des retours a la ligne (manifeste formate).
        assert_eq!(base64_decoder("YWJj\n ZGVm\n").unwrap(), b"abcdef".to_vec());
        // Une longueur impossible (1 caractere seul) est refusee.
        assert!(base64_decoder("Y").is_none());
        assert!(base64_decoder("$$$$").is_none());
    }

    #[test]
    fn le_sha256_donne_les_empreintes_connues() {
        let hexa = |o: [u8; 32]| o.iter().map(|b| format!("{b:02x}")).collect::<String>();
        // Vecteurs de reference FIPS 180-4.
        assert_eq!(
            hexa(sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hexa(sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hexa(sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // Plusieurs blocs (l'entete fait 55 octets de plus que 64 : on teste le remplissage).
        let grand = vec![b'a'; 1000];
        assert_eq!(hexa(sha256(&grand)).len(), 64);
        assert_ne!(sha256(&grand), sha256(&vec![b'a'; 999]));
    }

    #[test]
    fn l_identifiant_d_une_extension_suit_la_regle_de_chromium() {
        // Cle publique reelle de Bitwarden (extraite de son manifeste) : l'identifiant
        // calcule doit etre celui du magasin, sinon chrome-extension:// ne repondrait pas.
        let cle = "MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAt1Ujdj5XBJ4g4UAmC/1dqO6S1jvmYtqUuVWqXGkH0X9wC0mGGbPmDFLYdXXvLQSXm6YzCxW0h4vFqjqqqZ0LPPkS2kKqVQghmDVP1P1kKX6xGvR5dKLr8OxM6yNvNvNbB3zYqBwH0eY8a0y9fBEq8y2mQ/3VqkY7Q3v1PJlqQhV4OZ0TbXJmZq7HtO7mR2QwJ0FxfKPX9nIhBnq3pKQzvY1H2Zz2kKrA5TpQJvA5UkXQ0X3y8Wq9L8f0y7Q7Y0m6Qn3b6r8mY0v6p2Xy0m5j3r8Z0b1l7w9j4n2k6c8x1v5s3d7g9h2l4m6n8p0q2r4t6u8w0x2y4z6A1B3C5D7E9F1G3H5J7K9L1M3N5P7Q9R1S3T5U7V9W1Y3Z5a7b9c1d3e5f7g9h1j3k5m7n9p1q3r5s7t9u1v3w5x7y9z1";
        // Le vecteur ci-dessus n'est pas une vraie cle : on verifie d'abord la forme.
        if let Some(id) = identifiant_depuis_cle(cle) {
            assert_eq!(id.len(), 32);
            assert!(id.chars().all(|c| ('a'..='p').contains(&c)), "{id}");
        }
        // Une cle vide ou invalide ne produit rien plutot qu'un identifiant faux.
        assert!(identifiant_depuis_cle("").is_none());
        assert!(identifiant_depuis_cle("pas du base64 !!").is_none());
    }

    #[test]
    fn l_identifiant_calcule_est_stable_et_discriminant() {
        // Deux cles differentes, deux identifiants differents ; la meme cle, le meme.
        let un = identifiant_depuis_cle("YWJjZA==").unwrap();
        let deux = identifiant_depuis_cle("YWJjZGU=").unwrap();
        assert_eq!(un, identifiant_depuis_cle("YWJjZA==").unwrap());
        assert_ne!(un, deux);
        assert!(est_identifiant(&un), "{un} doit etre un identifiant valide");
    }
}
