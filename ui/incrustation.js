/**
 * Video flottante (incrustation), injectee dans chaque page visitee.
 *
 * Au survol d'une video, une petite pastille apparait en haut de l'image : un clic sort la
 * video dans la fenetre flottante de Chromium (toujours au premier plan, deplacable sur
 * n'importe quel ecran, redimensionnable). Un second clic, ou Alt+P, la ramene dans la page.
 * Alt+P sans survol prend la video principale (celle qui joue, sinon la plus grande).
 * Demande de Clement (2026-11-22), sur le modele de l'icone d'Opera GX.
 *
 * Contraintes de page hostile :
 * - YouTube impose les Trusted Types : aucune affectation de HTML brut, tout est construit par le DOM ;
 * - la pastille vit dans une racine fantome fermee, pour que le CSS du site ne la touche pas ;
 * - un clic sur la pastille ne doit pas atteindre le lecteur (YouTube mettrait en pause).
 *
 * Embarque a la compilation (`include_str!` dans `app.rs`), teste par `outils/tester-ui.mjs`.
 */
(() => {
  if (window.__trame_incrustation) return;
  window.__trame_incrustation = true;
  if (!("pictureInPictureEnabled" in document)) return;

  const LARGEUR_MIN = 200;
  const HAUTEUR_MIN = 120;
  const DELAI_MASQUE = 2500;
  const NS = "http://www.w3.org/2000/svg";

  let hote = null;
  let bouton = null;
  let libelle = null;
  let cible = null;
  let minuterie = 0;

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

  const construire = () => {
    hote = document.createElement("trame-incrustation");
    hote.style.cssText =
      "all:initial;position:fixed;left:0;top:0;width:0;height:0;z-index:2147483647;pointer-events:none;";
    const racine = hote.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = `
      button {
        position: fixed; display: none; align-items: center; gap: 6px;
        height: 30px; padding: 0 11px 0 9px; border: 0; border-radius: 15px;
        background: rgba(12, 14, 20, .78); color: #f2f4f8; cursor: pointer;
        font: 600 12px/1 "Segoe UI", system-ui, sans-serif; letter-spacing: .01em;
        box-shadow: 0 2px 10px rgba(0, 0, 0, .35); pointer-events: auto;
        opacity: 0; transform: translate(-50%, -4px);
        transition: opacity .15s ease, transform .15s ease, background .15s ease;
        -webkit-backdrop-filter: blur(8px); backdrop-filter: blur(8px);
      }
      button.vu { opacity: 1; transform: translate(-50%, 0); }
      button:hover { background: rgba(12, 14, 20, .94); }
      button:active { transform: translate(-50%, 1px); }
      svg { width: 16px; height: 16px; flex: none; }
    `;
    bouton = document.createElement("button");
    bouton.type = "button";
    const svg = document.createElementNS(NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.8");
    const cadre = document.createElementNS(NS, "rect");
    for (const [k, v] of [["x", "2.5"], ["y", "4.5"], ["width", "19"], ["height", "15"], ["rx", "2.5"]]) {
      cadre.setAttribute(k, v);
    }
    const mini = document.createElementNS(NS, "rect");
    for (const [k, v] of [["x", "12"], ["y", "11.5"], ["width", "7"], ["height", "5.5"], ["rx", "1"]]) {
      mini.setAttribute(k, v);
    }
    mini.setAttribute("fill", "currentColor");
    mini.setAttribute("stroke", "none");
    svg.append(cadre, mini);
    libelle = document.createElement("span");
    bouton.append(svg, libelle);
    racine.append(style, bouton);

    // Le lecteur ne doit voir ni l'appui ni le clic (pause, plein ecran au double clic…).
    for (const type of ["pointerdown", "pointerup", "mousedown", "mouseup", "dblclick"]) {
      bouton.addEventListener(type, (e) => e.stopPropagation());
    }
    bouton.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      basculer(cible);
    });
    bouton.addEventListener("pointerenter", () => clearTimeout(minuterie));
    bouton.addEventListener("pointerleave", () => armer());
    (document.documentElement || document).appendChild(hote);
  };

  const placer = () => {
    if (!cible || !bouton) return;
    const r = cible.getBoundingClientRect();
    bouton.style.left = Math.round(r.left + r.width / 2) + "px";
    bouton.style.top = Math.round(Math.max(r.top, 0) + 12) + "px";
    libelle.textContent = document.pictureInPictureElement === cible ? "Ramener" : "Sortir la vidéo";
  };

  const montrer = (v) => {
    if (document.fullscreenElement) return;
    if (!hote || !hote.isConnected) construire();
    cible = v;
    placer();
    bouton.style.display = "flex";
    requestAnimationFrame(() => bouton && bouton.classList.add("vu"));
    armer();
  };

  const cacher = () => {
    clearTimeout(minuterie);
    if (!bouton) return;
    bouton.classList.remove("vu");
    bouton.style.display = "none";
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
      try {
        window.ipc.postMessage(
          JSON.stringify({ m: "erreurUi", message: "incrustation : " + String((e && e.message) || e), source: "", ligne: 0 })
        );
      } catch (e2) {
        /* page sans pont */
      }
    }
    placer();
  };

  // Survol : un message par image d'animation au plus.
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
        const v = sous(x, y);
        if (v) montrer(v);
        else if (cible && !(bouton && bouton.matches(":hover"))) cacher();
      });
    },
    { capture: true, passive: true }
  );
  window.addEventListener("scroll", () => cible && placer(), { capture: true, passive: true });
  window.addEventListener("resize", () => cible && placer(), { passive: true });
  document.addEventListener("fullscreenchange", () => document.fullscreenElement && cacher());
  document.addEventListener("enterpictureinpicture", placer, true);
  document.addEventListener("leavepictureinpicture", placer, true);

  window.addEventListener(
    "keydown",
    (e) => {
      if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
      if ((e.key || "").toLowerCase() !== "p" && e.code !== "KeyP") return;
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
