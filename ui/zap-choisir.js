(() => {
  /* Selecteur intelligent du zappeur, injecte quand on clique « Choisir un élément ».
     - survol : un halo entoure l'element, une etiquette dit ce que c'est et combien la page
       en compte de semblables ;
     - molette (ou fleches haut/bas) : agrandit au bloc parent / revient a l'enfant ;
     - clic : un petit menu propose « celui-ci seulement », « tous les semblables », et selon le
       cas « ceux qui menent a /shorts/… » ou « seulement ceux dont le texte est … » ;
     - Echap annule.
     Les selecteurs produits n'utilisent jamais `>` : Trame retire `<` et `>` des regles (elles
     sont injectees dans une balise <style>). Ils privilegient les noms stables du site (balises
     personnalisees, attributs aria/title/role, classes lisibles) plutot que les positions. */
  if (window.__trame_pick_fin) { window.__trame_pick_fin(); return; }
  // La teinte de Trame, posee par Rust a l'injection (src/teinte.rs remplace __TRAME_TEINTE__) ;
  // sans elle (banc d'essai), l'aspect d'origine.
  const T = (typeof __TRAME_TEINTE__ !== "undefined" && __TRAME_TEINTE__) ||
    { fond: "#080b11", surface: "#101521", surface2: "#151c2a", accent: "#60a5fa", rgb: "96, 165, 250", clair: "#dbe9ff" };

  const racine = document.body || document.documentElement;
  const Z = 2147483646;
  const halo = document.createElement("div");
  halo.style.cssText = `position:fixed;z-index:${Z};pointer-events:none;border:2px solid ${T.accent};background:rgba(${T.rgb},.16);border-radius:4px;box-sizing:border-box;left:0;top:0;width:0;height:0;transition:all .05s`;
  const semblablesHalo = document.createElement("div");
  semblablesHalo.style.cssText = `position:fixed;inset:0;z-index:${Z - 1};pointer-events:none`;
  const etiquette = document.createElement("div");
  etiquette.style.cssText = `position:fixed;z-index:${Z};pointer-events:none;max-width:460px;padding:6px 9px;border-radius:7px;background:${T.surface};color:${T.clair};font:12px/1.4 "Segoe UI",system-ui,sans-serif;box-shadow:0 6px 20px rgba(0,0,0,.45);border:1px solid rgba(${T.rgb},.35)`;
  racine.appendChild(semblablesHalo);
  racine.appendChild(halo);
  racine.appendChild(etiquette);

  let base = null; // element sous la souris
  let cible = null; // element choisi (base ou un de ses parents)
  const pile = []; // enfants quittes en remontant
  let menu = null;
  let figeX = 0, figeY = 0;

  // ------------------------------------------------------------ fabrication des selecteurs
  const css = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s.replace(/([^\w-])/g, "\\$1"));
  // Une classe qui porte le nom d'un composant englobant (`ytd-consent-bump-v2-lightbox` sur
  // tous les enfants du composant) ne designe rien : c'est une marque de portee du site.
  const classeStable = (c, el) =>
    !(c.includes("-") && /^[a-z][a-z0-9]*(-[a-z0-9]+)+$/.test(c) && el && el.closest && (() => { try { return el.closest(c); } catch (e) { return null; } })()) &&
    c.length <= 32 &&
    !/\d{3,}/.test(c) &&
    !(/\d/.test(c) && /[A-Z]/.test(c)) &&
    !/^(style-scope|active|hover|focus|focused|selected|open|opened|visible|hidden|show|is-)/.test(c) &&
    !/[:\[\]()]/.test(c);
  const ATTRS = ["is-shorts", "tab-title", "aria-label", "title", "role", "data-testid", "name", "type"];
  const attrStable = (el, a) => {
    if (!el.hasAttribute(a)) return null;
    const v = el.getAttribute(a);
    if (v === "") return `[${a}]`;
    if (v.length > 40 || /\d{4,}/.test(v) || /["\\\n]/.test(v)) return null;
    return `[${a}="${v}"]`;
  };
  // « Semblable » : la balise et ce qui la nomme durablement, sans position.
  const semblable = (el) => {
    const tag = el.tagName.toLowerCase();
    if (tag.includes("-")) {
      const marque = ["is-shorts", "tab-title"].map((a) => attrStable(el, a)).find(Boolean);
      return tag + (marque || "");
    }
    const classes = Array.from(el.classList).filter((c) => classeStable(c, el)).slice(0, 2).map((c) => "." + css(c));
    if (classes.length) return tag + classes.join("");
    const attr = ["role", "data-testid", "aria-label"].map((a) => attrStable(el, a)).find(Boolean);
    return tag + (attr || "");
  };
  const compter = (sel) => {
    try { return document.querySelectorAll(sel).length; } catch (e) { return 0; }
  };
  // « Celui-ci » : on remonte jusqu'a un identifiant stable ou jusqu'a ce que ce soit unique.
  const unique = (el) => {
    // Un identifiant ne sert que s'il est vraiment unique : YouTube repete `#video-title`,
    // `#contents`… sur chaque bloc.
    const idStable = (n) => n.id && /^[A-Za-z][\w-]{1,40}$/.test(n.id) && !/\d{3,}/.test(n.id) && compter("#" + css(n.id)) === 1;
    if (idStable(el)) return "#" + css(el.id);
    const parts = [];
    let n = el;
    for (let niveau = 0; n && n !== document.documentElement && niveau < 6; niveau++) {
      if (idStable(n) && niveau > 0) { parts.unshift("#" + css(n.id)); break; }
      let p = semblable(n);
      const attr = ATTRS.map((a) => attrStable(n, a)).find((x) => x && !p.includes(x));
      if (attr) p += attr;
      const parent = n.parentElement;
      if (parent) {
        const memes = Array.from(parent.children).filter((f) => f.tagName === n.tagName);
        if (memes.length > 1) p += `:nth-of-type(${memes.indexOf(n) + 1})`;
      }
      parts.unshift(p);
      const sel = parts.join(" ");
      if (compter(sel) === 1) return sel;
      n = parent;
    }
    return parts.join(" ");
  };
  // Les liens de l'element menent-ils tous a la meme rubrique (/shorts/, /promo/…) ?
  const rubrique = (el) => {
    const liens = Array.from(el.querySelectorAll("a[href^='/']"));
    if (el.matches && el.matches("a[href^='/']")) liens.push(el);
    if (!liens.length) return null;
    const tetes = {};
    for (const a of liens) {
      const m = (a.getAttribute("href") || "").match(/^\/([\w-]{2,30})\//);
      if (m) tetes[m[1]] = (tetes[m[1]] || 0) + 1;
    }
    const [tete, n] = Object.entries(tetes).sort((x, y) => y[1] - x[1])[0] || [];
    if (!tete || n / liens.length < 0.6 || ["watch", "channel", "c", "user"].includes(tete)) return null;
    return tete;
  };
  const texteDe = (el) => (el.textContent || "").replace(/\s+/g, " ").trim();
  const libelleDe = (el) => {
    const titre = el.querySelector && el.querySelector("h1, h2, h3, #video-title, [title]");
    const t =
      el.getAttribute("aria-label") ||
      el.getAttribute("title") ||
      (titre && (titre.getAttribute("title") || texteDe(titre))) ||
      texteDe(el);
    return t.slice(0, 60) || el.tagName.toLowerCase();
  };
  // Garde-fou : un element qui couvre presque toute la fenetre est la page elle-meme
  // (`ytd-app`, le conteneur principal) — le masquer viderait le site.
  const tropGrand = (el) => {
    if (el === document.body || el === document.documentElement) return true;
    const r = el.getBoundingClientRect();
    return r.width * Math.min(r.height, innerHeight) >= 0.85 * innerWidth * innerHeight;
  };

  // ------------------------------------------------------------ affichage
  const placer = (el) => {
    const r = el.getBoundingClientRect();
    Object.assign(halo.style, { left: r.left + "px", top: r.top + "px", width: r.width + "px", height: r.height + "px" });
    const sel = semblable(el);
    const n = compter(sel);
    etiquette.textContent = "";
    const l1 = document.createElement("div");
    l1.style.cssText = `font:11px Consolas,monospace;color:${T.clair}`;
    l1.textContent = sel;
    const l2 = document.createElement("div");
    l2.textContent = `${n} semblable${n > 1 ? "s" : ""} sur la page · molette : bloc parent / enfant · clic : choisir · Échap`;
    l2.style.color = "#8fa3bd";
    etiquette.append(l1, l2);
    const haut = r.top > 70 ? r.top - 52 : Math.min(innerHeight - 56, r.bottom + 6);
    Object.assign(etiquette.style, { left: Math.max(6, Math.min(innerWidth - 470, r.left)) + "px", top: Math.max(6, haut) + "px" });
    // Les semblables, en pointille.
    semblablesHalo.textContent = "";
    if (n > 1 && n < 400) {
      let i = 0;
      for (const autre of document.querySelectorAll(sel)) {
        if (autre === el || i++ > 150) continue;
        const ra = autre.getBoundingClientRect();
        if (ra.bottom < 0 || ra.top > innerHeight || !ra.width) continue;
        const d = document.createElement("div");
        d.style.cssText = `position:fixed;left:${ra.left}px;top:${ra.top}px;width:${ra.width}px;height:${ra.height}px;border:1px dashed rgba(${T.rgb},.7);border-radius:4px;box-sizing:border-box`;
        semblablesHalo.appendChild(d);
      }
    }
  };
  const viser = (el) => {
    if (!el || el === halo || el === etiquette || etiquette.contains(el)) return;
    cible = el;
    placer(el);
  };

  // ------------------------------------------------------------ menu de choix
  const poster = (selecteur, libelle, texte, action, valeur) => {
    try {
      window.ipc.postMessage(JSON.stringify({ m: "zapChoisi", selecteur, libelle, texte: texte || "", action: action || "", valeur: valeur || "" }));
    } catch (e) {}
    fin();
  };
  // Deuxieme etape : que faire de ce qu'on a designe. Une condition de texte ne sait que masquer.
  const envoyer = (selecteur, libelle, texte) => {
    if (texte) return poster(selecteur, libelle, texte, "", "");
    const x = parseFloat(menu.style.left), y = parseFloat(menu.style.top);
    menu.remove();
    menu = null;
    const actions = [
      ["Masquer", () => poster(selecteur, libelle, "", "", "")],
      ["Élargir (pleine largeur)", () => poster(selecteur, libelle, "", "elargir", "")],
      ["Replier (un clic le déplie)", () => poster(selecteur, libelle, "", "replier", "")],
    ];
    if (!selecteur.includes(":has(")) {
      actions.push(["Garder seulement ceci (le reste de la page disparaît)", () => poster(selecteur, libelle, "", "garder", "")]);
    }
    // Deplacer : seulement dans une rangee ou une grille, ou `order` reordonne sans rien casser.
    let parent = null;
    try { parent = (document.querySelector(selecteur) || cible).parentElement; } catch (e) { parent = cible && cible.parentElement; }
    const dispo = parent ? getComputedStyle(parent).display : "";
    if (/flex|grid/.test(dispo)) {
      actions.push(
        ["Remonter en premier dans son bloc", () => poster(selecteur, libelle, "", "haut", "")],
        ["Descendre en dernier dans son bloc", () => poster(selecteur, libelle, "", "bas", "")],
      );
    }
    actions.push(
      ["Changer le texte…", () => demander("Nouveau texte", cible ? texteDe(cible).slice(0, 120) : "", "text", (v) => poster(selecteur, libelle, "", "texte", v))],
      ["Texte plus grand", () => poster(selecteur, libelle, "", "style", "font-size: 1.25em")],
      ["Texte plus petit", () => poster(selecteur, libelle, "", "style", "font-size: 0.85em")],
      ["Atténuer", () => poster(selecteur, libelle, "", "style", "opacity: 0.45")],
      ["Changer la couleur du texte…", () => demander("Couleur", "#60a5fa", "color", (v) => poster(selecteur, libelle, "", "style", `color: ${v}`))],
      ["Annuler", fin],
    );
    afficherMenu(x, y, actions);
  };
  // Une saisie dans le menu (texte ou couleur), validee par Entree ou « OK ».
  const demander = (titre, defaut, type, suite) => {
    const x = parseFloat(menu.style.left), y = parseFloat(menu.style.top);
    menu.remove();
    menu = null;
    afficherMenu(x, y, [["Annuler", fin]]);
    const bloc = document.createElement("div");
    bloc.style.cssText = "display:flex;gap:6px;padding:4px 4px 6px";
    const champ = document.createElement("input");
    champ.type = type;
    champ.value = defaut;
    champ.placeholder = titre;
    champ.style.cssText = `flex:1;min-width:0;padding:7px 9px;border-radius:7px;border:1px solid rgba(${T.rgb},.4);background:${T.surface2};color:${T.clair};font:inherit`;
    const ok = document.createElement("button");
    ok.textContent = "OK";
    ok.style.cssText = `padding:7px 12px;border:0;border-radius:7px;background:rgba(${T.rgb},.25);color:${T.clair};cursor:pointer;font:inherit`;
    const valider = () => { const v = champ.value.trim(); if (v) suite(v); };
    ok.addEventListener("click", (e) => { e.preventDefault(); e.stopPropagation(); valider(); }, true);
    champ.addEventListener("keydown", (e) => { e.stopPropagation(); if (e.key === "Enter") { e.preventDefault(); valider(); } if (e.key === "Escape") fin(); }, true);
    bloc.append(champ, ok);
    menu.prepend(bloc);
    setTimeout(() => champ.focus(), 0);
  };
  const ouvrirMenu = (x, y) => {
    const el = cible;
    const sel = semblable(el);
    const n = compter(sel);
    const options = [];
    if (tropGrand(el)) {
      options.push(["Cet élément est la page entière : visez plus petit (molette vers le bas)", () => { menu.remove(); menu = null; etiquette.style.display = ""; }]);
      options.push(["Annuler", fin]);
      return afficherMenu(x, y, options);
    }
    options.push([`Masquer celui-ci seulement`, () => envoyer(unique(el), libelleDe(el))]);
    if (n > 1) options.push([`Masquer les ${n} semblables (${sel})`, () => envoyer(sel, `${libelleDe(el)} et semblables`)]);
    const tete = rubrique(el);
    if (tete) {
      const selR = `${sel}:has(a[href^="/${tete}/"])`;
      const nR = compter(selR);
      if (nR > 0) options.push([`Masquer ceux qui mènent à /${tete}/ (${nR})`, () => envoyer(selR, `${sel} vers /${tete}/`)]);
    }
    const t = texteDe(el);
    if (t && t.length <= 30 && n > 1) {
      const pareils = Array.from(document.querySelectorAll(sel)).filter((e) => texteDe(e) === t).length;
      if (pareils < n) options.push([`Masquer seulement ceux dont le texte est « ${t} » (${pareils})`, () => envoyer(sel, `« ${t} »`, t)]);
    }
    options.push(["Annuler", fin]);
    afficherMenu(x, y, options);
  };
  const afficherMenu = (x, y, options) => {
    menu = document.createElement("div");
    menu.style.cssText = `position:fixed;z-index:${Z};left:${Math.min(x, innerWidth - 380)}px;top:${Math.min(y, innerHeight - 40 * options.length - 20)}px;min-width:300px;max-width:420px;padding:6px;border-radius:10px;background:${T.surface};border:1px solid rgba(${T.rgb},.4);box-shadow:0 10px 30px rgba(0,0,0,.5);font:13px/1.35 "Segoe UI",system-ui,sans-serif`;
    for (const [texte, action] of options) {
      const b = document.createElement("button");
      b.textContent = texte;
      b.style.cssText = `display:block;width:100%;text-align:left;margin:2px 0;padding:8px 10px;border:0;border-radius:7px;background:transparent;color:${T.clair};cursor:pointer;font:inherit`;
      b.onmouseenter = () => (b.style.background = `rgba(${T.rgb},.16)`);
      b.onmouseleave = () => (b.style.background = "transparent");
      b.addEventListener("click", (e) => { e.preventDefault(); e.stopPropagation(); action(); }, true);
      menu.appendChild(b);
    }
    racine.appendChild(menu);
    etiquette.style.display = "none";
  };

  // ------------------------------------------------------------ evenements
  const dansMenu = (e) => menu && menu.contains(e.target);
  const bouge = (e) => {
    if (menu) return;
    if (pile.length && Math.abs(e.clientX - figeX) + Math.abs(e.clientY - figeY) < 12) return;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (!el || el === base) return;
    base = el;
    pile.length = 0;
    viser(el);
  };
  const monter = () => {
    if (cible && cible.parentElement && !tropGrand(cible.parentElement)) {
      pile.push(cible);
      viser(cible.parentElement);
    }
  };
  const descendre = () => { if (pile.length) viser(pile.pop()); };
  const molette = (e) => {
    if (menu) return;
    e.preventDefault();
    e.stopPropagation();
    figeX = e.clientX; figeY = e.clientY;
    if (e.deltaY < 0) monter(); else descendre();
  };
  const clic = (e) => {
    if (dansMenu(e)) return;
    e.preventDefault();
    e.stopPropagation();
    if (menu) { fin(); return; }
    if (cible) ouvrirMenu(e.clientX + 8, e.clientY + 8);
  };
  const bloquer = (e) => { if (!dansMenu(e)) { e.preventDefault(); e.stopPropagation(); } };
  const touche = (e) => {
    if (dansMenu(e) && e.target.tagName === "INPUT") return;
    if (e.key === "Escape") { e.preventDefault(); fin(); }
    else if (!menu && e.key === "ArrowUp") { e.preventDefault(); monter(); }
    else if (!menu && e.key === "ArrowDown") { e.preventDefault(); descendre(); }
  };
  const fin = () => {
    for (const n of [halo, semblablesHalo, etiquette, menu]) if (n) n.remove();
    document.removeEventListener("mousemove", bouge, true);
    document.removeEventListener("wheel", molette, { capture: true });
    document.removeEventListener("click", clic, true);
    document.removeEventListener("mousedown", bloquer, true);
    document.removeEventListener("mouseup", bloquer, true);
    document.removeEventListener("keydown", touche, true);
    delete window.__trame_pick_fin;
  };
  window.__trame_pick_fin = fin;
  document.addEventListener("mousemove", bouge, true);
  document.addEventListener("wheel", molette, { capture: true, passive: false });
  document.addEventListener("click", clic, true);
  document.addEventListener("mousedown", bloquer, true);
  document.addEventListener("mouseup", bloquer, true);
  document.addEventListener("keydown", touche, true);
})();
