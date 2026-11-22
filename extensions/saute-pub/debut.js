/* Saute-pub YouTube — retrait des pubs AVANT le lecteur (injecte au debut du document).
   YouTube decrit les pubs d'une video dans la reponse du lecteur : `adPlacements`, `adSlots`,
   `playerAds`. Le lecteur ne programme que ce qu'il y trouve : on retire ces cles partout ou
   la reponse passe, avant que le code de YouTube ne la lise :
     - `ytInitialPlayerResponse` / `ytInitialData` (ecrits par un script de la page) ;
     - `JSON.parse` (reponses lues en texte) ;
     - `Response.prototype.json` et `.text` (appels `fetch` a /youtubei/v1/player et /next).
   Une pub cousue dans le flux video lui-meme (essais de YouTube depuis 2025) n'est pas
   decrite ainsi : contenu.js reste le filet (son coupe, x16, « Passer »).
   Aucune requete n'est envoyee, rien n'est lu hors de la reponse. */

(() => {
  if (window.__trameSansPub) return;
  const etat = { retirees: 0, avertissements: 0 };
  window.__trameSansPub = { etat: () => ({ ...etat }) };

  const CLES = ["adPlacements", "adSlots", "playerAds", "adBreakHeartbeatParams"];

  // Retire les cles de pub d'un objet de reponse (et de son `playerResponse` imbrique).
  // Renvoie vrai si quelque chose a ete retire.
  function elaguer(o, profondeur = 0) {
    if (!o || typeof o !== "object" || profondeur > 3) return false;
    if (Array.isArray(o)) {
      let touche = false;
      for (const x of o) touche = elaguer(x, profondeur + 1) || touche;
      return touche;
    }
    let touche = false;
    for (const c of CLES) {
      if (Object.prototype.hasOwnProperty.call(o, c)) {
        delete o[c];
        touche = true;
      }
    }
    // Message « les bloqueurs de pub ne sont pas autorises » (fenetre qui bloque la lecture).
    const msg = o.auxiliaryUi && o.auxiliaryUi.messageRenderers;
    if (msg && msg.enforcementMessageViewModel) {
      delete msg.enforcementMessageViewModel;
      etat.avertissements++;
    }
    if (o.playerResponse) touche = elaguer(o.playerResponse, profondeur + 1) || touche;
    if (touche) etat.retirees++;
    return touche;
  }

  // 1. Variables globales ecrites par les scripts de la page.
  for (const nom of ["ytInitialPlayerResponse", "ytInitialData"]) {
    let valeur;
    try {
      Object.defineProperty(window, nom, {
        configurable: true,
        get: () => valeur,
        set: (v) => {
          try { elaguer(v); } catch (e) {}
          valeur = v;
        },
      });
    } catch (e) {
      /* deja defini par la page : on n'y touche pas */
    }
  }

  // 2. JSON.parse : la verification est un simple test de cle, sans cout sensible.
  const parseOrigine = JSON.parse;
  JSON.parse = function (texte, ...reste) {
    const r = parseOrigine.call(this, texte, ...reste);
    try {
      if (r && typeof r === "object" && !Array.isArray(r)) elaguer(r);
    } catch (e) {}
    return r;
  };

  // 3. fetch : reponses du lecteur et de la page de lecture.
  const VISEES = /\/youtubei\/v1\/(player|next|reel\/reel_watch_sequence)\b/;
  const viseeParLaReponse = (rep) => {
    try { return VISEES.test(rep.url || ""); } catch (e) { return false; }
  };
  const jsonOrigine = Response.prototype.json;
  Response.prototype.json = function () {
    const p = jsonOrigine.call(this);
    if (!viseeParLaReponse(this)) return p;
    return p.then((o) => {
      try { elaguer(o); } catch (e) {}
      return o;
    });
  };
  const texteOrigine = Response.prototype.text;
  Response.prototype.text = function () {
    const p = texteOrigine.call(this);
    if (!viseeParLaReponse(this)) return p;
    return p.then((t) => {
      if (typeof t !== "string" || !/"(adPlacements|adSlots|playerAds)"/.test(t)) return t;
      try {
        const o = parseOrigine(t);
        return elaguer(o) ? JSON.stringify(o) : t;
      } catch (e) {
        return t;
      }
    });
  };
})();
