//! Le carnet : favoris, pages guettees et memoire de lecture.
//!
//! Trois choses que l'on garde d'une session a l'autre, toutes **dans le profil, sur le PC** :
//!   - les favoris (`favoris.json`) ;
//!   - les guets (`guets.json`) : un element d'une page (un prix, un stock, un resultat) que
//!     Trame relit de lui-meme et dont il signale le changement ;
//!   - la memoire de lecture (`lectures.jsonl`) : le texte des pages reellement lues, pour
//!     retrouver une page a partir d'une phrase dont on se souvient.
//!
//! La memoire de lecture n'est jamais gardee en memoire vive : elle peut peser une vingtaine de
//! megaoctets, et Trame doit rester leger. Le fichier est relu par un fil a part a chaque
//! recherche (quelques dizaines de millisecondes), puis oublie.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::path::Path;

/// Un favori : **une page precise** (un article, une video, un chemin), gardee pour y revenir
/// (`favoris.json`, Ctrl+D). A ne pas confondre avec un acces rapide, qui mene a un site.
/// Distincts depuis le 2026-11-22 (demande de Clement) : avant, c'etait une seule liste.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Favori {
    pub url: String,
    #[serde(default)]
    pub titre: String,
    #[serde(default)]
    pub ajoute_le: u64,
    /// Adresse de l'icone du site, reprise de l'onglet au moment de l'ajout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icone: Option<String>,
    /// Dossier du favori (« Recettes », « Travail › Clients »…). Vide : hors dossier.
    /// Un seul niveau dans l'interface ; un import garde le chemin des sous-dossiers en un nom.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub dossier: String,
}

/// Le fichier des dossiers de favoris crees (un dossier vide doit survivre a un redemarrage).
pub const NOM_FICHIER_FAVORIS_DOSSIERS: &str = "favoris-dossiers.json";

/// Le nom du fichier des acces rapides, ecrit ici et non a la main a chaque appel : la
/// sauvegarde (`sauvegarde.rs`) doit emporter le meme fichier que l'application ecrit, sinon
/// elle sauvegarde une liste vide sans que rien ne le signale.
pub const NOM_FICHIER_ACCES: &str = "acces.json";

/// Un acces rapide : un bouton qui mene **a un site** (`acces.json`), sur la page « Nouvel
/// onglet » et, pour au plus [`RAIL_MAX`] d'entre eux, dans la bande sous le logo du rail.
/// Une seule liste, dans l'ordre choisi par l'utilisateur.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Acces {
    pub url: String,
    #[serde(default)]
    pub titre: String,
    #[serde(default)]
    pub ajoute_le: u64,
    /// Groupe d'acces rapides (« Maison », « Loisirs »…). Vide : aucun groupe.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub groupe: String,
    /// Aussi affiche dans la bande d'acces rapides du rail.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rail: bool,
    /// Adresse de l'icone du site, reprise de l'onglet au moment de l'ajout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icone: Option<String>,
}

/// Encodage base64 standard (pour garder une icone en `data:` dans le carnet).
pub fn base64(octets: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(octets.len().div_ceil(3) * 4);
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16 | (*bloc.get(1).unwrap_or(&0) as u32) << 8 | *bloc.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if bloc.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if bloc.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// L'icone est-elle deja gardee en image (et non en adresse a recharger) ?
pub fn icone_locale(icone: &Option<String>) -> bool {
    icone.as_deref().map(|i| i.starts_with("data:")).unwrap_or(false)
}

/// Places de la bande d'acces rapides du rail.
pub const RAIL_MAX: usize = 10;

/// Une adresse qui peut devenir un favori / acces rapide (page web ou fichier local).
pub fn favorisable(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file:")
}

/// La racine du site d'une page (`https://www.youtube.com/watch?v=…` → `https://www.youtube.com/`) :
/// un acces rapide cree depuis un onglet mene au site, pas a la page.
pub fn racine_du_site(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) if u.scheme() == "http" || u.scheme() == "https" => match u.host_str() {
            Some(h) => match u.port() {
                Some(p) => format!("{}://{h}:{p}/", u.scheme()),
                None => format!("{}://{h}/", u.scheme()),
            },
            None => url.to_string(),
        },
        _ => url.to_string(),
    }
}

/// Nom court d'un site pour un acces rapide : `youtube.com` → « Youtube », une adresse IP ou un
/// nom sans point (avec son port) reste tel quel.
pub fn nom_du_site(url: &str) -> String {
    let h = hote(url);
    let sans_port = h.split(':').next().unwrap_or("");
    if sans_port.is_empty() || sans_port.parse::<std::net::IpAddr>().is_ok() || !sans_port.contains('.') {
        return h;
    }
    let parties: Vec<&str> = sans_port.split('.').collect();
    // Le nom avant le suffixe : « amazon » de amazon.fr, « bbc » de bbc.co.uk.
    let i = if parties.len() >= 3 && parties[parties.len() - 2].len() <= 3 { parties.len() - 3 } else { parties.len() - 2 };
    let mot = parties[i];
    let mut c = mot.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => h,
    }
}

/// Hote d'une adresse, sans `www.`, avec le port s'il y en a un. Sert a dire si un acces
/// rapide est deja ouvert dans un onglet.
pub fn hote(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => {
            let h = u.host_str().unwrap_or("").trim_start_matches("www.").to_ascii_lowercase();
            match u.port() {
                Some(p) => format!("{h}:{p}"),
                None => h,
            }
        }
        Err(_) => String::new(),
    }
}

/// Une page guettee : un element (selecteur CSS) dont on surveille le texte.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guet {
    pub id: u64,
    pub url: String,
    pub selecteur: String,
    /// Titre de la page au moment ou le guet a ete pose.
    #[serde(default)]
    pub libelle: String,
    /// Dernier texte lu.
    #[serde(default)]
    pub texte: String,
    /// Texte d'avant le dernier changement (pour dire « 39,90 € → 34,90 € »).
    #[serde(default)]
    pub ancien: String,
    #[serde(default)]
    pub cree_le: u64,
    #[serde(default)]
    pub verifie_le: u64,
    #[serde(default)]
    pub change_le: Option<u64>,
    /// Le changement a ete vu (le guet a ete ouvert depuis).
    #[serde(default = "vrai")]
    pub vu: bool,
    /// Lectures ratees d'affilee (element introuvable, page en erreur).
    #[serde(default)]
    pub echecs: u32,
    /// Intervalle entre deux lectures, en minutes.
    #[serde(default = "intervalle_defaut")]
    pub intervalle_min: u32,
}

fn vrai() -> bool {
    true
}

fn intervalle_defaut() -> u32 {
    30
}

impl Guet {
    /// Le guet est-il a relire maintenant ? Apres des echecs, on espace (jusqu'a 4 fois).
    pub fn du(&self, now: u64) -> bool {
        let facteur = (1 + self.echecs.min(3)) as u64;
        let attente = self.intervalle_min.max(5) as u64 * 60_000 * facteur;
        now.saturating_sub(self.verifie_le) >= attente
    }
}

/// Une page lue, telle qu'elle est ecrite dans `lectures.jsonl`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lecture {
    pub url: String,
    #[serde(default)]
    pub titre: String,
    pub ts: u64,
    #[serde(default)]
    pub texte: String,
}

/// Une page retrouvee dans la memoire de lecture.
#[derive(Clone, Debug, PartialEq)]
pub struct Trouvee {
    pub url: String,
    pub titre: String,
    pub ts: u64,
    /// Le passage ou les mots apparaissent, tire du texte d'origine.
    pub extrait: String,
    pub score: f32,
}

/// Taille maximale du texte garde par page (caracteres).
pub const TEXTE_MAX: usize = 12_000;
/// Au-dela, le fichier est compacte (une entree par adresse, les plus recentes).
pub const FICHIER_MAX_OCTETS: u64 = 24 * 1024 * 1024;
/// Pages gardees apres compactage.
pub const LECTURES_MAX: usize = 1500;

/// Repli d'un caractere pour la recherche : minuscule, sans accent. Un caractere donne
/// toujours **un** caractere : les positions du texte replie sont celles du texte d'origine,
/// ce qui permet d'en tirer l'extrait.
pub fn replier(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ý' | 'ÿ' => 'y',
        'œ' => 'o',
        'æ' => 'a',
        '’' | '‘' => '\'',
        _ => c,
    }
}

/// Mots d'une requete, replies ; les mots d'une lettre sont ignores.
pub fn mots(requete: &str) -> Vec<String> {
    let replie: String = requete.chars().map(replier).collect();
    let mut vus = HashSet::new();
    replie
        .split(|c: char| !c.is_alphanumeric())
        .filter(|m| m.chars().count() >= 2)
        .filter(|m| vus.insert(m.to_string()))
        .map(|m| m.to_string())
        .collect()
}

/// Ramene un texte de page a l'essentiel : espaces fusionnes, longueur bornee.
pub fn condenser(texte: &str) -> String {
    let mut out = String::with_capacity(texte.len().min(TEXTE_MAX * 2));
    let mut espace = false;
    let mut n = 0usize;
    for c in texte.chars() {
        if c.is_whitespace() {
            espace = true;
            continue;
        }
        if espace && !out.is_empty() {
            out.push(' ');
            n += 1;
        }
        espace = false;
        out.push(c);
        n += 1;
        if n >= TEXTE_MAX {
            break;
        }
    }
    out
}

/// Deux lectures d'un guet sont-elles differentes ? Les espaces ne comptent pas.
pub fn texte_change(ancien: &str, nouveau: &str) -> bool {
    condenser(ancien) != condenser(nouveau)
}

fn extrait(texte: &[char], replie: &[char], mot: &[char]) -> String {
    let pos = replie
        .windows(mot.len().max(1))
        .position(|w| w == mot)
        .unwrap_or(0);
    let debut = pos.saturating_sub(60);
    let fin = (pos + mot.len() + 90).min(texte.len());
    // Couper sur des espaces, pour ne pas commencer au milieu d'un mot.
    let debut = (debut..pos).find(|&i| i == 0 || texte[i - 1] == ' ').unwrap_or(debut);
    let fin = (fin..texte.len()).find(|&i| texte[i] == ' ').unwrap_or(fin).min(texte.len());
    let mut s: String = texte[debut..fin].iter().collect();
    if debut > 0 {
        s.insert(0, '…');
    }
    if fin < texte.len() {
        s.push('…');
    }
    s
}

/// Cherche dans la memoire de lecture. Tous les mots doivent figurer (titre, adresse ou
/// texte) ; une page lue plusieurs fois compte une fois (sa lecture la plus recente).
/// `abandon` est consulte entre deux lignes : une frappe plus recente arrete la recherche.
pub fn chercher_lectures(
    chemin: &Path,
    requete: &str,
    limite: usize,
    now: u64,
    abandon: &dyn Fn() -> bool,
) -> Vec<Trouvee> {
    let mots: Vec<Vec<char>> = mots(requete).iter().map(|m| m.chars().collect()).collect();
    if mots.is_empty() {
        return Vec::new();
    }
    let Ok(fichier) = std::fs::File::open(chemin) else {
        return Vec::new();
    };
    let lignes: Vec<String> = std::io::BufReader::new(fichier).lines().map_while(Result::ok).collect();
    let mut vues: HashSet<String> = HashSet::new();
    let mut out: Vec<Trouvee> = Vec::new();
    for (n, ligne) in lignes.iter().rev().enumerate() {
        if n % 64 == 0 && abandon() {
            return Vec::new();
        }
        let Ok(l) = serde_json::from_str::<Lecture>(ligne) else { continue };
        if !vues.insert(l.url.clone()) {
            continue;
        }
        let texte: Vec<char> = l.texte.chars().collect();
        let replie: Vec<char> = texte.iter().map(|c| replier(*c)).collect();
        let titre: Vec<char> = l.titre.chars().map(replier).collect();
        let adresse: Vec<char> = l.url.chars().map(replier).collect();
        let contient = |botte: &[char], aiguille: &[char]| {
            !aiguille.is_empty() && botte.windows(aiguille.len()).any(|w| w == aiguille)
        };
        let mut score = 0.0f32;
        let mut tous = true;
        let mut premier_dans_texte: Option<&Vec<char>> = None;
        for m in &mots {
            let t = contient(&titre, m);
            let x = contient(&replie, m);
            let a = contient(&adresse, m);
            if !(t || x || a) {
                tous = false;
                break;
            }
            score += if t { 1.0 } else { 0.0 } + if x { 0.6 } else { 0.0 } + if a { 0.3 } else { 0.0 };
            if x && premier_dans_texte.is_none() {
                premier_dans_texte = Some(m);
            }
        }
        if !tous {
            continue;
        }
        // Les mots qui se suivent comme dans la requete valent mieux que des mots epars.
        let phrase: Vec<char> = mots.join(&' ').into_iter().collect();
        if mots.len() > 1 && contient(&replie, &phrase) {
            score += 1.5;
        }
        let age_j = now.saturating_sub(l.ts) as f32 / 86_400_000.0;
        score += 0.8 * (-age_j / 30.0).exp();
        let passage = match premier_dans_texte {
            Some(m) => extrait(&texte, &replie, m),
            None => texte.iter().take(140).collect(),
        };
        out.push(Trouvee { url: l.url, titre: l.titre, ts: l.ts, extrait: passage, score });
    }
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limite);
    out
}

/// Ajoute une page lue a la memoire (une ligne), puis compacte si le fichier est trop gros.
pub fn ajouter_lecture(chemin: &Path, lecture: &Lecture) -> std::io::Result<()> {
    let ligne = serde_json::to_string(lecture).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(chemin)?;
    writeln!(f, "{ligne}")?;
    drop(f);
    if std::fs::metadata(chemin).map(|m| m.len()).unwrap_or(0) > FICHIER_MAX_OCTETS {
        compacter(chemin, LECTURES_MAX, &|_| true)?;
    }
    Ok(())
}

/// Reecrit la memoire : une ligne par adresse (la plus recente), au plus `garder` pages, et
/// seulement celles que `retenir` accepte. Rend le nombre de pages gardees.
pub fn compacter(chemin: &Path, garder: usize, retenir: &dyn Fn(&Lecture) -> bool) -> std::io::Result<usize> {
    let Ok(fichier) = std::fs::File::open(chemin) else { return Ok(0) };
    let lignes: Vec<String> = std::io::BufReader::new(fichier).lines().map_while(Result::ok).collect();
    let mut vues = HashSet::new();
    let mut gardees: Vec<String> = Vec::new();
    for ligne in lignes.iter().rev() {
        let Ok(l) = serde_json::from_str::<Lecture>(ligne) else { continue };
        if !vues.insert(l.url.clone()) || !retenir(&l) {
            continue;
        }
        gardees.push(ligne.clone());
        if gardees.len() >= garder {
            break;
        }
    }
    gardees.reverse();
    let mut texte = gardees.join("\n");
    if !texte.is_empty() {
        texte.push('\n');
    }
    let temporaire = chemin.with_extension("tmp");
    std::fs::write(&temporaire, texte)?;
    std::fs::rename(&temporaire, chemin)?;
    Ok(gardees.len())
}

/// Nombre de pages dans la memoire (une par adresse).
pub fn compter_lectures(chemin: &Path) -> usize {
    let Ok(fichier) = std::fs::File::open(chemin) else { return 0 };
    let mut vues = HashSet::new();
    for ligne in std::io::BufReader::new(fichier).lines().map_while(Result::ok) {
        if let Ok(l) = serde_json::from_str::<Lecture>(&ligne) {
            vues.insert(l.url);
        }
    }
    vues.len()
}

/// Ce qui ne doit jamais entrer dans la memoire de lecture.
pub fn memorisable(url: &str, domaine: &str, exclus: &[String]) -> bool {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return false;
    }
    !exclus.iter().any(|d| !d.is_empty() && (domaine == d || domaine.ends_with(&format!(".{d}"))))
}

/// Script de lecture d'une page : rend `null` si la page ne doit pas etre gardee (un champ de
/// mot de passe, un document vide), sinon `{titre, texte}`.
pub const SCRIPT_LECTURE: &str = r#"(function(){try{
if(document.querySelector('input[type=password]'))return null;
var b=document.body;if(!b)return null;
var t=(b.innerText||'').slice(0,40000);
if(t.replace(/\s+/g,'').length<200)return null;
return {titre:document.title||'',texte:t};
}catch(e){return null;}})()"#;

/// Script de lecture d'un guet : le texte de l'element, ou `null` s'il est introuvable.
pub fn script_guet(selecteur: &str) -> String {
    let s = serde_json::to_string(selecteur).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "(function(){{try{{var e=document.querySelector({s});if(!e)return null;\
         var t=(e.innerText||e.textContent||'').trim();return t.slice(0,400);}}catch(x){{return null;}}}})()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dossier() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "trame-carnet-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn base64_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn acces_rapide_depuis_une_page() {
        assert_eq!(racine_du_site("https://www.youtube.com/watch?v=abc"), "https://www.youtube.com/");
        assert_eq!(racine_du_site("http://192.0.2.23:8720/courbes/"), "http://192.0.2.23:8720/");
        assert_eq!(nom_du_site("https://www.youtube.com/watch?v=abc"), "Youtube");
        assert_eq!(nom_du_site("https://www.amazon.fr/dp/123"), "Amazon");
        assert_eq!(nom_du_site("https://www.bbc.co.uk/news"), "Bbc");
        assert_eq!(nom_du_site("http://192.0.2.23:8720/"), "192.0.2.23:8720");
        assert_eq!(nom_du_site("http://localhost:3000/"), "localhost:3000");
    }

    #[test]
    fn le_repli_garde_une_position_par_caractere() {
        let texte = "Élégance À Noël";
        let replie: String = texte.chars().map(replier).collect();
        assert_eq!(replie, "elegance a noel");
        assert_eq!(texte.chars().count(), replie.chars().count());
    }

    #[test]
    fn les_mots_d_une_lettre_et_les_doublons_sont_ignores() {
        assert_eq!(mots("la batterie à l'état solide la"), vec!["la", "batterie", "etat", "solide"]);
    }

    #[test]
    fn on_retrouve_une_page_par_une_phrase_sans_accents() {
        let d = dossier();
        let f = d.join("lectures.jsonl");
        let texte = "Introduction. Les batteries à électrolyte solide promettent une densité double. Fin.";
        ajouter_lecture(&f, &Lecture { url: "https://a.fr/x".into(), titre: "Batteries".into(), ts: 1000, texte: texte.into() }).unwrap();
        ajouter_lecture(&f, &Lecture { url: "https://b.fr/y".into(), titre: "Cuisine".into(), ts: 2000, texte: "Une recette de tarte aux pommes, très simple et rapide à faire chez soi.".into() }).unwrap();
        let r = chercher_lectures(&f, "electrolyte solide", 5, 3000, &|| false);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].url, "https://a.fr/x");
        assert!(r[0].extrait.contains("électrolyte solide"), "extrait tire du texte d'origine : {}", r[0].extrait);
        assert!(chercher_lectures(&f, "electrolyte pommes", 5, 3000, &|| false).is_empty(), "tous les mots doivent figurer");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn une_page_relue_ne_compte_qu_une_fois_et_le_compactage_garde_la_plus_recente() {
        let d = dossier();
        let f = d.join("lectures.jsonl");
        for (ts, t) in [(1, "ancien texte sur les moteurs"), (2, "nouveau texte sur les moteurs")] {
            ajouter_lecture(&f, &Lecture { url: "https://a.fr/".into(), titre: "A".into(), ts, texte: t.into() }).unwrap();
        }
        let r = chercher_lectures(&f, "moteurs", 5, 3, &|| false);
        assert_eq!(r.len(), 1);
        assert!(r[0].extrait.contains("nouveau"));
        assert_eq!(compacter(&f, 10, &|_| true).unwrap(), 1);
        assert_eq!(compter_lectures(&f), 1);
        assert!(std::fs::read_to_string(&f).unwrap().contains("nouveau"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn une_recherche_abandonnee_ne_rend_rien() {
        let d = dossier();
        let f = d.join("lectures.jsonl");
        ajouter_lecture(&f, &Lecture { url: "https://a.fr/".into(), titre: "A".into(), ts: 1, texte: "moteurs".into() }).unwrap();
        assert!(chercher_lectures(&f, "moteurs", 5, 3, &|| true).is_empty());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn les_sites_exclus_et_les_pages_internes_ne_sont_pas_memorises() {
        let exclus = vec!["banque.fr".to_string()];
        assert!(memorisable("https://wikipedia.org/a", "wikipedia.org", &exclus));
        assert!(!memorisable("https://www.banque.fr/compte", "banque.fr", &exclus));
        assert!(!memorisable("https://client.banque.fr/", "client.banque.fr", &exclus));
        assert!(!memorisable("trame://localhost/newtab.html", "", &exclus));
        assert!(!memorisable("chrome-extension://abc/popup.html", "", &exclus));
    }

    #[test]
    fn un_guet_ignore_les_espaces_et_s_espace_apres_des_echecs() {
        assert!(!texte_change("39,90 €", "  39,90\n€ "));
        assert!(texte_change("39,90 €", "34,90 €"));
        let mut g = Guet {
            id: 1, url: "https://a.fr".into(), selecteur: "#prix".into(), libelle: String::new(),
            texte: String::new(), ancien: String::new(), cree_le: 0, verifie_le: 0, change_le: None,
            vu: true, echecs: 0, intervalle_min: 30,
        };
        assert!(g.du(30 * 60_000));
        assert!(!g.du(29 * 60_000));
        g.echecs = 2;
        assert!(!g.du(60 * 60_000), "apres deux echecs, on attend trois intervalles");
        assert!(g.du(90 * 60_000));
    }

    #[test]
    fn le_texte_est_condense_et_borne() {
        assert_eq!(condenser("  a \n\n b\tc  "), "a b c");
        assert_eq!(condenser(&"x".repeat(TEXTE_MAX + 50)).chars().count(), TEXTE_MAX);
    }

    #[test]
    fn le_script_de_guet_echappe_le_selecteur() {
        let s = script_guet("a[title=\"x\"]");
        assert!(s.contains(r#"querySelector("a[title=\"x\"]")"#));
    }
}
