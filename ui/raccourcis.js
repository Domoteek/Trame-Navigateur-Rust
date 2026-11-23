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

  // Reprise de la lecture apres un rechargement (F5, Ctrl+R, menu « Recharger », secours).
  //
  // La position de la video principale est notee dans `sessionStorage` (qui survit au
  // rechargement d'un onglet) : a chaque rechargement demande, a la sortie de la page, et
  // toutes les 2 s tant que la page est visible — un onglet tue ou endormi n'a pas toujours de
  // `pagehide`. Au chargement suivant, si c'est la meme page, la MEME VIDEO, et que la note a
  // moins de 2 minutes, la position est reposee **puis verifiee** : un lecteur qui replace sa
  // video ailleurs est corrige, sauf si Clément touche la page entre-temps (son geste prime).
  //
  // Mesure du 2026-11-22 (outils/eprouver-reprise.mjs, vraie application, vrai YouTube) : la
  // version precedente reecrivait l'adresse avec `t=…s` et sautait la reprise des que ce
  // parametre etait la. Or YouTube ne s'y place **qu'au demarrage de la lecture** — apres un
  // rechargement la video reste a 0 s tant qu'on n'appuie pas sur lecture —, et `t=` restait
  // ensuite dans l'adresse (un lien copie, un onglet rouvert, repartaient de la). Le `t=` n'est
  // donc plus ecrit : une seule source de verite, verifiee. Un `t=` deja present au chargement
  // (lien partage) est respecte : c'est une intention, pas une note.
  const CLE_REPRISE = "__trame_reprise";
  const NOTE_VALABLE_MS = 120000;
  const adresse = () => {
    try {
      const x = new URL(location.href);
      x.searchParams.delete("t");
      x.hash = "";
      return x.toString();
    } catch (e) {
      return location.href;
    }
  };
  // Identifiant de la video sur YouTube : l'adresse y gagne des parametres en cours de vie
  // (`pp`, `si`…), donc comparer l'adresse entiere refuserait la reprise sur la meme video.
  const videoDuSite = () => {
    try {
      const x = new URL(location.href);
      return x.pathname === "/watch" ? x.searchParams.get("v") : null;
    } catch (e) {
      return null;
    }
  };
  const aire = (v) => {
    const r = v.getBoundingClientRect();
    return r.width * r.height;
  };
  const enPub = () => !!document.querySelector(".ad-showing, .ad-interrupting");
  /** La video principale : la plus grande a l'ecran. `enCours` exige qu'elle ait deja joue. */
  const videoPrincipale = (enCours) => {
    let meilleure = null;
    let plus = 0;
    for (const v of document.querySelectorAll("video")) {
      if (!isFinite(v.duration) || v.duration <= 0) continue;
      if (enCours && v.currentTime < 3) continue;
      const a = aire(v);
      if (a >= plus) {
        plus = a;
        meilleure = v;
      }
    }
    return meilleure;
  };
  const noterLecture = () => {
    try {
      const v = videoPrincipale(true);
      if (!v || v.currentTime < 3 || enPub()) return;
      sessionStorage.setItem(
        CLE_REPRISE,
        JSON.stringify({
          url: adresse(),
          video: videoDuSite(),
          duree: isFinite(v.duration) ? Math.round(v.duration * 10) / 10 : null,
          t: Math.round(v.currentTime * 10) / 10,
          lecture: !v.paused,
          quand: Date.now(),
        })
      );
    } catch (e) {
      /* stockage refuse : rien a reprendre */
    }
  };
  window.addEventListener("pagehide", noterLecture);
  window.addEventListener("beforeunload", noterLecture);
  // Point de controle (essais, diagnostic) : noter la lecture a la demande.
  window.__trame_noter_lecture = noterLecture;
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) noterLecture();
  });
  // Filet : la note suit la lecture, pour survivre a un onglet tue sans `pagehide`.
  setInterval(() => {
    if (!document.hidden) noterLecture();
  }, 2000);

  // La reprise elle-meme.
  (() => {
    let note = null;
    try {
      note = JSON.parse(sessionStorage.getItem(CLE_REPRISE) || "null");
      sessionStorage.removeItem(CLE_REPRISE);
    } catch (e) {
      return;
    }
    if (!note || typeof note.t !== "number") return;
    if (!(Date.now() - note.quand < NOTE_VALABLE_MS)) return;
    const memePage = note.url === adresse() || (note.video && note.video === videoDuSite());
    if (!memePage) return;
    // Lien partage avec `t=` : l'adresse dit ou se placer, la note n'a pas a s'en meler.
    try {
      if (new URL(location.href).searchParams.has("t")) return;
    } catch (e) {
      /* adresse illisible : on continue */
    }

    // Un geste de Clement pendant la reprise annule la reprise : il a decide autre chose.
    let touche = false;
    const surGeste = () => {
      touche = true;
    };
    window.addEventListener("pointerdown", surGeste, true);
    window.addEventListener("keydown", surGeste, true);

    const debut = Date.now();
    let fin = debut + 25000;
    // La position attendue avance avec la lecture : on retient quand on l'a posee, pour
    // distinguer « la video avance normalement » de « le lecteur l'a deplacee ».
    let poseA = 0;
    let poseT = note.t;
    let dAccord = 0;

    const choisir = () => {
      let meilleure = null;
      let plus = 0;
      for (const v of document.querySelectorAll("video")) {
        if (!isFinite(v.duration) || v.duration <= note.t + 1) continue;
        // Une autre video du meme site (ou une pub) a une autre duree : on ne s'y place pas.
        if (typeof note.duree === "number" && Math.abs(v.duration - note.duree) > 1.5) continue;
        const a = aire(v);
        if (a >= plus) {
          plus = a;
          meilleure = v;
        }
      }
      return meilleure;
    };

    const terminer = () => {
      window.removeEventListener("pointerdown", surGeste, true);
      window.removeEventListener("keydown", surGeste, true);
    };

    const essayer = () => {
      if (touche) {
        terminer();
        return;
      }
      if (Date.now() >= fin) {
        // Une publicite tient l'element : ce n'est pas la video, on patiente (3 minutes au plus).
        if (enPub() && Date.now() < debut + 180000) {
          fin = Date.now() + 15000;
        } else {
          terminer();
          return;
        }
      }
      const v = choisir();
      if (!v) {
        // Un lecteur qui n'est pas encore pret, ou une publicite qui tient l'element : on attend.
        setTimeout(essayer, 300);
        return;
      }
      const vitesse = isFinite(v.playbackRate) && v.playbackRate > 0 ? v.playbackRate : 1;
      const attendu = poseT + (poseA ? ((Date.now() - poseA) / 1000) * vitesse : 0);
      const ecart = v.currentTime - attendu;
      if (!poseA || ecart < -3 || ecart > 3) {
        // Le lecteur ne s'est pas (ou plus) place la ou la note le dit : on repose. Cas mesure :
        // apres un rechargement, YouTube laisse la video a 0 tant que la lecture n'a pas demarre.
        try {
          v.currentTime = note.t;
          poseT = note.t;
          poseA = Date.now();
          dAccord = 0;
        } catch (e) {
          /* lecteur qui refuse : on reessaie au tour suivant */
        }
        if (note.lecture && v.paused) v.play().catch(() => {});
      } else if (!v.paused) {
        // La lecture avance et la position suit : la reprise tient.
        dAccord += 1;
        if (dAccord >= 6) {
          terminer();
          return;
        }
      } else if (note.lecture) {
        // Elle jouait : on relance (un rechargement n'a pas toujours le droit de demarrer seul).
        v.play().catch(() => {});
      }
      setTimeout(essayer, 400);
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
      // Navigation privee : Ctrl+Maj+N, comme les autres navigateurs.
      else if (ctrl && e.shiftKey && k === "n") action = "prive";
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
        if (action === "recharger") noterLecture();
        envoi({ m: "raccourci", action: action });
      }
    },
    true
  );

  // Une SURFACE d'interface (rail, barre) ne recoit que les touches. Le defilement, l'apercu
  // Alt+clic et le clic « page » n'ont de sens que dans une page visitee : sans cette garde, le
  // defilement du rail ecraserait la position de lecture de l'onglet actif, et un Alt+clic sur un
  // acces rapide ouvrirait un apercu.
  const surface = /[?&]surface=/.test(location.search);
  if (surface) return;

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
