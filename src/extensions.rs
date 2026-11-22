//! Moteur d'extensions : manifeste, correspondance d'URL, injection CSS/JS, stockage.
//!
//! Perimetre assume et documente (cf. `docs/FORMAT-EXTENSION.md`) : scripts de contenu
//! injectes en fin de chargement, une API `chrome` reduite mais reelle (`storage.local`,
//! `runtime.id`, `runtime.getManifest`, `dom.style`). Il n'y a pas de page d'arriere-plan,
//! pas de `webRequest`, pas de `tabs` : mieux vaut une API petite et honnete qu'une API
//! imitee qui ment sur ses capacites.

use std::fs;
use std::path::Path;

use crate::model::{Extension, Manifeste, Motif};
use crate::{alerte, info};

/// Amorce installee au demarrage de chaque document (avant les scripts de la page).
/// Elle fabrique l'objet `chrome` remis a chaque extension : un objet par extension, pour
/// que `chrome.runtime.id` et le stockage soient cloisonnes.
pub const AMORCE: &str = r#"(() => {
  if (window.__trame_amorce) return;
  window.__trame_amorce = true;
  const envoi = (o) => { try { window.ipc.postMessage(JSON.stringify(o)); } catch (e) {} };
  window.__trame_attente = {};
  window.__trame_seq = 0;
  window.__trame_appel = (ext, op, payload) => new Promise((res) => {
    const rid = ext + ':' + (++window.__trame_seq);
    window.__trame_attente[rid] = res;
    envoi({ m: 'extCall', rid, ext, op, payload });
    setTimeout(() => {
      if (window.__trame_attente[rid]) { delete window.__trame_attente[rid]; res(null); }
    }, 5000);
  });
  window.__trame_reponse = (rid, data) => {
    const f = window.__trame_attente[rid];
    if (f) { delete window.__trame_attente[rid]; f(data); }
  };
  window.__trame_make = (id, manifeste) => {
    const appel = (op, payload) => window.__trame_appel(id, op, payload);
    return {
      runtime: { id: id, getManifest: () => manifeste, lastError: null },
      storage: {
        local: {
          get(cles, cb) { const p = appel('storage.get', cles); if (cb) p.then(cb); return p; },
          set(obj, cb) { const p = appel('storage.set', obj); if (cb) p.then(cb); return p; },
          remove(cles, cb) { const p = appel('storage.remove', cles); if (cb) p.then(cb); return p; },
          clear(cb) { const p = appel('storage.clear', null); if (cb) p.then(cb); return p; }
        }
      },
      dom: {
        style(css) {
          const s = document.createElement('style');
          s.setAttribute('data-trame-ext', id);
          s.textContent = css;
          (document.head || document.documentElement).appendChild(s);
          return s;
        }
      }
    };
  };
})();"#;

/// Charge toutes les extensions d'un dossier. Une extension cassee n'empeche pas les autres.
pub fn charger(dossier: &Path) -> Vec<Extension> {
    let mut out = Vec::new();
    let Ok(lecture) = fs::read_dir(dossier) else {
        return out;
    };
    let mut dossiers: Vec<_> = lecture.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dossiers.sort();
    for chemin in dossiers {
        let id = chemin
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let manifeste_path = chemin.join("manifest.json");
        let Ok(texte) = fs::read_to_string(&manifeste_path) else {
            continue;
        };
        let manifeste: Manifeste = match serde_json::from_str(&texte) {
            Ok(m) => m,
            Err(e) => {
                alerte!("extension {id} : manifeste invalide ({e})");
                continue;
            }
        };
        if manifeste.run_at != "document_end" {
            info!(
                "extension {id} : run_at=\"{}\" ignore, injection en fin de chargement",
                manifeste.run_at
            );
        }
        let mut js = Vec::new();
        for fichier in &manifeste.js {
            match fs::read_to_string(chemin.join(fichier)) {
                Ok(contenu) => js.push(contenu),
                Err(e) => alerte!("extension {id} : {fichier} illisible ({e})"),
            }
        }
        let mut css = String::new();
        for fichier in &manifeste.css {
            match fs::read_to_string(chemin.join(fichier)) {
                Ok(contenu) => {
                    css.push_str(&contenu);
                    css.push('\n');
                }
                Err(e) => alerte!("extension {id} : {fichier} illisible ({e})"),
            }
        }
        let motifs: Vec<Motif> = manifeste
            .matches
            .iter()
            .filter_map(|m| compiler_motif(m))
            .collect();
        if motifs.is_empty() {
            alerte!("extension {id} : aucun motif d'URL exploitable, elle ne s'appliquera jamais");
        }
        info!(
            "extension chargee : {id} v{} ({} script(s), {} motif(s), active={})",
            manifeste.version,
            js.len(),
            motifs.len(),
            manifeste.enabled_by_default
        );
        out.push(Extension {
            id,
            dossier: chemin,
            // Une extension qui touche a l'apparence d'un site doit etre livree eteinte :
            // `enabled_by_default: false` dans le manifeste, sinon allumee.
            active: manifeste.enabled_by_default,
            manifeste,
            js,
            css,
            motifs,
        });
    }
    out
}

/// Compile `*://*.exemple.fr/chemin*` en motif comparable.
pub fn compiler_motif(motif: &str) -> Option<Motif> {
    let (schema, reste) = motif.split_once("://")?;
    let (hote, chemin) = match reste.find('/') {
        Some(pos) => (&reste[..pos], &reste[pos..]),
        None => (reste, "/*"),
    };
    if hote.is_empty() {
        return None;
    }
    Some(Motif {
        tout_schema: schema == "*",
        schema: schema.to_string(),
        hote: hote.to_string(),
        chemin: chemin.to_string(),
    })
}

/// L'URL correspond-elle au motif ?
pub fn correspond(motif: &Motif, url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let schema = u.scheme();
    if !motif.tout_schema && motif.schema != schema {
        return false;
    }
    let Some(hote) = u.host_str() else {
        return false;
    };
    let hote_ok = if let Some(base) = motif.hote.strip_prefix("*.") {
        hote == base || hote.ends_with(&format!(".{base}"))
    } else if motif.hote == "*" {
        true
    } else {
        hote == motif.hote
    };
    if !hote_ok {
        return false;
    }
    correspond_glob(&motif.chemin, u.path())
}

/// Glob `*` simple (sans `?` ni classes), iteratif, sans allocation.
fn correspond_glob(motif: &str, texte: &str) -> bool {
    let m: Vec<char> = motif.chars().collect();
    let t: Vec<char> = texte.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut etoile, mut reprise) = (None::<usize>, 0usize);
    while j < t.len() {
        if i < m.len() && (m[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < m.len() && m[i] == '*' {
            etoile = Some(i);
            reprise = j;
            i += 1;
        } else if let Some(e) = etoile {
            i = e + 1;
            reprise += 1;
            j = reprise;
        } else {
            return false;
        }
    }
    while i < m.len() && m[i] == '*' {
        i += 1;
    }
    i == m.len()
}

/// Code JS injecte dans une page pour une extension donnee.
pub fn injection(ext: &Extension) -> String {
    let id = serde_json::to_string(&ext.id).unwrap_or_else(|_| "\"\"".to_string());
    let manifeste = serde_json::json!({
        "name": ext.manifeste.name,
        "version": ext.manifeste.version,
        "description": ext.manifeste.description,
        "author": ext.manifeste.author,
    });
    let manifeste = serde_json::to_string(&manifeste).unwrap_or_else(|_| "{}".to_string());
    let css = serde_json::to_string(&ext.css).unwrap_or_else(|_| "\"\"".to_string());
    let corps = ext.js.join("\n;\n");
    format!(
        r#"(function () {{
  if (!window.__trame_make) return;
  window.__trame_ext = window.__trame_ext || {{}};
  if (window.__trame_ext[{id}]) return;
  window.__trame_ext[{id}] = true;
  var chrome = window.__trame_make({id}, {manifeste});
  try {{
    var css = {css};
    if (css && css.length) chrome.dom.style(css);
  }} catch (e) {{ console.error('[trame] css {id}', e); }}
  try {{
    (function (chrome) {{
{corps}
    }})(chrome);
  }} catch (e) {{ console.error('[trame] {id}', e); }}
}})();"#
    )
}

/// Extensions actives qui doivent s'appliquer a cette URL.
pub fn applicables<'a>(extensions: &'a [Extension], url: &str) -> Vec<&'a Extension> {
    extensions
        .iter()
        .filter(|e| e.active && e.motifs.iter().any(|m| correspond(m, url)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_motifs_comprennent_les_sous_domaines() {
        let m = compiler_motif("*://*.exemple.fr/*").unwrap();
        assert!(correspond(&m, "https://exemple.fr/a"));
        assert!(correspond(&m, "https://www.exemple.fr/a"));
        assert!(correspond(&m, "http://sous.exemple.fr/a"));
        assert!(!correspond(&m, "https://autre.fr/a"));
        assert!(!correspond(&m, "https://exemple.fr.evil.com/a"));
    }

    #[test]
    fn le_chemin_est_un_glob() {
        let m = compiler_motif("https://exemple.fr/wiki/*").unwrap();
        assert!(correspond(&m, "https://exemple.fr/wiki/Rust"));
        assert!(correspond(&m, "https://exemple.fr/wiki/"));
        assert!(!correspond(&m, "https://exemple.fr/blog/Rust"));
    }

    #[test]
    fn un_schema_impose_est_respecte() {
        let m = compiler_motif("https://exemple.fr/*").unwrap();
        assert!(!correspond(&m, "http://exemple.fr/a"));
    }

    #[test]
    fn glob_quelques_cas() {
        assert!(correspond_glob("/*", "/nimporte/quoi"));
        assert!(correspond_glob("/wiki/*", "/wiki/Rust"));
        assert!(!correspond_glob("/wiki/*", "/blog/Rust"));
        assert!(correspond_glob("*lithium*", "/a/lithium-batterie/x"));
    }
}
