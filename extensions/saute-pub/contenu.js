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

  function signaler() {
    let bulle = document.getElementById("trame-saute-pub");
    if (!bulle) {
      bulle = document.createElement("div");
      bulle.id = "trame-saute-pub";
      document.body.appendChild(bulle);
    }
    bulle.classList.remove("parti");
    bulle.textContent = "Publicité sautée";
    clearTimeout(signaler.minuterie);
    signaler.minuterie = setTimeout(() => bulle.classList.add("parti"), 1800);
  }

  async function compter() {
    try {
      const { sautees } = await chrome.storage.local.get(["sautees"]);
      await chrome.storage.local.set({ sautees: (sautees || 0) + 1 });
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
  // Point de controle (essais) : etat courant et passage a la main.
  window.__trameSautePub = {
    etat: () => ({ pubEnCours, lecteurObserve: !!observe, avant }),
    traiter,
  };
  // Filet : une verification par seconde, si un changement passait entre les mailles.
  setInterval(traiter, 1000);
})();
