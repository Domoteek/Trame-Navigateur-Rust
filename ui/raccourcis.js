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

  // Reprise de la lecture apres un rechargement (F5, Ctrl+R, menu « Recharger », secours) :
  // la position de la video principale est notee dans `sessionStorage` (qui survit a un
  // rechargement de l'onglet) et remise au chargement suivant, si c'est la meme adresse et
  // que la note date de moins de 30 s. Sur YouTube, l'adresse recoit en plus `t=` avant le
  // rechargement : son lecteur s'y place lui-meme, publicite comprise (demande de Clement,
  // 2026-11-22 : F5 reprenait la video au debut).
  const CLE_REPRISE = "__trame_reprise";
  const sansT = (u) => {
    try {
      const x = new URL(u);
      x.searchParams.delete("t");
      x.hash = "";
      return x.toString();
    } catch (e) {
      return u;
    }
  };
  const videoPrincipale = () => {
    let meilleure = null;
    let aire = 0;
    for (const v of document.querySelectorAll("video")) {
      if (!(v.currentTime > 0) || !isFinite(v.duration)) continue;
      const r = v.getBoundingClientRect();
      const a = r.width * r.height;
      if (a > aire) {
        aire = a;
        meilleure = v;
      }
    }
    return meilleure;
  };
  const youtube = () => /(^|\.)youtube\.com$/.test(location.hostname) && location.pathname === "/watch";
  const noterLecture = (pourRecharger) => {
    try {
      const v = videoPrincipale();
      if (!v || v.currentTime < 3) return;
      const t = Math.floor(v.currentTime);
      sessionStorage.setItem(
        CLE_REPRISE,
        JSON.stringify({ url: sansT(location.href), t: t, lecture: !v.paused, quand: Date.now() })
      );
      if (pourRecharger && youtube()) {
        const x = new URL(location.href);
        x.searchParams.set("t", t + "s");
        history.replaceState(history.state, "", x.toString());
      }
    } catch (e) {
      /* stockage refuse : rien a reprendre */
    }
  };
  window.addEventListener("pagehide", () => noterLecture(false));
  window.__trame_noter_lecture = noterLecture;
  (() => {
    let note = null;
    try {
      note = JSON.parse(sessionStorage.getItem(CLE_REPRISE) || "null");
      sessionStorage.removeItem(CLE_REPRISE);
    } catch (e) {
      return;
    }
    if (!note || Date.now() - note.quand > 30000 || note.url !== sansT(location.href)) return;
    // YouTube se place seul grace a `t=` ; ailleurs, on place la video des qu'elle est prete.
    if (youtube() && new URL(location.href).searchParams.has("t")) return;
    const fin = Date.now() + 20000;
    const essayer = () => {
      const v = [...document.querySelectorAll("video")].find((x) => isFinite(x.duration) && x.duration > note.t);
      if (!v) {
        if (Date.now() < fin) setTimeout(essayer, 300);
        return;
      }
      try {
        v.currentTime = note.t;
        if (note.lecture && v.paused) v.play().catch(() => {});
      } catch (e) {
        /* lecteur qui refuse : tant pis */
      }
    };
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", essayer);
    else essayer();
  })();

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
        if (action === "recharger") noterLecture(true);
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
