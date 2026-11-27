/* Rubrique « Téléchargements » des réglages : rangement des fichiers au moment où ils arrivent.
   La décision vit dans Trame (src/rangement.rs) ; cette page ne fait que montrer le réglage et
   envoyer les gestes. L'essai (« où irait ce fichier ? ») est calculé par Trame aussi : une seule
   logique, jamais une copie en JavaScript qui finirait par dire autre chose.

   Portée fermée : wry pose `window.ipc` en propriété non configurable. */
(function () {
  const ipc = (o) => {
    try { window.ipc.postMessage(JSON.stringify(o)); } catch (e) {}
  };
  const $ = (id) => document.getElementById(id);
  if (!$("r-telechargements")) return;

  const style = document.createElement("style");
  style.textContent = `
    .rg-maitre { display: flex; align-items: center; gap: 16px; padding: 16px 18px; border-radius: 14px;
      border: 1px solid rgba(94,234,212,.25); background: rgba(94,234,212,.04); color: var(--vif); }
    .rg-maitre .libelle { flex: 1; color: var(--texte); }
    .rg-maitre b { font-size: 14px; display: block; }
    .rg-maitre span { color: var(--texte-2); font-size: 11.5px; }
    #r-telechargements.rg-coupe .rg-deux { opacity: .45; filter: saturate(.4); }
    .rg-deux { display: grid; grid-template-columns: minmax(0,1fr) 250px; gap: 18px; align-items: start; transition: opacity .2s; }
    @media (max-width: 1100px) { .rg-deux { grid-template-columns: minmax(0,1fr); } .rg-arbre { position: static !important; } }
    .rg-droite { margin-left: auto; letter-spacing: 0; text-transform: none; font-weight: 400; display: flex; gap: 6px; }
    .rg-pastille { font-size: 10px; color: var(--ambre); border: 1px solid rgba(251,191,36,.4); border-radius: 5px; padding: 1px 6px; letter-spacing: .04em; }
    button.rg-leger { background: transparent; border-color: transparent; color: var(--texte-3); padding: 4px 8px; }
    button.rg-leger:hover { color: var(--texte); background: rgba(255,255,255,.06); }
    .rg-cats { display: grid; grid-template-columns: repeat(3, minmax(0,1fr)); gap: 10px; }
    @media (max-width: 760px) { .rg-cats { grid-template-columns: repeat(2, minmax(0,1fr)); } }
    .rg-cat { border: 1px solid var(--bord); border-radius: 12px; padding: 12px 13px; background: rgba(255,255,255,.025);
      display: flex; flex-direction: column; gap: 8px; min-width: 0; }
    .rg-cat.eteinte { opacity: .45; }
    .rg-cat .tete { display: flex; align-items: center; gap: 9px; }
    .rg-cat .ico { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; flex: 0 0 auto; }
    .rg-cat .nom { font-weight: 600; font-size: 12.5px; flex: 1; min-width: 0; }
    .rg-cat .vers { font-size: 11px; color: var(--texte-2); display: flex; align-items: center; gap: 5px; min-width: 0; }
    .rg-cat .vers code { color: var(--texte); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .rg-cat .modifier { margin-left: auto; padding: 1px 6px; font-size: 11px; }
    .rg-cat input { width: 100%; font-size: 11.5px; padding: 4px 7px; }
    .rg-puces { display: flex; flex-wrap: wrap; gap: 4px; }
    .rg-puce { font-size: 10.5px; padding: 1px 6px; border-radius: 5px; background: rgba(255,255,255,.06); color: var(--texte-2); font-family: ui-monospace, Consolas, monospace; }
    .rg-puce.plus { color: var(--texte-3); background: transparent; border: 1px dashed var(--bord-fort); font-family: inherit; }
    .rg-regle { display: grid; grid-template-columns: 18px minmax(0,1fr) auto auto auto; align-items: center; gap: 12px; padding: 11px 2px; border-bottom: 1px solid var(--bord); }
    .rg-regle:last-child { border-bottom: 0; }
    .rg-regle.glisse { opacity: .4; }
    .rg-regle.dessus { box-shadow: 0 -2px 0 var(--accent) inset; }
    .rg-regle .poignee { color: var(--texte-3); cursor: grab; text-align: center; user-select: none; }
    .rg-regle .phrase { line-height: 1.8; min-width: 0; }
    .rg-regle.eteinte .phrase { opacity: .45; }
    .rg-mot { color: var(--texte-3); }
    .rg-jeton { display: inline-block; padding: 0 7px; border-radius: 6px; border: 1px solid var(--bord-fort); background: rgba(255,255,255,.04); margin: 0 2px; }
    .rg-jeton.site { border-color: rgba(167,139,250,.45); background: rgba(167,139,250,.08); }
    .rg-jeton.type { border-color: rgba(var(--accent-rgb),.45); background: rgba(var(--accent-rgb),.08); }
    .rg-jeton.dest { border-color: rgba(94,234,212,.45); background: rgba(94,234,212,.08); font-family: ui-monospace, Consolas, monospace; font-size: 11.5px; }
    .rg-compte { font-size: 10.5px; color: var(--texte-3); white-space: nowrap; }
    .rg-vide { color: var(--texte-3); padding: 14px 2px; font-size: 12px; }
    .rg-edition { padding: 14px 2px; border-bottom: 1px solid var(--bord); display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
    .rg-edition:last-child { border-bottom: 0; }
    .rg-edition label, .rg-essai label { display: flex; flex-direction: column; gap: 4px; font-size: 10.5px; color: var(--texte-3); text-transform: uppercase; letter-spacing: .08em; }
    .rg-edition input, .rg-essai input { font: inherit; font-size: 12.5px; text-transform: none; letter-spacing: 0; color: var(--texte);
      background: rgba(255,255,255,.05); border: 1px solid var(--bord); border-radius: 8px; padding: 7px 10px; }
    .rg-edition input:focus, .rg-essai input:focus { outline: none; border-color: rgba(var(--accent-rgb),.55); }
    .rg-edition .aide { grid-column: 1 / -1; color: var(--texte-3); font-size: 11px; text-transform: none; letter-spacing: 0; }
    .rg-edition .boutons { grid-column: 1 / -1; display: flex; gap: 8px; }
    .rg-edition .boutons .danger { margin-left: auto; }
    .rg-essai { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
    .rg-resultat { margin-top: 12px; padding: 11px 13px; border-radius: 10px; background: rgba(0,0,0,.25); border: 1px solid var(--bord); line-height: 1.6; }
    .rg-resultat .fleche { color: var(--vif); margin-right: 8px; }
    .rg-resultat code { font-size: 12px; color: var(--texte); }
    .rg-resultat .pourquoi { display: block; color: var(--texte-3); font-size: 11px; }
    .rg-arbre { position: sticky; top: 24px; border: 1px solid var(--bord); border-radius: 14px; padding: 14px 16px; background: rgba(255,255,255,.02); }
    .rg-arbre h4 { margin: 0 0 10px; font-size: 10.5px; letter-spacing: .16em; text-transform: uppercase; color: var(--texte-3); font-weight: 600; }
    .rg-arbre ul { list-style: none; margin: 0; padding-left: 14px; border-left: 1px solid var(--bord); }
    .rg-arbre > ul { padding-left: 0; border-left: 0; }
    .rg-arbre li { padding: 2px 0; font-size: 12px; }
    .rg-arbre li > a { color: var(--texte); text-decoration: none; cursor: pointer; }
    .rg-arbre li > a:hover { color: var(--accent); }
    .rg-arbre li .n { color: var(--texte-3); font-size: 10.5px; margin-left: 5px; }
    .rg-arbre li.regle > a { color: var(--vif); }
    .rg-arbre .note { margin-bottom: 0; }
  `;
  document.head.appendChild(style);

  const ICONES = {
    images: ["#a78bfa", '<path d="M4 5h16v14H4z"/><path d="m4 16 5-5 4 4 3-3 4 4"/><circle cx="15.5" cy="8.5" r="1.3"/>'],
    videos: ["#f472b6", '<rect x="3" y="6" width="13" height="12" rx="2"/><path d="m16 10 5-3v10l-5-3"/>'],
    musique: ["#fbbf24", '<path d="M9 18V5l11-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="17" cy="16" r="3"/>'],
    documents: ["#60a5fa", '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4M9 12h6M9 16h6"/>'],
    archives: ["#fb923c", '<path d="M4 7h16v13H4zM3 4h18v3H3z"/><path d="M10 11h4"/>'],
    installeurs: ["#5eead4", '<path d="M12 3v12m0 0 4-4m-4 4-4-4"/><path d="M4 17v3h16v-3"/>'],
  };
  const ICONE_AUTRE = ["#97a4bb", '<path d="M3 7h6l2 2h10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>'];

  const el = (tag, classe, texte) => {
    const d = document.createElement(tag);
    if (classe) d.className = classe;
    if (texte !== undefined && texte !== null) d.textContent = String(texte);
    return d;
  };
  const jeton = (classe, texte) => el("span", "rg-jeton " + classe, texte);
  const mot = (texte) => el("span", "rg-mot", " " + texte + " ");

  let r = null;            // etat.reglages.rangement
  let catEditee = "";      // identifiant de la tuile en modification
  let regleEditee = null;  // identifiant de la règle en modification (0 = nouvelle)
  let essaiEnvoye = "";

  /* ------------------------------------------------------------------ types */
  function rendreCats() {
    const zone = $("rg-cats");
    zone.textContent = "";
    for (const c of r.categories) {
      const [coul, trace] = ICONES[c.id] || ICONE_AUTRE;
      const tuile = el("div", "rg-cat" + (c.actif ? "" : " eteinte"));
      const tete = el("div", "tete");
      const ico = el("span", "ico");
      ico.style.background = coul + "22";
      ico.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="${coul}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${trace}</svg>`;
      const bascule = el("div", "bascule" + (c.actif ? " on" : ""));
      bascule.title = c.actif ? `${c.nom} : rangés dans ${c.dossier}\\` : `${c.nom} : laissés à la racine`;
      bascule.onclick = () => ipc({ m: "rangementCategorie", id: c.id, actif: !c.actif });
      tete.append(ico, el("span", "nom", c.nom), bascule);
      tuile.append(tete);

      if (catEditee === c.id) {
        const dossier = el("input");
        dossier.value = c.dossier;
        dossier.title = "Sous-dossier où ranger ce type";
        const ext = el("input");
        ext.value = c.extensions.join(", ");
        ext.title = "Extensions, séparées par des virgules";
        const ok = el("button", "principal", "Enregistrer");
        const valider = () => {
          ipc({ m: "rangementCategorie", id: c.id, dossier: dossier.value, extensions: ext.value });
          catEditee = "";
        };
        ok.onclick = valider;
        for (const champ of [dossier, ext]) {
          champ.onkeydown = (e) => {
            if (e.key === "Enter") valider();
            if (e.key === "Escape") { catEditee = ""; rendreCats(); }
          };
        }
        tuile.append(dossier, ext, ok);
        setTimeout(() => dossier.focus(), 0);
      } else {
        const vers = el("div", "vers", "→ ");
        vers.append(el("code", "", c.dossier + "\\"));
        const modifier = el("button", "rg-leger modifier", "✎");
        modifier.title = "Modifier le dossier et les extensions";
        modifier.onclick = () => { catEditee = c.id; rendreCats(); };
        vers.append(modifier);
        const puces = el("div", "rg-puces");
        c.extensions.slice(0, 4).forEach((e) => puces.append(el("span", "rg-puce", "." + e)));
        if (c.extensions.length > 4) {
          const plus = el("span", "rg-puce plus", "+" + (c.extensions.length - 4));
          plus.title = c.extensions.slice(4).map((e) => "." + e).join(" ");
          puces.append(plus);
        }
        tuile.append(vers, puces);
      }
      zone.append(tuile);
    }
  }

  /* ---------------------------------------------------------------- règles */
  const NOMS_TYPES = () => Object.fromEntries(r.categories.map((c) => [c.id, c.nom]));
  const compteDe = (dossier) => {
    // Le dossier d'une règle peut contenir {année} : on compte tous les sous-dossiers qu'il a produits.
    const morceaux = dossier.split(/[\\/]+/).filter(Boolean);
    if (!morceaux.length) return 0;
    const motif = morceaux
      .map((m) => (/\{(année|annee|mois|date|nom)\}/.test(m) ? "[^\\\\]+" : m.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")))
      .join("\\\\");
    const re = new RegExp("^" + motif + "(\\\\|$)", "i");
    return Object.entries(r.compteurs || {}).reduce((n, [d, k]) => n + (re.test(d) ? k : 0), 0);
  };

  function phrase(g) {
    const p = el("div", "phrase");
    const noms = NOMS_TYPES();
    let premier = true;
    const lien = (txt) => { p.append(mot(premier ? txt[0] : txt[1])); premier = false; };
    if (g.sites.length) {
      lien(["Si ça vient de", "de"]);
      g.sites.forEach((s, i) => { if (i) p.append(mot("ou")); p.append(jeton("site", s)); });
    }
    if (g.types.length) {
      lien(["Si c'est", "et que c'est"]);
      g.types.forEach((t, i) => { if (i) p.append(mot("ou")); p.append(jeton("type", noms[t] || t.toUpperCase())); });
    }
    if (g.nomContient) {
      lien(["Si le nom contient", "et que le nom contient"]);
      p.append(jeton("", g.nomContient));
    }
    p.append(mot("→"), jeton("dest", g.dossier.replace(/[\\/]+$/, "") + "\\"));
    if (g.renommer) { p.append(mot("et renommer en"), jeton("dest", g.renommer)); }
    return p;
  }

  function editeur(g) {
    const f = el("div", "rg-edition");
    const champ = (libelle, valeur, aide) => {
      const l = el("label", "", libelle);
      const i = el("input");
      i.value = valeur || "";
      i.spellcheck = false;
      if (aide) i.placeholder = aide;
      l.append(i);
      f.append(l);
      return i;
    };
    const sites = champ("Si ça vient de", g.sites.join(", "), "pronote.fr, ameli.fr");
    const types = champ("Et que c'est", g.types.join(", "), "pdf, images…");
    const nom = champ("Et que le nom contient", g.nomContient, "facture");
    const dossier = champ("Ranger dans", g.dossier, "Papiers\\{année}");
    const renommer = champ("Renommer en (facultatif)", g.renommer, "{date} {nom}");
    f.append(el("span"));
    f.append(el("div", "aide",
      "Une condition vide ne filtre rien, mais il en faut au moins une. Types : une extension (pdf, zip…) ou un type de fichier ("
      + r.categories.map((c) => c.id).join(", ") + "). L'extension d'origine est toujours gardée."));
    const boutons = el("div", "boutons");
    const ok = el("button", "principal", g.id ? "Enregistrer" : "Ajouter la règle");
    const annuler = el("button", "", "Annuler");
    const valider = () => {
      ipc({ m: "rangementRegle", regle: {
        id: g.id || 0, actif: g.id ? g.actif : true,
        sites: sites.value, types: types.value, nomContient: nom.value, dossier: dossier.value, renommer: renommer.value,
      } });
      regleEditee = null;
    };
    ok.onclick = valider;
    annuler.onclick = () => { regleEditee = null; rendreRegles(); };
    boutons.append(ok, annuler);
    if (g.id) {
      const retirer = el("button", "danger", "Supprimer");
      retirer.onclick = () => { ipc({ m: "rangementRegleRetirer", id: g.id }); regleEditee = null; };
      boutons.append(retirer);
    }
    f.append(boutons);
    f.querySelectorAll("input").forEach((i) => {
      i.onkeydown = (e) => {
        if (e.key === "Enter") valider();
        if (e.key === "Escape") { regleEditee = null; rendreRegles(); }
      };
    });
    setTimeout(() => (g.id ? dossier : sites).focus(), 0);
    return f;
  }

  let glisse = null;
  function rendreRegles() {
    const zone = $("rg-regles");
    zone.textContent = "";
    if (!r.regles.length && regleEditee === null) {
      zone.append(el("div", "rg-vide", "Aucune règle : seuls les types rangent. Une règle sert à ranger selon le site ou le nom, par exemple « tout ce qui vient de pronote.fr → École »."));
    }
    for (const g of r.regles) {
      if (regleEditee === g.id) { zone.append(editeur(g)); continue; }
      const ligne = el("div", "rg-regle" + (g.actif ? "" : " eteinte"));
      ligne.dataset.id = g.id;
      ligne.draggable = true;
      const poignee = el("span", "poignee", "⋮⋮");
      poignee.title = "Glisser pour changer l'ordre";
      const n = compteDe(g.dossier);
      const compte = el("span", "rg-compte", n ? `${n} fichier${n > 1 ? "s" : ""}` : "—");
      const modifier = el("button", "rg-leger", "Modifier");
      modifier.onclick = () => { regleEditee = g.id; rendreRegles(); };
      const bascule = el("div", "bascule" + (g.actif ? " on" : ""));
      bascule.title = g.actif ? "Règle active : cliquer pour la suspendre" : "Règle suspendue : cliquer pour la reprendre";
      bascule.onclick = () => ipc({ m: "rangementRegleActif", id: g.id, actif: !g.actif });
      ligne.append(poignee, phrase(g), compte, modifier, bascule);
      ligne.addEventListener("dragstart", (e) => { glisse = g.id; ligne.classList.add("glisse"); e.dataTransfer.effectAllowed = "move"; });
      ligne.addEventListener("dragend", () => { glisse = null; ligne.classList.remove("glisse"); });
      ligne.addEventListener("dragover", (e) => { if (glisse !== null && glisse !== g.id) { e.preventDefault(); ligne.classList.add("dessus"); } });
      ligne.addEventListener("dragleave", () => ligne.classList.remove("dessus"));
      ligne.addEventListener("drop", (e) => {
        e.preventDefault();
        ligne.classList.remove("dessus");
        if (glisse === null || glisse === g.id) return;
        const ids = r.regles.map((x) => x.id).filter((id) => id !== glisse);
        ids.splice(ids.indexOf(g.id), 0, glisse);
        ipc({ m: "rangementReglesOrdre", ids });
      });
      zone.append(ligne);
    }
    if (regleEditee === 0) {
      zone.append(editeur({ id: 0, actif: true, sites: [], types: [], nomContient: "", dossier: "", renommer: "" }));
    }
  }

  /* ----------------------------------------------------------- arborescence */
  function rendreArbre() {
    const zone = $("rg-arbre");
    zone.textContent = "";
    zone.append(el("h4", "", "Ce que ça donne"));
    // Dossiers de premier niveau : ceux des règles (en vert) puis ceux des types allumés.
    const noeuds = new Map();
    const ajouter = (chemin, regle) => {
      const morceaux = chemin.split(/[\\/]+/).filter(Boolean);
      if (!morceaux.length) return;
      const tete = morceaux[0];
      const n = noeuds.get(tete) || { regle: false, enfants: new Set(), chemin: tete };
      n.regle = n.regle || regle;
      if (morceaux[1]) n.enfants.add(morceaux[1]);
      noeuds.set(tete, n);
    };
    r.regles.filter((g) => g.actif).forEach((g) => ajouter(g.dossier, true));
    r.categories.filter((c) => c.actif).forEach((c) => ajouter(c.dossier, false));
    const compteSous = (prefixe) => Object.entries(r.compteurs || {})
      .reduce((s, [d, k]) => s + ((d === prefixe || d.startsWith(prefixe + "\\")) ? k : 0), 0);
    const racine = el("ul");
    const li = el("li");
    const nomRacine = (r.racine || "Téléchargements").split(/[\\/]/).filter(Boolean).pop() || "Téléchargements";
    const a = el("a", "", "📁 " + nomRacine);
    a.title = "Ouvrir " + r.racine;
    a.onclick = () => ipc({ m: "rangementOuvrir", dossier: "" });
    li.append(a);
    const ul = el("ul");
    [...noeuds.values()].sort((x, y) => x.chemin.localeCompare(y.chemin, "fr")).forEach((n) => {
      const item = el("li", n.regle ? "regle" : "");
      const lien = el("a", "", "📁 " + n.chemin);
      lien.title = "Ouvrir " + n.chemin;
      lien.onclick = () => ipc({ m: "rangementOuvrir", dossier: n.chemin });
      item.append(lien);
      const k = compteSous(n.chemin);
      if (k) item.append(el("span", "n", k));
      if (n.enfants.size) {
        const sous = el("ul");
        [...n.enfants].sort().forEach((e) => {
          const s = el("li", n.regle ? "regle" : "");
          const auj = new Date();
          const exemple = e
            .replace(/\{(année|annee)\}/g, String(auj.getFullYear()))
            .replace(/\{mois\}/g, String(auj.getMonth() + 1).padStart(2, "0"));
          const l = el("a", "", "📁 " + exemple);
          if (!/\{/.test(e)) l.onclick = () => ipc({ m: "rangementOuvrir", dossier: n.chemin + "\\" + e });
          s.append(l);
          const k2 = compteSous(n.chemin + "\\" + e);
          if (k2) s.append(el("span", "n", k2));
          sous.append(s);
        });
        item.append(sous);
      }
      ul.append(item);
    });
    const reste = el("li", "");
    reste.append(el("span", "rg-mot", "… et le reste, à la racine"));
    ul.append(reste);
    li.append(ul);
    racine.append(li);
    zone.append(racine);
    zone.append(el("p", "note", "Les nombres comptent ce que Trame a rangé. Un clic ouvre le dossier. Rien n'est lu dans les fichiers eux-mêmes."));
  }

  /* ------------------------------------------------------------------ essai */
  function demanderEssai() {
    const cle = $("rg-e-nom").value + "|" + $("rg-e-site").value;
    if (cle === essaiEnvoye) return;
    essaiEnvoye = cle;
    ipc({ m: "rangementEssai", nom: $("rg-e-nom").value, site: $("rg-e-site").value });
  }
  function rendreEssai() {
    const e = r.essai;
    const zone = $("rg-e-res");
    zone.textContent = "";
    if (!e) return;
    zone.append(el("span", "fleche", "→"), el("code", "", (r.racine ? r.racine.replace(/[\\/]+$/, "") + "\\" : "") + e.chemin));
    zone.append(el("span", "pourquoi", e.raison + (r.actif ? "" : " — le rangement est coupé : pour l'instant, tout arrive à la racine.")));
  }
  let attente = 0;
  for (const id of ["rg-e-nom", "rg-e-site"]) {
    $(id).addEventListener("input", () => { clearTimeout(attente); attente = setTimeout(demanderEssai, 150); });
  }

  /* --------------------------------------------------------------- boutons */
  $("rg-actif").onclick = () => r && ipc({ m: "rangementActif", actif: !r.actif });
  $("rg-mois").onclick = () => r && ipc({ m: "rangementParMois", actif: !r.parMois });
  $("rg-ajouter").onclick = () => { regleEditee = 0; rendreRegles(); };
  $("rg-types-defaut").onclick = () => {
    if (confirm("Remettre les six types de fichiers comme au départ ? Vos dossiers et extensions modifiés seront oubliés (les règles restent).")) {
      ipc({ m: "rangementCategoriesDefaut" });
    }
  };
  $("rg-ouvrir").onclick = () => ipc({ m: "telechargementsDossier" });
  $("rg-changer").onclick = () => ipc({ m: "telechargementsChoisirDossier" });

  window.TrameRangement = {
    rendre(etat) {
      const neuf = etat && etat.reglages && etat.reglages.rangement;
      if (!neuf) return;
      r = neuf;
      $("rg-racine").textContent = r.racine || "—";
      $("rg-actif").className = "bascule" + (r.actif ? " on" : "");
      $("rg-mois").className = "bascule" + (r.parMois ? " on" : "");
      $("r-telechargements").classList.toggle("rg-coupe", !r.actif);
      const nav = $("nav-rangement");
      if (nav) nav.textContent = r.actif ? "rangés" : "";
      // Un champ en cours de saisie n'est pas redessiné sous les doigts.
      const saisie = document.activeElement && document.activeElement.closest && document.activeElement.closest(".rg-cat, .rg-edition");
      if (!saisie) { rendreCats(); rendreRegles(); }
      rendreArbre();
      rendreEssai();
      if (!r.essai) demanderEssai();
    },
  };
})();
