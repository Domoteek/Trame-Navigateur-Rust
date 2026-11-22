(function (regles) {
  /* Script vivant du zappeur, pose dans la page a chaque chargement (et a chaque nouvelle regle).
     Chaque regle : i (index), s (selecteur), t (condition de texte), a (action), v (valeur).
     - masquer avec condition de texte (`t`) : le CSS ne lit pas un texte, on cache ici ;
     - `texte` : reecrit le texte de l'element (l'original est garde pour la page d'origine) ;
     - `replier` : le CSS replie, un premier clic deplie (le clic ne part pas vers la page) ;
     - compte ce que chaque regle attrape, remonte a Trame (`zapCompte`) quand ca change ;
     - recommence quand la page ajoute du contenu (YouTube charge en continu), 700 ms au plus ;
     - `origine(true/false)` : montre la page sans les regles, le temps d'un appui.
     Le texte est lu par `textContent` (pas `innerText`) : pas de calcul de mise en page, et un
     element deja cache garde son texte. Expression de fonction : Trame l'appelle avec les regles. */
  if (window.__trame_zap_vivant) window.__trame_zap_vivant.arreter();
  const CACHE = "data-trame-zap-texte";
  const REECRIT = "data-trame-zap-reecrit";
  const PRET = "data-trame-repli-pret";
  const originaux = new WeakMap();
  let dernier = "";
  let minuterie = null;
  let enOrigine = false;

  const deplier = (e) => {
    const el = e.currentTarget;
    if (!el.hasAttribute("data-trame-deplie")) {
      el.setAttribute("data-trame-deplie", "1");
      e.preventDefault();
      e.stopPropagation();
    }
  };

  const passer = () => {
    minuterie = null;
    if (enOrigine) return;
    const comptes = [];
    for (const r of regles) {
      let liste;
      try {
        liste = document.querySelectorAll(r.s);
      } catch (e) {
        comptes.push({ i: r.i, n: 0 });
        continue;
      }
      let n = 0;
      if (r.t && !r.a) {
        for (const el of liste) {
          if (el.getAttribute(CACHE) === "1" || (el.textContent || "").trim() === r.t) {
            if (el.getAttribute(CACHE) !== "1") {
              el.setAttribute(CACHE, "1");
              el.style.setProperty("display", "none", "important");
            }
            n++;
          }
        }
      } else if (r.a === "texte") {
        for (const el of liste) {
          if (el.textContent !== r.v) {
            if (!originaux.has(el)) originaux.set(el, el.textContent);
            el.textContent = r.v;
            el.setAttribute(REECRIT, "1");
          }
          n++;
        }
      } else if (r.a === "replier") {
        for (const el of liste) {
          if (!el.hasAttribute(PRET)) {
            el.setAttribute(PRET, "1");
            el.addEventListener("click", deplier, true);
          }
          n++;
        }
      } else {
        n = liste.length;
      }
      comptes.push({ i: r.i, n });
    }
    const cle = JSON.stringify(comptes);
    if (cle !== dernier) {
      dernier = cle;
      try {
        window.ipc.postMessage(JSON.stringify({ m: "zapCompte", comptes }));
      } catch (e) {}
    }
  };

  // La page d'origine : feuille coupee, caches rendus visibles, textes d'origine remis.
  const origine = (voir) => {
    enOrigine = !!voir;
    const feuille = document.querySelector("style[data-trame-zap]");
    if (feuille) feuille.disabled = enOrigine;
    for (const el of document.querySelectorAll(`[${CACHE}]`)) {
      if (enOrigine) el.style.removeProperty("display");
      else el.style.setProperty("display", "none", "important");
    }
    for (const el of document.querySelectorAll(`[${REECRIT}]`)) {
      const avant = originaux.get(el);
      if (avant === undefined) continue;
      if (enOrigine) el.textContent = avant;
    }
    if (!enOrigine) passer();
  };

  const planifier = () => {
    if (!minuterie) minuterie = setTimeout(passer, 700);
  };
  const observateur = new MutationObserver(planifier);
  observateur.observe(document.documentElement, { childList: true, subtree: true });
  window.__trame_zap_vivant = {
    arreter() {
      observateur.disconnect();
      if (minuterie) clearTimeout(minuterie);
    },
    passer,
    origine,
  };
  passer();
})
