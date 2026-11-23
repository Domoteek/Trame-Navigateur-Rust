/* Infobulles de Trame — UNE implementation, chargee par la coquille (rail, barre) et par les
 * pages internes (reglages, nouvel onglet, telechargements, zap).
 *
 * Pourquoi : les `title` donnaient la bulle de Windows, blanche, carree, hors teinte (retour de
 * Clement le 2026-11-23, « tu as oublie les autres »). Les `title` restent la source — rien a
 * changer dans le code qui les ecrit : au survol, le texte passe dans `data-bulle` pour faire
 * taire la bulle native, et c'est la page qui dessine.
 *
 * Mise en forme tiree du texte lui-meme :
 *   « Cloisonner : connexions… »        action en gras, explication en dessous
 *   « Double-clic pour renommer »       le geste en pastille, l'action en clair
 *   « Replier le menu (Ctrl+B) »        la touche en touches de clavier
 *   « Nom v1.0.0\nDescription\n…»        premiere ligne = titre (version a part), suite = details,
 *                                        un motif d'adresse (« *://* ») en police fixe
 *
 * Une webview ne peut pas peindre hors de ses bords. Quand la bulle n'y tient pas, `brancher`
 * accepte deux echappatoires :
 *   `natif()`  vrai : on rend le `title` a Windows (rail replie, 64 px de large) ;
 *   `hote`     { montrer(rect, hauteur) → bool, cacher() } : l'hote agrandit la webview le temps
 *              de la bulle (la barre, via la colonne de son panneau) ;
 *   `deleguer` { actif() → bool, montrer(rect, texte), cacher() } : une autre webview dessine
 *              (rail replie : la barre pose la carte a cote de l'icone, `carte(texte, y)`).
 */
(function () {
  const GESTE = /^(Double-clic|Clic droit|Clic du milieu|Clic|Glisser|Cliquer)\s+pour\s+(.+)$/i;
  const TOUCHE = /\s*\(([^()]*(?:Ctrl|Maj|Alt|Shift|F\d+|Échap|Entrée|Suppr)[^()]*)\)\s*$/;
  const MOTIF = /^(\S*:\/\/\S*)(\s+\S*:\/\/\S*)*$|^<all_urls>$/;
  const VERSION = /^(.*\S)\s+(v\d[\w.-]*)$/;
  const MARGE = 8;

  const CSS = `
.trame-bulle {
  position: fixed; z-index: 2147483000; box-sizing: border-box;
  max-width: min(300px, calc(100vw - 16px)); padding: 7px 10px; border-radius: 9px;
  background: color-mix(in srgb, var(--surface-3, #1b2434) 94%, var(--accent, #60a5fa) 6%);
  border: 1px solid color-mix(in srgb, var(--bord-fort, rgba(255,255,255,.14)) 70%, rgba(var(--accent-rgb, 96,165,250), .35));
  box-shadow: 0 10px 28px rgba(0,0,0,.55), 0 1px 0 rgba(255,255,255,.04) inset;
  color: var(--texte, #e9eefa); font: 12px/1.35 "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  text-align: left; white-space: normal; overflow-wrap: anywhere;
  pointer-events: none; opacity: 0; transform: translateY(-3px) scale(.98);
  transform-origin: var(--pointe, 50%) top;
  transition: opacity .12s ease, transform .14s cubic-bezier(.2,.8,.2,1);
}
.trame-bulle.dessus { transform: translateY(3px) scale(.98); transform-origin: var(--pointe, 50%) bottom; }
.trame-bulle.cote { transform: translateX(-3px) scale(.98); transform-origin: left center; }
.trame-bulle.visible { opacity: 1; transform: none; }
.trame-bulle::before {
  content: ""; position: absolute; top: -5px; left: calc(var(--pointe, 50%) - 4.5px);
  width: 8px; height: 8px; background: inherit; border-left: inherit; border-top: inherit;
  transform: rotate(45deg); border-top-left-radius: 2px;
}
.trame-bulle.dessus::before {
  top: auto; bottom: -5px; border: none; border-right: inherit; border-bottom: inherit;
  border-radius: 0 0 2px 0;
}
.trame-bulle.cote::before {
  top: calc(50% - 4.5px); left: -5px; border: none; border-left: inherit; border-bottom: inherit;
  border-radius: 0 0 0 2px;
}
.trame-bulle.carte { display: flex; flex-direction: column; justify-content: center; border-radius: 12px; box-shadow: none; }
.trame-bulle.carte::before { display: none; }
.trame-bulle .b-ligne { display: flex; align-items: center; gap: 7px; }
.trame-bulle .b-titre { font-weight: 600; min-width: 0; }
.trame-bulle .b-version { color: var(--texte-3, #616e85); font-size: 11px; font-variant-numeric: tabular-nums; }
.trame-bulle .b-detail { color: var(--texte-2, #97a4bb); font-size: 11.5px; margin-top: 3px; }
.trame-bulle .b-motif {
  margin-top: 4px; color: var(--texte-2, #97a4bb); font: 11px/1.4 "Cascadia Mono", Consolas, monospace;
}
.trame-bulle .b-pied { margin-top: 6px; padding-top: 6px; border-top: 1px solid var(--bord, rgba(255,255,255,.07)); color: var(--texte-2, #97a4bb); font-size: 11.5px; }
.trame-bulle .b-pied .b-ligne + .b-ligne { margin-top: 3px; }
.trame-bulle .b-geste {
  flex: none; padding: 1px 6px; border-radius: 5px; font-size: 11px; font-weight: 600;
  background: rgba(var(--accent-rgb, 96,165,250), .16);
  color: color-mix(in srgb, var(--accent, #60a5fa) 60%, #ffffff);
}
.trame-bulle .b-touches { display: inline-flex; gap: 3px; margin-left: auto; padding-left: 6px; }
.trame-bulle kbd {
  font: inherit; font-size: 10.5px; font-variant-numeric: tabular-nums; min-width: 18px; padding: 0 5px;
  text-align: center; border-radius: 4px; background: var(--surface, #101521);
  border: 1px solid var(--bord-fort, rgba(255,255,255,.14)); border-bottom-width: 2px; color: var(--texte-2, #97a4bb);
}
@media (prefers-reduced-motion: reduce) {
  .trame-bulle, .trame-bulle.dessus, .trame-bulle.cote { transition: opacity .1s; transform: none; }
}`;

  function el(tag, classe, texte) {
    const d = document.createElement(tag);
    if (classe) d.className = classe;
    if (texte != null) d.textContent = String(texte);
    return d;
  }
  const majuscule = (s) => s.charAt(0).toUpperCase() + s.slice(1);

  /** « Double-clic pour renommer » → [pastille, action] ; « Clic : nouvel onglet » aussi. */
  function geste(texte) {
    const g = texte.match(GESTE);
    if (g) return [majuscule(g[1].toLowerCase() === "cliquer" ? "clic" : g[1]), majuscule(g[2])];
    const d = texte.match(/^(Double-clic|Clic droit|Clic du milieu|Clic|Glisser)\s*:\s*(.+)$/i);
    if (d) return [majuscule(d[1]), majuscule(d[2])];
    return null;
  }

  function ligneGeste(g) {
    const l = el("div", "b-ligne");
    l.appendChild(el("span", "b-geste", g[0]));
    l.appendChild(el("span", "", g[1]));
    return l;
  }

  /** Le contenu de la bulle, tire du texte du `title`. */
  function remplir(bulle, texte) {
    bulle.textContent = "";
    const lignes = texte.split("\n").map((l) => l.trim()).filter(Boolean);
    if (!lignes.length) return;
    let premiere = lignes.shift();
    let touche = "";
    const t = premiere.match(TOUCHE);
    if (t) {
      touche = t[1];
      premiere = premiere.slice(0, t.index);
    }
    const tete = el("div", "b-ligne");
    let detail = "";
    const g = geste(premiere);
    if (g) {
      tete.appendChild(el("span", "b-geste", g[0]));
      tete.appendChild(el("span", "b-titre", g[1]));
    } else {
      const i = premiere.indexOf(" : ");
      let titre = i > 0 ? premiere.slice(0, i) : premiere;
      if (i > 0) detail = majuscule(premiere.slice(i + 3));
      const v = titre.match(VERSION);
      tete.appendChild(el("span", "b-titre", v ? v[1] : titre));
      if (v) tete.appendChild(el("span", "b-version", v[2]));
    }
    if (touche) {
      const kbd = el("span", "b-touches");
      for (const k of touche.split("+")) kbd.appendChild(el("kbd", "", k.trim()));
      tete.appendChild(kbd);
    }
    bulle.appendChild(tete);
    if (detail) bulle.appendChild(el("div", "b-detail", detail));

    // Lignes suivantes : details, motifs d'adresse, et les gestes regroupes en pied.
    const pied = el("div", "b-pied");
    for (const l of lignes) {
      // « Clic : nouvel onglet · clic droit : modifier · glisser : ranger » : un geste par ligne.
      const morceaux = l.split(/\s+·\s+/);
      const gestes = morceaux.map(geste);
      if (gestes.every(Boolean)) {
        for (const x of gestes) pied.appendChild(ligneGeste(x));
      } else if (MOTIF.test(l)) {
        bulle.appendChild(el("div", "b-motif", l.split(/\s+/).join("  ")));
      } else {
        bulle.appendChild(el("div", "b-detail", l));
      }
    }
    if (pied.childElementCount) bulle.appendChild(pied);
  }

  /** Branche les bulles sur le document. Une seule fois par page. */
  function brancher(options = {}) {
    if (window.__trameBulles) return window.__trameBulles;
    const style = el("style");
    style.textContent = CSS;
    document.head.appendChild(style);
    const bulle = el("div", "trame-bulle");
    bulle.setAttribute("role", "tooltip");
    document.body.appendChild(bulle);

    let cible = null;
    let minuterie = 0;
    let vueA = 0; // derniere fermeture : d'une bulle a la voisine, pas de nouveau delai
    let chezHote = false;
    let delegue = false;

    const texteDe = (e) => {
      if (e.title) {
        e.dataset.bulle = e.title;
        e.removeAttribute("title");
      }
      return e.dataset.bulle || "";
    };
    const rendreNatif = (e) => {
      if (!e.title && e.dataset.bulle) e.title = e.dataset.bulle;
    };

    function cacher() {
      clearTimeout(minuterie);
      if (bulle.classList.contains("visible")) vueA = performance.now();
      cible = null;
      bulle.classList.remove("visible");
      if (delegue) {
        delegue = false;
        vueA = performance.now(); // d'une icone a la voisine, pas de nouveau delai
        try {
          options.deleguer.cacher();
        } catch (e) {
          /* l'hote est parti */
        }
      }
      if (chezHote) {
        chezHote = false;
        try {
          options.hote.cacher();
        } catch (e) {
          /* l'hote est parti : rien a rendre */
        }
      }
    }

    function poser(x, y, classe, pointe) {
      bulle.style.left = `${Math.round(x)}px`;
      bulle.style.top = `${Math.round(y)}px`;
      bulle.classList.remove("dessus", "cote", "carte");
      if (classe) bulle.classList.add(classe);
      if (pointe != null) bulle.style.setProperty("--pointe", `${Math.round(pointe)}px`);
    }

    function placer(e) {
      const r = e.getBoundingClientRect();
      bulle.style.left = "0px";
      bulle.style.top = "0px";
      bulle.style.height = "";
      const b = bulle.getBoundingClientRect();
      const L = window.innerWidth;
      const H = window.innerHeight;
      const centre = r.left + r.width / 2;
      const x = Math.max(MARGE, Math.min(centre - b.width / 2, L - b.width - MARGE));
      const pointe = Math.max(12, Math.min(centre - x, b.width - 12));
      // 1. Dessous, puis dessus : la place habituelle.
      if (r.bottom + 8 + b.height <= H - 2) return poser(x, r.bottom + 8, "", pointe), true;
      if (r.top - 8 - b.height >= 2) return poser(x, r.top - 8 - b.height, "dessus", pointe), true;
      // 2. A cote, si elle tient en hauteur (barre de 45 px, une ligne).
      if (b.height <= H - 4) {
        const y = Math.max(2, Math.min(r.top + r.height / 2 - b.height / 2, H - b.height - 2));
        if (r.right + 10 + b.width <= L - MARGE) return poser(r.right + 10, y, "cote"), true;
      }
      // 3. L'hote agrandit la webview : une carte detachee sous la barre.
      return enCarte(x, Math.ceil(H) + 6);
    }

    /** Carte sous la barre, a partir de `haut` (jamais plus haut que la barre elle-meme). */
    function enCarte(x, haut) {
      if (!options.hote) return false;
      const H = window.innerHeight;
      haut = Math.max(Math.ceil(H) + 6, Math.round(haut));
      bulle.style.height = "";
      const b = bulle.getBoundingClientRect();
      const montre = options.hote.montrer({ x: Math.round(x), l: Math.ceil(b.width), y: haut }, Math.ceil(b.height));
      if (!montre) return false;
      chezHote = true;
      poser(x, haut, "carte");
      bulle.style.height = `${montre - haut}px`;
      return true;
    }

    /** Carte demandee par une autre webview (le rail replie) : `y` dans le repere de la fenetre,
     *  `fenetre` sa hauteur — une icone du bas fait remonter la carte au lieu de la couper. */
    function carte(texte, y, fenetre) {
      cacher();
      if (!texte) return;
      remplir(bulle, texte);
      bulle.style.left = "0px";
      bulle.style.top = "0px";
      bulle.style.height = "";
      if (fenetre > 0) y = Math.min(y, fenetre - bulle.getBoundingClientRect().height - MARGE);
      if (!enCarte(6, y)) return;
      void bulle.offsetWidth;
      bulle.classList.add("visible");
    }

    function montrer(e) {
      const texte = texteDe(e);
      if (!texte || !e.isConnected) return;
      if (options.deleguer && options.deleguer.actif()) {
        delegue = true;
        options.deleguer.montrer(e.getBoundingClientRect(), texte);
        return;
      }
      remplir(bulle, texte);
      bulle.classList.remove("visible");
      if (!placer(e)) {
        rendreNatif(e);
        return;
      }
      void bulle.offsetWidth;
      bulle.classList.add("visible");
    }

    document.addEventListener("pointerover", (ev) => {
      const e = ev.target.closest && ev.target.closest("[title], [data-bulle]");
      if (e === cible) return;
      if (!e) return cacher();
      if (options.natif && options.natif()) {
        rendreNatif(e);
        return cacher();
      }
      texteDe(e);
      const enchaine = performance.now() - vueA < 400 || bulle.classList.contains("visible");
      cacher();
      cible = e;
      minuterie = setTimeout(() => cible === e && montrer(e), enchaine ? 0 : 450);
    });
    document.addEventListener("pointerout", (ev) => {
      if (cible && !cible.contains(ev.relatedTarget)) cacher();
    });
    for (const type of ["pointerdown", "wheel", "keydown", "scroll"]) {
      document.addEventListener(type, cacher, { capture: true, passive: true });
    }
    window.addEventListener("blur", cacher);
    // Passage en mode natif (rail qui se replie) : chaque texte deja pris retourne a Windows.
    window.addEventListener("resize", () => {
      if (chezHote) return; // c'est nous qui avons agrandi la webview
      cacher();
      if (options.natif && options.natif()) {
        for (const e of document.querySelectorAll("[data-bulle]")) rendreNatif(e);
      }
    });

    window.__trameBulles = { cacher, carte, remplir: (t) => remplir(bulle, t), element: bulle };
    return window.__trameBulles;
  }

  window.TrameBulles = { brancher, remplir };
  // Les pages internes n'ont rien a configurer : branchement d'office, une fois le document lu.
  // La coquille (chrome.js) branche AVANT, pendant la lecture, avec ses echappatoires : ce
  // branchement d'office la trouve deja faite et ne fait rien.
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => brancher());
  } else {
    setTimeout(() => brancher(), 0);
  }
})();
