/**
 * Teinte du site, pour le mode « dynamique » des reglages.
 *
 * La page annonce sa couleur dans `<meta name="theme-color">` — c'est fait pour ca, et beaucoup
 * de sites la posent. On la relit a chaque fois que la page devient VISIBLE (changement
 * d'onglet), pour que la coquille suive l'onglet actif et pas le dernier site charge.
 *
 * Rien n'est devine : pas de meta, pas de message, et la coquille garde l'aspect d'origine.
 * Trame valide la couleur avant de s'en servir (une page ne pose pas n'importe quoi dans le
 * style du navigateur).
 */
(function signalerTeinte() {
  function envoyer() {
    try {
      var meta = document.querySelector('meta[name="theme-color"]');
      if (!meta) return;
      var valeur = (meta.getAttribute('content') || '').trim();
      if (!valeur) return;
      window.ipc.postMessage(JSON.stringify({ m: 'couleur', valeur: valeur }));
    } catch (e) {
      /* une page sans pont n'est pas une panne */
    }
  }
  if (document.visibilityState === 'visible') envoyer();
  document.addEventListener('visibilitychange', function () {
    if (document.visibilityState === 'visible') envoyer();
  });
  // Un site pose souvent son meta apres le premier rendu : un second passage ne coute rien.
  setTimeout(envoyer, 1500);
})();

/**
 * Video flottante, injectee dans chaque page visitee (et dans ses cadres).
 *
 * Au survol d'une video, une pastille apparait en haut de l'image, en deux parties :
 * - « Sortir la vidéo » (Alt+P) : la fenetre d'incrustation de Chromium, toujours au premier
 *   plan, deplacable sur n'importe quel ecran ; lecture/pause seulement. Un second clic, ou
 *   Alt+P, la ramene ;
 * - l'icone de fenetre (Alt+Maj+P) : l'onglet entier part dans une mini-fenetre de Trame
 *   (`src/app/flottant.rs`), le lecteur y garde toutes ses commandes. Page principale seulement.
 * Alt+P sans survol prend la video principale (celle qui joue, sinon la plus grande).
 * Demande de Clement (2026-11-22), sur le modele de l'icone d'Opera GX.
 *
 * Dans la mini-fenetre, l'hote appelle `__trame_flottant(true)` : le lecteur remplit la
 * fenetre, et une barre au survol permet de la deplacer ou de ramener l'onglet.
 *
 * Contraintes de page hostile :
 * - YouTube impose les Trusted Types : aucune affectation de HTML brut, tout est construit par le DOM ;
 * - tout vit dans une racine fantome fermee, pour que le CSS du site ne le touche pas ;
 * - un clic sur la pastille ne doit pas atteindre le lecteur (YouTube mettrait en pause).
 *
 * Embarque a la compilation (`include_str!` dans `app.rs`), teste par `outils/tester-ui.mjs`.
 */
(() => {
  if (window.__trame_incrustation) return;
  window.__trame_incrustation = true;

  const LARGEUR_MIN = 200;
  const HAUTEUR_MIN = 120;
  const DELAI_MASQUE = 2500;
  const NS = "http://www.w3.org/2000/svg";
  const PRINCIPALE = window.top === window;
  const incrustable = "pictureInPictureEnabled" in document;

  const envoi = (o) => {
    try {
      window.ipc.postMessage(JSON.stringify(o));
    } catch (e) {
      /* page sans pont (cadre) */
    }
  };

  let hote = null;
  let pastille = null;
  let principal = null;
  let libelle = null;
  let barre = null;
  let cible = null;
  // Video choisie a la pastille, gardee pour la mini-fenetre (la pastille se cache avant).
  let derniere = null;
  let minuterie = 0;
  let enVol = false;

  const utilisable = (v) => {
    if (!v || !v.isConnected) return false;
    const r = v.getBoundingClientRect();
    return r.width >= LARGEUR_MIN && r.height >= HAUTEUR_MIN && v.readyState > 0;
  };

  const videoPrincipale = () => {
    let meilleure = null;
    let score = 0;
    for (const v of document.querySelectorAll("video")) {
      if (!utilisable(v)) continue;
      const r = v.getBoundingClientRect();
      const s = r.width * r.height * (v.paused ? 1 : 10);
      if (s > score) {
        score = s;
        meilleure = v;
      }
    }
    return meilleure;
  };

  const sous = (x, y) => {
    for (const v of document.querySelectorAll("video")) {
      const r = v.getBoundingClientRect();
      if (x >= r.left && x <= r.right && y >= r.top && y <= r.bottom && utilisable(v)) return v;
    }
    return null;
  };

  const icone = (formes) => {
    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.8");
    for (const [nom, attributs] of formes) {
      const f = document.createElementNS(NS, nom);
      for (const [k, v] of Object.entries(attributs)) f.setAttribute(k, v);
      svg.append(f);
    }
    return svg;
  };
  // Petite video dans un grand cadre : l'incrustation.
  const ICONE_INCRUSTATION = [
    ["rect", { x: "2.5", y: "4.5", width: "19", height: "15", rx: "2.5" }],
    ["rect", { x: "12", y: "11.5", width: "7", height: "5.5", rx: "1", fill: "currentColor", stroke: "none" }],
  ];
  // Fenetre avec sa barre : l'onglet en mini-fenetre.
  const ICONE_FENETRE = [
    ["rect", { x: "3", y: "4", width: "18", height: "16", rx: "2.5" }],
    ["path", { d: "M3 8.5h18" }],
    ["path", { d: "M10 13.5l2 2 2-2", "stroke-linecap": "round", "stroke-linejoin": "round" }],
  ];
  const ICONE_RAMENER = [
    ["path", { d: "M9 14L4 9l5-5", "stroke-linecap": "round", "stroke-linejoin": "round" }],
    ["path", { d: "M4 9h10.5a5.5 5.5 0 010 11H11", "stroke-linecap": "round" }],
  ];

  // Ni appui ni clic ne doivent atteindre le lecteur (pause, plein ecran au double clic…).
  const isoler = (el) => {
    for (const type of ["pointerdown", "pointerup", "mousedown", "mouseup", "click", "dblclick"]) {
      el.addEventListener(type, (e) => e.stopPropagation());
    }
  };

  // La teinte de Trame, posee par Rust a la creation de l'onglet (src/teinte.rs) ; sans elle,
  // l'aspect d'origine. Fond de la pastille = surface de la teinte, en verre sur la video.
  const T = (typeof __TRAME_TEINTE__ !== "undefined" && __TRAME_TEINTE__) ||
    { fond: "#080b11", surface: "#101521", surface2: "#151c2a", accent: "#60a5fa", rgb: "96, 165, 250", clair: "#dbe9ff" };

  const construire = () => {
    hote = document.createElement("trame-incrustation");
    hote.style.cssText =
      "all:initial;position:fixed;left:0;top:0;width:0;height:0;z-index:2147483647;pointer-events:none;";
    const racine = hote.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = `
      .pastille {
        position: fixed; display: none; align-items: stretch; height: 30px;
        border-radius: 15px; overflow: hidden; pointer-events: auto;
        background: ${T.surface}c7; box-shadow: 0 2px 10px rgba(0, 0, 0, .35);
        opacity: 0; transform: translate(-50%, -4px);
        transition: opacity .15s ease, transform .15s ease;
        -webkit-backdrop-filter: blur(8px); backdrop-filter: blur(8px);
      }
      .pastille.vu { opacity: 1; transform: translate(-50%, 0); }
      button {
        all: unset; display: flex; align-items: center; gap: 6px; cursor: pointer;
        color: #f2f4f8; font: 600 12px/1 "Segoe UI", system-ui, sans-serif; letter-spacing: .01em;
        transition: background .15s ease;
      }
      button:hover { background: rgba(255, 255, 255, .12); }
      button:active { background: rgba(255, 255, 255, .2); }
      button:hover { color: ${T.clair}; }
      button:focus-visible { outline: 2px solid ${T.accent}; outline-offset: -2px; }
      .principal { padding: 0 11px 0 9px; }
      .onglet { padding: 0 9px; border-left: 1px solid rgba(255, 255, 255, .16); }
      svg { width: 16px; height: 16px; flex: none; }
      .barre {
        position: fixed; left: 0; right: 0; top: 0; height: 34px; display: flex;
        align-items: center; gap: 8px; padding: 0 6px 0 12px; box-sizing: border-box;
        background: linear-gradient(rgba(0, 0, 0, .72), rgba(0, 0, 0, 0));
        color: #f2f4f8; font: 600 12px/1 "Segoe UI", system-ui, sans-serif;
        pointer-events: auto; cursor: grab; opacity: 0; transition: opacity .15s ease;
      }
      .barre.vu { opacity: 1; }
      .barre .titre { flex: 1; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; opacity: .85; }
      .barre button { height: 26px; padding: 0 9px; border-radius: 13px; background: ${T.surface}99; }
    `;

    pastille = document.createElement("div");
    pastille.className = "pastille";
    principal = document.createElement("button");
    principal.type = "button";
    principal.className = "principal";
    libelle = document.createElement("span");
    principal.append(icone(ICONE_INCRUSTATION), libelle);
    principal.addEventListener("click", (e) => {
      e.preventDefault();
      basculer(cible);
    });
    pastille.append(principal);
    if (PRINCIPALE) {
      const onglet = document.createElement("button");
      onglet.type = "button";
      onglet.className = "onglet";
      onglet.title = "Mini-fenêtre avec les commandes du lecteur (Alt+Maj+P)";
      onglet.setAttribute("aria-label", onglet.title);
      onglet.append(icone(ICONE_FENETRE));
      onglet.addEventListener("click", (e) => {
        e.preventDefault();
        derniere = cible;
        cacher();
        envoi({ m: "flottant", action: "ouvrir" });
      });
      pastille.append(onglet);
    }
    if (!incrustable) principal.style.display = "none";
    isoler(pastille);
    pastille.addEventListener("pointerenter", () => clearTimeout(minuterie));
    pastille.addEventListener("pointerleave", () => armer());

    barre = document.createElement("div");
    barre.className = "barre";
    barre.style.display = "none";
    const titre = document.createElement("span");
    titre.className = "titre";
    const ramener = document.createElement("button");
    ramener.type = "button";
    ramener.append(icone(ICONE_RAMENER), document.createTextNode("Ramener"));
    ramener.title = "Remettre la page dans son onglet";
    ramener.addEventListener("click", (e) => {
      e.preventDefault();
      envoi({ m: "flottant", action: "ramener" });
    });
    isoler(ramener);
    barre.append(titre, ramener);
    // La barre deplace la fenetre, comme une barre de titre.
    barre.addEventListener("pointerdown", (e) => {
      e.stopPropagation();
      if (e.button === 0 && (e.target === barre || e.target === titre)) envoi({ m: "flottant", action: "deplacer" });
    });
    barre.addEventListener("pointerenter", () => clearTimeout(minuterie));
    barre.addEventListener("pointerleave", () => armer());

    racine.append(style, pastille, barre);
    (document.documentElement || document).appendChild(hote);
  };

  const pret = () => {
    if (!hote || !hote.isConnected) construire();
  };

  const placer = () => {
    if (!cible || !pastille) return;
    const r = cible.getBoundingClientRect();
    pastille.style.left = Math.round(r.left + r.width / 2) + "px";
    pastille.style.top = Math.round(Math.max(r.top, 0) + 12) + "px";
    libelle.textContent = document.pictureInPictureElement === cible ? "Ramener" : "Sortir la vidéo";
  };

  const montrer = (v) => {
    if (document.fullscreenElement) return;
    pret();
    if (enVol) {
      barre.firstChild.textContent = document.title || location.hostname;
      barre.style.display = "flex";
      requestAnimationFrame(() => barre && barre.classList.add("vu"));
      armer();
      return;
    }
    cible = v;
    placer();
    pastille.style.display = "flex";
    requestAnimationFrame(() => pastille && pastille.classList.add("vu"));
    armer();
  };

  const cacher = () => {
    clearTimeout(minuterie);
    if (pastille) {
      pastille.classList.remove("vu");
      pastille.style.display = "none";
    }
    if (barre) {
      barre.classList.remove("vu");
      barre.style.display = "none";
    }
    cible = null;
  };

  const armer = () => {
    clearTimeout(minuterie);
    minuterie = setTimeout(cacher, DELAI_MASQUE);
  };

  const basculer = async (v) => {
    try {
      if (document.pictureInPictureElement && (!v || document.pictureInPictureElement === v)) {
        await document.exitPictureInPicture();
      } else if (v) {
        // Certains sites interdisent l'incrustation par attribut : le choix revient a Clement.
        if (v.disablePictureInPicture) v.disablePictureInPicture = false;
        v.removeAttribute("disablepictureinpicture");
        await v.requestPictureInPicture();
      }
    } catch (e) {
      envoi({ m: "erreurUi", message: "incrustation : " + String((e && e.message) || e), source: "", ligne: 0 });
    }
    placer();
  };

  // ---------------------------------------------------------------- mini-fenetre

  // Le lecteur a etirer : sur YouTube, son conteneur (les commandes vivent dedans) ; ailleurs,
  // le plus haut ancetre de la video qui ne la deborde pas de plus de 48 px.
  const lecteurDe = (v) => {
    const yt = v.closest("#movie_player, .html5-video-player");
    if (yt) return yt;
    const r = v.getBoundingClientRect();
    let meilleur = v;
    for (let a = v.parentElement; a && a !== document.body && a !== document.documentElement; a = a.parentElement) {
      const q = a.getBoundingClientRect();
      if (q.width - r.width > 48 || q.height - r.height > 48) break;
      meilleur = a;
    }
    return meilleur;
  };

  let styleVol = null;
  const MARQUE = "data-trame-flottant";
  window.__trame_flottant = (actif) => {
    for (const el of document.querySelectorAll("[" + MARQUE + "]")) el.removeAttribute(MARQUE);
    if (styleVol) styleVol.remove();
    styleVol = null;
    enVol = !!actif;
    cacher();
    if (enVol) {
      const v = [cible, derniere].find((x) => utilisable(x)) || videoPrincipale();
      if (v) {
        const lecteur = lecteurDe(v);
        lecteur.setAttribute(MARQUE, "lecteur");
        // YouTube taille lui-meme sa video dans son conteneur : on n'y touche pas.
        if (lecteur !== v && !lecteur.matches("#movie_player, .html5-video-player")) v.setAttribute(MARQUE, "video");
        styleVol = document.createElement("style");
        styleVol.textContent = `
          html, body { overflow: hidden !important; background: #000 !important; }
          [${MARQUE}="lecteur"] {
            position: fixed !important; inset: 0 !important; width: 100vw !important;
            height: 100vh !important; max-width: none !important; max-height: none !important;
            margin: 0 !important; transform: none !important; z-index: 2147483646 !important;
            background: #000 !important;
          }
          [${MARQUE}="video"] {
            width: 100% !important; height: 100% !important; object-fit: contain !important;
          }
        `;
        (document.head || document.documentElement).appendChild(styleVol);
      }
    }
    // Le lecteur de YouTube recalcule sa taille au redimensionnement de la fenetre.
    setTimeout(() => window.dispatchEvent(new Event("resize")), 50);
    setTimeout(() => window.dispatchEvent(new Event("resize")), 400);
  };

  // ---------------------------------------------------------------- evenements

  // Survol : un calcul par image d'animation au plus.
  let enAttente = false;
  window.addEventListener(
    "pointermove",
    (e) => {
      if (enAttente || e.pointerType === "touch") return;
      enAttente = true;
      const x = e.clientX;
      const y = e.clientY;
      requestAnimationFrame(() => {
        enAttente = false;
        if (enVol) {
          montrer(null);
          return;
        }
        const v = sous(x, y);
        if (v) montrer(v);
        else if (cible && !(pastille && pastille.matches(":hover"))) cacher();
      });
    },
    { capture: true, passive: true }
  );
  window.addEventListener("scroll", () => cible && placer(), { capture: true, passive: true });
  window.addEventListener("resize", () => cible && placer(), { passive: true });
  document.addEventListener("fullscreenchange", () => document.fullscreenElement && cacher());
  document.addEventListener("enterpictureinpicture", placer, true);
  document.addEventListener("leavepictureinpicture", placer, true);

  // La croix de la fenetre d'incrustation met la video en pause (choix de Chromium) : Clement
  // veut qu'elle continue dans la page (2026-11-22). Une pause qui tombe a moins d'une seconde
  // de la fermeture, sur une video qui jouait, vient de la croix : on relance. Une pause faite
  // plus tot dans la fenetre flottante (bouton pause) est respectee. L'ordre des deux
  // evenements n'est pas garanti : on traite les deux sens.
  const ECART_FERMETURE = 1000;
  let pauseFlottante = null; // { v, quand } : derniere pause d'une video en incrustation
  let sortie = null; // { v, quand } : derniere sortie d'incrustation d'une video qui jouait
  const relancer = (v) => {
    if (v.paused && !v.ended) v.play().catch(() => {});
  };
  document.addEventListener(
    "pause",
    (e) => {
      const v = e.target;
      if (!(v instanceof HTMLVideoElement)) return;
      const t = Date.now();
      if (document.pictureInPictureElement === v) pauseFlottante = { v, quand: t };
      else if (sortie && sortie.v === v && t - sortie.quand < ECART_FERMETURE) {
        sortie = null;
        relancer(v);
      }
    },
    true
  );
  document.addEventListener(
    "leavepictureinpicture",
    (e) => {
      const v = e.target;
      if (!(v instanceof HTMLVideoElement)) return;
      const t = Date.now();
      if (pauseFlottante && pauseFlottante.v === v && t - pauseFlottante.quand < ECART_FERMETURE) {
        pauseFlottante = null;
        relancer(v);
      } else if (!v.paused) {
        sortie = { v, quand: t };
      }
      // Chromium ramene la video dans sa page, mais il ne sait pas montrer un onglet de Trame :
      // mesure du 2026-11-22, la page restait `hidden` et Clement ne voyait pas l'onglet de la
      // video. On demande donc a l'hote de l'afficher. Une video qui se termine ferme aussi la
      // fenetre d'incrustation : la, on ne lui vole pas son onglet.
      if (PRINCIPALE && !v.ended && document.visibilityState === "hidden") {
        envoi({ m: "flottant", action: "montrer" });
      }
    },
    true
  );

  window.addEventListener(
    "keydown",
    (e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey) return;
      if ((e.key || "").toLowerCase() !== "p" && e.code !== "KeyP") return;
      if (e.shiftKey) {
        if (!PRINCIPALE) return;
        e.preventDefault();
        e.stopPropagation();
        envoi({ m: "flottant", action: enVol ? "ramener" : "ouvrir" });
        return;
      }
      if (!incrustable) return;
      const v = (cible && utilisable(cible) && cible) || document.pictureInPictureElement || videoPrincipale();
      if (!v) return;
      e.preventDefault();
      e.stopPropagation();
      basculer(v);
    },
    true
  );

  window.__trame_incruster = (v) => basculer(v || videoPrincipale());
})();
