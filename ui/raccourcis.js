/**
 * Raccourcis clavier de Trame, injectes dans chaque page visitee.
 *
 * WebView2 consomme ses propres raccourcis de navigateur avant la page : Trame les desactive
 * (`with_browser_accelerator_keys(false)`) et gere les siens ici, parce qu'un raccourci global
 * doit fonctionner quelle que soit la page — y compris quand elle capture le clavier.
 *
 * Le fichier est embarque a la compilation (`include_str!` dans `app.rs`) : il ne depend pas
 * du dossier `ui/` au moment de la frappe. Il est aussi teste par `outils/tester-ui.mjs`, qui
 * envoye de vraies combinaisons de touches et verifie les messages partis vers l'hote.
 */
(() => {
  if (window.__trame_touches) return;
  window.__trame_touches = true;

  const envoi = (o) => {
    try {
      window.ipc.postMessage(JSON.stringify(o));
    } catch (e) {
      /* page sans pont : on n'echoue pas sur un raccourci */
    }
  };

  window.addEventListener(
    "keydown",
    (e) => {
      const k = (e.key || "").toLowerCase();
      const ctrl = e.ctrlKey || e.metaKey;
      let action = null;
      if (e.key === "F5") action = "recharger";
      else if (e.key === "F12") action = "devtools";
      else if (e.key === "Escape") action = "arret";
      else if (ctrl && e.shiftKey && k === "t") action = "reouverture";
      else if (ctrl && k === "t") action = "nouvelOnglet";
      else if (ctrl && k === "w") action = "fermerOnglet";
      else if (ctrl && k === "l") action = "couture";
      else if (ctrl && k === "k") action = "couture";
      else if (ctrl && k === "r") action = "recharger";
      else if (ctrl && k === "b") action = "rail";
      else if (ctrl && k === ",") action = "reglages";
      else if (ctrl && k === "j") action = "telechargements";
      else if (ctrl && (e.key === " " || k === "spacebar")) action = "trame";
      else if (ctrl && k === "tab") action = e.shiftKey ? "ongletPrecedent" : "ongletSuivant";
      else if (ctrl && (k === "=" || k === "+")) action = "zoomPlus";
      else if (ctrl && (k === "-" || k === "_")) action = "zoomMoins";
      else if (ctrl && k === "0") action = "zoomZero";
      else if (ctrl && k === "f") action = "chercherPage";
      else if (ctrl && k === "d") action = "favori";
      else if (ctrl && k === "p") action = "imprimer";
      else if (ctrl && /^[1-9]$/.test(k)) action = "onglet:" + k;
      if (action) {
        e.preventDefault();
        e.stopPropagation();
        envoi({ m: "raccourci", action: action });
      }
    },
    true
  );

  // Defilement : sert a restaurer la position de lecture au reveil. Delai pour ne pas noyer
  // l'hote sous un message par pixel.
  let dernier = 0;
  window.addEventListener(
    "scroll",
    () => {
      const t = Date.now();
      if (t - dernier < 400) return;
      dernier = t;
      envoi({ m: "defilement", y: Math.round(window.scrollY || 0) });
    },
    { passive: true }
  );

  // Coup d'oeil : Alt + clic sur un lien l'ouvre dans un apercu flottant, par-dessus la page
  // (le geste de Zen ; Arc utilise Maj + clic, qui ouvre deja une fenetre dans Chromium).
  window.addEventListener(
    "click",
    (e) => {
      if (!e.altKey || e.button !== 0) return;
      const lien = e.target && e.target.closest ? e.target.closest("a[href]") : null;
      if (!lien) return;
      const url = lien.href;
      if (!/^https?:/i.test(url)) return;
      e.preventDefault();
      e.stopPropagation();
      envoi({ m: "coupDoeil", url: url });
    },
    true
  );

  // Un clic dans la page ferme le panneau de la barre : le changement de focus entre deux
  // webviews voisines ne produit pas de `blur` cote barre.
  let dernierClic = 0;
  window.addEventListener(
    "pointerdown",
    () => {
      const t = Date.now();
      if (t - dernierClic < 300) return;
      dernierClic = t;
      envoi({ m: "clicPage" });
    },
    true
  );
})();
