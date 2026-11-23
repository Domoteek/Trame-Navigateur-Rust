/* Trame — logique des surfaces d'interface (rail + barre).
   Regle de securite : aucun titre d'onglet ni URL n'est insere via innerHTML.
   Un titre de page est une chaine hostile : `<img onerror=...>` s'executerait dans
   l'origine de l'interface, qui a le droit de commander le navigateur. Tout passe par
   textContent.

   POURQUOI TOUT EST DANS UNE FONCTION : wry definit `window.ipc` en propriete NON
   configurable. Un `const ipc` de premier niveau jette alors
   « Uncaught SyntaxError: Identifier 'ipc' has already been declared » et TOUT le
   script ne s'execute pas — ecran noir, sans erreur visible. Mesure du 2026-11-20. */

(function () {
  const params = new URLSearchParams(location.search);
const surface = params.get("surface") === "barre" ? "barre" : "rail";
const ipc = (o) => {
  try {
    window.ipc.postMessage(JSON.stringify(o));
  } catch (e) {
    /* pas d'IPC : l'interface reste utilisable en lecture */
  }
};

let etat = null;
let pret = false;

function elem(tag, classe, texte) {
  const d = document.createElement(tag);
  if (classe) d.className = classe;
  if (texte !== undefined && texte !== null) d.textContent = String(texte);
  return d;
}

function initiale(texte) {
  const t = (texte || "?").trim();
  return t ? t[0].toUpperCase() : "?";
}

/** Construit une icone du jeu de symboles de chrome.html. */
const NS_SVG = "http://www.w3.org/2000/svg";
function icone(nom, taille) {
  const svg = document.createElementNS(NS_SVG, "svg");
  svg.setAttribute("class", taille === "petit" ? "ic petit" : "ic");
  const use = document.createElementNS(NS_SVG, "use");
  use.setAttribute("href", "#i-" + nom);
  svg.appendChild(use);
  return svg;
}

function arrondi(v) {
  return Math.round(v * 100) / 100;
}

/* ------------------------------------------------------------------ toast */

let minuterieToast = null;
function toast(message) {
  const t = document.getElementById("toast");
  if (!t) return;
  t.textContent = message;
  t.classList.add("visible");
  clearTimeout(minuterieToast);
  minuterieToast = setTimeout(() => t.classList.remove("visible"), 5200);
}

/* --------------------------------------------------------------- API hote */

window.trame = {
  etat(nouvelEtat) {
    pret = true;
    etat = nouvelEtat;
    rendreFenetre(etat);
    if (surface === "rail") rendreRail(etat);
    else rendreBarre(etat);
  },
  budget(b) {
    if (!etat) return;
    etat.budget = b;
    rendreBudget();
  },
  resultats(r) {
    if (surface !== "barre") return;
    recevoirResultats(r);
  },
  /** Resultats venus plus tard (memoire de lecture, cherchee par un fil a part). */
  resultatsPlus(r) {
    if (surface !== "barre" || r.rid !== rid || !panneauOuvert) return;
    const deja = new Set(items.map((i) => i.id));
    const nouveaux = (r.items || []).filter((i) => !deja.has(i.id));
    lecturesAttendues = false;
    if (!nouveaux.length && items.length) return;
    items = items.concat(nouveaux);
    if (selection < 0 && items.length) selection = 0;
    rendreResultats();
  },
  menuPage(ancre) {
    if (surface !== "barre") return;
    ouvrirMenuPage(ancre);
  },
  /** Bulle du rail replie, relayee par l'hote (message `bulle`) : `texte` vide = la cacher. */
  bulle(b) {
    if (surface !== "barre" || !window.__trameBulles) return;
    if (b && b.texte) window.__trameBulles.carte(b.texte, b.y || 0, b.h || 0);
    else window.__trameBulles.cacher();
  },
  menuCommandes(liste) {
    if (surface === "rail") ouvrirCommandesRail(liste || []);
    else ouvrirMenuCommandes(liste || []);
  },
  /** Clic droit sur un acces du rail replie, relaye par l'hote : `{ acces, y, h }`. */
  menuAcces(x) {
    if (surface !== "barre" || !x || !x.acces) return;
    ouvrirMenuAccesBarre(x);
  },
  toast,
  focusCouture() {
    if (surface !== "barre") return;
    const champ = document.getElementById("couture");
    champ.focus();
    champ.select();
    ouvrirPanneau();
  },
};

/* ===================================================== BARRE DE LA FENETRE */
/* Reglage « Barre de la fenetre » : hors du style « windows », la fenetre n'a pas de barre de
   titre. Les boutons (reduire, agrandir, fermer) sont dessines ici ; les zones vides du haut
   deplacent la fenetre, un double-clic l'agrandit, et la bande de 4 px tout en haut la
   redimensionne par le bord du haut. Le reste (bords gauche, droit et bas) est celui de Windows. */

const BOUTONS_FENETRE = [
  ["reduire", "f-reduire", "Réduire"],
  ["agrandir", "f-agrandir", "Agrandir"],
  ["fermer", "f-fermer", "Fermer"],
];

/** Zones qui deplacent la fenetre quand on les attrape dans le vide. */
const ZONES_GLISSER = ".titre-fenetre, .barre-ligne, .rail-tete";
/** Ce qui, dans ces zones, garde son propre clic. */
const PAS_GLISSER = "button, input, select, a, .couture, .pin-chips, .pastilles, .badge";

let styleFenetre = "";
let teintePosee = "";
let fenetreAgrandie = null;
let minuterieDispositions = 0;

function rendreFenetre(s) {
  const f = s.fenetre || {};
  const style = f.style || "windows";
  const agrandie = !!f.agrandie;
  if (style !== styleFenetre) {
    styleFenetre = style;
    document.body.dataset.fenetre = style;
  }
  // Teinte de la coquille (reglages > Apparence) : fonds, surfaces et accents, depuis la table
  // commune `ui/teintes.js` (la meme que les pastilles des reglages). Reposee seulement quand
  // elle change : l'etat arrive plusieurs fois par seconde.
  const teinte = (s && s.theme) || "defaut";
  const site = teinte === "dynamique" ? (s && s.teinteSite) || "" : "";
  if (window.TrameTeintes && teinte + "|" + site !== teintePosee) {
    teintePosee = teinte + "|" + site;
    window.TrameTeintes.appliquer(teinte, site);
  }
  // Style du rail replie (reglages > Apparence) : acces rapides et groupes, choisis separement.
  // Tout est en CSS (chrome.css, « styles du rail replie »), cle sur ces deux attributs.
  const rs = (s && s.railStyle) || {};
  const railAcces = rs.acces || "cases";
  const railOnglets = rs.onglets || "etiquettes";
  if (document.body.dataset.railAcces !== railAcces) document.body.dataset.railAcces = railAcces;
  if (document.body.dataset.railOnglets !== railOnglets) document.body.dataset.railOnglets = railOnglets;
  if (agrandie !== fenetreAgrandie) {
    fenetreAgrandie = agrandie;
    if (agrandie) document.body.dataset.agrandie = "1";
    else delete document.body.dataset.agrandie;
    for (const b of document.querySelectorAll('button[data-fenetre="agrandir"]')) {
      b.title = agrandie ? "Restaurer" : "Agrandir";
      const use = b.querySelector("use");
      if (use) use.setAttribute("href", agrandie ? "#i-f-restaurer" : "#i-f-agrandir");
    }
  }
  const titre = document.getElementById("titre-texte");
  if (titre && style === "bandeau") {
    const t = s.interne ? "Trame" : (s.titre || "Trame");
    if (titre.textContent !== t) titre.textContent = t;
  }
}

function initFenetre() {
  for (const zone of document.querySelectorAll(".boutons-fenetre")) {
    for (const [action, ic, titre] of BOUTONS_FENETRE) {
      const b = elem("button", action);
      b.dataset.fenetre = action;
      b.title = titre;
      b.appendChild(icone(ic));
      zone.appendChild(b);
    }
  }
  document.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-fenetre]");
    if (!b) return;
    clearTimeout(minuterieDispositions);
    ipc({ m: "fenetre", action: b.dataset.fenetre });
  });
  // Survol du bouton agrandir : les dispositions d'ancrage de Windows 11, comme sur une vraie
  // barre de titre (ouvertes par Win+Z, voir app/fenetre.rs).
  document.addEventListener("mouseover", (e) => {
    const b = e.target.closest('.boutons-fenetre [data-fenetre="agrandir"]');
    if (!b) return;
    clearTimeout(minuterieDispositions);
    minuterieDispositions = setTimeout(() => {
      if (b.matches(":hover")) ipc({ m: "fenetre", action: "dispositions" });
    }, 700);
  });
  document.addEventListener("mouseout", (e) => {
    if (e.target.closest('.boutons-fenetre [data-fenetre="agrandir"]')) clearTimeout(minuterieDispositions);
  });
  document.addEventListener("mousedown", (e) => {
    if (e.button !== 0 || styleFenetre === "windows" || !styleFenetre) return;
    const bord = e.target.closest(".bord-haut");
    if (bord) {
      e.preventDefault();
      // Les coins du haut : le rail tient le coin gauche, la barre le coin droit.
      const coin = 10;
      let direction = "haut";
      if (surface === "rail" && e.clientX < coin) direction = "haut-gauche";
      if (surface === "barre" && e.clientX > window.innerWidth - coin) direction = "haut-droite";
      ipc({ m: "fenetre", action: "bord", bord: direction });
      return;
    }
    if (!e.target.closest(ZONES_GLISSER) || e.target.closest(PAS_GLISSER)) return;
    e.preventDefault();
    ipc({ m: "fenetre", action: e.detail === 2 ? "agrandir" : "deplacer" });
  });
}

/* ==================================================================== RAIL */

function rendreBudget() {
  if (!etat || !etat.budget) return;
  const b = etat.budget;
  const txt = document.getElementById("budget-txt");
  const jauge = document.getElementById("budget-jauge");
  // Le plafond baisse quand la memoire libre du PC manque : on le dit, sinon des onglets
  // s'endormiraient sans raison visible.
  const limite = b.auto && b.plafondChoisi && b.plafond < b.plafondChoisi;
  if (txt) {
    txt.textContent =
      `${b.reveilles} éveillé(s) · ${b.endormis} en veille · ${Math.round(b.mo)} Mo` +
      (limite ? " · mémoire juste" : "");
    txt.classList.toggle("limite", !!limite);
    const lignes = [
      `Mémoire réelle de l'application : ${b.mo} Mo sur ${b.processus} processus`,
      b.extensionsProcessus
        ? `dont extensions Chrome : ${Math.round(b.extensionsMo)} Mo (${b.extensionsProcessus} processus)`
        : "aucune extension Chrome en cours d'exécution",
      b.dispoMo ? `Mémoire libre du PC : ${Math.round(b.dispoMo)} Mo` : "",
      limite
        ? `Mémoire libre faible : ${b.plafond} onglet(s) ouvert(s) au plus au lieu de ${b.plafondChoisi}. Les plus froids s'endorment ; rien n'est fermé.`
        : `Jusqu'à ${b.plafond} onglet(s) ouvert(s) ; une mise en veille détruit la webview, rien n'est fermé.`,
    ];
    txt.title = lignes.filter(Boolean).join("\n");
    // Rail replie : le texte est cache, la jauge porte le detail.
    document.getElementById("budget").title = txt.textContent + "\n" + txt.title;
  }
  const ratio = b.plafond > 0 ? Math.min(1, b.reveilles / b.plafond) : 0;
  if (jauge) jauge.style.width = `${Math.round(ratio * 100)}%`;
  // Rail replie : la jauge devient l'anneau autour du bouton Veille, qui porte aussi son detail.
  const veille = document.getElementById("btn-veille");
  if (veille) {
    veille.style.setProperty("--jauge", `${Math.round(ratio * 100)}%`);
    const base = "Endormir maintenant les onglets froids (leur page se ferme, l'onglet reste)";
    veille.title = txt ? `${base}\n\n${txt.textContent}\n${txt.title}` : base;
  }
}

/* Couleur d'un fil dans le rail replie : tiree de son identifiant, donc stable d'une session a
   l'autre. Un fil cloisonne garde l'ambre, sa marque partout ailleurs. */
const COULEURS_FIL = ["#5eead4", "#f0a35e", "#60a5fa", "#a78bfa", "#f472b6", "#facc15", "#34d399", "#fb7185"];
function couleurFil(f) {
  if (f.cloison) return "#fbbf24";
  let h = 0;
  for (const c of String(f.id)) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return COULEURS_FIL[h % COULEURS_FIL.length];
}

/* Fil ferme, rail replie : ses icones empilees sur une ligne (3 au plus, la 3e devient « +N »). */
function pileFil(f, onglets) {
  const pile = elem("button", "fil-pile");
  pile.title = `${f.nom} : ${onglets.length} onglet(s) — cliquer pour déplier`;
  const montres = onglets.length > 3 ? onglets.slice(0, 2) : onglets;
  for (const o of montres) {
    const p = elem("span", "pastille lettre", initiale(o.titre || o.domaine));
    if (o.favicon && /^https?:|^data:/.test(o.favicon)) {
      const img = document.createElement("img");
      img.src = o.favicon;
      img.alt = "";
      img.onerror = () => img.remove();
      p.textContent = "";
      p.appendChild(img);
    }
    pile.appendChild(p);
  }
  if (onglets.length > 3) pile.appendChild(elem("span", "pastille lettre plus", `+${onglets.length - 2}`));
  pile.onclick = (e) => {
    e.stopPropagation();
    ipc({ m: "filReplier", id: f.id, replie: false });
  };
  return pile;
}

function rendreSuggestion(s) {
  const zone = document.getElementById("suggestion");
  if (!zone) return;
  if (!s.suggestion) {
    zone.hidden = true;
    return;
  }
  const sug = s.suggestion;
  zone.hidden = false;
  const txt = document.getElementById("suggestion-txt");
  txt.textContent = "";
  txt.appendChild(document.createTextNode("« "));
  txt.appendChild(elem("b", null, sug.nomA));
  txt.appendChild(document.createTextNode(" » et « "));
  txt.appendChild(elem("b", null, sug.nomB));
  txt.appendChild(document.createTextNode(" » se ressemblent."));
  const oui = document.getElementById("suggestion-oui");
  oui.onclick = () => {
    ipc({ m: "fusionner", a: sug.a, b: sug.b });
    zone.hidden = true;
  };
  document.getElementById("suggestion-non").onclick = () => {
    zone.hidden = true;
  };
}

function ligneOnglet(o) {
  const d = elem("div", "onglet" + (o.actif ? " actif" : "") + (o.eveille ? "" : " endormi"));
  d.dataset.palier = o.palier;
  d.draggable = true;
  d.title = `${o.titre || o.urlCourte}\n${o.domaine || "page interne"}\n${o.eveille ? "éveillé" : "en veille — un clic le réveille"}\nvitalité ${arrondi(o.vitalite)} (${o.palier})`;

  d.appendChild(elem("span", "noeud"));

  const pastille = elem("span", "pastille lettre", initiale(o.titre || o.domaine));
  if (o.favicon && /^https?:|^data:/.test(o.favicon)) {
    const img = document.createElement("img");
    img.src = o.favicon;
    img.alt = "";
    img.onerror = () => img.remove();
    pastille.textContent = "";
    pastille.appendChild(img);
  }
  d.appendChild(pastille);

  const texte = elem("span", "texte");
  texte.appendChild(elem("span", "titre", o.titre || o.urlCourte || "Nouvel onglet"));
  texte.appendChild(elem("span", "domaine", o.domaine || (o.interne ? "page interne" : "")));
  d.appendChild(texte);

  const marques = elem("span", "marques");
  if (o.ancre) marques.appendChild(elem("span", "marque-mini ancre", "ancre"));
  if (o.alire) marques.appendChild(elem("span", "marque-mini alire", "à lire"));
  if (o.revenu) marques.appendChild(elem("span", "marque-mini revenu", "revenu"));
  if (o.fermeture) marques.appendChild(elem("span", "marque-mini fermeture", "ferme " + heure(o.fermeture)));
  const cote = etat && etat.carnet && etat.carnet.cote;
  if (cote && (cote.gauche === o.id || cote.droite === o.id)) {
    marques.appendChild(elem("span", "marque-mini cote", cote.gauche === o.id ? "gauche" : "droite"));
  }
  if (!o.eveille) marques.appendChild(elem("span", "marque-mini veille", "veille"));
  if (marques.childNodes.length) d.appendChild(marques);

  // Haut-parleur : l'onglet joue du son (clic = couper) ou son son est coupe (clic = rendre).
  if (o.son || o.muet) {
    const son = elem("button", "son" + (o.muet ? " coupe" : ""));
    son.title = o.muet ? "Son coupé — cliquer pour le rendre" : "Cet onglet joue du son — cliquer pour le couper";
    son.setAttribute("aria-label", son.title);
    son.appendChild(icone(o.muet ? "muet" : "son", "petit"));
    son.onclick = (e) => {
      e.stopPropagation();
      ipc({ m: "muet", id: o.id });
    };
    d.appendChild(son);
  }

  const fermer = elem("button", "fermer");
  fermer.title = "Fermer l'onglet";
  fermer.appendChild(icone("x", "petit"));
  fermer.onclick = (e) => {
    e.stopPropagation();
    ipc({ m: "fermer", id: o.id });
  };
  d.appendChild(fermer);

  // Maj+clic : a cote de la page affichee. Clic droit : le menu de cette page.
  d.onclick = (e) => ipc(e.shiftKey && !o.actif ? { m: "coteAvec", id: o.id } : { m: "activer", id: o.id });
  d.oncontextmenu = (e) => {
    e.preventDefault();
    // Hauteur du clic : le menu s'ouvre a cote de l'onglet, pas en haut a droite.
    ipc({ m: "menuPage", id: o.id, y: Math.round(e.clientY) });
  };
  d.title += "\nMaj+clic : à côté de la page affichée · clic droit : rendez-vous, guet…";
  d.addEventListener("dragstart", (e) => {
    e.dataTransfer.setData("text/plain", String(o.id));
    e.dataTransfer.effectAllowed = "move";
  });
  return d;
}

/// Signature de ce qui est dessine dans le rail. Les champs qui ne changent pas l'affichage
/// (positions de lecture, date de creation, vitalite exacte) n'y sont pas : ils declencheraient
/// des reconstructions inutiles.
function signatureDuRail(s) {
  const fils = (s.fils || []).map((f) => f.id + ":" + f.nom + ":" + (f.replie ? 1 : 0) + ":" + (f.cloison ? "C" : "")).join("|");
  const onglets = (s.onglets || [])
    .map(
      (o) =>
        o.id +
        ":" +
        o.fil +
        ":" +
        (o.titre || "") +
        ":" +
        (o.domaine || "") +
        (o.actif ? "A" : "") +
        (o.eveille ? "E" : "") +
        (o.ancre ? "N" : "") +
        (o.alire ? "L" : "") +
        (o.rdv ? "R" : "") +
        (o.revenu ? "V" : "") +
        (o.fermeture ? "F" + o.fermeture : "") +
        (o.son ? "S" : "") +
        (o.muet ? "M" : "") +
        ":" +
        o.palier +
        (o.favicon ? o.favicon : "")
    )
    .join("|");
  const c = (s.carnet && s.carnet.cote) || null;
  return fils + "#" + onglets + "#" + (c ? c.gauche + ":" + c.droite + ":" + c.affiche : "");
}

/* Bande d'acces rapides du rail, sous le logo : 10 places (carnet.accesRailMax).
   Un clic ouvre le site dans un nouvel onglet, a chaque fois ; « + » ajoute le site
   de la page affichee ; glisser une icone sur une autre la range devant elle ; deposer un
   onglet sur la bande ajoute son site ; clic droit : ouvrir, renommer, changer l'adresse,
   retirer du rail, supprimer. Tous les acces rapides se gerent aussi sur le Nouvel onglet. */
let signatureBande = null;
let glisseRail = null; // url de l'icone deplacee dans la bande
function rendreBandeAcces(s) {
  const bande = document.getElementById("bande-acces");
  if (!bande) return;
  const tous = (s.carnet && s.carnet.acces) || [];
  const liste = tous.filter((a) => a.rail);
  const max = (s.carnet && s.carnet.accesRailMax) || 10;
  const pageWeb = !!(s.carnet && s.carnet.pageWeb);
  const signature = JSON.stringify([tous.map((a) => [a.url, a.titre, a.icone, a.ouvert, a.rail]), max, pageWeb]);
  if (signature === signatureBande || glisseRail) return;
  signatureBande = signature;
  bande.textContent = "";
  bande.hidden = false;
  for (const a of liste.slice(0, max)) bande.appendChild(iconeRail(a, tous));
  if (liste.length < max) {
    const plus = elem("button", "acces ajout");
    plus.appendChild(icone("plus", "petit"));
    plus.title = pageWeb
      ? "Ajouter le site de cette page au rail\n(ou déposer un onglet ici)"
      : "Ouvre une page web, puis « + » pour ajouter son site\n(ou déposer un onglet ici)";
    plus.onclick = () => ipc({ m: "accesDepuisOnglet", actif: true, rail: true });
    plus.oncontextmenu = (e) => {
      e.preventDefault();
      ipc({ m: "nouvelOnglet" });
    };
    bande.appendChild(plus);
    // Pas de places vides dessinees : la bande ne prend que ce qu'elle montre (retour de
    // Clement, 2026-11-22 : deux rangees de pointilles « faisaient vide »).
  }
}

function iconeRail(a, tous) {
  const b = elem("button", "acces");
  b.title = `${a.titre}\n${a.url}` + "\nClic : nouvel onglet · clic droit : modifier · glisser : ranger";
  const lettre = elem("span", "lettre", (a.titre || a.domaine || "?").trim().charAt(0).toUpperCase());
  lettre.style.background = teinteAcces(a.domaine || a.url);
  if (a.icone) {
    const img = document.createElement("img");
    img.alt = "";
    img.draggable = false;
    img.referrerPolicy = "no-referrer";
    img.onerror = () => img.replaceWith(lettre);
    img.src = a.icone;
    b.appendChild(img);
  } else {
    b.appendChild(lettre);
  }
  if (a.ouvert) b.appendChild(elem("i", "ouvert"));
  b.onclick = () => ipc({ m: "accesOuvrir", url: a.url, nouveau: true });
  b.oncontextmenu = (e) => {
    e.preventDefault();
    if (window.innerWidth < 180) {
      // Rail replie : trop etroit pour le menu. La barre le dessine, colle au rail a la hauteur
      // de l'icone (l'hote relaie `menuAcces`). Avant, on ouvrait un Nouvel onglet a la place :
      // « le clic droit ne fonctionne pas » (Clement, 2026-11-23).
      ipc({
        m: "menuAcces",
        acces: { titre: a.titre || "", url: a.url, groupe: a.groupe || "" },
        y: Math.round(b.getBoundingClientRect().top),
      });
      return;
    }
    ouvrirMenuAccesRail(a, b);
  };
  b.draggable = true;
  b.addEventListener("dragstart", (e) => {
    glisseRail = a.url;
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/x-trame-acces", a.url);
    setTimeout(() => b.classList.add("glisse"), 0);
  });
  b.addEventListener("dragend", () => {
    glisseRail = null;
    b.classList.remove("glisse");
    signatureBande = null;
    if (etat) rendreBandeAcces(etat);
  });
  b.addEventListener("dragover", (e) => {
    if (!glisseRail || glisseRail === a.url) return;
    e.preventDefault();
    e.stopPropagation();
    b.classList.add("cible");
  });
  b.addEventListener("dragleave", () => b.classList.remove("cible"));
  b.addEventListener("drop", (e) => {
    b.classList.remove("cible");
    if (!glisseRail || glisseRail === a.url) return;
    e.preventDefault();
    e.stopPropagation();
    const ordre = tous.map((x) => x.url).filter((u) => u !== glisseRail);
    ordre.splice(ordre.indexOf(a.url), 0, glisseRail);
    glisseRail = null;
    ipc({ m: "accesOrdre", urls: ordre });
  });
  return b;
}

/* Menu d'un acces du rail (rail deplie) : par-dessus les fils, comme les commandes du logo. */
function fermerMenuAccesRail() {
  const m = document.getElementById("menu-acces-rail");
  if (m) m.remove();
  document.removeEventListener("pointerdown", clicHorsMenuAcces, true);
  document.removeEventListener("keydown", echapMenuAcces, true);
  window.removeEventListener("blur", fermerMenuAccesRail);
}
function clicHorsMenuAcces(e) {
  const m = document.getElementById("menu-acces-rail");
  if (m && !m.contains(e.target)) fermerMenuAccesRail();
}
function echapMenuAcces(e) {
  if (e.key === "Escape") fermerMenuAccesRail();
}
function ouvrirMenuAccesRail(a, ancre) {
  fermerMenuAccesRail();
  fermerCommandesRail();
  const m = menuAcces(a, fermerMenuAccesRail);
  m.id = "menu-acces-rail";
  const r = ancre.getBoundingClientRect();
  m.style.top = Math.round(r.bottom + 6) + "px";
  document.getElementById("rail").appendChild(m);
  document.addEventListener("pointerdown", clicHorsMenuAcces, true);
  document.addEventListener("keydown", echapMenuAcces, true);
  window.addEventListener("blur", fermerMenuAccesRail);
}

/** Le menu d'un acces, commun au rail deplie et a la barre (rail replie). */
function menuAcces(a, fermer) {
  const m = elem("div", "menu-acces-rail");
  const tete = elem("div", "mar-tete");
  tete.appendChild(elem("b", null, a.titre));
  tete.appendChild(elem("span", null, a.url));
  m.appendChild(tete);
  const ligne = (texte, faire, classe) => {
    const b = elem("button", classe || "", texte);
    b.onclick = (e) => {
      e.stopPropagation();
      faire();
    };
    m.appendChild(b);
  };
  ligne("Ouvrir dans un nouvel onglet", () => {
    fermer();
    ipc({ m: "accesOuvrir", url: a.url, nouveau: true });
  });
  ligne("Modifier le nom ou l'adresse…", () => formulaireAccesRail(m, a, fermer));
  ligne("Retirer du rail (reste sur le Nouvel onglet)", () => {
    fermer();
    ipc({ m: "accesRail", url: a.url, rail: false });
  });
  ligne("Supprimer l'accès rapide", () => {
    fermer();
    ipc({ m: "accesRetirer", url: a.url });
  }, "rouge");
  return m;
}

function formulaireAccesRail(m, a, fermer = fermerMenuAccesRail) {
  m.textContent = "";
  const f = document.createElement("form");
  f.className = "mar-form";
  const champ = (libelle, valeur) => {
    const l = elem("label", null, libelle);
    const i = document.createElement("input");
    i.type = "text";
    i.spellcheck = false;
    i.value = valeur || "";
    l.appendChild(i);
    f.appendChild(l);
    return i;
  };
  const iNom = champ("Nom", a.titre);
  const iUrl = champ("Adresse", a.url);
  const boutons = elem("div", "mar-boutons");
  const annuler = elem("button", null, "Annuler");
  annuler.type = "button";
  annuler.onclick = fermer;
  const ok = elem("button", "ok", "Enregistrer");
  ok.type = "submit";
  boutons.appendChild(annuler);
  boutons.appendChild(ok);
  f.appendChild(boutons);
  f.onsubmit = (e) => {
    e.preventDefault();
    const url = iUrl.value.trim();
    if (!url) return iUrl.focus();
    ipc({ m: "accesEnregistrer", url, titre: iNom.value.trim(), groupe: a.groupe || "", rail: true, ancienne: a.url });
    fermer();
  };
  m.appendChild(f);
  iNom.focus();
  iNom.select();
  // Dans la barre, le formulaire n'a pas la hauteur du menu : la colonne de l'hote suit.
  if (surface === "barre") ajusterHauteur();
}

const TEINTES_ACCES = ["#ef4444", "#f59e0b", "#18bcf2", "#0f766e", "#3b82f6", "#22c55e", "#8b5cf6", "#e11d48", "#c2410c", "#0ea5e9", "#a855f7", "#14b8a6"];
function teinteAcces(texte) {
  let h = 0;
  for (const c of texte || "") h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TEINTES_ACCES[h % TEINTES_ACCES.length];
}

/* ------------------------------------------------ ranger les groupes du rail

   Ranger un groupe, c'est deplacer `session.fils` : c'est la MEME liste qui ordonne les cartes du
   rail et les bandes de la trame, donc l'ordre tient d'un redemarrage a l'autre et les deux vues
   restent d'accord. Le geste est un glissement du pointeur sur la tete du groupe.

   Pourquoi le pointeur et pas le glisser-deposer HTML des onglets : sur les memes cartes, il
   faudrait distinguer un onglet d'un groupe au depot, et le glisser HTML n'existe pas au doigt.
   Rien n'est envoye pendant le mouvement : un seul message au relachement, l'hote reste seul
   juge de l'ordre. */
let rangeFil = null; // { id, x0, y0, actif } — le groupe saisi, tant qu'on ne l'a pas relache
let vientDeRanger = false; // avale le clic qui suit un glissement (rail replie : replier)

/** Ou tomberait le groupe lache a cette hauteur.
 *  `avant` = l'identifiant du groupe devant lequel le poser, 0 voulant dire « apres le dernier ». */
function cibleRangement(y) {
  const blocs = Array.from(document.querySelectorAll("#fils .fil"));
  if (!blocs.length) return null;
  for (const bloc of blocs) {
    const r = bloc.getBoundingClientRect();
    // La moitie haute d'une carte : devant elle. La moitie basse : on continue, donc derriere.
    if (y < r.top + r.height / 2) return { bloc, avant: Number(bloc.dataset.fil), apres: false };
  }
  const dernier = blocs[blocs.length - 1];
  return { bloc: dernier, avant: 0, apres: true };
}

function peindreRangement(cible) {
  for (const bloc of document.querySelectorAll("#fils .fil")) {
    bloc.classList.toggle("avant", !!cible && bloc === cible.bloc && !cible.apres);
    bloc.classList.toggle("apres", !!cible && bloc === cible.bloc && !!cible.apres);
  }
}

function saisirRangement(evenement, fil) {
  // Les commandes du groupe (replier, cloisonner, sceller, fermer, renommer) restent des clics.
  if (evenement.target.closest("button, input")) return;
  if (evenement.pointerType === "mouse" && evenement.button !== 0) return;
  // Capture du pointeur : sans elle, un relachement hors de la fenetre n'arrive jamais, le geste
  // resterait ouvert et le rail cesserait de se redessiner. `blur` est le second filet.
  try {
    evenement.currentTarget.setPointerCapture(evenement.pointerId);
  } catch (e) {
    /* capture impossible : le geste marche encore dans la fenetre */
  }
  vientDeRanger = false;
  rangeFil = { id: fil.id, x0: evenement.clientX, y0: evenement.clientY, actif: false };
}

function suivreRangement(evenement) {
  if (!rangeFil) return;
  if (!rangeFil.actif) {
    // Un seuil, sinon un simple appui sur la tete se lirait comme un rangement.
    if (Math.hypot(evenement.clientX - rangeFil.x0, evenement.clientY - rangeFil.y0) < 6) return;
    rangeFil.actif = true;
    const conteneur = document.getElementById("fils");
    if (conteneur) conteneur.classList.add("rangement");
    const pris = document.querySelector(`#fils .fil[data-fil="${rangeFil.id}"]`);
    if (pris) pris.classList.add("en-glissement");
  }
  peindreRangement(cibleRangement(evenement.clientY));
}

function relacherRangement(evenement) {
  const range = rangeFil;
  if (!range) return;
  rangeFil = null;
  const conteneur = document.getElementById("fils");
  if (conteneur) conteneur.classList.remove("rangement");
  for (const bloc of document.querySelectorAll("#fils .fil")) {
    bloc.classList.remove("en-glissement", "avant", "apres");
  }
  if (!range.actif) return; // simple appui : c'est un clic, laisse le faire son oeuvre
  vientDeRanger = true;
  const cible = cibleRangement(evenement.clientY);
  if (!cible) {
    if (etat) rendreRail(etat);
    return;
  }
  // Meme un depot sur sa propre place part : l'hote est seul juge, et il ne salira rien.
  ipc({ m: "filDeplacer", id: range.id, avant: cible.avant });
}

/** Geste interrompu (pointercancel) : on remet la carte en place sans rien demander a l'hote. */
function annulerRangement() {
  if (!rangeFil) return;
  rangeFil = null;
  const conteneur = document.getElementById("fils");
  if (conteneur) conteneur.classList.remove("rangement");
  for (const bloc of document.querySelectorAll("#fils .fil")) {
    bloc.classList.remove("en-glissement", "avant", "apres");
  }
  if (etat) rendreRail(etat);
}

function rendreRail(s) {
  rendreSuggestion(s);
  rendreBandeAcces(s);
  // Le rail replie fait 64 px : c'est la largeur, pas le reglage, qui dit ce que l'on voit
  // (une fenetre etroite replie aussi le rail).
  const btnRail = document.getElementById("btn-rail");
  if (btnRail) btnRail.title = window.innerWidth < 130 ? "Déplier le menu (Ctrl+B)" : "Replier le menu (Ctrl+B)";

  const conteneur = document.getElementById("fils");
  const signature = signatureDuRail(s);
  // Pendant un rangement, on ne reconstruit rien : la carte saisie doit survivre au geste. Les
  // poussees de l'hote (une toutes les 400 ms) sont rattrapees au relachement, parce que
  // `signatureRail` n'a pas ete mis a jour.
  // Pendant un renommage aussi : reconstruire effacerait le champ en cours de frappe (un titre
  // d'onglet qui change suffit). Le blur ou Entree rendent la main.
  if (rangeFil || document.querySelector("#fils input.saisie")) {
    rendreBudget();
    if (!document.getElementById("volet").hidden) rendreVolet(s);
    return;
  }
  if (signature === signatureRail && conteneur.childElementCount > 0) {
    // Rien de visible n'a change : on rafraichit seulement ce qui vit a cote du rail.
    rendreBudget();
    if (!document.getElementById("volet").hidden) rendreVolet(s);
    return;
  }
  signatureRail = signature;

  const defilement = conteneur.scrollTop;
  conteneur.textContent = "";

  const enAttente = s.onglets.filter((o) => o.rdv);
  for (const f of s.fils) {
    const onglets = s.onglets.filter((o) => o.fil === f.id && !o.rdv);
    if (!onglets.length) continue;

    const bloc = elem("div", "fil" + (f.replie ? " replie" : "") + (f.cloison ? " cloisonne" : ""));
    bloc.dataset.fil = f.id;
    bloc.style.setProperty("--c", couleurFil(f));

    const tete = elem("div", "fil-tete");
    // Rail replie : le chevron est cache, la tete (pastille + nom) replie et deplie le fil.
    tete.onclick = () => {
      // Apres un glissement, un clic sur la tete ne doit pas replier le groupe par surprise.
      if (vientDeRanger) return;
      if (window.innerWidth < 130) ipc({ m: "filReplier", id: f.id, replie: !f.replie });
    };
    // Le nom d'abord : rail replie en style « trait », la bulle est le seul endroit ou il se lit.
    tete.title = `${f.nom}\nGlisser pour ranger ce groupe`;
    tete.addEventListener("pointerdown", (e) => saisirRangement(e, f));
    const chev = elem("button", "chev");
    chev.title = f.replie ? "Déplier" : "Replier";
    chev.appendChild(icone("chevron", "petit"));
    chev.onclick = (e) => {
      e.stopPropagation();
      ipc({ m: "filReplier", id: f.id, replie: !f.replie });
    };
    tete.appendChild(chev);

    const nom = elem("span", "fil-nom", f.nom);
    nom.title = "Double-clic pour renommer";
    tete.appendChild(nom);
    // Le double-clic s'ecoute sur la tete, pas sur le nom : `saisirRangement` capture le pointeur
    // sur la tete, et Chromium y renvoie alors click et dblclick — le nom ne les recevait jamais.
    tete.addEventListener("dblclick", (e) => {
      if (e.target.closest("button, input")) return;
      if (!nom.isConnected || window.innerWidth < 130) return; // rail replie : nom cache
      e.stopPropagation();
      renommerFil(f, nom);
    });

    tete.appendChild(elem("span", "fil-compte", `${onglets.length}`));

    const outils = elem("span", "fil-outils");
    const cloisonner = elem("button", f.cloison ? "on" : "");
    cloisonner.title = f.cloison
      ? "Décloisonner : rejoindre le profil principal (connexions et extensions partagées)"
      : "Cloisonner : connexions, cookies et extensions à part pour ce fil";
    cloisonner.appendChild(icone("bouclier", "petit"));
    cloisonner.onclick = (e) => {
      e.stopPropagation();
      ipc({ m: "filCloisonner", fil: f.id, actif: !f.cloison });
    };
    outils.appendChild(cloisonner);
    const sceller = elem("button");
    sceller.title = "Sceller ce fil en capsule";
    sceller.appendChild(icone("capsule", "petit"));
    sceller.onclick = (e) => {
      e.stopPropagation();
      ipc({ m: "sceller", fil: f.id });
    };
    outils.appendChild(sceller);
    const fermerFil = elem("button");
    fermerFil.title = "Fermer tous les onglets de ce fil";
    fermerFil.appendChild(icone("x", "petit"));
    fermerFil.onclick = (e) => {
      e.stopPropagation();
      ipc({ m: "filFermer", id: f.id });
    };
    outils.appendChild(fermerFil);
    tete.appendChild(outils);

    bloc.appendChild(tete);

    if (!f.replie) {
      const liste = elem("div", "fil-onglets");
      for (const o of onglets) liste.appendChild(ligneOnglet(o));
      bloc.appendChild(liste);
    } else {
      bloc.appendChild(pileFil(f, onglets));
    }

    // depot : tisser un onglet dans ce fil
    bloc.addEventListener("dragover", (e) => {
      e.preventDefault();
      bloc.classList.add("survol-tissage");
    });
    bloc.addEventListener("dragleave", () => bloc.classList.remove("survol-tissage"));
    bloc.addEventListener("drop", (e) => {
      e.preventDefault();
      bloc.classList.remove("survol-tissage");
      const id = Number(e.dataTransfer.getData("text/plain"));
      if (id) ipc({ m: "tisser", onglet: id, fil: f.id });
    });

    conteneur.appendChild(bloc);
  }

  if (!s.fils.length) {
    conteneur.appendChild(elem("div", "vide", "Aucun fil. Ouvre un onglet pour commencer."));
  }
  // Onglets en rendez-vous : hors du rail, mais jamais perdus de vue.
  if (enAttente.length) {
    const attente = elem("button", "attente");
    attente.appendChild(icone("horloge", "petit"));
    attente.appendChild(
      elem("span", "", enAttente.length === 1 ? "1 onglet en rendez-vous" : `${enAttente.length} onglets en rendez-vous`)
    );
    attente.onclick = () => basculerVolet("rdv");
    conteneur.appendChild(attente);
  }

  conteneur.scrollTop = defilement;
  rendreBudget();
  if (!document.getElementById("volet").hidden) rendreVolet(s);
}

function renommerFil(fil, span) {
  const champ = elem("input", "fil-nom saisie");
  champ.value = fil.nom;
  let annule = false;
  let fini = false;
  span.replaceWith(champ);
  champ.focus();
  champ.select();
  const terminer = (valider) => {
    // Retirer le champ declenche `blur`, qui rappelle terminer : une seule fois suffit.
    if (fini || !champ.isConnected) return;
    fini = true;
    const nouveau = champ.value.trim();
    const garder = valider && !annule && nouveau;
    // Le champ part d'abord : tant qu'il est la, rendreRail ne reconstruit pas le rail.
    if (garder) span.textContent = nouveau;
    champ.replaceWith(span);
    if (garder) ipc({ m: "filRenommer", id: fil.id, nom: nouveau });
    else if (etat) rendreRail(etat);
  };
  champ.onkeydown = (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      terminer(true);
    } else if (e.key === "Escape") {
      annule = true;
      terminer(false);
    }
  };
  champ.onblur = () => terminer(true);
}

let voletMode = "capsules";
function basculerVolet(mode) {
  const volet = document.getElementById("volet");
  volet.hidden = !volet.hidden && voletMode === mode ? true : false;
  voletMode = mode;
  if (!volet.hidden && etat) rendreVolet(etat);
}

function heure(ms) {
  const d = new Date(ms);
  const auj = new Date();
  const h = d.toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
  if (d.toDateString() === auj.toDateString()) return h;
  const demain = new Date(auj.getFullYear(), auj.getMonth(), auj.getDate() + 1);
  if (d.toDateString() === demain.toDateString()) return "demain " + h;
  return d.toLocaleDateString("fr-FR", { weekday: "short", day: "numeric", month: "short" }) + " " + h;
}

function rendreVolet(s) {
  const corps = document.getElementById("volet-corps");
  corps.textContent = "";
  document.getElementById("volet-titre").textContent = voletMode === "rdv" ? "Rendez-vous" : "Capsules";
  if (voletMode === "rdv") {
    const liste = s.onglets.filter((o) => o.rdv).sort((a, b) => a.rdv - b.rdv);
    if (!liste.length) {
      corps.appendChild(elem("div", "vide", "Aucun onglet en rendez-vous. Clic droit sur un onglet → Rendez-vous."));
      return;
    }
    for (const o of liste) {
      const item = elem("div", "capsule-item");
      item.appendChild(elem("div", "nom", o.titre || o.urlCourte));
      item.appendChild(elem("div", "chemin", "revient " + heure(o.rdv) + " · " + (o.domaine || "")));
      item.title = "Ouvrir maintenant";
      item.onclick = () => {
        ipc({ m: "rendezVousMaintenant", id: o.id });
        document.getElementById("volet").hidden = true;
      };
      corps.appendChild(item);
    }
    return;
  }
  if (!s.capsules.length) {
    corps.appendChild(
      elem(
        "div",
        "vide",
        "Aucune capsule. Une capsule scelle un fil entier (URL, titre, position de lecture) dans un fichier JSON que tu peux déplacer ou partager."
      )
    );
    return;
  }
  for (const c of s.capsules) {
    const item = elem("div", "capsule-item");
    item.appendChild(elem("div", "nom", c.nom));
    item.appendChild(elem("div", "chemin", c.chemin));
    item.onclick = () => {
      ipc({ m: "capsuleOuvrir", chemin: c.chemin });
      document.getElementById("volet").hidden = true;
    };
    corps.appendChild(item);
  }
}

/* =================================================================== BARRE */

let rid = 0;
let items = [];
let selection = -1;
let extensionsVisibles = false;
let pageVisible = false;
/** `{ y, h }` quand le menu de la page vient d'un clic droit sur un onglet du rail. */
let ancreMenu = null;
let commandesVisibles = false;
let signaturePage = "";
let lecturesAttendues = false;
let panneauOuvert = false;
// Signatures de rendu : une poussee d'etat qui ne change rien ne doit rien reconstruire.
// Sans cela, chaque defilement de page (une poussee toutes les 400 ms) reconstruisait tout le
// rail — dizaines de noeuds, favicons recharges — et le CPU du moteur suivait.
let signatureRail = "";
let signaturePuces = "";

function champ() {
  return document.getElementById("couture");
}

function ouvrirPanneau() {
  if (!panneauOuvert) {
    panneauOuvert = true;
    ipc({ m: "panneau", open: true });
    // Ce message sans zone EFFACE la zone cote hote, qui replafonne alors la barre a 72 % de la
    // fenetre. La mesure qui suit doit donc toujours partir, meme identique a celle d'avant la
    // fermeture : sans cela, rouvrir le meme menu le coupait net a mi-hauteur (2026-11-23, 0.4.57).
    derniereHauteur = 0;
    derniereCle = "";
    document.getElementById("panneau").hidden = false;
    // Pas de requete ici : c'est la saisie (focus, frappe) qui la declenche. Sans cela,
    // cliquer sur ◇ affichait la liste des extensions puis la remplacait aussitot par des
    // resultats de recherche, sans que l'utilisateur ait rien tape.
  }
  // TOUJOURS remesurer, meme si le panneau est deja ouvert : un second clic droit change le
  // contenu (un autre onglet, donc d'autres lignes), et garder la hauteur de la fois precedente
  // laissait la barre trop courte — le menu se dessinait hors de la webview (2026-11-23).
  ajusterHauteur();
}

function fermerPanneau() {
  if (!panneauOuvert) return;
  panneauOuvert = false;
  ipc({ m: "panneau", open: false });
  document.getElementById("panneau").hidden = true;
  document.getElementById("extensions").hidden = true;
  document.getElementById("menu-page").hidden = true;
  document.getElementById("menu-commandes").hidden = true;
  extensionsVisibles = false;
  pageVisible = false;
  commandesVisibles = false;
  champ().blur();
}

function demander() {
  const q = champ().value.trim();
  rid += 1;
  ipc({ m: "requete", rid, q });
}

/** Demande a l'hote une hauteur de panneau qui epouse le contenu affiche.
 *  Une hauteur fixe laissait un grand bloc vide sous une liste courte. */
let derniereHauteur = 0;
let derniereCle = "";
let minuteurHauteur = null;
function ajusterHauteur() {
  clearTimeout(minuteurHauteur);
  minuteurHauteur = setTimeout(() => {
    if (!panneauOuvert) return;
    const ligne = document.querySelector(".barre-ligne");
    const extbar = document.getElementById("extbar");
    const hLigne = ligne ? ligne.getBoundingClientRect().height : 45;
    const hExt = extbar && !extbar.hidden ? extbar.getBoundingClientRect().height : 0;
    // Hauteur NATURELLE du contenu : la zone est etiree (`flex: 1`), donc son `scrollHeight`
    // vaut au moins la hauteur deja accordee — le panneau ne retrecissait jamais, et une liste
    // courte laissait une bande vide sous elle.
    const naturelle = (zone) => {
      const dernier = zone.lastElementChild;
      if (!dernier) return zone.scrollHeight;
      const style = getComputedStyle(zone);
      const bas = dernier.getBoundingClientRect().bottom - zone.getBoundingClientRect().top + zone.scrollTop;
      return Math.ceil(bas + parseFloat(style.paddingBottom || "0"));
    };
    let hContenu = 0;
    for (const zone of [
      document.getElementById("resultats"),
      document.getElementById("extensions"),
      document.getElementById("menu-page"),
      document.getElementById("menu-commandes"),
    ]) {
      if (zone && !zone.hidden) hContenu = Math.max(hContenu, naturelle(zone));
    }
    const total = Math.ceil(hLigne + hExt + hContenu + 8);
    // Menu des extensions : sa colonne, pour que la barre ne recouvre de la page que ce qu'il
    // occupe. Resultats de la Couture : toute la largeur.
    const colonne = extensionsVisibles
      ? document.querySelector("#extensions .menu-colonne")
      : pageVisible
        ? document.querySelector("#menu-page .menu-colonne")
        : commandesVisibles
          ? document.querySelector("#menu-commandes .menu-colonne, #menu-commandes .menu-acces-rail")
          : null;
    const r = colonne ? colonne.getBoundingClientRect() : null;
    const zone = r ? { x: Math.floor(r.left), l: Math.ceil(r.width) } : null;
    // Menu pose a la hauteur d'un onglet (ou d'un acces du rail replie) : la barre ne couvre que
    // lui, pas la bande au-dessus.
    if (zone && ((pageVisible && ancreMenu) || (commandesVisibles && ancreAcces))) zone.y = Math.floor(r.top);
    const cle = `${total}|${zone ? zone.x + ":" + zone.l + ":" + (zone.y || 0) : "-"}`;
    if (Math.abs(total - derniereHauteur) < 4 && cle === derniereCle) return;
    derniereHauteur = total;
    derniereCle = cle;
    ipc({ m: "panneau", open: true, hauteur: total, zone });
  }, 30);
}

function recevoirResultats(r) {
  if (r.rid !== rid) return; // reponse perimee
  items = r.items || [];
  selection = items.length ? 0 : -1;
  lecturesAttendues = /^\s*\?/.test(r.q || "");
  extensionsVisibles = false;
  pageVisible = false;
  commandesVisibles = false;
  document.getElementById("extensions").hidden = true;
  document.getElementById("menu-page").hidden = true;
  document.getElementById("menu-commandes").hidden = true;
  document.getElementById("resultats").hidden = false;
  rendreResultats();
}

/** Met a jour la surbrillance SANS reconstruire la liste.
 *  Reconstruire au survol detruisait l'element sous la souris : le clic (qui exige
 *  enfoncement et relachement sur le MEME noeud) ne se declenchait jamais. Constate a l'usage. */
function surligner() {
  const lignes = document.querySelectorAll("#resultats .resultat");
  lignes.forEach((el, index) => el.classList.toggle("sur", index === selection));
}

function rendreResultats() {
  const zone = document.getElementById("resultats");
  zone.textContent = "";
  if (!items.length) {
    zone.appendChild(
      elem(
        "div",
        "vide",
        lecturesAttendues
          ? "Recherche dans ce que vous avez lu…"
          : !champ().value.trim()
            ? "Tapez une adresse, un onglet ou un fil. Les commandes : « > », ou clic droit sur le logo."
            : "Rien trouvé. Entrée ouvre la saisie comme une adresse."
      )
    );
    ajusterHauteur();
    return;
  }
  items.forEach((item, index) => {
    const ligne = elem("div", "resultat" + (index === selection ? " sur" : ""));
    const libelles = { "onglet-endormi": "veille", lecture: "lu" };
    const genre = elem("span", "genre " + item.genre, libelles[item.genre] || item.genre);
    ligne.appendChild(genre);
    const corps = elem("div", "corps");
    corps.appendChild(elem("div", "t", item.titre));
    corps.appendChild(elem("div", "s", item.sousTitre));
    ligne.appendChild(corps);
    ligne.onmouseenter = () => {
      selection = index;
      surligner();
    };
    // On lance des l'enfoncement : meme si un rafraichissement d'etat remplace le noeud
    // entre l'enfoncement et le relachement, l'action est deja partie. Le garde-fou
    // empeche une double execution si le clic arrive aussi.
    let dernier = 0;
    const lancer = (e) => {
      const maintenant = Date.now();
      if (maintenant - dernier < 400) return;
      dernier = maintenant;
      ouvrir(item, mode(e));
    };
    ligne.onpointerdown = (evenement) => {
      if (evenement.button === 0) lancer(evenement);
    };
    ligne.onclick = lancer;
    zone.appendChild(ligne);
  });
  ajusterHauteur();
}

/** L'icone d'une extension Chrome : son image si elle en a une, sinon son initiale. */
function iconeExtension(e, classe) {
  const boite = elem("span", classe);
  const initiale = () => {
    boite.textContent = (e.nom || "?").trim().charAt(0).toUpperCase();
    boite.classList.add("initiale");
  };
  if (e.icone) {
    const img = document.createElement("img");
    img.alt = "";
    img.draggable = false;
    img.src = e.icone;
    img.onerror = () => {
      img.remove();
      initiale();
    };
    boite.appendChild(img);
  } else {
    initiale();
  }
  return boite;
}

/** Ce que l'extension fait maintenant, en clair. */
function etatExtension(e) {
  if (!e.valide) return { txt: "Manifeste illisible", genre: "mal" };
  if (!e.actif) return { txt: "Désactivée" + cout(e, " · coûtait "), genre: "off" };
  if (e.chargee === false) return { txt: "Pas encore chargée — redémarrez Trame", genre: "mal" };
  return { txt: "Active" + cout(e, " · "), genre: "on" };
}

/** Ce que coute une extension, quand c'est mesure : direct, ou par difference a l'allumage. */
function cout(e, prefixe) {
  if (typeof e.coutMo !== "number") return "";
  return `${prefixe}${Math.round(e.coutMo)} Mo${e.coutDirect ? "" : " (mesuré à l'allumage)"}`;
}

/** Bouton d'action du menu (sans reconstruction perdue : l'action part au contact). */
function actionMenu(classe, titre, message, contenu) {
  const b = elem("button", classe);
  b.title = titre;
  if (contenu) b.appendChild(contenu);
  b.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    ipc(message);
    // Ouvrir quelque chose referme le menu ; epingler ou basculer le laisse ouvert, comme
    // dans Chrome, pour enchainer plusieurs reglages.
    if (
      [
        "nouvelOnglet", "chromePopup", "ouvrirReglages", "imprimer", "chercherPage", "guetPoser",
        "guetOuvrir", "rendezVous", "coteAvec", "coteFermer", "coteInverser", "lectureExclure",
        "endormir", "reveillerFond", "recharger", "tisser",
      ].includes(message.m)
    )
      fermerPanneau();
  });
  return b;
}

let signatureMenu = "";

/** La cloison du fil de l'onglet affiche, s'il est cloisonne. */
function cloisonActive(s) {
  const fil = (s.fils || []).find((f) => f.id === s.filActif);
  return fil && fil.cloison ? { fil: fil.id, nom: fil.nom, extensions: fil.cloison.extensions || [] } : null;
}

/** Une ligne du menu dans un fil cloisonne : l'interrupteur dit « dans ce fil », pas « partout ». */
function ligneCloison(e, cloison, epinglees, s) {
  const dedans = cloison.extensions.includes(e.id);
  const epinglee = epinglees.includes(e.id);
  const ouverte = s.panneauExtension === e.id;
  const ligne = elem("div", "menu-ligne" + (dedans ? "" : " off") + (ouverte ? " ouverte" : ""));
  const principal = actionMenu(
    "menu-principal",
    dedans && e.popup ? `Ouvrir ${e.nom}` : dedans ? `${e.nom} n'a pas de fenêtre d'action` : `Activer ${e.nom} dans ce fil`,
    dedans && e.popup
      ? { m: "chromePopup", id: e.id }
      : { m: "cloisonExtension", fil: cloison.fil, id: e.id, actif: !dedans }
  );
  principal.appendChild(iconeExtension(e, "menu-icone"));
  const corps = elem("span", "corps");
  corps.appendChild(elem("span", "n", e.nom));
  corps.appendChild(
    elem("span", "d " + (dedans ? "etat-on" : ""), dedans ? "Active dans ce fil" : e.actif ? "Active ailleurs, pas ici" : "Pas dans ce fil")
  );
  principal.appendChild(corps);
  ligne.appendChild(principal);
  if (dedans) {
    ligne.appendChild(
      actionMenu(
        "menu-epingle" + (epinglee ? " on" : ""),
        epinglee ? "Retirer de la barre d'outils" : "Épingler dans la barre d'outils",
        { m: "chromeEpingler", id: e.id },
        icone("pin", "petit")
      )
    );
  }
  ligne.appendChild(
    actionMenu(
      "bascule" + (dedans ? " on" : ""),
      dedans ? "Retirer de ce fil (tout de suite)" : "Activer dans ce fil (tout de suite)",
      { m: "cloisonExtension", fil: cloison.fil, id: e.id, actif: !dedans }
    )
  );
  return ligne;
}

/** Le menu des extensions (bouton puzzle) : comme celui de Chrome, une ligne par extension,
 *  un clic ouvre sa fenetre d'action, l'epingle la met dans la barre, l'interrupteur l'allume
 *  ou l'eteint — tout prend effet tout de suite. */
function rendreExtensions(s) {
  const zone = document.getElementById("extensions");
  document.getElementById("resultats").hidden = true;
  document.getElementById("menu-commandes").hidden = true;
  commandesVisibles = false;
  zone.hidden = false;
  extensionsVisibles = true;

  const chrome = s.extensionsChrome || [];
  const epinglees = s.extensionsEpinglees || [];
  const installations = s.installations || [];
  // Une poussee d'etat qui ne change rien ne reconstruit rien : sinon un clic dont le
  // relachement tombe pendant la reconstruction est perdu.
  const cloison = cloisonActive(s);
  const signature = JSON.stringify([
    cloison,
    Math.round((s.budget && s.budget.extensionsMo) || 0),
    chrome.map((e) => [e.id, e.nom, e.actif, e.chargee, e.icone, e.popup, e.valide, Math.round(e.coutMo || 0)]),
    epinglees,
    installations,
    s.panneauExtension,
    (s.extensions || []).map((e) => [e.id, e.nom, e.active]),
  ]);
  if (signature === signatureMenu && zone.childElementCount) {
    ajusterHauteur();
    return;
  }
  signatureMenu = signature;
  zone.textContent = "";
  // Une seule colonne, bordee : c'est elle que la barre decoupe pour se poser sur la page.
  const colonne = elem("div", "menu-colonne");
  zone.appendChild(colonne);

  const b = s.budget || {};
  const tete = elem("div", "menu-tete");
  if (cloison) {
    const titre = elem("span", "menu-titre");
    titre.appendChild(icone("bouclier", "petit"));
    titre.appendChild(elem("span", "", `Extensions de « ${cloison.nom} »`));
    titre.classList.add("cloison");
    tete.appendChild(titre);
  } else {
    tete.appendChild(elem("span", "menu-titre", "Extensions"));
  }
  tete.appendChild(
    actionMenu("menu-lien", "Ouvrir le Chrome Web Store dans un nouvel onglet", {
      m: "nouvelOnglet",
      url: "https://chromewebstore.google.com/category/extensions?hl=fr",
    }, (() => {
      const f = document.createDocumentFragment();
      f.appendChild(elem("span", "", "Chrome Web Store"));
      f.appendChild(icone("external", "petit"));
      return f;
    })())
  );
  colonne.appendChild(tete);
  if (!cloison && b.extensionsProcessus) {
    colonne.appendChild(
      elem(
        "div",
        "menu-cout",
        `Les extensions Chrome occupent ${Math.round(b.extensionsMo)} Mo (${b.extensionsProcessus} processus).`
      )
    );
  }

  for (const libelle of installations) {
    const ligne = elem("div", "menu-ligne installation");
    ligne.appendChild(elem("span", "menu-icone tourne"));
    const corps = elem("div", "corps");
    corps.appendChild(elem("div", "n", "Installation en cours…"));
    corps.appendChild(elem("div", "d", libelle));
    ligne.appendChild(corps);
    colonne.appendChild(ligne);
  }

  if (!chrome.length && !installations.length) {
    colonne.appendChild(
      elem(
        "div",
        "vide menu-vide",
        "Aucune extension Chrome. Ouvrez une fiche du Chrome Web Store et cliquez « Ajouter à Chrome » : Trame l'installe et l'active aussitôt."
      )
    );
  }

  if (cloison) {
    colonne.appendChild(
      elem(
        "div",
        "vide menu-vide",
        "Fil cloisonné : ses connexions et ses extensions sont à part. Allumez ici celles qu'il doit avoir — elles ne tournent que dans ce fil."
      )
    );
    for (const e of chrome.filter((x) => x.valide)) colonne.appendChild(ligneCloison(e, cloison, epinglees, s));
  }

  for (const e of cloison ? [] : chrome) {
    const etat = etatExtension(e);
    const epinglee = epinglees.includes(e.id);
    const ouverte = s.panneauExtension === e.id;
    const ligne = elem("div", "menu-ligne" + (e.actif ? "" : " off") + (ouverte ? " ouverte" : ""));

    // Icone + nom : ouvre la fenetre d'action, comme un clic sur l'extension dans Chrome.
    const principal = actionMenu(
      "menu-principal",
      e.popup && e.actif ? `Ouvrir ${e.nom}` : e.actif ? `${e.nom} n'a pas de fenêtre d'action` : `${e.nom} est désactivée`,
      e.popup && e.actif ? { m: "chromePopup", id: e.id } : { m: "ouvrirReglages" }
    );
    principal.appendChild(iconeExtension(e, "menu-icone"));
    const corps = elem("span", "corps");
    corps.appendChild(elem("span", "n", e.nom));
    corps.appendChild(elem("span", "d etat-" + etat.genre, etat.txt));
    principal.appendChild(corps);
    ligne.appendChild(principal);

    if (e.actif) {
      ligne.appendChild(
        actionMenu(
          "menu-epingle" + (epinglee ? " on" : ""),
          epinglee ? "Retirer de la barre d'outils" : "Épingler dans la barre d'outils",
          { m: "chromeEpingler", id: e.id },
          icone("pin", "petit")
        )
      );
    }
    const bascule = actionMenu(
      "bascule" + (e.actif ? " on" : ""),
      e.actif ? "Désactiver (prend effet tout de suite)" : "Activer (prend effet tout de suite)",
      { m: e.actif ? "chromeDesactiver" : "chromeReactiver", id: e.id }
    );
    ligne.appendChild(bascule);
    colonne.appendChild(ligne);
  }

  // Les extensions maison, plus bas : meme interrupteur, meme effet immediat.
  const maison = s.extensions || [];
  if (maison.length) {
    colonne.appendChild(elem("div", "menu-section", "Extensions Trame"));
    for (const e of maison) {
      const ligne = elem("div", "menu-ligne" + (e.active ? "" : " off"));
      const principal = elem("span", "menu-principal statique");
      const pastille = elem("span", "menu-icone initiale maison", (e.nom || "?").trim().charAt(0).toUpperCase());
      if (e.accent) pastille.style.background = e.accent;
      principal.appendChild(pastille);
      const corps = elem("span", "corps");
      corps.appendChild(elem("span", "n", e.nom));
      corps.appendChild(elem("span", "d", e.description || (e.motifs || []).join("  ")));
      principal.appendChild(corps);
      principal.title = `${e.nom}${e.version ? " v" + e.version : ""}\n${(e.motifs || []).join("  ")}`;
      ligne.appendChild(principal);
      ligne.appendChild(
        actionMenu(
          "bascule" + (e.active ? " on" : ""),
          e.active ? "Désactiver" : "Activer",
          { m: "extensionBasculer", id: e.id, actif: !e.active }
        )
      );
      colonne.appendChild(ligne);
    }
  }

  const pied = elem("div", "menu-pied");
  if (cloison) {
    const sortir = actionMenu(
      "menu-bouton",
      "Rejoindre le profil principal : connexions et extensions partagées avec les autres fils",
      { m: "filCloisonner", fil: cloison.fil, actif: false }
    );
    sortir.appendChild(icone("bouclier", "petit"));
    sortir.appendChild(elem("span", "", "Décloisonner ce fil"));
    pied.appendChild(sortir);
  }
  const gerer = actionMenu("menu-bouton", "Installer, mettre à jour, retirer", { m: "ouvrirReglages" });
  gerer.appendChild(icone("sliders", "petit"));
  gerer.appendChild(elem("span", "", "Gérer les extensions"));
  pied.appendChild(gerer);
  colonne.appendChild(pied);
  ajusterHauteur();
}

let signatureEpingles = "";

function rendreEpingles(s) {
  const zone = document.getElementById("pin-chips");
  if (!zone) return 0;
  const ids = s.extensionsEpinglees || [];
  const toutes = s.extensionsChrome || [];
  // Dans un fil cloisonne, ce sont les extensions de la cloison qui comptent, pas celles du
  // profil principal : une extension coupee partout ailleurs peut tourner ici.
  const cloison = cloisonActive(s);
  const liste = ids
    .map((id) => toutes.find((e) => e.id === id))
    .filter((e) => e && (cloison ? e.valide && cloison.extensions.includes(e.id) : e.actif));
  // Chaque poussee d'etat reconstruisait les puces : un clic dont le relachement tombait
  // pendant la reconstruction etait perdu. On ne reconstruit que si l'affichage change.
  const signature = liste
    .map((e) => `${e.id}|${e.nom}|${e.version || ""}|${e.icone || ""}|${s.panneauExtension === e.id}`)
    .join("§") + (cloison ? "#C" + cloison.fil : "");
  zone.hidden = !liste.length;
  if (signature === signatureEpingles && zone.childElementCount === liste.length) {
    return liste.length;
  }
  signatureEpingles = signature;
  zone.textContent = "";
  for (const e of liste) {
    const ouvert = s.panneauExtension === e.id;
    const chip = elem("button", "pin-chip" + (ouvert ? " on" : ""));
    chip.appendChild(iconeExtension(e, "pastille"));
    chip.title =
      `${e.nom}${e.version ? " v" + e.version : ""}\n` +
      (e.popup
        ? ouvert
          ? "Cliquer pour refermer"
          : "Cliquer pour ouvrir"
        : "Cette extension n'a pas de fenêtre d'action");
    // `pointerdown` et non `click` : l'action part au contact, sans dependre du relachement.
    chip.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      ipc({ m: "chromePopup", id: e.id });
    });
    zone.appendChild(chip);
  }
  return liste.length;
}

function rendreBarreExtensions(s) {
  const barre = document.getElementById("extbar");
  if (!barre) return;

  rendreEpingles(s);
  if (!s.barreExtensions) {
    barre.hidden = true;
    return;
  }
  barre.hidden = false;

  const zone = document.getElementById("ext-chips");
  // Une puce n'est reconstruite que si son nom, son etat ou sa couleur changent.
  const signature = (s.extensions || [])
    .map((e) => e.id + ":" + e.nom + ":" + (e.active ? 1 : 0) + ":" + (e.accent || ""))
    .join("|");
  if (signature === signaturePuces && zone.childElementCount > 0) {
    const nomMoteurInchange = document.getElementById("moteur-nom");
    if (nomMoteurInchange) nomMoteurInchange.textContent = (s.moteur && s.moteur.nom) || "—";
    return;
  }
  signaturePuces = signature;
  zone.textContent = "";
  const liste = s.extensions || [];
  if (!liste.length) {
    zone.appendChild(elem("span", "ext-chip off", "aucune extension"));
  }
  for (const e of liste) {
    const puce = elem("span", "pt");
    if (e.accent) puce.style.background = e.active ? e.accent : "";
    const chip = elem("button", "ext-chip" + (e.active ? "" : " off"));
    chip.appendChild(puce);
    chip.appendChild(elem("span", "nom", e.nom));
    chip.title = `${e.nom}${e.version ? " v" + e.version : ""}\n${e.description || ""}\n${(e.motifs || []).join("  ")}\n${e.active ? "Cliquer pour désactiver" : "Cliquer pour activer"}`;
    chip.addEventListener("pointerdown", (ev) => {
      ev.preventDefault();
      ev.stopPropagation();
      ipc({ m: "extensionBasculer", id: e.id, actif: !e.active });
    });
    zone.appendChild(chip);
  }

  const nomMoteur = document.getElementById("moteur-nom");
  if (nomMoteur) nomMoteur.textContent = (s.moteur && s.moteur.nom) || "—";
  const chipMoteur = document.getElementById("btn-moteur");
  if (chipMoteur) chipMoteur.title = `Moteur de recherche : ${(s.moteur && s.moteur.nom) || "?"}\nCliquer pour ouvrir les réglages`;
}

/// Bouton « Téléchargements » de la barre : anneau de progression (ou rotation si la taille
/// est inconnue), nombre en cours, et un eclat quand un fichier vient d'arriver.
let dernierTermineVu = null;
function rendreTelechargements(t) {
  const b = document.getElementById("btn-dl");
  if (!b || !t) return;
  const actifs = t.actifs || 0;
  b.classList.toggle("actif", actifs > 0);
  b.classList.toggle("indetermine", actifs > 0 && (t.progression === null || t.progression === undefined));
  const anneau = document.getElementById("dl-avance");
  if (anneau) anneau.style.strokeDashoffset = String(97.4 * (1 - Math.min(1, Math.max(0, t.progression || 0))));
  const badge = document.getElementById("dl-badge");
  badge.hidden = actifs < 1;
  badge.textContent = actifs > 9 ? "9+" : String(actifs);
  const enCours = (t.liste || []).filter((d) => d.etat === "en_cours" || d.etat === "en_pause");
  b.title = actifs
    ? `Téléchargements : ${actifs} en cours${t.progression != null ? " — " + Math.round(t.progression * 100) + " %" : ""}\n${enCours.map((d) => d.fichier).slice(0, 3).join("\n")}`
    : "Téléchargements (Ctrl+J)";
  // Eclat : seulement pour un telechargement termine pendant que l'interface est ouverte.
  if (dernierTermineVu !== null && t.dernierTermine > dernierTermineVu) {
    b.classList.remove("arrive");
    void b.offsetWidth;
    b.classList.add("arrive");
    setTimeout(() => b.classList.remove("arrive"), 2000);
  }
  dernierTermineVu = t.dernierTermine || 0;
  // Point vert jusqu'a ce que la page des telechargements soit ouverte. Le dernier fichier vu
  // est garde d'un lancement a l'autre ; au tout premier lancement, rien n'est « non vu ».
  const recu = t.dernierTermine || 0;
  let vu = lireNombre(CLE_DL_VU);
  if (vu === null || pageInterneOuverte("telechargements")) {
    vu = recu;
    ecrireNombre(CLE_DL_VU, vu);
  }
  b.classList.toggle("nonvu", recu > vu);
}

const CLE_DL_VU = "trame.telechargements.vu";
function lireNombre(cle) {
  try {
    const v = localStorage.getItem(cle);
    return v === null ? null : Number(v) || 0;
  } catch {
    return 0;
  }
}
function ecrireNombre(cle, v) {
  try {
    localStorage.setItem(cle, String(v));
  } catch {}
}

/// La page affichee est-elle la page interne `nom` (reglages, telechargements) ?
function pageInterneOuverte(nom) {
  const u = (etat && etat.urlBrute) || "";
  return new RegExp("^(trame://localhost|https?://trame\\.localhost)/" + nom + "\\.html").test(u);
}

/// Allume le bouton de la barre dont le panneau ou la page est ouvert (maquettes 1 et 2).
function marquerBoutons() {
  const allumer = (id, oui) => {
    const b = document.getElementById(id);
    if (b) b.classList.toggle("ouvert", !!oui);
  };
  const panneau = document.getElementById("panneau");
  const montre = (id) => panneau && !panneau.hidden && !document.getElementById(id).hidden;
  allumer("btn-ext", montre("extensions"));
  allumer("btn-page", montre("menu-page"));
  allumer("btn-zap", etat && etat.zap && etat.zap.ouvert);
  allumer("btn-reglages", pageInterneOuverte("reglages"));
  allumer("btn-dl", pageInterneOuverte("telechargements"));
}

function rendreBarre(s) {
  const c = champ();
  // On ne reecrit pas l'adresse pendant qu'on tape. Mais « le champ a le focus » ne suffit pas a
  // le dire : un clic dans la page (une autre webview) laisse le champ actif dans SON document,
  // et la barre affichait encore la page de connexion d'Amazon une fois connecte (constate le
  // 2026-11-21). Il faut donc aussi que la barre elle-meme ait le focus.
  const enSaisie = document.activeElement === c && document.hasFocus();
  if (!enSaisie) {
    c.value = s.interne ? "" : s.url;
    c.placeholder = s.actif
      ? "Chercher un onglet, un fil, une adresse — ou › pour les commandes"
      : "Saisir une adresse";
  }
  document.getElementById("btn-retour").disabled = !s.retourPossible;
  document.getElementById("btn-avant").disabled = !s.avantPossible;
  // Le puzzle ne porte un badge que pendant une installation : c'est la seule chose qui
  // merite d'attirer l'oeil sans qu'on l'ait demande.
  const badge = document.getElementById("ext-badge");
  const enCours = (s.installations || []).length;
  badge.hidden = !enCours;
  badge.textContent = enCours ? "…" : "";
  document.getElementById("btn-ext").classList.toggle("occupe", enCours > 0);

  rendreTelechargements(s.telechargements);
  rendreBarreExtensions(s);

  const carnet = s.carnet || {};
  const etoile = document.getElementById("btn-favori");
  etoile.hidden = !carnet.pageWeb;
  etoile.classList.toggle("on", !!carnet.favori);
  etoile.title = carnet.favori ? "Retirer des favoris (Ctrl+D)" : "Ajouter aux favoris (Ctrl+D)";
  const badgePage = document.getElementById("page-badge");
  badgePage.hidden = !carnet.guetsNonVus;
  badgePage.textContent = carnet.guetsNonVus || "";

  if (!s.panneau && panneauOuvert) {
    panneauOuvert = false;
    document.getElementById("panneau").hidden = true;
  }
  if (extensionsVisibles) rendreExtensions(s);
  if (pageVisible) rendreMenuPage(s);
  marquerBoutons();
}

/* ---------------------------------------------------------- menu de la page */

let cibleMenu = null; // onglet clique dans le rail ; null : la page affichee (bouton « … »)
function ouvrirMenuPage(ancre) {
  ancreMenu = ancre && typeof ancre.y === "number" ? ancre : null;
  cibleMenu = ancre && ancre.cible ? ancre.cible : null;
  ouvrirPanneau();
  extensionsVisibles = false;
  commandesVisibles = false;
  pageVisible = true;
  document.getElementById("extensions").hidden = true;
  document.getElementById("resultats").hidden = true;
  document.getElementById("menu-commandes").hidden = true;
  document.getElementById("menu-page").hidden = false;
  signaturePage = "";
  if (etat) rendreMenuPage(etat);
}

/* ------------------------------------------------- menu des commandes (logo) */

/* Menu d'un acces du rail replie : la barre le dessine dans sa colonne de panneau, colle au rail
   (x = 6) et a la hauteur de l'icone — remonte s'il deborderait du bas de la fenetre. Il reprend
   la zone des commandes : un seul menu ouvert a la fois. */
let ancreAcces = null;
function ouvrirMenuAccesBarre(x) {
  ouvrirMenuCommandes(null);
  ancreAcces = { y: Number(x.y) || 0, h: Number(x.h) || 0 };
  const zone = document.getElementById("menu-commandes");
  zone.textContent = "";
  const m = menuAcces(x.acces, fermerPanneau);
  m.classList.add("dans-barre");
  zone.appendChild(m);
  const haut = zone.getBoundingClientRect().top;
  const hMenu = Math.ceil(m.getBoundingClientRect().height);
  const voulu = ancreAcces.y - haut;
  const maxi = ancreAcces.h ? ancreAcces.h - haut - 8 - hMenu : voulu;
  m.style.marginTop = Math.max(0, Math.min(voulu, maxi)) + "px";
  ajusterHauteur();
}

function ouvrirMenuCommandes(liste) {
  ancreAcces = null;
  ouvrirPanneau();
  extensionsVisibles = false;
  pageVisible = false;
  commandesVisibles = true;
  document.getElementById("extensions").hidden = true;
  document.getElementById("resultats").hidden = true;
  document.getElementById("menu-page").hidden = true;
  const zone = document.getElementById("menu-commandes");
  zone.hidden = false;
  zone.textContent = "";
  // Pas de classe « page » : sa hauteur maximale suit la fenetre de la barre, encore basse a
  // l'ouverture — le menu sortait tronque a une ligne (retour de Clement, 2026-11-22).
  if (liste) zone.appendChild(colonneCommandes(liste, fermerPanneau));
  ajusterHauteur();
}

/** La colonne des commandes, commune au rail et a la barre. */
function colonneCommandes(liste, fermer) {
  const colonne = elem("div", "menu-colonne commandes");
  const tete = elem("div", "menu-tete");
  tete.appendChild(elem("span", "menu-titre", "Commandes"));
  colonne.appendChild(tete);
  for (const c of liste) {
    const ligne = elem("div", "menu-ligne");
    const b = elem("button", "menu-principal");
    b.title = c.titre;
    const corps = elem("span", "corps");
    corps.appendChild(elem("span", "n", c.titre));
    if (c.raccourci) corps.appendChild(elem("span", "d", c.raccourci));
    b.appendChild(corps);
    b.addEventListener("pointerdown", (ev) => {
      if (ev.button !== 0) return;
      ev.preventDefault();
      ev.stopPropagation();
      fermer();
      ipc({ m: "ouvrir", action: { type: "commande", id: c.id } });
    });
    ligne.appendChild(b);
    colonne.appendChild(ligne);
  }
  colonne.appendChild(elem("div", "menu-astuce", "Aussi au clavier : tapez « > » dans la barre d'adresse."));
  return colonne;
}

/* Rail deplie : le menu s'ouvre sous le logo, par-dessus les fils. */
function fermerCommandesRail() {
  const m = document.getElementById("commandes-rail");
  if (m) m.remove();
  document.removeEventListener("pointerdown", clicHorsCommandes, true);
  document.removeEventListener("keydown", echapCommandes, true);
  window.removeEventListener("blur", fermerCommandesRail);
}
function clicHorsCommandes(e) {
  const m = document.getElementById("commandes-rail");
  if (m && !m.contains(e.target)) fermerCommandesRail();
}
function echapCommandes(e) {
  if (e.key === "Escape") fermerCommandesRail();
}
function ouvrirCommandesRail(liste) {
  fermerCommandesRail();
  const m = elem("div", "commandes-rail");
  m.id = "commandes-rail";
  const tete = document.querySelector(".rail-tete");
  m.style.top = Math.round(tete ? tete.getBoundingClientRect().bottom + 4 : 56) + "px";
  m.appendChild(colonneCommandes(liste, fermerCommandesRail));
  document.getElementById("rail").appendChild(m);
  document.addEventListener("pointerdown", clicHorsCommandes, true);
  document.addEventListener("keydown", echapCommandes, true);
  window.addEventListener("blur", fermerCommandesRail);
}

/** Heure locale `h` dans `jours` jours, en ms (l'hote ne connait pas le fuseau). */
function aHeure(jours, h) {
  const d = new Date();
  d.setDate(d.getDate() + jours);
  d.setHours(h, 0, 0, 0);
  return d.getTime();
}

function echeances() {
  const now = Date.now();
  const d = new Date();
  const liste = [{ t: "Dans 1 h", quand: now + 3600e3, libelle: "dans 1 h" }];
  if (d.getHours() < 17) liste.push({ t: "Ce soir", quand: aHeure(0, 18), libelle: "ce soir à 18 h" });
  liste.push({ t: "Demain matin", quand: aHeure(1, 9), libelle: "demain à 9 h" });
  const versSamedi = (6 - d.getDay() + 7) % 7 || 7;
  if (versSamedi > 1) liste.push({ t: "Ce week-end", quand: aHeure(versSamedi, 10), libelle: "samedi à 10 h" });
  const versLundi = (8 - d.getDay()) % 7 || 7;
  liste.push({ t: "Lundi", quand: aHeure(versLundi, 9), libelle: "lundi à 9 h" });
  return liste;
}

function lignePage(nom, detail, ic, message, classe) {
  const ligne = elem("div", "menu-ligne" + (classe ? " " + classe : ""));
  const f = document.createDocumentFragment();
  const boite = elem("span", "menu-icone page");
  boite.appendChild(icone(ic, "petit"));
  f.appendChild(boite);
  const corps = elem("span", "corps");
  corps.appendChild(elem("span", "n", nom));
  if (detail) corps.appendChild(elem("span", "d", detail));
  f.appendChild(corps);
  const b = actionMenu("menu-principal", nom, message, f);
  ligne.appendChild(b);
  return ligne;
}

/** « Copier l'adresse » : le presse-papiers est celui de la barre (ce document), pas l'hote. */
function ligneCopierAdresse(url) {
  const ligne = elem("div", "menu-ligne");
  const b = elem("button", "menu-principal");
  b.title = url;
  const boite = elem("span", "menu-icone page");
  boite.appendChild(icone("external", "petit"));
  b.appendChild(boite);
  const corps = elem("span", "corps");
  corps.appendChild(elem("span", "n", "Copier l'adresse"));
  corps.appendChild(elem("span", "d", url.length > 60 ? url.slice(0, 57) + "…" : url));
  b.appendChild(corps);
  b.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    const fini = () => {
      fermerPanneau();
      toast("Adresse copiée.");
    };
    const secours = () => {
      const t = document.createElement("textarea");
      t.value = url;
      document.body.appendChild(t);
      t.select();
      try { document.execCommand("copy"); } catch (e) {}
      t.remove();
      fini();
    };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(url).then(fini, secours);
    else secours();
  });
  ligne.appendChild(b);
  return ligne;
}

/** « Copier le titre » : meme presse-papiers que l'adresse, mais le texte de l'onglet. */
function ligneCopierTitre(titre, url) {
  const ligne = elem("div", "menu-ligne");
  const b = elem("button", "menu-principal");
  b.title = titre;
  const boite = elem("span", "menu-icone page");
  boite.appendChild(icone("livre", "petit"));
  b.appendChild(boite);
  const corps = elem("span", "corps");
  corps.appendChild(elem("span", "n", "Copier le titre"));
  corps.appendChild(elem("span", "d", titre.length > 60 ? titre.slice(0, 57) + "…" : titre));
  b.appendChild(corps);
  b.addEventListener("pointerdown", (ev) => {
    if (ev.button !== 0) return;
    ev.preventDefault();
    ev.stopPropagation();
    const fini = () => {
      fermerPanneau();
      toast("Titre copié.");
    };
    const secours = () => {
      const t = document.createElement("textarea");
      t.value = titre;
      document.body.appendChild(t);
      t.select();
      try { document.execCommand("copy"); } catch (e) {}
      t.remove();
      fini();
    };
    if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(titre).then(fini, secours);
    else secours();
  });
  ligne.appendChild(b);
  return ligne;
}

function puces(liste) {  const zone = elem("div", "menu-puces");
  for (const p of liste) {
    const b = actionMenu("puce" + (p.classe ? " " + p.classe : ""), p.titre || p.t, p.message);
    b.textContent = p.t;
    zone.appendChild(b);
  }
  return zone;
}

function rendreMenuPage(s) {
  const zone = document.getElementById("menu-page");
  const c = s.carnet || {};
  const onglets = s.onglets || [];
  const affiche = onglets.find((o) => o.actif) || {};
  // L'onglet vise : celui du clic droit dans le rail, sinon la page affichee. Les lignes propres
  // a la page (favori, imprimer, guetter, cote a cote…) ne valent que pour la page affichee.
  const actif = (cibleMenu && onglets.find((o) => o.id === cibleMenu)) || affiche;
  const surPage = actif.id === affiche.id;
  const fils = (s.fils || []).map((f) => [f.id, f.nom]);
  const signature = JSON.stringify([c, actif.id, actif.fermeture, actif.titre, actif.eveille, actif.muet, actif.fil, fils, s.url, surPage, actif.ancre, actif.alire]);
  if (signature === signaturePage && zone.childElementCount) return;
  signaturePage = signature;
  zone.textContent = "";
  const colonne = elem("div", "menu-colonne page");
  zone.appendChild(colonne);

  const tete = elem("div", "menu-tete");
  tete.appendChild(elem("span", "menu-titre", surPage ? "Cette page" : "Cet onglet — " + (actif.titre || actif.urlCourte || "")));
  colonne.appendChild(tete);

  // L'onglet lui-meme : tout ce qu'on peut lui faire, pas seulement la veille et le son.
  if (actif.id) {
    // Fermer : l'action la plus attendue d'un menu d'onglet, et elle manquait.
    colonne.appendChild(lignePage("Fermer l'onglet", surPage ? "Ctrl+W" : "sans l'afficher", "x", { m: "fermer", id: actif.id }));
    const dansLeFil = onglets.filter((o) => o.fil === actif.fil && o.id !== actif.id).length;
    if (dansLeFil > 0) {
      colonne.appendChild(
        lignePage(
          "Fermer les autres du fil",
          `${dansLeFil} autre(s) · Ctrl+Maj+T les rouvre`,
          "x",
          { m: "fermerAutres", id: actif.id }
        )
      );
    }
    colonne.appendChild(lignePage("Dupliquer l'onglet", "même fil, juste à côté", "plus", { m: "dupliquer", id: actif.id }));
    colonne.appendChild(
      actif.eveille
        ? lignePage("Mettre en veille", surPage ? "libère sa mémoire ; on passe à l'onglet précédent" : "libère sa mémoire, rien n'est fermé", "sleep", { m: "endormir", id: actif.id })
        : lignePage("Réveiller", "recharge la page en arrière-plan, sans l'afficher", "reload", { m: "reveillerFond", id: actif.id })
    );
    colonne.appendChild(lignePage("Recharger", surPage ? "F5" : "", "reload", { m: "recharger", id: actif.id }));
    colonne.appendChild(
      actif.muet
        ? lignePage("Remettre le son", "", "son", { m: "muet", id: actif.id }, "on")
        : lignePage("Couper le son", "cet onglet seulement", "muet", { m: "muet", id: actif.id })
    );
    colonne.appendChild(
      actif.ancre
        ? lignePage("Détacher", "l'onglet redevient sensible à la veille", "pin", { m: "ancrer", id: actif.id, actif: false }, "on")
        : lignePage("Ancrer", "cet onglet ne refroidit jamais, il reste éveillé", "pin", { m: "ancrer", id: actif.id, actif: true })
    );
    colonne.appendChild(
      actif.alire
        ? lignePage("Retirer de « à lire »", "", "livre", { m: "alire", id: actif.id, actif: false }, "on")
        : lignePage("À lire plus tard", "le garde hors de la veille, pour y revenir", "livre", { m: "alire", id: actif.id, actif: true })
    );
    if (actif.url && /^https?:/.test(actif.url)) {
      // Un site, pas une page : c'est ce que la bande du rail et le Nouvel onglet attendent.
      colonne.appendChild(
        lignePage("Ajouter aux accès rapides", "le site, pas cette page", "points", { m: "accesDepuisOnglet", onglet: actif.id })
      );
    }
    if (actif.url && /^(https?|file):/.test(actif.url)) colonne.appendChild(ligneCopierAdresse(actif.url));
    if (actif.titre) colonne.appendChild(ligneCopierTitre(actif.titre, actif.url));
    const autres = (s.fils || []).filter((f) => f.id !== actif.fil);
    colonne.appendChild(elem("div", "menu-sous", "Déplacer vers un fil"));
    colonne.appendChild(
      puces(
        autres
          .map((f) => ({ t: f.nom, titre: "Ranger cet onglet dans « " + f.nom + " »", message: { m: "tisser", onglet: actif.id, fil: f.id } }))
          .concat([{ t: "＋ Nouveau fil", titre: "Ouvrir un nouveau fil avec cet onglet", message: { m: "tisser", onglet: actif.id, fil: 0 } }])
      )
    );
  }

  if (c.pageWeb && surPage) {
    colonne.appendChild(
      lignePage(c.favori ? "Retirer des favoris" : "Ajouter aux favoris", "Ctrl+D", "etoile", { m: "favori" }, c.favori ? "on" : "")
    );
    colonne.appendChild(lignePage("Imprimer ou enregistrer en PDF", "Ctrl+P", "imprimer", { m: "imprimer" }));
    colonne.appendChild(lignePage("Chercher dans la page", "Ctrl+F", "search", { m: "chercherPage" }));
    // La navigation privee se trouve la ou l'on cherche une action sur la page en cours.
    colonne.appendChild(
      lignePage(
        "Ouvrir en navigation privée",
        "Ctrl+Maj+N · rien n'est gardé, ni onglets, ni historique, ni cookies",
        "masque",
        { m: "prive" }
      )
    );
  }

  // Rendez-vous
  colonne.appendChild(elem("div", "menu-section", "Rendez-vous — l'onglet dort hors du rail et revient seul"));
  colonne.appendChild(
    puces(echeances().map((e) => ({ t: e.t, titre: "Revient " + e.libelle, message: { m: "rendezVous", id: actif.id, quand: e.quand, libelle: e.libelle } })))
  );
  const fermetures = [
    { t: "Dans 1 h", quand: Date.now() + 3600e3, libelle: "dans 1 h" },
    { t: "Dans 3 h", quand: Date.now() + 3 * 3600e3, libelle: "dans 3 h" },
    { t: "Ce soir", quand: aHeure(new Date().getHours() >= 23 ? 1 : 0, 23), libelle: "ce soir à 23 h" },
  ].map((e) => ({ t: e.t, titre: "Se ferme " + e.libelle, message: { m: "fermeture", id: actif.id, quand: e.quand, libelle: e.libelle } }));
  if (actif.fermeture) {
    fermetures.push({ t: "Annuler (" + heure(actif.fermeture) + ")", classe: "annuler", message: { m: "fermeture", id: actif.id, quand: null } });
  }
  colonne.appendChild(elem("div", "menu-sous", "Fermer tout seul"));
  colonne.appendChild(puces(fermetures));

  if (!surPage) {
    colonne.appendChild(elem("div", "menu-astuce", "Clic sur l'onglet pour l'afficher : favori, impression, guet et côte à côte s'y trouvent."));
    placerMenuPage();
    ajusterHauteur();
    return;
  }

  // Cote a cote
  colonne.appendChild(elem("div", "menu-section", "Côte à côte"));
  if (c.cote && c.cote.affiche) {
    colonne.appendChild(
      puces([
        { t: "Inverser", message: { m: "coteInverser" } },
        { t: c.cote.lie ? "Défilement lié ✓" : "Lier le défilement", classe: c.cote.lie ? "on" : "", message: { m: "coteLie", actif: !c.cote.lie } },
        { t: "Quitter", classe: "annuler", message: { m: "coteFermer" } },
      ])
    );
    colonne.appendChild(elem("div", "menu-astuce", "Faites glisser la séparation entre les deux pages pour changer leur largeur."));
  } else if (c.precedent) {
    colonne.appendChild(
      lignePage("À côté de « " + c.precedent.titre + " »", "ou Maj+clic sur un onglet du rail", "cote", { m: "coteAvec", id: c.precedent.id })
    );
  }
  colonne.appendChild(
    elem("div", "menu-astuce", "Dans la barre d'adresse : Alt+Entrée ouvre dans un nouvel onglet de ce fil, Maj+Entrée à côté de cette page.")
  );

  // Guets
  colonne.appendChild(elem("div", "menu-section", "Guetteur — prévient quand un élément change"));
  if (c.pageWeb) {
    colonne.appendChild(lignePage("Guetter un élément de cette page", "un prix, un stock, un résultat…", "cible", { m: "guetPoser" }));
  }
  for (const g of c.guets || []) {
    const ligne = elem("div", "menu-ligne guet" + (g.vu ? "" : " change"));
    const f = document.createDocumentFragment();
    const corps = elem("span", "corps");
    corps.appendChild(elem("span", "n", g.libelle));
    let detail = g.texte;
    if (!g.vu && g.ancien) detail = g.ancien + " → " + g.texte;
    else if (g.enCours) detail = "relecture en cours… · " + g.texte;
    else if (g.echecs) detail = "élément introuvable (" + g.echecs + ") · dernier : " + g.texte;
    corps.appendChild(elem("span", "d", detail));
    f.appendChild(corps);
    const b = actionMenu("menu-principal", "Ouvrir la page guettée", { m: "guetOuvrir", id: g.id }, f);
    ligne.appendChild(b);
    const relire = actionMenu("menu-epingle", "Relire maintenant", { m: "guetRelire", id: g.id }, icone("reload", "petit"));
    ligne.appendChild(relire);
    const retirer = actionMenu("menu-epingle", "Ne plus guetter", { m: "guetRetirer", id: g.id }, icone("x", "petit"));
    ligne.appendChild(retirer);
    colonne.appendChild(ligne);
  }

  // Memoire de lecture
  colonne.appendChild(elem("div", "menu-section", "Mémoire de lecture"));
  if (!(s.reglages && s.reglages.memoireLecture)) {
    colonne.appendChild(lignePage("Activer la mémoire de lecture", "retrouver une page par une phrase lue", "livre", { m: "memoireLecture", actif: true }));
  } else {
    colonne.appendChild(elem("div", "menu-astuce", "Tapez ? suivi de quelques mots dans la barre pour retrouver une page lue. Tout reste sur ce PC."));
    if (c.pageWeb && !c.lectureExclu) {
      colonne.appendChild(lignePage("Ne jamais garder ce site", "et effacer ce qui en a été gardé", "livre", { m: "lectureExclure" }));
    } else if (c.lectureExclu) {
      colonne.appendChild(elem("div", "menu-astuce", "Ce site n'est jamais gardé (Réglages pour changer)."));
    }
  }
  placerMenuPage();
  ajusterHauteur();
}

/**
 * Menu ouvert par un clic droit sur un onglet du rail : colle au rail, a la hauteur du clic
 * (remonte s'il deborderait du bas de la fenetre). Ouvert par le bouton « … » : sous ce bouton,
 * a droite, comme avant.
 */
function placerMenuPage() {
  const zone = document.getElementById("menu-page");
  const colonne = zone.querySelector(".menu-colonne");
  if (!colonne) return;
  colonne.classList.toggle("ancre", !!ancreMenu);
  poserPoignee(colonne);
  const haut = zone.getBoundingClientRect().top;
  const marge = 8;
  // Hauteur choisie a la poignee (gardee d'une ouverture a l'autre), bornee par la place reelle.
  const choisie = tailleMenuChoisie();
  if (!ancreMenu) {
    // Sous le bouton « … » : l'hote accorde 720 px au plus a la barre. La hauteur maximale ne
    // suit plus 100vh : la barre, encore basse a l'ouverture, bridait le menu a quelques lignes.
    const dispoBouton = Math.max(160, 720 - haut - marge);
    colonne.style.marginTop = "";
    colonne.style.maxHeight = Math.min(dispoBouton, choisie || dispoBouton) + "px";
    return;
  }
  const dispo = Math.max(160, ancreMenu.h - haut - 2 * marge);
  colonne.style.maxHeight = Math.min(dispo, choisie || dispo) + "px";
  colonne.style.marginTop = "0px";
  const hMenu = Math.ceil(colonne.getBoundingClientRect().height);
  const voulu = ancreMenu.y - haut - 12;
  const maxi = ancreMenu.h - haut - marge - hMenu;
  colonne.style.marginTop = Math.max(0, Math.min(voulu, maxi)) + "px";
}

/* Taille du menu de la page, choisie a la poignee du bas (gardee dans ce profil seulement).
   Version 2 de la cle : les hauteurs enregistrees avant la 0.4.57 sont IGNOREES. Constat du
   2026-11-23 chez Clement — un menu plafonne a 250 px alors qu'il lui en fallait 893, coupe nette,
   parce qu'une valeur avait ete enregistree par un simple clic sur la poignee (voir `poserPoignee`)
   et qu'aucun moyen visible ne permettait de revenir en arriere. */
const CLE_TAILLE_MENU = "trame.menuPage.hauteur.v2";
function tailleMenuChoisie() {
  try {
    const v = parseInt(localStorage.getItem(CLE_TAILLE_MENU) || "", 10);
    return v >= 160 ? v : 0;
  } catch (e) {
    return 0;
  }
}
function poserPoignee(colonne) {
  if (colonne.querySelector(".menu-poignee")) return;
  const p = elem("div", "menu-poignee");
  p.title = "Glisser pour agrandir ou réduire le menu · double-clic : taille automatique";
  p.appendChild(elem("span", "trait"));
  p.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    p.setPointerCapture(e.pointerId);
    const depart = e.clientY;
    const hDepart = colonne.getBoundingClientRect().height;
    // Un CLIC sur la poignee ne doit rien enregistrer : seule une vraie traction choisit une
    // hauteur. Sans cela, un clic (ou un clic droit a cote) memorisait la hauteur du moment —
    // et un menu plus tardif restait plafonne a cette valeur, coupe au milieu de ses lignes
    // (constat du 2026-11-23 : 250 px enregistres pour un contenu de 893).
    let tiree = false;
    // Place reelle : jusqu'au bas de la fenetre (ancre) ou jusqu'aux 720 px de la barre.
    const haut = colonne.getBoundingClientRect().top;
    const limite = Math.max(160, (ancreMenu ? ancreMenu.h : 720) - haut - 8);
    const bouger = (ev) => {
      if (!tiree && Math.abs(ev.clientY - depart) < 4) return;
      tiree = true;
      const h = Math.round(Math.min(limite, Math.max(160, hDepart + ev.clientY - depart)));
      colonne.style.maxHeight = h + "px";
      colonne.style.height = h + "px";
      ajusterHauteur();
    };
    const lacher = () => {
      p.removeEventListener("pointermove", bouger);
      p.removeEventListener("pointerup", lacher);
      p.removeEventListener("pointercancel", lacher);
      const h = Math.round(colonne.getBoundingClientRect().height);
      colonne.style.height = "";
      if (tiree) {
        try {
          localStorage.setItem(CLE_TAILLE_MENU, String(h));
        } catch (err) {
          /* stockage indisponible : la taille vaut pour cette ouverture */
        }
      }
      placerMenuPage();
      ajusterHauteur();
    };
    p.addEventListener("pointermove", bouger);
    p.addEventListener("pointerup", lacher);
    p.addEventListener("pointercancel", lacher);
  });
  p.addEventListener("dblclick", (e) => {
    e.preventDefault();
    try {
      localStorage.removeItem(CLE_TAILLE_MENU);
    } catch (err) {
      /* rien a oublier */
    }
    placerMenuPage();
    ajusterHauteur();
  });
  colonne.appendChild(p);
}

/** Alt : nouvel onglet dans le fil courant. Maj : a cote de la page affichee. */
function mode(e) {
  if (!e) return "";
  return e.altKey ? "onglet" : e.shiftKey ? "cote" : "";
}

function ouvrir(item, m) {
  ipc(m ? { m: "ouvrir", action: item.action, mode: m } : { m: "ouvrir", action: item.action });
  fermerPanneau();
}

function bouger(pas) {
  if (!items.length) return;
  selection = (selection + pas + items.length) % items.length;
  surligner();
  const actif = document.querySelector(".resultat.sur");
  if (actif) actif.scrollIntoView({ block: "nearest" });
}

function valider(e) {
  const m = mode(e);
  if (selection >= 0 && items[selection]) {
    ouvrir(items[selection], m);
    return;
  }
  const valeur = champ().value.trim();
  if (valeur) ipc(m ? { m: "naviguer", url: valeur, mode: m } : { m: "naviguer", url: valeur });
  fermerPanneau();
}

/* ------------------------------------------------------------------ commun */

function brancherBarre() {
  const c = champ();
  let minuterie = null;
  c.addEventListener("input", () => {
    clearTimeout(minuterie);
    minuterie = setTimeout(() => {
      ouvrirPanneau();
      demander();
    }, 90);
  });
  c.addEventListener("focus", () => {
    ouvrirPanneau();
    demander();
  });
  c.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      valider(e);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      bouger(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      bouger(-1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      fermerPanneau();
    }
  });

  document.getElementById("btn-retour").onclick = () => ipc({ m: "retour" });
  document.getElementById("btn-avant").onclick = () => ipc({ m: "avant" });
  document.getElementById("btn-recharger").onclick = () => ipc({ m: "recharger" });
  document.getElementById("btn-ext").onclick = () => {
    if (extensionsVisibles) {
      fermerPanneau();
      return;
    }
    ouvrirPanneau();
    signatureMenu = "";
    if (etat) rendreExtensions(etat);
  };
  document.getElementById("btn-reglages").onclick = () => ipc({ m: "ouvrirReglages" });
  document.getElementById("btn-favori").onclick = (e) => {
    e.stopPropagation();
    ipc({ m: "favori" });
  };
  document.getElementById("btn-page").onclick = () => {
    if (pageVisible) {
      fermerPanneau();
      return;
    }
    ouvrirMenuPage();
  };
  // Raccourcis de la barre elle-meme (le focus y est quand on tape une adresse).
  document.addEventListener("keydown", (e) => {
    const ctrl = e.ctrlKey || e.metaKey;
    const k = (e.key || "").toLowerCase();
    if (ctrl && k === "d") {
      e.preventDefault();
      ipc({ m: "favori" });
    } else if (ctrl && k === "p") {
      e.preventDefault();
      ipc({ m: "imprimer" });
    }
  });
  document.getElementById("btn-zap").onclick = () => ipc({ m: "zap" });
  document.getElementById("btn-dl").onclick = () => ipc({ m: "ouvrirTelechargements" });
  // Le panneau s'ouvre et se ferme a plusieurs endroits : on suit ses `hidden` pour allumer
  // le bon bouton, plutot que d'appeler marquerBoutons() partout.
  const panneau = document.getElementById("panneau");
  if (panneau) new MutationObserver(marquerBoutons).observe(panneau, { attributes: true, attributeFilter: ["hidden"], subtree: true });
  document.getElementById("btn-moteur").onclick = () => ipc({ m: "ouvrirReglages" });
  document.getElementById("btn-dossier-ext").onclick = () => ipc({ m: "dossierExtensions" });

  // Perdre le focus de la fenetre = clic dans la page : le panneau se referme.
  window.addEventListener("blur", () => {
    if (panneauOuvert) fermerPanneau();
    // La saisie est abandonnee : le champ reprend l'adresse reelle de la page tout de suite,
    // sans attendre la prochaine poussee d'etat.
    const c = champ();
    if (document.activeElement === c) {
      c.blur();
      if (etat) c.value = etat.interne ? "" : etat.url;
    }
  });
}

function brancherRail() {
  // Toute action dans le rail ferme le panneau de la barre (un clic dans la page ou le rail
  // ne retire pas le focus de la barre : deux webviews voisines, pas de `blur`).
  let dernierClic = 0;
  document.addEventListener(
    "pointerdown",
    () => {
      const maintenant = Date.now();
      if (maintenant - dernierClic < 300) return;
      dernierClic = maintenant;
      ipc({ m: "clicPage" });
    },
    true
  );

  document.getElementById("btn-nouvel").onclick = () => ipc({ m: "nouvelOnglet" });
  // Clic droit sur le logo : la liste des commandes (elle ne s'affiche plus sous la barre vide).
  const marque = document.querySelector(".rail-tete .marque");
  if (marque) {
    marque.oncontextmenu = (e) => {
      e.preventDefault();
      // Rail replie (64 px) : trop etroit, la barre dessine le menu.
      ipc({ m: "menuCommandes", ici: window.innerWidth >= 180 });
    };
  }
  // Replier ou deplier le rail, a toute largeur de fenetre (choix retenu dans la session).
  const btnRail = document.getElementById("btn-rail");
  if (btnRail) btnRail.onclick = () => ipc({ m: "rail" });
  document.getElementById("btn-trame-vue").onclick = () => ipc({ m: "trame" });
  document.getElementById("btn-veille").onclick = () => ipc({ m: "endormirFroids" });
  // Navigation privee, depuis le pied du rail : meme message que Ctrl+Maj+N, qui reste le
  // raccourci pour qui le connait. L'entree existe pour qui ne le connait pas.
  document.getElementById("btn-prive").onclick = () => ipc({ m: "prive" });
  document.getElementById("btn-capsules").onclick = () => basculerVolet("capsules");
  document.getElementById("volet-fermer").onclick = () => {
    document.getElementById("volet").hidden = true;
  };

  // Deposer un onglet sur la bande d'acces rapides : il devient un acces rapide du rail.
  const bande = document.getElementById("bande-acces");
  bande.addEventListener("dragover", (e) => {
    if (glisseRail) return;
    e.preventDefault();
    bande.classList.add("survol-tissage");
  });
  bande.addEventListener("dragleave", () => bande.classList.remove("survol-tissage"));
  bande.addEventListener("drop", (e) => {
    e.preventDefault();
    bande.classList.remove("survol-tissage");
    if (glisseRail) return;
    const id = Number(e.dataTransfer.getData("text/plain"));
    if (id) ipc({ m: "accesDepuisOnglet", onglet: id, rail: true });
  });

  const depot = document.getElementById("depot");
  depot.hidden = false;
  depot.addEventListener("dragover", (e) => {
    e.preventDefault();
    depot.classList.add("survol-tissage");
  });
  depot.addEventListener("dragleave", () => depot.classList.remove("survol-tissage"));
  depot.addEventListener("drop", (e) => {
    e.preventDefault();
    depot.classList.remove("survol-tissage");
    const id = Number(e.dataTransfer.getData("text/plain"));
    if (id) ipc({ m: "tisser", onglet: id, fil: 0 });
  });
  // Ranger les groupes du rail : le pointeur est suivi sur le document, parce que la carte
  // saisie ne recoit plus les evenements des qu'on la fait glisser hors de sa tete.
  document.addEventListener("pointermove", suivreRangement);
  document.addEventListener("pointerup", relacherRangement);
  // Un geste interrompu par le systeme (perte de capture, fenetre inactive) n'est pas un depot :
  // on range a la hauteur 0, ce qui mettrait le groupe en tete sans l'avoir demande.
  document.addEventListener("pointercancel", annulerRangement);
  window.addEventListener("blur", annulerRangement);
}

/* ------------------------------------------------------------------ depart */

if (surface === "rail") {
  document.getElementById("rail").hidden = false;
  brancherRail();
  // Rail replie (64 px) : une bulle dessinee ici serait coupee par le bord de la webview. C'est
  // la barre qui la dessine, en carte juste a droite de l'icone (l'hote relaie `bulle`).
  TrameBulles.brancher({
    deleguer: {
      actif: () => window.innerWidth < 130,
      montrer: (r, texte) => ipc({ m: "bulle", texte, y: Math.round(r.top) }),
      cacher: () => ipc({ m: "bulle" }),
    },
  });
} else {
  document.getElementById("barre").hidden = false;
  brancherBarre();
  // La barre ne fait que 45 a 85 px de haut : une bulle qui n'y tient pas (puces d'extensions,
  // barre sans rangee d'extensions) passe par la colonne du panneau — l'hote agrandit la barre
  // et ne decoupe que cette carte, la page reste visible et cliquable autour (`superposer`).
  TrameBulles.brancher({
    hote: {
      montrer(zone, hauteur) {
        if (panneauOuvert) return 0; // un vrai menu occupe deja la colonne
        // L'hote borne la hauteur a 150 px au moins : la carte va jusqu'en bas de cette bande.
        const bas = Math.max(150, zone.y + hauteur);
        ipc({ m: "panneau", open: true, zone, hauteur: bas });
        return bas;
      },
      cacher() {
        if (!panneauOuvert) ipc({ m: "panneau", open: false });
      },
    },
  });
}
initFenetre();

  // Handshake : le premier message peut se perdre si la page s'annonce trop tot.
  let essais = 0;
  const annonce = () => {
    if (pret) return;
    essais += 1;
    ipc({ m: "pret", surface });
    if (essais > 20) clearInterval(minuteriePret);
  };
  const minuteriePret = setInterval(annonce, 700);
  annonce();
})();
