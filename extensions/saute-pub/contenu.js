/* Saute-pub YouTube — extension Trame.
   YouTube signale une publicite en cours par la classe `ad-showing` sur le lecteur
   (`.html5-video-player`). Tant qu'elle est la :
     - le son est coupe et la lecture passe au plus vite (x16) ;
     - la video de pub est menee a sa fin (sa duree est connue) ;
     - le bouton « Passer » est clique des qu'il apparait.
   Quand la pub se termine, le son et la vitesse d'avant sont rendus tels quels.
   YouTube change de page sans recharger : on surveille le document entier, pas seulement le
   lecteur du premier chargement. Le nombre de pubs sautees est garde dans le stockage de
   l'extension. */

(() => {
  if (window.__trameSautePub) return;
  window.__trameSautePub = true;

  const BOUTONS_PASSER = [
    ".ytp-skip-ad-button",
    ".ytp-ad-skip-button",
    ".ytp-ad-skip-button-modern",
    ".ytp-ad-skip-button-container button",
    "button[id^='skip-button']",
  ].join(",");

  let avant = null; // { muted, vitesse } du lecteur avant la pub
  let pubEnCours = false;

  const lecteur = () => document.querySelector(".html5-video-player");

  function signaler(texte = "Publicité sautée") {
    let bulle = document.getElementById("trame-saute-pub");
    if (!bulle) {
      bulle = document.createElement("div");
      bulle.id = "trame-saute-pub";
      document.body.appendChild(bulle);
    }
    bulle.classList.remove("parti");
    bulle.textContent = texte;
    clearTimeout(signaler.minuterie);
    signaler.minuterie = setTimeout(() => bulle.classList.add("parti"), 1800);
  }

  async function compter(cle = "sautees", n = 1) {
    try {
      const lu = await chrome.storage.local.get([cle]);
      await chrome.storage.local.set({ [cle]: (lu[cle] || 0) + n });
    } catch (e) {
      /* stockage indisponible : on saute quand meme */
    }
  }

  function traiter() {
    const p = lecteur();
    if (!p) return;
    const video = p.querySelector("video");
    const pub = p.classList.contains("ad-showing") || p.classList.contains("ad-interrupting");

    if (pub && video) {
      if (!pubEnCours) {
        pubEnCours = true;
        avant = { muted: video.muted, vitesse: video.playbackRate || 1 };
        compter();
        signaler();
      }
      video.muted = true;
      if (video.playbackRate !== 16) {
        try { video.playbackRate = 16; } catch (e) {}
      }
      if (Number.isFinite(video.duration) && video.duration > 0 && video.currentTime < video.duration - 0.25) {
        try { video.currentTime = video.duration - 0.1; } catch (e) {}
      }
      const passer = p.querySelector(BOUTONS_PASSER) || document.querySelector(BOUTONS_PASSER);
      if (passer) passer.click();
      return;
    }

    if (!pub && pubEnCours) {
      pubEnCours = false;
      if (video && avant) {
        video.muted = avant.muted;
        try { video.playbackRate = avant.vitesse === 16 ? 1 : avant.vitesse; } catch (e) {}
      }
      avant = null;
    }
  }

  // Le lecteur change de classes au debut et a la fin d'une pub : on observe ses attributs, et
  // le document pour retrouver un lecteur recree (navigation interne de YouTube).
  let observe = null;
  const surveillerLecteur = () => {
    const p = lecteur();
    if (p && p !== observe) {
      observe = p;
      new MutationObserver(traiter).observe(p, { attributes: true, attributeFilter: ["class"], childList: true, subtree: true });
      traiter();
    }
  };
  new MutationObserver(surveillerLecteur).observe(document.documentElement, { childList: true, subtree: true });
  surveillerLecteur();
  /* --- Passages sponsorises (SponsorBlock) ---------------------------------------------
     Un sponsor lu par le createur fait partie de la video : aucun bloqueur ne le voit.
     La base collaborative SponsorBlock (sponsor.ajay.app) en donne les bornes. Pour ne pas
     dire quelle video est regardee, on n'envoie que les 4 premiers caracteres de l'empreinte
     SHA-256 de son identifiant : le serveur renvoie les segments de toutes les videos de ce
     prefixe, on garde ceux de la notre. Chaque segment est saute une fois : revenir en arriere
     dedans le laisse jouer. */
  const CATEGORIES = {
    sponsor: "Sponsor sauté",
    selfpromo: "Autopromotion sautée",
    interaction: "Rappel d'abonnement sauté",
  };
  let sbVideo = null; // identifiant de la video dont on a les segments
  let sbSegments = []; // [{ debut, fin, categorie, uuid }]
  const sbSautes = new Set();

  const idVideo = () => {
    try {
      const u = new URL(location.href);
      if (u.pathname === "/watch") return u.searchParams.get("v");
      const m = u.pathname.match(/^\/(?:embed|live)\/([\w-]{11})/);
      return m ? m[1] : null;
    } catch (e) {
      return null;
    }
  };

  async function chargerSegments(id) {
    sbVideo = id;
    sbSegments = [];
    sbSautes.clear();
    try {
      const empreinte = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(id));
      const hex = Array.from(new Uint8Array(empreinte), (o) => o.toString(16).padStart(2, "0")).join("");
      const adresse =
        "https://sponsor.ajay.app/api/skipSegments/" + hex.slice(0, 4) +
        "?categories=" + encodeURIComponent(JSON.stringify(Object.keys(CATEGORIES))) +
        "&actionTypes=" + encodeURIComponent(JSON.stringify(["skip"]));
      const rep = await fetch(adresse, { credentials: "omit", referrerPolicy: "no-referrer" });
      if (!rep.ok || sbVideo !== id) return; // 404 = aucun segment connu
      const liste = await rep.json();
      const la = Array.isArray(liste) ? liste.find((v) => v.videoID === id) : null;
      if (!la || sbVideo !== id) return;
      sbSegments = (la.segments || [])
        .filter((s) => Array.isArray(s.segment) && s.segment[1] - s.segment[0] >= 1)
        .map((s) => ({ debut: s.segment[0], fin: s.segment[1], categorie: s.category, uuid: s.UUID }));
    } catch (e) {
      /* hors ligne ou service indisponible : la video joue normalement */
    }
  }

  function suivreSponsors() {
    const id = idVideo();
    if (id !== sbVideo) {
      if (id) chargerSegments(id);
      else { sbVideo = null; sbSegments = []; }
      return;
    }
    if (!sbSegments.length || pubEnCours) return;
    const p = lecteur();
    const video = p && p.querySelector("video");
    if (!video || video.paused) return;
    const t = video.currentTime;
    for (const s of sbSegments) {
      if (sbSautes.has(s.uuid)) continue;
      if (t >= s.debut && t < s.fin - 0.5) {
        sbSautes.add(s.uuid);
        video.currentTime = Math.min(s.fin, Number.isFinite(video.duration) ? video.duration : s.fin);
        signaler(CATEGORIES[s.categorie] || "Passage sauté");
        compter("sponsors");
        break;
      }
    }
  }
  setInterval(suivreSponsors, 300);

  /* --- Voile gris orphelin -----------------------------------------------------------------
     Une fenetre de YouTube (« les bloqueurs de publicites ne sont pas autorises ») s'ouvre dans
     une boite de dialogue qui pose sur toute la page un voile gris (`tp-yt-iron-overlay-backdrop`)
     qui prend les clics. style.css cache le message, debut.js le retire de la reponse : la boite
     peut alors rester ouverte, vide ou cachee, avec son voile — la video grisee, plus rien de
     cliquable, et a chaque rechargement (constat chez un fils de Clement, 2026-11-24).
     Regle : un voile ouvert alors qu'AUCUNE fenetre de YouTube n'est visible est un orphelin,
     on le ferme. Les vraies fenetres (partager, enregistrer…) gardent le leur. */
  const visible = (el) => {
    if (!el || !el.isConnected) return false;
    const st = getComputedStyle(el);
    if (st.display === "none" || st.visibility === "hidden" || Number(st.opacity) === 0) return false;
    const r = el.getBoundingClientRect();
    return r.width > 24 && r.height > 24;
  };
  let voilesFermes = 0;
  // Orphelin a deux releves de suite (1,4 s) : a l'ouverture d'une vraie fenetre, le voile peut
  // paraitre un instant avant elle.
  let orphelinVu = false;
  function fermerVoilesOrphelins() {
    const voiles = [...document.querySelectorAll("tp-yt-iron-overlay-backdrop")].filter(visible);
    if (!voiles.length) {
      orphelinVu = false;
      return;
    }
    const fenetres = document.querySelectorAll("tp-yt-paper-dialog, tp-yt-iron-dropdown, ytd-popup-container [role='dialog']");
    // Une fenetre d'avertissement de bloqueur ne compte pas : on ne veut pas la voir.
    const uneVraie = [...fenetres].some((f) => visible(f) && !f.querySelector("ytd-enforcement-message-view-model"));
    if (uneVraie) {
      orphelinVu = false;
      return;
    }
    if (!orphelinVu) {
      orphelinVu = true;
      return;
    }
    orphelinVu = false;
    for (const v of voiles) {
      try { if (typeof v.close === "function") v.close(); } catch (e) {}
      v.style.setProperty("display", "none", "important");
      v.removeAttribute("opened");
    }
    // La boite d'avertissement elle-meme, si elle est la : refermee, pas seulement cachee.
    for (const f of fenetres) {
      if (!f.querySelector("ytd-enforcement-message-view-model") && visible(f)) continue;
      try { if (typeof f.close === "function") f.close(); } catch (e) {}
    }
    // Le voile bloque aussi le defilement de la page.
    if (document.body && document.body.style.overflow === "hidden") document.body.style.overflow = "";
    voilesFermes += voiles.length;
    compter("voiles", voiles.length);
    signaler("Voile gris retiré");
  }
  setInterval(fermerVoilesOrphelins, 700);

  // Pubs retirees par debut.js avant le lecteur (retire en 3.0 : YouTube le detectait) : comptees ici (le stockage n'existe pas au
  // debut du document).
  let retireesVues = 0;
  setInterval(() => {
    const n = window.__trameSansPub && window.__trameSansPub.etat ? window.__trameSansPub.etat().retirees : 0;
    if (n > retireesVues) {
      compter("retirees", n - retireesVues);
      retireesVues = n;
    }
  }, 5000);

  // Point de controle (essais) : etat courant et passage a la main.
  window.__trameSautePub = {
    etat: () => ({
      pubEnCours,
      lecteurObserve: !!observe,
      avant,
      voilesFermes,
      sponsor: { video: sbVideo, segments: sbSegments.length, sautes: sbSautes.size },
      avantLecteur: window.__trameSansPub && window.__trameSansPub.etat ? window.__trameSansPub.etat() : null,
    }),
    traiter,
  };
  // Filet : une verification par seconde, si un changement passait entre les mailles.
  setInterval(traiter, 1000);
})();
