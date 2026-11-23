/* Propre à Android : relaie l'état du saute-pub à l'appli (pastille 🛡 de la barre d'adresse)
   et au journal (logcat, préfixe « [saute-pub] »), seulement quand il change.
   contenu.js et ce script partagent le même monde isolé : window.__trameSautePub y est visible. */
(() => {
  let dernier = "";
  let pubs = 0;
  let pubAvant = false;
  function envoyer() {
    const s = window.__trameSautePub;
    if (!s || typeof s.etat !== "function") return;
    const e = s.etat();
    if (e.pubEnCours && !pubAvant) pubs++;
    pubAvant = !!e.pubEnCours;
    const avant = e.avantLecteur || null;
    const msg = {
      type: "saute-pub",
      retirees: avant ? avant.retirees || 0 : 0,
      avertissements: avant ? avant.avertissements || 0 : 0,
      debutActif: !!avant,
      pubs,
      sponsors: e.sponsor ? e.sponsor.sautes : 0,
      pubEnCours: !!e.pubEnCours,
      lecteur: !!e.lecteurObserve,
    };
    const texte = JSON.stringify(msg);
    if (texte === dernier) return;
    dernier = texte;
    console.log("[saute-pub] " + location.pathname + " " + texte);
    try {
      browser.runtime.sendNativeMessage("trame", msg).catch(() => {});
    } catch (err) {
      /* appli sans relais : le journal suffit */
    }
  }
  setInterval(envoyer, 700);
  envoyer();
})();
