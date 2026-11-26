//! Normalisation de ce que l'utilisateur tape dans la Couture, et extraction du domaine.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use url::Url;

/// Page d'accueil interne.
pub const NOUVEL_ONGLET: &str = "trame://localhost/newtab.html";

/// Un moteur de recherche choisi par l'utilisateur.
pub struct Moteur {
    pub id: &'static str,
    pub nom: &'static str,
    /// Gabarit d'URL, `{q}` remplace la requete encodee.
    pub gabarit: &'static str,
    /// Parametre de langue, deja inclus dans le gabarit (champ informatif).
    pub langue: &'static str,
}

/// Moteurs proposes. Ajouter une ligne suffit : l'interface et le reglage suivent.
///
/// Les gabarits ont ete verifies le 2026-11-20. Perplexity redirige `…/search?q=`
/// vers sa route interne `…/search/new?q=` : la redirection observee prouve que la
/// route est acceptee.
///
/// `langue` : parametre a ajouter pour forcer une interface et des resultats francais.
/// Sans lui, DuckDuckGo choisit sa langue d'apres l'adresse reseau et peut repondre en
/// anglais (« aliexpress at DuckDuckGo »). Ce sont des parametres d'URL publics et
/// documentes par les moteurs ; on ne peut pas garantir qu'ils restent eternels.
pub const MOTEURS: [Moteur; 8] = [
    Moteur { id: "perplexity", nom: "Perplexity", gabarit: "https://www.perplexity.ai/search?q={q}", langue: "" },
    Moteur { id: "google", nom: "Google", gabarit: "https://www.google.com/search?q={q}&hl=fr&gl=fr", langue: "fr" },
    Moteur { id: "duckduckgo", nom: "DuckDuckGo", gabarit: "https://duckduckgo.com/?q={q}&kl=fr-fr", langue: "fr" },
    Moteur { id: "brave", nom: "Brave Search", gabarit: "https://search.brave.com/search?q={q}", langue: "" },
    Moteur { id: "qwant", nom: "Qwant", gabarit: "https://www.qwant.com/?q={q}&t=web", langue: "fr" },
    Moteur { id: "startpage", nom: "Startpage", gabarit: "https://www.startpage.com/sp/search?query={q}", langue: "" },
    Moteur { id: "ecosia", nom: "Ecosia", gabarit: "https://www.ecosia.org/search?q={q}", langue: "" },
    Moteur { id: "bing", nom: "Bing", gabarit: "https://www.bing.com/search?q={q}&setlang=fr&cc=FR", langue: "fr" },
];

/// Moteur par defaut.
pub const MOTEUR_DEFAUT: &str = "perplexity";

/// Retrouve un moteur par identifiant ; retombe sur le defaut si l'identifiant est inconnu.
pub fn moteur(id: &str) -> &'static Moteur {
    MOTEURS.iter().find(|m| m.id == id).unwrap_or(&MOTEURS[0])
}

/// Transforme une saisie en URL navigable, avec le moteur choisi.
///
/// Regles, dans l'ordre :
/// 1. vide -> page d'accueil interne ;
/// 2. schema explicite -> conserve tel quel ;
/// 3. forme `hote[:port][/chemin]` sans espace -> `https://` ;
/// 4. sinon -> recherche sur le moteur choisi.
/// Adresse interne de Chromium, que WebView2 **refuse** de servir.
///
/// `chrome-search://local-ntp/local-ntp.html` est la page d'accueil interne du moteur. Quand
/// un code de page y navigue, WebView2 affiche « impossible d'accéder à cette page /
/// ERR_INVALID_URL » et la webview est morte : plus rien ne s'y charge. On refuse donc la
/// navigation **avant** qu'elle n'ait lieu, et on ne restaure jamais une telle adresse.
pub fn interne_chromium(url: &str) -> bool {
    let minuscule = url.trim().to_ascii_lowercase();
    // `chrome-extension://` n'est pas interne : c'est une vraie origine servie par WebView2.
    if minuscule.starts_with("chrome-extension://") {
        return false;
    }
    [
        "chrome-search:",
        "chrome://",
        "chrome-untrusted:",
        "devtools:",
        "edge:",
        "about:srcdoc",
        "view-source:",
    ]
    .iter()
    .any(|schema| minuscule.starts_with(schema))
}

pub fn normaliser_avec(saisie: &str, moteur_id: &str) -> String {
    let s = saisie.trim();
    if s.is_empty() {
        return NOUVEL_ONGLET.to_string();
    }

    let minuscule = s.to_ascii_lowercase();
    for schema in ["http://", "https://", "file://", "trame://", "about:"] {
        if minuscule.starts_with(schema) {
            return s.to_string();
        }
    }
    // Autre schema explicite (ftp:, mailto:...) : on respecte l'intention.
    if let Some(pos) = s.find("://") {
        if pos > 0 && s[..pos].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.') {
            return s.to_string();
        }
    }

    if ressemble_a_un_hote(s) {
        let prefixe = if s.starts_with("localhost") { "http://" } else { "https://" };
        return format!("{prefixe}{s}");
    }

    let requete = utf8_percent_encode(s, NON_ALPHANUMERIC).to_string();
    moteur(moteur_id).gabarit.replace("{q}", &requete)
}

// ------------------------------------------------------------------ parametres de pistage

/// Parametres de pistage connus : ils ne servent qu'a la mesure publicitaire, jamais a la page.
/// Liste volontairement **prudente** (mieux vaut en laisser un que casser un site) ; les prefixes
/// finissent par `_` ou `*`.
const PISTAGE: &[&str] = &[
    // campagnes et clics publicitaires (Google, Microsoft, Meta, TikTok, Yandex, Mailchimp...)
    "utm_*", "gclid", "gclsrc", "gbraid", "wbraid", "dclid", "fbclid", "msclkid", "ttclid",
    "twclid", "yclid", "igshid", "mc_cid", "mc_eid", "_hsenc", "_hsmi", "mkt_tok", "oly_anon_id",
    "oly_enc_id", "vero_id", "wickedid", "rb_clickid", "s_cid", "ef_id",
    // Amazon : parametres de campagne et de placement (la page n'en a pas besoin)
    "hv*", "adgrpid", "hydadcr", "mcid", "pd_rd_*", "pf_rd_*", "ref_",
    // AliExpress : traces de navigation
    "spm", "scm", "algo_pvid", "algo_exp_id", "utparam", "gatewayadapt", "_randl_shipto",
];

/// Identifiants d'affiliation : ils remunerent un createur, on peut vouloir les garder.
const AFFILIATION: &[&str] = &["tag", "ascsubtag", "aff_fcid", "aff_fsk", "aff_platform", "aff_trace_key"];

fn correspond(nom: &str, motif: &str) -> bool {
    let nom = nom.to_ascii_lowercase();
    match motif.strip_suffix('*') {
        Some(prefixe) => nom.starts_with(prefixe),
        None => nom == motif,
    }
}

/// Retire d'une adresse web ses parametres de pistage. Rend `None` si rien n'a change (ou si ce
/// n'est pas une adresse http(s)) : l'appelant sait alors qu'il n'y a rien a faire.
pub fn nettoyer(url: &str, garder_affiliation: bool) -> Option<String> {
    let mut u = Url::parse(url).ok()?;
    if !matches!(u.scheme(), "http" | "https") || u.query().is_none() {
        return None;
    }
    let paires: Vec<(String, String)> = u.query_pairs().into_owned().collect();
    let gardees: Vec<&(String, String)> = paires
        .iter()
        .filter(|(nom, _)| {
            let pistage = PISTAGE.iter().any(|m| correspond(nom, m));
            let affiliation = AFFILIATION.iter().any(|m| correspond(nom, m));
            !(pistage || (affiliation && !garder_affiliation))
        })
        .collect();
    if gardees.len() == paires.len() {
        return None;
    }
    if gardees.is_empty() {
        u.set_query(None);
    } else {
        u.query_pairs_mut()
            .clear()
            .extend_pairs(gardees.iter().map(|(n, v)| (n.as_str(), v.as_str())));
    }
    Some(u.to_string())
}

/// Raccourci pour les tests : moteur par defaut.
#[cfg(test)]
pub fn normaliser(saisie: &str) -> String {
    normaliser_avec(saisie, MOTEUR_DEFAUT)
}

/// Deux adresses désignent-elles le **même fichier** ? On compare l'hôte et le chemin, pas la
/// requête : les CDN y mettent des jetons signés qui changent d'un appel à l'autre (Discord :
/// `ex=`, `is=`, `hm=`), et une redirection peut en ajouter.
///
/// Sert à reconnaître l'onglet dont la navigation **est** un téléchargement (voir
/// `App::telechargement_debut`) : cet onglet n'a aucune page à montrer.
pub fn meme_fichier(a: &str, b: &str) -> bool {
    let cle = |s: &str| -> Option<(String, String)> {
        let u = url::Url::parse(s).ok()?;
        Some((u.host_str()?.to_ascii_lowercase(), u.path().to_string()))
    };
    match (cle(a), cle(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

fn ressemble_a_un_hote(s: &str) -> bool {
    if s.contains(char::is_whitespace) {
        return false;
    }
    let hote = s.split('/').next().unwrap_or("");
    let hote = hote.split(':').next().unwrap_or(hote);
    if hote.is_empty() {
        return false;
    }
    if hote == "localhost" {
        return true;
    }
    // Suffixe alphabetique de 2 a 24 caracteres, sans point final.
    if let Some(point) = hote.rfind('.') {
        let tld = &hote[point + 1..];
        let avant = &hote[..point];
        !avant.is_empty()
            && !tld.is_empty()
            && tld.len() <= 24
            && tld.chars().all(|c| c.is_ascii_alphabetic())
            && !hote.contains(char::is_uppercase) || avant.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    } else {
        false
    }
}

/// Domaine enregistrable approximatif (`fr.wikipedia.org` -> `wikipedia.org`).
///
/// Liste volontairement courte : on ne cherche pas a implementer la Public Suffix List,
/// seulement a regrouper les onglets d'un meme site.
pub fn domaine_enregistrable(url: &str) -> String {
    let hote = match Url::parse(url) {
        Ok(u) => match u.host_str() {
            Some(h) => h.to_string(),
            None => return String::new(),
        },
        Err(_) => return String::new(),
    };
    let h = hote.trim_start_matches("www.").to_string();
    // Une adresse IP n'a pas de domaine enregistrable : le domaine, c'est l'adresse.
    if !h.is_empty() && h.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return h;
    }
    if h.contains(':') {
        return h;
    }
    let parties: Vec<&str> = h.split('.').collect();
    if parties.len() <= 2 {
        return h;
    }
    const SUFFIXES_DOUBLES: [&str; 12] = [
        "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "net.au", "co.nz", "co.jp", "com.br",
        "co.za", "gouv.fr", "asso.fr",
    ];
    let deux_derniers = format!("{}.{}", parties[parties.len() - 2], parties[parties.len() - 1]);
    if SUFFIXES_DOUBLES.contains(&deux_derniers.as_str()) && parties.len() >= 3 {
        return format!("{}.{deux_derniers}", parties[parties.len() - 3]);
    }
    deux_derniers
}

/// Nom lisible d'une page interne, pour l'onglet et la fenetre.
pub fn nom_page_interne(url: &str) -> Option<&'static str> {
    if !url.starts_with("trame:") {
        return None;
    }
    if url.contains("reglages.html") {
        Some("Reglages")
    } else if url.contains("newtab.html") {
        Some("Nouvel onglet")
    } else if url.contains("trame.html") {
        Some("La trame")
    } else {
        None
    }
}

/// Affichage court d'une URL dans la barre : retire le schema et le `www.`.
pub fn affichage_court(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    if let Some(reste) = url.strip_prefix("trame://localhost/") {
        return format!("trame://{reste}");
    }
    let sans_schema = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    let mut s = sans_schema.trim_end_matches('/').to_string();
    if let Some(reste) = s.strip_prefix("www.") {
        s = reste.to_string();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_pistage_est_retire_et_le_reste_garde() {
        // L'adresse Amazon reelle du 2026-11-21 (abregee) : campagne et placement partent.
        let brut = "https://www.amazon.fr/?tag=hydraamazon05-21&ref=nav_signin&adgrpid=199603402545&hvpone=&hvadid=826521108237&hvpos=&hydadcr=11802_2559773_371192&mcid=2e9ea856";
        assert_eq!(
            nettoyer(brut, true).as_deref(),
            Some("https://www.amazon.fr/?tag=hydraamazon05-21&ref=nav_signin")
        );
        // Sans garder l'affiliation, `tag` part aussi.
        assert_eq!(nettoyer(brut, false).as_deref(), Some("https://www.amazon.fr/?ref=nav_signin"));
        // Campagnes classiques, et un parametre utile garde a sa place.
        assert_eq!(
            nettoyer("https://exemple.fr/article?id=42&utm_source=x&utm_medium=y&fbclid=abc", true).as_deref(),
            Some("https://exemple.fr/article?id=42")
        );
        // Tout part : plus de « ? » orphelin. Le fragment reste.
        assert_eq!(
            nettoyer("https://exemple.fr/a?gclid=1#section", true).as_deref(),
            Some("https://exemple.fr/a#section")
        );
    }

    #[test]
    fn rien_a_nettoyer_ne_change_rien() {
        assert_eq!(nettoyer("https://exemple.fr/recherche?q=rust&page=2", true), None);
        assert_eq!(nettoyer("https://exemple.fr/", true), None);
        assert_eq!(nettoyer("trame://localhost/newtab.html?utm_source=x", true), None);
        assert_eq!(nettoyer("pas une adresse", true), None);
        // Un parametre qui ressemble sans etre de la liste (« tagada ») n'est pas touche.
        assert_eq!(nettoyer("https://exemple.fr/?tagada=1&spmx=2", false), None);
        // Une adresse de retour encodee dans un parametre n'est pas decortiquee.
        assert_eq!(
            nettoyer("https://www.amazon.fr/ap/signin?openid.return_to=https%3A%2F%2Fwww.amazon.fr%2F%3Futm_source%3Dx", true),
            None
        );
    }

    #[test]
    fn saisies() {
        assert_eq!(normaliser(""), NOUVEL_ONGLET);
        assert_eq!(normaliser("example.com"), "https://example.com");
        assert_eq!(normaliser("localhost:3080"), "http://localhost:3080");
        assert_eq!(normaliser("https://a.fr/x"), "https://a.fr/x");
        assert!(normaliser("combien d onglets").starts_with("https://www.perplexity.ai/search?q="));
        assert_eq!(domaine_enregistrable("https://fr.wikipedia.org/wiki/X"), "wikipedia.org");
        assert_eq!(domaine_enregistrable("https://news.bbc.co.uk/x"), "bbc.co.uk");
        assert_eq!(domaine_enregistrable("https://192.0.2.5:8006/"), "192.0.2.5");
        assert_eq!(affichage_court("https://www.example.com/a/"), "example.com/a");
    }

    #[test]
    fn chaque_moteur_a_un_gabarit_utilisable() {
        for m in MOTEURS.iter() {
            assert!(m.gabarit.contains("{q}"), "{} sans emplacement de requete", m.id);
            let url = normaliser_avec("test de recherche", m.id);
            assert!(url.starts_with("https://"), "{} -> {url}", m.id);
            assert!(!url.contains("{q}"), "{} gabarit non substitue", m.id);
        }
    }

    #[test]
    fn un_moteur_inconnu_retombe_sur_le_defaut() {
        let url = normaliser_avec("bonjour", "moteur-qui-nexiste-pas");
        assert!(url.starts_with(moteur(MOTEUR_DEFAUT).gabarit.split("{q}").next().unwrap()));
    }

    #[test]
    fn les_adresses_internes_de_chromium_sont_reconnues() {
        // WebView2 refuse ces schemas (ERR_INVALID_URL) : ils ne doivent jamais etre charges.
        assert!(super::interne_chromium(
            "chrome-search://local-ntp/local-ntp.html"
        ));
        assert!(super::interne_chromium("chrome://settings"));
        assert!(super::interne_chromium("  CHROME-SEARCH://local-ntp/  "));
        assert!(super::interne_chromium("devtools://devtools/bundled/x.html"));
        assert!(super::interne_chromium("edge://flags"));
    }

    #[test]
    fn une_extension_et_le_web_ne_sont_pas_des_adresses_internes() {
        // `chrome-extension://` est une vraie origine servie par WebView2 : la refuser
        // casserait toutes les extensions Chrome.
        assert!(!super::interne_chromium(
            "chrome-extension://nngceckbapebfimnlniiiahkandclblb/popup/index.html"
        ));
        assert!(!super::interne_chromium("https://fr.aliexpress.com/item/1.html"));
        assert!(!super::interne_chromium("trame://localhost/newtab.html"));
        assert!(!super::interne_chromium("about:blank"));
    }

    /// Deux adresses du même fichier se reconnaissent malgré le jeton signé du CDN : c'est ce qui
    /// permet de refermer l'onglet ouvert pour un téléchargement (0.4.73).
    #[test]
    fn deux_adresses_du_meme_fichier_se_reconnaissent() {
        let a = "https://cdn.discordapp.com/attachments/772/157/Trame-setup.exe?ex=1&is=2&hm=3";
        let b = "https://cdn.discordapp.com/attachments/772/157/Trame-setup.exe?ex=9&is=8&hm=7";
        assert!(super::meme_fichier(a, b), "même hôte, même chemin : même fichier");
        assert!(
            super::meme_fichier("https://cdn.exemple.fr/gros.exe", "https://CDN.EXEMPLE.fr/gros.exe"),
            "la casse de l'hôte ne compte pas"
        );
        // Deux fichiers differents du meme hote ne se confondent pas.
        assert!(!super::meme_fichier("https://cdn.exemple.fr/a.exe", "https://cdn.exemple.fr/b.exe"));
        assert!(!super::meme_fichier("https://a.fr/x.exe", "https://b.fr/x.exe"));
        // Ce qui n'est pas une adresse se compare tel quel (jamais d'egalite abusive).
        assert!(super::meme_fichier("pas une adresse", "pas une adresse"));
        assert!(!super::meme_fichier("pas une adresse", "autre chose"));
    }
}
