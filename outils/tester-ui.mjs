#!/usr/bin/env node
/**
 * Tests de l'interface de Trame, sans WebView2.
 *
 * Pourquoi ce harnais : l'interface (`ui/`) est du HTML/JS servi par le protocole `trame:`.
 * Elle ne peut pas etre testee par `cargo test`, et la tester a la souris dans la vraie
 * fenetre s'est revele trompeur plusieurs fois (coordonnees, DPI, clics perdus). Ici, un
 * Chromium de Playwright sert les memes fichiers par un petit serveur HTTP, `window.ipc` est
 * remplace par un collecteur de messages, et l'etat est pousse a la main : on verifie le DOM
 * rendu et les messages envoyes a l'hote.
 *
 * Lancement :
 *   node outils/tester-ui.mjs
 *   node outils/tester-ui.mjs --voir       (affiche le navigateur)
 *   node outils/tester-ui.mjs --filtre rail
 *
 * Playwright n'est pas une dependance du projet : le script le cherche dans l'ordre
 * `PLAYWRIGHT_CORE`, `playwright-core`, `playwright`, puis dans les installations deja
 * presentes sur la machine (dont `D:\\LIFE\\_tools\\playwright`).
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const UI = path.join(RACINE, "ui");
const RECHERCHE_JS = fs.readFileSync(path.join(UI, "recherche.js"), "utf8");
// Une image PNG de 1x1 : l'icone d'extension du banc d'essai, chargeable sans serveur.
const PIXEL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

const args = process.argv.slice(2);
const VOIR = args.includes("--voir");
const filtre = (() => {
  const i = args.indexOf("--filtre");
  return i >= 0 ? args[i + 1] : null;
})();

// ------------------------------------------------------------------ playwright

const require = createRequire(import.meta.url);
const CANDIDATS = [
  process.env.PLAYWRIGHT_CORE,
  "playwright-core",
  "playwright",
  "D:/LIFE/_tools/playwright/node_modules/playwright-core",
  "C:/Users/crimo/.agents/skills/playwright-skill/node_modules/playwright-core",
].filter(Boolean);

let playwright = null;
for (const candidat of CANDIDATS) {
  try {
    playwright = require(candidat);
    break;
  } catch {
    /* on essaie le suivant */
  }
}
if (!playwright) {
  console.error(
    "Aucun Playwright trouve. Installer (npm i -D playwright-core) ou definir PLAYWRIGHT_CORE."
  );
  process.exit(2);
}

// ------------------------------------------------------------------ serveur

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

function servir() {
  return new Promise((resoudre) => {
    const serveur = http.createServer((req, rep) => {
      const chemin = decodeURIComponent(new URL(req.url, "http://x").pathname);
      const fichier = path.join(UI, chemin === "/" ? "/chrome.html" : chemin);
      if (!fichier.startsWith(UI)) {
        rep.writeHead(403).end("hors racine");
        return;
      }
      fs.readFile(fichier, (erreur, octets) => {
        if (erreur) {
          rep.writeHead(404).end("introuvable");
          return;
        }
        rep.writeHead(200, {
          "content-type": MIME[path.extname(fichier)] || "application/octet-stream",
          "cache-control": "no-store",
        });
        rep.end(octets);
      });
    });
    serveur.listen(0, "127.0.0.1", () => resoudre(serveur));
  });
}

// ------------------------------------------------------------------ outils de test

const resultats = [];
let courant = "";

async function cas(nom, fn) {
  if (filtre && !nom.toLowerCase().includes(filtre.toLowerCase())) return;
  courant = nom;
  try {
    await fn();
    resultats.push({ nom, ok: true });
    console.log(`  ok   ${nom}`);
  } catch (e) {
    resultats.push({ nom, ok: false, erreur: e.message });
    console.log(`  ECHEC ${nom}\n        ${e.message}`);
  }
}

function verifier(condition, message) {
  if (!condition) throw new Error(message);
}

function egal(obtenu, attendu, message) {
  const a = JSON.stringify(obtenu);
  const b = JSON.stringify(attendu);
  if (a !== b) throw new Error(`${message} : obtenu ${a}, attendu ${b}`);
}

// Un etat complet, tel que l'hote le pousse. Les tests le modifient ensuite.
function etatDeBase() {
  return {
    onglets: [],
    fils: [],
    actif: null,
    actifTitre: "",
    url: "",
    interne: false,
    retourPossible: false,
    avantPossible: false,
    moteur: { id: "perplexity", nom: "Perplexity" },
    barreExtensions: true,
    panneau: false,
    vue: "page",
    extensions: [],
    capsules: [],
    suggestion: null,
    budget: { reveilles: 1, endormis: 2, mo: 660.7, processus: 10, plafond: 6 },
    reglages: { veilleAuto: true, budget: 6, barreExtensions: true, moteur: "perplexity" },
    profil: "C:/temp",
    dossierExtensions: "C:/temp/extensions",
    dossierExtensionsChrome: "C:/temp/extensions-chrome",
    extensionsChrome: [],
    extensionsEpinglees: [],
    panneauExtension: null,
    chromeDisponibles: [],
    version: "0.1.0",
    historique: 0,
    zap: { ouvert: false, domaine: "", regles: [] },
  };
}

function onglet(id, fil, titre, url, extra = {}) {
  return {
    id,
    fil,
    titre,
    url,
    urlCourte: url.replace(/^https?:\/\//, "").slice(0, 40),
    domaine: url.replace(/^https?:\/\/([^/]+).*$/, "$1"),
    interne: url.startsWith("trame:"),
    favicon: null,
    actif: false,
    eveille: true,
    ancre: false,
    alire: false,
    vitalite: 0.8,
    palier: "vif",
    ...extra,
  };
}

// ------------------------------------------------------------------ pages

let navigateur = null;
let base = "";

async function page(fichier, options = {}) {
  const ongletPage = await navigateur.newPage({ viewport: { width: 1280, height: 800 } });
  ongletPage.on("pageerror", (e) => {
    // Une erreur JavaScript dans l'interface est un echec en soi : on la remonte.
    throw new Error(`erreur JS dans ${fichier} : ${e.message}`);
  });
  await ongletPage.addInitScript(() => {
    window.__ipc = [];
    // wry definit `window.ipc` ainsi : un objet avec `postMessage`.
    window.ipc = { postMessage: (brut) => window.__ipc.push(JSON.parse(brut)) };
  });
  await ongletPage.goto(`${base}/${fichier}${options.query || ""}`);
  // Certaines pages internes ne reçoivent aucun état (la page du tunnel lit son adresse dans
  // l'URL) : elles n'ont pas de `window.trame`, et l'attendre ferait echouer le cas.
  if (!options.sansEtat) {
    await ongletPage.waitForFunction(() => window.trame && typeof window.trame.etat === "function");
  }
  await ongletPage.evaluate(() => {
    window.__ipc = [];
  });
  return ongletPage;
}

async function pousser(p, etat) {
  await p.evaluate((e) => window.trame.etat(e), etat);
}

async function messages(p) {
  return p.evaluate(() => window.__ipc);
}

async function vider(p) {
  await p.evaluate(() => {
    window.__ipc = [];
  });
}

// Les reglages montrent une rubrique a la fois : on y va comme l'utilisateur, par la colonne
// de gauche, avant de cliquer ce qu'elle contient.
async function rubrique(p, nom) {
  await p.click(`#nav a[data-rubrique="${nom}"]`);
}

// Un geste de pointeur, etape par etape : `[selecteur, type, clientX, clientY]`.
// Le rangement d'un groupe se fait au pointeur (voir « ranger les groupes » dans chrome.js),
// pas au glisser-deposer HTML : les evenements sont donc synthetises ici, avec de vraies
// coordonnees tirees des rectangles.
async function geste(p, etapes) {
  await p.evaluate((liste) => {
    for (const [selecteur, type, x, y] of liste) {
      const cible = selecteur === null ? document : document.querySelector(selecteur);
      cible.dispatchEvent(
        new PointerEvent(type, {
          bubbles: true,
          cancelable: true,
          composed: true,
          pointerType: "mouse",
          button: 0,
          buttons: type === "pointerup" ? 0 : 1,
          clientX: x,
          clientY: y,
        })
      );
    }
  }, etapes);
}

// Le point de saisie d'un groupe (sa tete) et une hauteur dans une carte.
async function pointsRail(p, idSaisi) {
  return p.evaluate((id) => {
    const tete = document.querySelector(`.fil[data-fil="${id}"] .fil-tete`).getBoundingClientRect();
    const cartes = [...document.querySelectorAll("#fils .fil")].map((f) => {
      const r = f.getBoundingClientRect();
      return { id: Number(f.dataset.fil), haut: r.top, milieu: r.top + r.height / 2, bas: r.bottom };
    });
    return { x: Math.round(tete.left + 40), y: Math.round(tete.top + tete.height / 2), cartes };
  }, idSaisi);
}

// ------------------------------------------------------------------ suite

async function suite() {
  const serveur = await servir();
  base = `http://127.0.0.1:${serveur.address().port}`;
  navigateur = await playwright.chromium.launch({ headless: !VOIR });
  console.log("Tests de l'interface — ui/");

  // ---------------------------------------------------------------- rail

  await cas("rail : les fils et leurs onglets sont rendus dans l'ordre", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [
      { id: 1, nom: "Bitwarden", replie: false, compte: 2 },
      { id: 2, nom: "Home lab", replie: false, compte: 2 },
    ];
    etat.onglets = [
      onglet(10, 1, "Bitwarden", "chrome-extension://abc/popup/index.html", { actif: true }),
      onglet(11, 1, "Perplexity", "https://www.perplexity.ai/search/new?q=x"),
      onglet(12, 2, "Proxmox", "https://192.0.2.5:8006/"),
      onglet(13, 2, "OPNsense", "https://192.0.2.1:4443/"),
    ];
    await pousser(p, etat);
    const fils = await p.$$eval(".fil", (n) => n.map((x) => x.dataset.fil));
    egal(fils, ["1", "2"], "deux fils rendus");
    const noms = await p.$$eval(".fil-nom", (n) => n.map((x) => x.textContent));
    egal(noms, ["Bitwarden", "Home lab"], "noms des fils");
    const lignes = await p.$$eval(".onglet", (n) => n.length);
    egal(lignes, 4, "quatre lignes d'onglets");
    const actif = await p.$$eval(".onglet.actif .titre", (n) => n.map((x) => x.textContent));
    egal(actif, ["Bitwarden"], "un seul onglet actif");
    const comptes = await p.$$eval(".fil-compte", (n) => n.map((x) => x.textContent));
    egal(comptes, ["2", "2"], "compteurs par fil");
    await p.close();
  });

  await cas("rail : un onglet en veille le dit, un onglet ancre aussi", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil", replie: false }];
    etat.onglets = [
      onglet(1, 1, "Dort", "https://a.fr/", { eveille: false, palier: "froid" }),
      onglet(2, 1, "Ancre", "https://b.fr/", { ancre: true, alire: true }),
    ];
    await pousser(p, etat);
    const dort = await p.$eval(".onglet.endormi .titre", (n) => n.textContent);
    egal(dort, "Dort", "classe endormi");
    const marques = await p.$$eval(".onglet:nth-child(2) .marque-mini", (n) =>
      n.map((x) => x.textContent)
    );
    egal(marques, ["ancre", "à lire"], "marques ancre et à lire");
    const veille = await p.$$eval(".onglet.endormi .marque-mini", (n) => n.map((x) => x.textContent));
    egal(veille, ["veille"], "la pastille veille est sur l'onglet endormi seulement");
    await p.close();
  });

  await cas("rail : cliquer un onglet demande son activation", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil", replie: false }];
    etat.onglets = [onglet(7, 1, "Cible", "https://a.fr/")];
    await pousser(p, etat);
    await p.click(".onglet .texte");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "activer" && m.id === 7),
      `attendu activer/7, recu ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("rail : le bouton fermer d'un onglet envoie fermer, sans activer", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil", replie: false }];
    etat.onglets = [onglet(8, 1, "A fermer", "https://a.fr/")];
    await pousser(p, etat);
    await p.evaluate(() => document.querySelector(".onglet .fermer").click());
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "fermer" && m.id === 8),
      "fermer doit partir"
    );
    verifier(
      !recus.some((m) => m.m === "activer"),
      "fermer ne doit pas activer l'onglet"
    );
    await p.close();
  });

  await cas("rail : un fil replie montre son compte mais pas ses onglets", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Plie", replie: true }];
    etat.onglets = [onglet(1, 1, "Cache", "https://a.fr/"), onglet(2, 1, "Cache 2", "https://b.fr/")];
    await pousser(p, etat);
    egal(await p.$$eval(".onglet", (n) => n.length), 0, "aucune ligne visible");
    egal(await p.$eval(".fil-compte", (n) => n.textContent), "2", "le compte reste");
    await p.click(".fil .chev");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "filReplier" && m.id === 1 && m.replie === false),
      "le chevron doit demander le depliage"
    );
    await p.close();
  });

  await cas("rail : le budget affiche les chiffres reels et la jauge", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.budget = { reveilles: 3, endormis: 23, mo: 660.7, processus: 10, plafond: 6 };
    await pousser(p, etat);
    const txt = await p.$eval("#budget-txt", (n) => n.textContent);
    egal(txt, "3 éveillé(s) · 23 en veille · 661 Mo", "texte du budget");
    const largeur = await p.$eval("#budget-jauge", (n) => n.style.width);
    egal(largeur, "50%", "3 sur 6 = 50 %");
    await p.close();
  });

  await cas("rail : la suggestion de fusion nomme les deux fils", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.suggestion = { a: 1, b: 2, nomA: "Bitwarden", nomB: "Bitwarden Chrome" };
    await pousser(p, etat);
    const txt = await p.$eval("#suggestion-txt", (n) => n.textContent);
    verifier(txt.includes("Bitwarden") && txt.includes("Bitwarden Chrome"), `texte: ${txt}`);
    await p.click("#suggestion-oui");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "fusionner" && m.a === 1 && m.b === 2),
      "fusionner a/b attendu"
    );
    await p.close();
  });

  await cas("rail : un titre hostile reste du texte, il n'est pas execute", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "<img src=x onerror=window.__xss=1>", replie: false }];
    etat.onglets = [
      onglet(1, 1, "<img src=x onerror=window.__xss=1>", "https://a.fr/"),
      onglet(2, 1, "« guillemets » & <balises>", "https://b.fr/"),
    ];
    await pousser(p, etat);
    const images = await p.$$eval(".onglet img", (n) => n.length);
    egal(images, 0, "aucune image injectee");
    const xss = await p.evaluate(() => window.__xss);
    egal(xss, undefined, "aucun script execute");
    const titre = await p.$eval(".onglet .titre", (n) => n.textContent);
    egal(titre, "<img src=x onerror=window.__xss=1>", "le titre est affiche tel quel");
    const nomFil = await p.$eval(".fil-nom", (n) => n.textContent);
    egal(nomFil, "<img src=x onerror=window.__xss=1>", "le nom de fil aussi");
    await p.close();
  });

  await cas("rail : un double-clic sur le nom d'un groupe le renomme", async () => {
    // Vrai geste souris : la tete capture le pointeur (rangement), ce qui detournait le dblclick.
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Ancien", replie: false }];
    etat.onglets = [onglet(1, 1, "Page", "https://a.fr/")];
    await pousser(p, etat);
    await p.dblclick(".fil-nom");
    const champ = await p.$("#fils input.saisie");
    verifier(champ, "le champ de saisie doit apparaitre");
    // Une poussee de l'hote pendant la frappe ne doit pas effacer le champ.
    etat.onglets = [onglet(1, 1, "Titre change", "https://a.fr/")];
    await pousser(p, etat);
    verifier(await p.$("#fils input.saisie"), "le champ survit a une poussee");
    await p.fill("#fils input.saisie", "Nouveau");
    await p.press("#fils input.saisie", "Enter");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "filRenommer" && m.id === 1 && m.nom === "Nouveau"),
      "filRenommer doit partir avec le nouveau nom"
    );
    egal(await p.$("#fils input.saisie"), null, "le champ est parti");
    await p.close();
  });

  await cas("rail : les infobulles sont dessinees par Trame, pas par Windows", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 340, height: 700 });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Occt Download", replie: false }];
    etat.onglets = [onglet(1, 1, "Page", "https://a.fr/")];
    await pousser(p, etat);
    await p.hover(".fil-nom");
    await p.waitForSelector(".trame-bulle.visible", { timeout: 2000 });
    egal(await p.$eval(".trame-bulle .b-geste", (n) => n.textContent), "Double-clic", "le geste en pastille");
    egal(await p.$eval(".trame-bulle .b-titre", (n) => n.textContent), "Renommer", "l'action en clair");
    egal(await p.$eval(".fil-nom", (n) => n.getAttribute("title")), null, "plus de bulle native");
    const r = await p.$eval(".trame-bulle", (n) => n.getBoundingClientRect().toJSON());
    verifier(r.left >= 8 && r.right <= 332, `la bulle reste dans le rail (${r.left}..${r.right})`);
    if (process.env.TRAME_CAPTURES) await p.screenshot({ path: `${process.env.TRAME_CAPTURES}/bulle-nom.png`, clip: { x: 0, y: 0, width: 340, height: 330 } });
    // Titre a explication : l'action en gras, le detail en dessous.
    await p.mouse.move(5, 690);
    await p.waitForSelector(".trame-bulle:not(.visible)");
    await p.hover(".fil-outils button");
    await p.waitForSelector(".trame-bulle.visible .b-detail", { timeout: 2000 });
    if (process.env.TRAME_CAPTURES) await p.screenshot({ path: `${process.env.TRAME_CAPTURES}/bulle-detail.png`, clip: { x: 0, y: 0, width: 340, height: 330 } });
    // Rail replie : rien n'est dessine ici (ce serait coupe), la bulle part a la barre.
    await p.mouse.move(5, 690);
    await p.setViewportSize({ width: 64, height: 700 });
    await p.hover("#btn-rail");
    await p.waitForTimeout(600);
    egal(await p.$eval(".trame-bulle", (n) => n.classList.contains("visible")), false, "rien de dessine dans le rail replie");
    await p.close();
  });

  await cas("barre : bulle dans la barre si elle y tient, sinon carte demandee a l'hote", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.barreExtensions = true;
    etat.extensions = [
      { id: "compteur", nom: "Compteur de visites", version: "1.0.0", active: true, accent: "#60a5fa",
        description: "Compte combien de fois tu es venu sur ce site.", motifs: ["*://*/*"] },
    ];
    await pousser(p, etat);
    const hBarre = await p.$eval("body", () => Math.round(document.querySelector(".barre-ligne").getBoundingClientRect().height + document.getElementById("extbar").getBoundingClientRect().height));
    await p.setViewportSize({ width: 1280, height: hBarre });
    // Puce d'extension, en bas de la barre : rien n'y tient, la carte part chez l'hote.
    await p.evaluate(() => (window.__ipc.length = 0));
    await p.hover(".ext-chip:has(.pt)");
    await p.waitForSelector(".trame-bulle.visible.carte", { timeout: 2000 });
    const demande = (await messages(p)).find((m) => m.m === "panneau" && m.open);
    verifier(demande && demande.zone && demande.zone.y > hBarre, `panneau demande sous la barre ${JSON.stringify(demande)}`);
    verifier(demande.hauteur >= 150, "hauteur bornee comme l'hote la borne");
    egal(await p.$eval(".trame-bulle .b-titre", (n) => n.textContent), "Compteur de visites", "titre sans la version");
    egal(await p.$eval(".trame-bulle .b-version", (n) => n.textContent), "v1.0.0", "version a part");
    egal(await p.$eval(".trame-bulle .b-motif", (n) => n.textContent), "*://*/*", "motif en police fixe");
    egal(await p.$eval(".trame-bulle .b-pied .b-geste", (n) => n.textContent), "Clic", "« Cliquer pour » devient un geste");
    // L'hote agrandit la barre : on simule, pour la capture.
    await p.setViewportSize({ width: 1280, height: demande.hauteur });
    if (process.env.TRAME_CAPTURES) await p.screenshot({ path: `${process.env.TRAME_CAPTURES}/bulle-extension.png`, clip: { x: 0, y: 0, width: 700, height: demande.hauteur } });
    await p.evaluate(() => (window.__ipc.length = 0));
    await p.mouse.move(640, 2);
    await p.mouse.move(1270, demande.hauteur - 2);
    await p.waitForSelector(".trame-bulle:not(.visible)");
    verifier((await messages(p)).some((m) => m.m === "panneau" && m.open === false), "la carte est rendue a l'hote");
    await p.close();
  });

  await cas("rail replie : la bulle part a la barre, qui la pose a cote de l'icone", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 64, height: 700 });
    await pousser(r, etatDeBase());
    await vider(r);
    await r.hover("#btn-rail");
    await r.waitForFunction(() => window.__ipc.some((m) => m.m === "bulle" && m.texte), null, { timeout: 2000 });
    const envoi = (await messages(r)).find((m) => m.m === "bulle" && m.texte);
    verifier(/Déplier/.test(envoi.texte) && typeof envoi.y === "number", `texte et hauteur envoyes ${JSON.stringify(envoi)}`);
    egal(await r.$eval("#btn-rail", (n) => n.getAttribute("title")), null, "plus de bulle native");
    await r.mouse.move(32, 690);
    await r.waitForFunction(() => window.__ipc.some((m) => m.m === "bulle" && !m.texte), null, { timeout: 2000 });
    await r.close();

    const b = await page("chrome.html", { query: "?surface=barre" });
    await pousser(b, etatDeBase());
    await b.setViewportSize({ width: 1200, height: 45 });
    await vider(b);
    await b.evaluate(() => window.trame.bulle({ texte: "Youtube\nhttps://www.youtube.com/\nClic : nouvel onglet · clic droit : modifier · glisser : ranger", y: 160 }));
    await b.waitForSelector(".trame-bulle.visible.carte", { timeout: 2000 });
    const demande = (await messages(b)).find((m) => m.m === "panneau" && m.open);
    verifier(demande && demande.zone.y === 160 && demande.zone.x === 6, `carte a cote de l'icone ${JSON.stringify(demande)}`);
    egal(await b.$$eval(".trame-bulle .b-pied .b-geste", (l) => l.map((n) => n.textContent).join(",")), "Clic,Clic droit,Glisser", "trois gestes en pied");
    await b.setViewportSize({ width: 1200, height: demande.hauteur });
    if (process.env.TRAME_CAPTURES) await b.screenshot({ path: `${process.env.TRAME_CAPTURES}/bulle-rail-replie.png`, clip: { x: 0, y: 140, width: 420, height: demande.hauteur - 140 } });
    await vider(b);
    await b.evaluate(() => window.trame.bulle({}));
    await b.waitForSelector(".trame-bulle:not(.visible)");
    verifier((await messages(b)).some((m) => m.m === "panneau" && m.open === false), "la carte est rendue");
    await b.close();
  });

  await cas("reglages : les infobulles des pages internes sont celles de Trame", async () => {
    const p = await page("reglages.html");
    await p.waitForFunction(() => !!window.__trameBulles);
    // Les title des reglages vivent dans des rubriques fermees : un bouton pose suffit a
    // prouver que la page dessine ses bulles.
    await p.evaluate(() => {
      const b = document.createElement("button");
      b.id = "essai-bulle";
      b.textContent = "essai";
      b.title = "Vider le cache : les pages se rechargeront depuis le réseau";
      b.style.cssText = "position:fixed;left:40px;top:40px;z-index:99";
      document.body.appendChild(b);
    });
    const avecTitre = p.locator("#essai-bulle");
    await avecTitre.hover();
    await p.waitForSelector(".trame-bulle.visible", { timeout: 2000 });
    egal(await avecTitre.evaluate((n) => n.getAttribute("title")), null, "plus de bulle native");
    await p.close();
  });

  await cas("rail : tout clic dans le rail ferme le panneau de la barre", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await pousser(p, etatDeBase());
    await p.click("#btn-nouvel");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "clicPage"),
      "clicPage doit partir pour fermer le panneau"
    );
    verifier(
      recus.some((m) => m.m === "nouvelOnglet"),
      "nouvelOnglet doit partir"
    );
    await p.close();
  });

  // ---------------------------------------------------------------- barre

  await cas("barre : l'adresse, les boutons et le compteur d'extensions suivent l'etat", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.url = "https://exemple.fr/page";
    etat.retourPossible = true;
    etat.avantPossible = false;
    etat.extensions = [{ id: "a", nom: "A", version: "1.0.0", active: true, motifs: [], accent: null }];
    await pousser(p, etat);
    egal(await p.$eval("#couture", (n) => n.value), "https://exemple.fr/page", "adresse affichee");
    egal(await p.$eval("#btn-retour", (n) => n.disabled), false, "retour actif");
    egal(await p.$eval("#btn-avant", (n) => n.disabled), true, "avant desactive");
    // Comme dans Chrome, le puzzle ne porte pas de compteur : son badge ne sert qu'a signaler
    // une installation en cours.
    egal(await p.$eval("#ext-badge", (n) => n.hidden), true, "pas de badge au repos");
    egal(await p.$eval("#moteur-nom", (n) => n.textContent), "Perplexity", "moteur courant");
    await p.close();
  });

  await cas("barre : taper une requete interroge l'hote", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.click("#couture");
    await p.type("#couture", "youtube", { delay: 20 });
    await p.waitForTimeout(250);
    const recus = await messages(p);
    const requete = recus.filter((m) => m.m === "requete").pop();
    verifier(requete && requete.q === "youtube", `requete attendue : ${JSON.stringify(recus)}`);
    verifier(typeof requete.rid === "number", "la requete porte un identifiant");
    verifier(
      recus.some((m) => m.m === "panneau" && m.open === true),
      "le panneau s'ouvre a la saisie"
    );
    await p.close();
  });

  await cas("barre : les resultats se rendent et Enter ouvre le premier", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.click("#couture");
    await p.type("#couture", "yt", { delay: 15 });
    await p.waitForTimeout(200);
    const recus = await messages(p);
    const rid = recus.filter((m) => m.m === "requete").pop().rid;
    await p.evaluate(
      (r) =>
        window.trame.resultats(r),
      {
        rid,
        items: [
          { genre: "onglet", titre: "YouTube", sousTitre: "youtube.com", action: { url: "https://youtube.com" } },
          { genre: "navigation", titre: "Aller sur yt", sousTitre: "", action: { url: "https://yt" } },
        ],
      }
    );
    egal(await p.$$eval(".resultat", (n) => n.length), 2, "deux resultats");
    egal(await p.$eval(".resultat.sur .t", (n) => n.textContent), "YouTube", "premier selectionne");
    await p.press("#couture", "Enter");
    const apres = await messages(p);
    verifier(apres.some((m) => m.m === "ouvrir"), `ouvrir attendu : ${JSON.stringify(apres)}`);
    await p.close();
  });

  await cas("barre : une reponse perimee est ignoree", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.click("#couture");
    await p.type("#couture", "abc", { delay: 15 });
    await p.waitForTimeout(200);
    // Une reponse avec un rid ancien ne doit rien afficher.
    await p.evaluate(() =>
      window.trame.resultats({
        rid: -1,
        items: [{ genre: "onglet", titre: "Vieux", sousTitre: "", action: { url: "https://vieux" } }],
      })
    );
    egal(await p.$$eval(".resultat", (n) => n.length), 0, "aucun resultat perime affiche");
    await p.close();
  });

  await cas("barre : l'epingle d'extension montre son icone, ouvre la popup et marque l'etat ouvert", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.extensionsChrome = [
      { id: "nngceck", nom: "Bitwarden Password Manager", version: "2026.7.0", popup: "popup/index.html", valide: true, actif: true, icone: null },
      { id: "eimadpb", nom: "Dark Reader", version: "4.9", popup: "ui/popup.html", valide: true, actif: true, icone: PIXEL },
      { id: "coupee", nom: "Coupee", version: "1", popup: "p.html", valide: true, actif: false },
    ];
    etat.extensionsEpinglees = ["nngceck", "eimadpb", "coupee"];
    etat.panneauExtension = "nngceck";
    await pousser(p, etat);
    // Une extension desactivee n'a pas d'icone dans la barre, meme epinglee.
    egal(await p.$$eval(".pin-chip", (n) => n.length), 2, "deux icones epinglees");
    // L'icone est dans la ligne du haut, juste avant le puzzle, comme dans Chrome.
    egal(
      await p.$eval("#pin-chips", (n) => n.parentElement.classList.contains("barre-ligne") && n.nextElementSibling.id),
      "btn-ext",
      "les epingles precedent le puzzle"
    );
    // Sans image : l'initiale. Avec image : l'image servie par trame:, sans texte.
    egal(await p.$eval(".pin-chip .pastille", (n) => n.textContent), "B", "initiale de repli");
    await p.waitForTimeout(100);
    egal(await p.$$eval(".pin-chip img", (n) => n.map((i) => i.naturalWidth)), [1], "l'icone de l'extension, chargee");
    // Une icone introuvable retombe sur l'initiale au lieu d'une image cassee.
    etat.extensionsChrome[1].icone = "/ext-icone/actives/eimadpb/absente.png";
    await pousser(p, etat);
    await p.waitForTimeout(200);
    egal(await p.$$eval(".pin-chip .pastille", (n) => n.map((x) => x.textContent)), ["B", "D"], "repli sur l'initiale");
    egal(await p.$eval(".pin-chip", (n) => n.classList.contains("on")), true, "marquee ouverte");
    await p.evaluate(() => document.querySelector(".pin-chip").dispatchEvent(new PointerEvent("pointerdown", { bubbles: true })));
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "chromePopup" && m.id === "nngceck"),
      `chromePopup attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("barre : le puzzle liste les extensions, epingle, bascule, ouvre", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.extensionsChrome = [
      { id: "aaa", nom: "Alpha", version: "1", popup: "popup.html", valide: true, actif: true, chargee: true },
      { id: "bbb", nom: "Beta", version: "1", popup: null, valide: true, actif: false, chargee: false },
      { id: "ccc", nom: "Gamma", version: "1", popup: "p.html", valide: true, actif: true, chargee: false },
    ];
    etat.extensionsEpinglees = ["aaa"];
    etat.installations = ["ddkjiahe"];
    await pousser(p, etat);
    egal(await p.$eval("#ext-badge", (n) => n.hidden), false, "badge pendant une installation");
    await p.click("#btn-ext");
    egal(await p.$$eval("#extensions .menu-ligne:not(.installation) .menu-principal:not(.statique)", (n) => n.length), 3, "trois extensions Chrome");
    egal(await p.$$eval("#extensions .menu-ligne.installation", (n) => n.length), 1, "une installation en cours");
    egal(
      await p.$$eval("#extensions .menu-principal .d", (n) => n.slice(0, 3).map((x) => x.textContent)),
      ["Active", "Désactivée", "Pas encore chargée — redémarrez Trame"],
      "l'etat reel de chaque extension"
    );
    egal(await p.$$eval("#extensions .menu-epingle", (n) => n.map((b) => b.classList.contains("on"))), [true, false], "epingle seulement si active");
    const appuyer = (selecteur, index) =>
      p.evaluate(
        ([s, i]) => document.querySelectorAll(s)[i].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 })),
        [selecteur, index]
      );
    await vider(p);
    await appuyer("#extensions .menu-epingle", 1);
    await appuyer("#extensions .menu-ligne .bascule", 1);
    await appuyer("#extensions .menu-ligne .bascule", 0);
    let recus = await messages(p);
    verifier(recus.some((m) => m.m === "chromeEpingler" && m.id === "ccc"), `chromeEpingler ccc : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.m === "chromeReactiver" && m.id === "bbb"), `chromeReactiver bbb : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.m === "chromeDesactiver" && m.id === "aaa"), `chromeDesactiver aaa : ${JSON.stringify(recus)}`);
    egal(await p.$eval("#panneau", (n) => n.hidden), false, "epingler et basculer laissent le menu ouvert");
    // Une poussee identique ne reconstruit pas le menu (un clic n'est jamais perdu).
    const avant = await p.$eval("#extensions .menu-ligne", (n) => { n.dataset.marque = "1"; return true; });
    await pousser(p, etat);
    egal(await p.$eval("#extensions .menu-ligne", (n) => n.dataset.marque || ""), "1", "pas de reconstruction inutile");
    // Clic sur l'extension : sa fenetre d'action, et le menu se referme.
    await vider(p);
    await appuyer("#extensions .menu-principal", 0);
    recus = await messages(p);
    verifier(recus.some((m) => m.m === "chromePopup" && m.id === "aaa"), `chromePopup aaa : ${JSON.stringify(recus)}`);
    egal(await p.$eval("#panneau", (n) => n.hidden), true, "ouvrir referme le menu");
    void avant;
    await p.close();
  });

  await cas("barre : dans un fil cloisonne, le puzzle et les epingles sont ceux du fil", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.fils = [{ id: 7, nom: "Comptes", replie: false, auto: false, onglets: [], cloison: { extensions: ["bw"] } }];
    etat.filActif = 7;
    etat.extensionsChrome = [
      { id: "bw", nom: "Bitwarden", version: "1", popup: "popup.html", valide: true, actif: false, icone: PIXEL },
      { id: "ub", nom: "uBlock", version: "1", popup: "popup.html", valide: true, actif: true },
    ];
    etat.extensionsEpinglees = ["bw", "ub"];
    await pousser(p, etat);
    // Bitwarden est coupee dans le profil principal, mais voulue dans ce fil : son icone y est.
    egal(await p.$$eval(".pin-chip", (n) => n.length), 1, "une seule epingle : celle du fil");
    await p.click("#btn-ext");
    verifier((await p.$eval(".menu-titre", (n) => n.textContent)).includes("Comptes"), "le menu nomme le fil");
    egal(
      await p.$$eval("#extensions .menu-principal .d", (n) => n.slice(0, 2).map((x) => x.textContent)),
      ["Active dans ce fil", "Active ailleurs, pas ici"],
      "l'etat se lit par rapport au fil"
    );
    await vider(p);
    await p.evaluate(() =>
      document.querySelectorAll("#extensions .menu-ligne .bascule")[1]
        .dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }))
    );
    let recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "cloisonExtension" && m.fil === 7 && m.id === "ub" && m.actif === true),
      `cloisonExtension attendu : ${JSON.stringify(recus)}`
    );
    // Decloisonner depuis le menu.
    await vider(p);
    await p.evaluate(() =>
      [...document.querySelectorAll("#extensions .menu-bouton")]
        .find((b) => b.textContent.includes("Décloisonner"))
        .dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }))
    );
    recus = await messages(p);
    verifier(recus.some((m) => m.m === "filCloisonner" && m.fil === 7 && m.actif === false), `filCloisonner : ${JSON.stringify(recus)}`);
    await p.close();
  });

  await cas("rail : un fil se cloisonne d'un clic et porte sa marque", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 4, nom: "Comptes", replie: false, compte: 1 }];
    etat.onglets = [onglet(40, 4, "Banque", "https://banque.fr/", { actif: true })];
    await pousser(p, etat);
    const id = etat.fils[0].id;
    egal(await p.$$eval(".fil.cloisonne", (n) => n.length), 0, "aucun fil cloisonne au depart");
    await vider(p);
    await p.evaluate(() =>
      [...document.querySelectorAll(".fil-outils button")].find((b) => b.title.startsWith("Cloisonner")).click()
    );
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "filCloisonner" && m.fil === id && m.actif === true), `filCloisonner : ${JSON.stringify(recus)}`);
    etat.fils[0].cloison = { extensions: [] };
    await pousser(p, etat);
    egal(await p.$$eval(".fil.cloisonne .fil-outils button.on", (n) => n.length), 1, "le fil porte le bouclier allume");
    await p.close();
  });

  await cas("barre : le puzzle dit ce que coutent les extensions", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.budget = { ...etat.budget, extensionsMo: 184.2, extensionsProcessus: 1 };
    etat.extensionsChrome = [
      { id: "bw", nom: "Bitwarden", version: "1", popup: "p.html", valide: true, actif: true, coutMo: 184.2, coutDirect: true },
      { id: "dr", nom: "Dark Reader", version: "1", popup: "p.html", valide: true, actif: false, coutMo: 52, coutDirect: false },
      { id: "ub", nom: "uBlock", version: "1", popup: "p.html", valide: true, actif: true },
    ];
    await pousser(p, etat);
    await p.click("#btn-ext");
    verifier((await p.$eval(".menu-cout", (n) => n.textContent)).includes("184 Mo (1 processus)"), "total des extensions");
    egal(
      await p.$$eval("#extensions .menu-principal .d", (n) => n.slice(0, 3).map((x) => x.textContent)),
      ["Active · 184 Mo", "Désactivée · coûtait 52 Mo (mesuré à l'allumage)", "Active"],
      "le cout de chaque extension, et d'ou il vient"
    );
    await p.close();
  });

  await cas("rail : une memoire libre faible se voit au pied du rail", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.budget = { reveilles: 3, endormis: 8, mo: 700, processus: 9, plafond: 3, plafondChoisi: 6, auto: true, dispoMo: 1200, extensionsMo: 0, extensionsProcessus: 0 };
    await pousser(p, etat);
    const txt = await p.$eval("#budget-txt", (n) => ({ t: n.textContent, titre: n.title, limite: n.classList.contains("limite") }));
    verifier(txt.t.includes("mémoire juste") && txt.limite, `pied du rail : ${JSON.stringify(txt)}`);
    verifier(txt.titre.includes("3 onglet(s) ouvert(s) au plus au lieu de 6"), `infobulle : ${txt.titre}`);
    await p.close();
  });

  await cas("barre : barre masquee, les epingles restent dans la ligne du haut", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.barreExtensions = false;
    etat.extensionsChrome = [{ id: "x", nom: "X", version: "1", popup: "p.html", valide: true, actif: true }];
    etat.extensionsEpinglees = ["x"];
    await pousser(p, etat);
    egal(await p.$eval("#extbar", (n) => n.hidden), true, "barre d'extensions masquee");
    egal(await p.$eval("#pin-chips", (n) => n.hidden), false, "l'epingle reste visible");
    await p.close();
  });

  await cas("barre : une puce d'extension Trame bascule son etat", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.extensions = [
      { id: "temps-lecture", nom: "Temps de lecture", version: "1.0.0", active: true, motifs: [], accent: null },
      { id: "nuit-douce", nom: "Nuit douce", version: "1.1.0", active: false, motifs: [], accent: "#333" },
    ];
    await pousser(p, etat);
    // `#ext-chips` seulement : le bouton « + » porte aussi la classe `ext-chip`.
    egal(await p.$$eval("#ext-chips .ext-chip", (n) => n.length), 2, "deux puces");
    const etats = await p.$$eval("#ext-chips .ext-chip", (n) => n.map((x) => x.classList.contains("off")));
    egal(etats, [false, true], "la seconde est eteinte");
    await p.evaluate(() =>
      document
        .querySelectorAll("#ext-chips .ext-chip")[0]
        .dispatchEvent(new PointerEvent("pointerdown", { bubbles: true }))
    );
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "extensionBasculer" && m.id === "temps-lecture" && m.actif === false),
      `extensionBasculer attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("barre : les boutons emettent les commandes attendues", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    for (const [selecteur, message] of [
      ["#btn-recharger", "recharger"],
      ["#btn-zap", "zap"],
      ["#btn-reglages", "ouvrirReglages"],
      ["#btn-moteur", "ouvrirReglages"],
      ["#btn-dossier-ext", "dossierExtensions"],
    ]) {
      await vider(p);
      await p.click(selecteur);
      const recus = await messages(p);
      verifier(
        recus.some((m) => m.m === message),
        `${selecteur} devrait envoyer ${message}, recu ${JSON.stringify(recus)}`
      );
    }
    await p.close();
  });

  await cas("barre : le bouton du panneau ou de la page ouverte est allume", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const allumes = () => p.evaluate(() => [...document.querySelectorAll(".nav.ouvert")].map((b) => b.id));
    await pousser(p, etatDeBase());
    verifier((await allumes()).length === 0, `aucun bouton ne devrait etre allume, vu ${await allumes()}`);
    await pousser(p, { ...etatDeBase(), zap: { ouvert: true, regles: [] } });
    verifier(JSON.stringify(await allumes()) === '["btn-zap"]', `zap ouvert : vu ${await allumes()}`);
    await pousser(p, { ...etatDeBase(), urlBrute: "http://trame.localhost/reglages.html" });
    verifier(JSON.stringify(await allumes()) === '["btn-reglages"]', `reglages : vu ${await allumes()}`);
    await pousser(p, etatDeBase());
    await p.click("#btn-ext");
    await p.waitForTimeout(50);
    verifier(JSON.stringify(await allumes()) === '["btn-ext"]', `puzzle ouvert : vu ${await allumes()}`);
    await p.click("#btn-ext");
    await p.waitForTimeout(50);
    verifier((await allumes()).length === 0, `puzzle referme : vu ${await allumes()}`);
    await p.close();
  });

  await cas("barre : un telechargement fini laisse un point vert jusqu'a la page des telechargements", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await p.evaluate(() => localStorage.clear());
    const nonvu = () => p.evaluate(() => document.getElementById("btn-dl").classList.contains("nonvu"));
    const dl = (n) => ({ actifs: 0, liste: [], dernierTermine: n });
    await pousser(p, { ...etatDeBase(), telechargements: dl(100) });
    verifier(!(await nonvu()), "premier lancement : pas de point");
    await pousser(p, { ...etatDeBase(), telechargements: dl(200) });
    verifier(await nonvu(), "fichier arrive : point vert attendu");
    await pousser(p, { ...etatDeBase(), telechargements: dl(200), urlBrute: "trame://localhost/telechargements.html" });
    verifier(!(await nonvu()), "page des telechargements ouverte : le point s'efface");
    await pousser(p, { ...etatDeBase(), telechargements: dl(200) });
    verifier(!(await nonvu()), "deja vu : pas de point");
    await p.close();
  });

  await cas("barre : apres un clic dans la page, l'adresse suit la navigation", async () => {
    // Cas reel du 2026-11-21 : champ d'adresse actif, clic dans la page Amazon (autre webview),
    // connexion -> la page change, la barre montrait encore l'adresse de connexion.
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.url = "amazon.fr/ap/signin?openid=x";
    await pousser(p, etat);
    await p.click("#couture");
    // La barre perd le focus (clic dans la page) : c'est un `blur` de SA fenetre.
    await p.evaluate(() => window.dispatchEvent(new Event("blur")));
    etat.url = "amazon.fr/";
    await pousser(p, etat);
    egal(await p.$eval("#couture", (n) => n.value), "amazon.fr/", "l'adresse reelle s'affiche");
    // Et pendant une vraie saisie, rien n'est ecrase.
    await p.click("#couture");
    await p.fill("#couture", "ma recherche");
    etat.url = "amazon.fr/autre";
    await pousser(p, etat);
    egal(await p.$eval("#couture", (n) => n.value), "ma recherche", "la saisie en cours est respectee");
    await p.close();
  });

  await cas("barre : entrer une adresse la navigue", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.click("#couture");
    await p.fill("#couture", "exemple.fr/x");
    await p.press("#couture", "Enter");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "naviguer" && m.url === "exemple.fr/x"),
      `naviguer attendu : ${JSON.stringify(recus)}`
    );
    egal(await p.$eval("#panneau", (n) => n.hidden), true, "le panneau se referme");
    await p.close();
  });

  // ---------------------------------------------------------------- reglages

  await cas("reglages : l'interrupteur de memoire basse suit l'etat et l'inverse", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), memoireBasse: true };
    await pousser(p, etat);
    await rubrique(p, "onglets");
    egal(await p.$eval("#bascule-memoire-basse", (n) => n.classList.contains("on")), true, "allume");
    await p.click("#bascule-memoire-basse");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "memoireBasse" && m.actif === false),
      `memoireBasse attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("reglages : l'interrupteur du budget selon la memoire libre", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), budgetAuto: true };
    await pousser(p, etat);
    await rubrique(p, "onglets");
    egal(await p.$eval("#bascule-budget-auto", (n) => n.classList.contains("on")), true, "allume");
    await p.click("#bascule-budget-auto");
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "budgetAuto" && m.actif === false), `budgetAuto : ${JSON.stringify(recus)}`);
    await p.close();
  });

  await cas("reglages : choisir un moteur l'annonce a l'hote", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.moteurs = [
      { id: "perplexity", nom: "Perplexity", langue: "en" },
      { id: "qwant", nom: "Qwant", langue: "fr" },
    ];
    await pousser(p, etat);
    // Les moteurs sont des `div.moteur` (pas des boutons) : tout le bloc est cliquable.
    egal(await p.$$eval(".moteur", (n) => n.length), 2, "deux moteurs rendus");
    egal(
      await p.$eval(".moteur.choisi .nom", (n) => n.textContent),
      "Perplexity",
      "le moteur courant est marque"
    );
    await p.evaluate(() => {
      const cible = [...document.querySelectorAll(".moteur")].find((el) =>
        el.textContent.includes("Qwant")
      );
      cible.click();
    });
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "moteur" && m.id === "qwant"),
      `moteur attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("reglages : epingler une extension Chrome envoie son identifiant", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.extensionsChrome = [
      { id: "nngceck", nom: "Bitwarden", version: "2026.7.0", popup: "popup/index.html", valide: true, actif: true },
    ];
    await pousser(p, etat);
    await p.evaluate(() => {
      const boutons = [...document.querySelectorAll("button")];
      boutons.find((b) => b.textContent.trim() === "Épingler").click();
    });
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "chromeEpingler" && m.id === "nngceck"),
      `chromeEpingler attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  await cas("reglages : l'installateur envoie l'identifiant saisi", async () => {
    const p = await page("reglages.html");
    await pousser(p, etatDeBase());
    await rubrique(p, "extensions");
    // Champ vide : rien ne part, et l'interface le dit.
    await p.click("#btn-chrome-installer");
    // (La page demande a l'ouverture si Trame est le navigateur par defaut : simple lecture.)
    egal((await messages(p)).filter((m) => !["etatParDefaut", "lecturesCompter"].includes(m.m)).length, 0, "aucun message pour un champ vide");
    verifier(
      (await p.$eval("#chrome-etat", (n) => n.textContent)).includes("Collez"),
      "l'interface doit demander une saisie"
    );
    // Saisie + Entree : le message part avec le texte brut.
    await p.fill("#chrome-saisie", "nngceckbapebfimnlniiiahkandclblb");
    await p.press("#chrome-saisie", "Enter");
    const recus = await messages(p);
    const message = recus.find((m) => m.m === "chromeInstallerMagasin");
    verifier(message, `chromeInstallerMagasin attendu : ${JSON.stringify(recus)}`);
    egal(message.saisie, "nngceckbapebfimnlniiiahkandclblb", "saisie transmise");
    // L'etat affiche le telechargement en cours.
    verifier(
      (await p.$eval("#chrome-etat", (n) => n.textContent)).includes("Téléchargement"),
      "l'interface doit annoncer le telechargement"
    );
    await p.close();
  });

  await cas("reglages : le bouton fichier demande le selecteur", async () => {
    const p = await page("reglages.html");
    await pousser(p, etatDeBase());
    await rubrique(p, "extensions");
    await p.click("#btn-chrome-fichier");
    const recus = await messages(p);
    const message = recus.find((m) => m.m === "chromeInstallerFichier");
    verifier(message, `chromeInstallerFichier attendu : ${JSON.stringify(recus)}`);
    egal(message.chemin, undefined, "aucun chemin : c'est l'hote qui ouvre le selecteur");
    await p.close();
  });

  await cas("reglages : une extension installee se gere (activer, retirer, mettre a jour)", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.extensionsChrome = [
      {
        id: "abc",
        nom: "Active",
        version: "1.0",
        valide: true,
        actif: true,
        popup: "p.html",
        options: "o.html",
        permissions: ["stockage local", "toutes les pages"],
        tailleKo: 2500,
        source: "magasin",
        chemin: "C:/x/abc",
      },
      {
        id: "def",
        nom: "Coupee",
        version: "2.0",
        valide: true,
        actif: false,
        popup: null,
        options: null,
        permissions: [],
        tailleKo: 100,
        source: "fichier",
        chemin: "C:/x/def",
      },
    ];
    await pousser(p, etat);
    egal(await p.$$eval(".ext", (n) => n.length), 2, "deux extensions listees");
    // Les permissions et la provenance sont visibles.
    const textes = await p.$$eval(".ext", (n) => n.map((x) => x.textContent));
    verifier(textes[0].includes("magasin"), "la provenance magasin doit etre affichee");
    verifier(textes[0].includes("stockage local"), "les permissions doivent etre affichees");
    verifier(textes[0].includes("2 Mo"), "la taille doit etre affichee en Mo");
    verifier(textes[1].includes("désactivée"), "l'extension coupee doit etre signalee");

    // Les commandes de gestion.
    const cliquer = async (texte) => {
      await vider(p);
      await p.evaluate((t) => {
        const b = [...document.querySelectorAll(".ext button")].find((x) => x.textContent.trim() === t);
        if (!b) throw new Error("bouton introuvable : " + t);
        b.click();
      }, texte);
      return messages(p);
    };
    verifier(
      (await cliquer("Désactiver")).some((m) => m.m === "chromeDesactiver" && m.id === "abc"),
      "Desactiver doit partir pour l'extension active"
    );
    verifier(
      (await cliquer("Activer")).some((m) => m.m === "chromeReactiver" && m.id === "def"),
      "Activer doit partir pour l'extension coupee"
    );
    verifier(
      (await cliquer("Mettre à jour")).some((m) => m.m === "chromeMettreAJour" && m.id === "abc"),
      "Mettre a jour doit partir"
    );
    verifier(
      (await cliquer("Retirer")).some((m) => m.m === "chromeDesinstaller" && m.id === "abc"),
      "Retirer doit partir"
    );
    verifier(
      (await cliquer("Options")).some((m) => m.m === "chromeOuvrir" && m.page === "o.html"),
      "Options doit ouvrir la page d'options"
    );
    await p.close();
  });

  // ---------------------------------------------------------------- accueil

  await cas("accueil : il salue et affiche la version", async () => {
    const p = await page("newtab.html");
    const etat = etatDeBase();
    etat.version = "0.1.0-test";
    await pousser(p, etat);
    const salut = await p.$eval("#salut", (n) => n.textContent);
    verifier(/bonjour|bonsoir|après-midi|Encore debout/i.test(salut), `salutation inattendue: ${salut}`);
    await p.close();
  });

  await cas("accueil : les accès rapides s'affichent, se filtrent et s'ouvrent", async () => {
    const p = await page("newtab.html");
    const etat = etatDeBase();
    etat.carnet = Object.assign({}, etat.carnet, {
      acces: [
        { url: "https://www.youtube.com/", titre: "YouTube", domaine: "youtube.com", groupe: "Loisirs", rail: true, icone: null, ouvert: 4 },
        { url: "http://192.0.2.23:8720/", titre: "Home lab", domaine: "192.0.2.23", groupe: "Maison", rail: false, icone: null, ouvert: null },
      ],
      accesGroupes: ["Maison", "Loisirs"],
    });
    await pousser(p, etat);
    egal(await p.$$eval("#tuiles .tuile:not(.ajout)", (n) => n.length), 2, "deux tuiles");
    egal(await p.$$eval("#tuiles .ouvert", (n) => n.length), 1, "un seul déjà ouvert");
    egal(await p.$$eval("#groupes .g", (n) => n.map((b) => b.textContent).join("|")), "Tous|Maison|Loisirs|＋ groupe", "groupes");
    await p.click("#groupes .g:nth-child(2)");
    egal(await p.$$eval("#tuiles .tuile:not(.ajout) .n", (n) => n.map((x) => x.textContent).join("|")), "Home lab", "filtre Maison");
    await vider(p);
    await p.click("#tuiles .tuile:not(.ajout)");
    verifier((await messages(p)).some((m) => m.m === "accesOuvrir" && m.url === "http://192.0.2.23:8720/"), "le clic ouvre l'accès");
    // Ajouter : la fenêtre envoie accesEnregistrer avec le groupe filtré.
    await p.click("#tuiles .tuile.ajout");
    await p.fill(".dialogue input >> nth=0", "amazon.fr");
    await vider(p);
    await p.click(".dialogue .ok");
    const envoi = (await messages(p)).find((m) => m.m === "accesEnregistrer");
    verifier(envoi && envoi.url === "amazon.fr" && envoi.groupe === "Maison", `ajout inattendu: ${JSON.stringify(envoi)}`);
    egal(await p.$$eval(".voile", (n) => n.length), 0, "la fenêtre se ferme");
    await p.close();
  });

  await cas("apparence : style du rail replié, accès et groupes choisis séparément", async () => {
    // Réglages : une vignette par style, le choix part vers l'hôte.
    const p = await page("reglages.html");
    await pousser(p, Object.assign(etatDeBase(), { railStyle: { acces: "cases", onglets: "etiquettes" } }));
    egal(await p.$$eval("#choix-rail-acces .option", (l) => l.map((o) => o.dataset.valeur).join(",")), "cases,socle,ronds,grille", "quatre styles d'accès");
    egal(await p.$$eval("#choix-rail-onglets .option", (l) => l.map((o) => o.dataset.valeur).join(",")), "etiquettes,cartes,trait", "trois styles de groupes");
    egal(await p.$eval("#choix-rail-acces .option.choisi", (o) => o.dataset.valeur), "cases", "l'état actuel est coché");
    await vider(p);
    await p.evaluate(() => document.querySelector('#choix-rail-acces .option[data-valeur="socle"]').click());
    await p.evaluate(() => document.querySelector('#choix-rail-onglets .option[data-valeur="cartes"]').click());
    const recus = (await messages(p)).filter((m) => m.m === "railStyle");
    verifier(recus.some((m) => m.acces === "socle") && recus.some((m) => m.onglets === "cartes"), `railStyle envoyés ${JSON.stringify(recus)}`);
    if (process.env.TRAME_CAPTURES) {
      await rubrique(p, "apparence");
      await pousser(p, Object.assign(etatDeBase(), { railStyle: { acces: "socle", onglets: "cartes" } }));
      const carte = p.locator("#choix-rail-acces").locator("xpath=..");
      await carte.screenshot({ path: `${process.env.TRAME_CAPTURES}/reglages-rail.png` });
    }
    await p.close();

    // Rail replié : chaque combinaison change ce qu'elle doit, et seulement replié.
    const r = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.carnet = Object.assign({}, etat.carnet, {
      acces: [
        { url: "https://www.youtube.com/", titre: "Youtube", domaine: "youtube.com", groupe: "", rail: true, icone: null, ouvert: 4 },
        { url: "https://github.com/", titre: "GitHub", domaine: "github.com", groupe: "", rail: true, icone: null, ouvert: null },
        { url: "https://mail.google.com/", titre: "Gmail", domaine: "mail.google.com", groupe: "", rail: true, icone: null, ouvert: null },
      ],
      accesGroupes: [],
    });
    etat.fils = [
      { id: 1, nom: "Gmail Boîte", replie: false, couleur: "#8b7cf6" },
      { id: 2, nom: "Occt Download", replie: false, couleur: "#fbbf24" },
    ];
    etat.onglets = [onglet(1, 1, "Boîte", "https://mail.google.com/"), onglet(2, 1, "Boîte 2", "https://mail.google.com/b"), onglet(3, 2, "OCCT", "https://ocbase.com/")];
    etat.actif = 3;
    await r.setViewportSize({ width: 64, height: 520 });
    const mesure = () => r.evaluate(() => {
      const b = getComputedStyle(document.querySelector(".bande-acces"));
      const img = getComputedStyle(document.querySelector(".bande-acces .acces .lettre, .bande-acces .acces img"));
      const f = getComputedStyle(document.querySelector(".fil"));
      return { rayonBande: b.borderTopLeftRadius, colonnes: b.gridTemplateColumns.split(" ").length, rayonIcone: img.borderTopLeftRadius, fondFil: f.backgroundColor, traitFil: f.borderLeftWidth, nomVisible: getComputedStyle(document.querySelector(".fil-nom")).display !== "none" };
    });
    const combos = [["cases", "etiquettes"], ["socle", "cartes"], ["ronds", "trait"], ["grille", "cartes"]];
    const vus = {};
    for (const [acces, onglets] of combos) {
      await pousser(r, Object.assign({}, etat, { railStyle: { acces, onglets } }));
      egal(await r.evaluate(() => document.body.dataset.railAcces + "/" + document.body.dataset.railOnglets), `${acces}/${onglets}`, "attributs posés");
      await r.waitForTimeout(300); // .fil a une transition de 0,2 s sur ses bords
      vus[acces + "/" + onglets] = await mesure();
      if (onglets === "trait") {
        const [c, bord] = await r.evaluate(() => {
          const f = document.querySelector(".fil");
          const t = document.createElement("i");
          t.style.color = f.style.getPropertyValue("--c");
          document.body.appendChild(t);
          const attendu = getComputedStyle(t).color;
          t.remove();
          return [attendu, getComputedStyle(f).borderLeftColor];
        });
        egal(bord, c, "trait : de la couleur du groupe");
      }
      if (process.env.TRAME_CAPTURES) await r.screenshot({ path: `${process.env.TRAME_CAPTURES}/rail-${acces}-${onglets}.png` });
    }
    verifier(vus["socle/cartes"].rayonBande === "14px", `socle : plateau arrondi ${JSON.stringify(vus["socle/cartes"])}`);
    verifier(vus["ronds/trait"].rayonIcone === "50%", `ronds : icônes rondes ${JSON.stringify(vus["ronds/trait"])}`);
    egal(vus["grille/cartes"].colonnes, 2, "grille : deux colonnes");
    verifier(vus["socle/cartes"].fondFil !== vus["cases/etiquettes"].fondFil, "cartes : le groupe a un fond");
    egal(vus["ronds/trait"].traitFil, "3px", "trait : bord gauche de couleur");
    egal(vus["ronds/trait"].nomVisible, false, "trait : sans nom");
    egal(vus["cases/etiquettes"].nomVisible, true, "étiquettes : le nom se voit");
    // Déplié, rien ne change : les styles ne valent que replié.
    await r.setViewportSize({ width: 300, height: 520 });
    const deplie = (onglets) =>
      r.evaluate((o) => {
        document.body.dataset.railOnglets = o;
        return getComputedStyle(document.querySelector(".fil")).borderLeftWidth;
      }, onglets);
    egal(await deplie("trait"), await deplie("etiquettes"), "déplié : le style « trait » ne s'applique pas");
    await r.close();
  });

  await cas("rail replié : le clic droit sur un accès ouvre son menu, dessiné par la barre", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 64, height: 700 });
    const etat = etatDeBase();
    etat.carnet = Object.assign({}, etat.carnet, {
      acces: [{ url: "https://www.youtube.com/", titre: "Youtube", domaine: "youtube.com", groupe: "Loisirs", rail: true, icone: null, ouvert: null }],
      accesGroupes: [],
    });
    await pousser(r, etat);
    await vider(r);
    await r.click("#bande-acces .acces:not(.ajout):not(.libre)", { button: "right" });
    const envoi = (await messages(r)).find((m) => m.m === "menuAcces");
    verifier(envoi && envoi.acces.url === "https://www.youtube.com/" && envoi.acces.groupe === "Loisirs" && typeof envoi.y === "number", `menuAcces envoyé ${JSON.stringify(envoi)}`);
    verifier(!(await messages(r)).some((m) => m.m === "nouvelOnglet"), "plus de Nouvel onglet à la place du menu");
    await r.close();

    const b = await page("chrome.html", { query: "?surface=barre" });
    await pousser(b, etatDeBase());
    await vider(b);
    await b.evaluate(() => window.trame.menuAcces({ acces: { titre: "Youtube", url: "https://www.youtube.com/", groupe: "Loisirs" }, y: 300, h: 800 }));
    await b.waitForSelector("#menu-commandes .menu-acces-rail.dans-barre", { timeout: 2000 });
    await b.waitForFunction(() => window.__ipc.some((m) => m.m === "panneau" && m.open && m.zone && m.zone.y > 0), null, { timeout: 2000 });
    const demande = (await messages(b)).filter((m) => m.m === "panneau" && m.open && m.zone).pop();
    verifier(demande.zone.x <= 10 && Math.abs(demande.zone.y - 300) <= 2, `menu collé au rail, à la hauteur de l'icône ${JSON.stringify(demande)}`);
    await b.setViewportSize({ width: 1200, height: demande.hauteur });
    if (process.env.TRAME_CAPTURES) await b.screenshot({ path: `${process.env.TRAME_CAPTURES}/menu-acces-replie.png`, clip: { x: 0, y: 280, width: 360, height: demande.hauteur - 280 } });
    await vider(b);
    await b.click("#menu-commandes .menu-acces-rail button >> nth=0");
    const recus = await messages(b);
    verifier(recus.some((m) => m.m === "accesOuvrir" && m.url === "https://www.youtube.com/"), "« Ouvrir dans un nouvel onglet » part");
    verifier(recus.some((m) => m.m === "panneau" && m.open === false), "le menu se referme");
    // « Modifier… » : le formulaire, puis l'enregistrement avec l'ancienne adresse et le groupe.
    await b.evaluate(() => window.trame.menuAcces({ acces: { titre: "Youtube", url: "https://www.youtube.com/", groupe: "Loisirs" }, y: 300, h: 800 }));
    await b.waitForSelector("#menu-commandes .menu-acces-rail.dans-barre");
    await b.click("#menu-commandes .menu-acces-rail button >> nth=1");
    await b.fill("#menu-commandes .mar-form input >> nth=0", "YouTube");
    await vider(b);
    await b.click("#menu-commandes .mar-boutons .ok");
    const env = (await messages(b)).find((m) => m.m === "accesEnregistrer");
    verifier(env && env.titre === "YouTube" && env.ancienne === "https://www.youtube.com/" && env.groupe === "Loisirs", `enregistrement ${JSON.stringify(env)}`);
    await b.close();
  });

  await cas("rail : 10 places sous le logo, seulement les accès cochés « rail »", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.carnet = Object.assign({}, etat.carnet, {
      acces: [
        { url: "https://www.youtube.com/", titre: "YouTube", domaine: "youtube.com", groupe: "", rail: true, icone: null, ouvert: null },
        { url: "https://www.amazon.fr/", titre: "Amazon", domaine: "amazon.fr", groupe: "", rail: false, icone: null, ouvert: null },
      ],
      accesGroupes: [],
    });
    await pousser(p, etat);
    egal(await p.$$eval("#bande-acces .acces:not(.ajout):not(.libre)", (n) => n.length), 1, "une pastille");
    egal(await p.$$eval("#bande-acces .acces", (n) => n.length), 2, "la pastille et « + », sans cases vides");
    await vider(p);
    await p.click("#bande-acces .acces >> nth=0");
    verifier((await messages(p)).some((m) => m.m === "accesOuvrir" && m.nouveau === true), "le clic ouvre dans un nouvel onglet");
    // « + » : le site de la page affichée rejoint le rail.
    await vider(p);
    await p.click("#bande-acces .acces.ajout");
    verifier((await messages(p)).some((m) => m.m === "accesDepuisOnglet" && m.actif === true && m.rail === true), "« + » ajoute le site affiché");
    // Clic droit : menu, puis modification du nom.
    await p.click("#bande-acces .acces >> nth=0", { button: "right" });
    verifier(await p.$("#menu-acces-rail"), "le menu s'ouvre");
    await p.click("#menu-acces-rail button >> nth=1");
    await p.fill("#menu-acces-rail input >> nth=0", "YT");
    await vider(p);
    await p.click("#menu-acces-rail .ok");
    const envoi = (await messages(p)).find((m) => m.m === "accesEnregistrer");
    verifier(envoi && envoi.titre === "YT" && envoi.ancienne === "https://www.youtube.com/" && envoi.rail === true, `modification inattendue: ${JSON.stringify(envoi)}`);
    verifier(!(await p.$("#menu-acces-rail")), "le menu se referme");
    // Sans accès au rail : la bande reste là, avec « + ».
    etat.carnet.acces[0].rail = false;
    await pousser(p, etat);
    verifier(await p.$eval("#bande-acces", (n) => !n.hidden), "bande toujours visible");
    egal(await p.$$eval("#bande-acces .acces.ajout", (n) => n.length), 1, "« + » présent");
    await p.close();
  });

  await cas("accueil : les favoris (pages) sont à part des accès rapides (sites)", async () => {
    const p = await page("newtab.html");
    const etat = etatDeBase();
    await pousser(p, etat);
    verifier(await p.$eval("#bloc-favoris", (n) => n.hidden), "pas de section sans favori");
    etat.carnet = Object.assign({}, etat.carnet, {
      favoris: [{ url: "https://www.youtube.com/watch?v=abc", titre: "Une vidéo", domaine: "youtube.com", icone: null }],
    });
    await pousser(p, etat);
    verifier(await p.$eval("#bloc-favoris", (n) => !n.hidden), "la section Favoris apparaît");
    egal(await p.$$eval("#favoris .fav", (n) => n.length), 1, "un favori");
    egal(await p.$$eval("#tuiles .tuile:not(.ajout)", (n) => n.length), 0, "aucun accès rapide pour autant");
    await vider(p);
    await p.click("#favoris .fav");
    verifier((await messages(p)).some((m) => m.m === "favoriOuvrir" && m.url === "https://www.youtube.com/watch?v=abc"), "le clic ouvre la page dans un nouvel onglet");
    await vider(p);
    await p.hover("#favoris .fav");
    await p.click("#favoris .fav .x");
    verifier((await messages(p)).some((m) => m.m === "favoriRetirer"), "× retire le favori");
    await p.close();
  });

  await cas("accueil : dossiers de favoris, et « Modifier… » un favori", async () => {
    const p = await page("newtab.html");
    await p.evaluate(() => localStorage.removeItem("trame.favoris.dossier"));
    const etat = etatDeBase();
    etat.carnet = Object.assign({}, etat.carnet, {
      favoris: [
        { url: "https://a.fr/tarte", titre: "Tarte", domaine: "a.fr", icone: null, dossier: "Cuisine" },
        { url: "https://b.fr/doc", titre: "Doc", domaine: "b.fr", icone: null, dossier: "" },
      ],
      favorisDossiers: ["Cuisine", "Vide"],
    });
    await pousser(p, etat);
    const dossiers = await p.$$eval("#dossiers-favoris .g", (n) => n.map((b) => b.textContent));
    egal(JSON.stringify(dossiers), JSON.stringify(["Tous", "Cuisine", "Vide", "＋ dossier"]), "Tous, chaque dossier, puis « + dossier »");
    egal(await p.$$eval("#favoris .fav", (n) => n.length), 2, "Tous : les deux favoris");
    await p.click("#dossiers-favoris .g:nth-child(2)");
    egal(await p.$$eval("#favoris .fav", (n) => n.length), 1, "Cuisine : un favori");
    await p.click("#dossiers-favoris .g:nth-child(3)");
    egal(await p.$$eval("#favoris .fav", (n) => n.length), 0, "Vide : aucun favori");
    verifier(await p.$eval("#favoris", (n) => n.textContent.includes("Aucun favori dans ce dossier")), "un dossier vide le dit");
    await p.click("#dossiers-favoris .g:nth-child(1)");
    // Clic droit sur un favori → Modifier… → nom, adresse et dossier envoyés ensemble.
    await vider(p);
    await p.click("#favoris .fav >> nth=1", { button: "right" });
    const lignes = await p.$$eval(".menu button", (n) => n.map((b) => b.textContent.trim()));
    verifier(lignes.includes("Modifier…") && lignes.includes("Cuisine"), `menu du favori : ${JSON.stringify(lignes)}`);
    await p.click(".menu button:has-text('Modifier…')");
    const champs = await p.$$(".dialogue input");
    egal(champs.length, 3, "nom, adresse, dossier");
    await champs[0].fill("La doc");
    await champs[2].fill("Travail");
    await p.click(".dialogue .ok");
    const envoi = (await messages(p)).find((m) => m.m === "favoriEnregistrer");
    verifier(envoi && envoi.ancienne === "https://b.fr/doc" && envoi.titre === "La doc" && envoi.dossier === "Travail" && envoi.url === "https://b.fr/doc", `envoi : ${JSON.stringify(envoi)}`);
    // Clic droit sur un dossier : renommer, supprimer.
    await vider(p);
    await p.click("#dossiers-favoris .g:nth-child(2)", { button: "right" });
    await p.click(".menu button.rouge");
    verifier((await messages(p)).some((m) => m.m === "favoriDossierRetirer" && m.nom === "Cuisine"), "supprimer le dossier");
    await p.close();
  });

  await cas("barre : Ctrl+D ouvre la fenêtre du favori sous l'étoile", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [onglet(1, 1, "Article", "https://a.fr/article", { actif: true })];
    etat.carnet = { pageWeb: true, favori: true, favoris: [], favorisDossiers: ["Lectures"], guets: [] };
    await pousser(p, etat);
    const carte = { url: "https://a.fr/article", titre: "Article", dossier: "", nouveau: true, dossiers: ["Lectures"] };
    await p.evaluate((x) => window.trame.carteFavori(x), carte);
    verifier(await p.$eval("#menu-commandes", (n) => !n.hidden), "la fenêtre est ouverte");
    egal(await p.$eval(".carte-favori .mar-tete b", (n) => n.textContent), "Favori ajouté", "titre « Favori ajouté »");
    const zone = await p.$eval(".carte-favori", (n) => n.getBoundingClientRect().right);
    const etoile = await p.$eval("#btn-favori", (n) => n.getBoundingClientRect().right);
    verifier(Math.abs(zone - etoile) < 2, `collée sous l'étoile (${zone} / ${etoile})`);
    verifier(await p.evaluate(() => document.activeElement && document.activeElement.value === "Article"), "le nom a le focus");
    // Entrée enregistre nom et dossier.
    await vider(p);
    const champs = await p.$$(".carte-favori input");
    await champs[0].fill("Bon article");
    await champs[1].fill("Lectures");
    await champs[1].press("Enter");
    const envoi = (await messages(p)).find((m) => m.m === "favoriEnregistrer");
    verifier(envoi && envoi.titre === "Bon article" && envoi.dossier === "Lectures" && envoi.ancienne === "https://a.fr/article", `envoi : ${JSON.stringify(envoi)}`);
    verifier(await p.$eval("#menu-commandes", (n) => n.hidden), "Entrée referme la fenêtre");
    // Echap ne touche à rien ; « Supprimer » retire le favori.
    await p.evaluate((x) => window.trame.carteFavori(x), Object.assign({}, carte, { nouveau: false }));
    egal(await p.$eval(".carte-favori .mar-tete b", (n) => n.textContent), "Modifier le favori", "déjà favori : « Modifier le favori »");
    await vider(p);
    await (await p.$(".carte-favori input")).fill("Autre nom");
    await p.keyboard.press("Escape");
    verifier(!(await messages(p)).some((m) => m.m === "favoriEnregistrer"), "Échap n'enregistre rien");
    await p.evaluate((x) => window.trame.carteFavori(x), carte);
    await vider(p);
    await p.click(".carte-favori button.rouge");
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "favoriRetirer" && m.url === "https://a.fr/article"), "Supprimer retire le favori");
    verifier(!recus.some((m) => m.m === "favoriEnregistrer"), "Supprimer n'enregistre pas en plus");
    // Sans changement, refermer n'envoie rien.
    await p.evaluate((x) => window.trame.carteFavori(x), carte);
    await vider(p);
    await p.click(".carte-favori .ok");
    verifier(!(await messages(p)).some((m) => m.m === "favoriEnregistrer"), "rien changé : rien envoyé");
    await p.close();
  });

  await cas("accueil : les fils deviennent des cartes, un onglet se clique", async () => {
    const p = await page("newtab.html");
    const etat = etatDeBase();
    etat.fils = [
      { id: 1, nom: "Bitwarden", onglets: [10, 11] },
      { id: 2, nom: "Home lab", onglets: [12] },
    ];
    etat.onglets = [
      onglet(10, 1, "Bitwarden", "chrome-extension://abc/popup/index.html"),
      onglet(11, 1, "Perplexity", "https://www.perplexity.ai/"),
      onglet(12, 2, "Proxmox", "https://192.0.2.5:8006/"),
    ];
    await pousser(p, etat);
    egal(await p.$$eval(".carte", (n) => n.length), 2, "une carte par fil");
    egal(await p.$$eval(".chip", (n) => n.length), 3, "un raccourci par onglet");
    await p.click(".chip");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "activer" && m.id === 10),
      `activer/10 attendu : ${JSON.stringify(recus)}`
    );
    await p.close();
  });

  // ---------------------------------------------------------------- trame

  await cas("vue trame : chaque onglet a un noeud, les fils une bande", async () => {
    const p = await page("trame.html");
    const etat = etatDeBase();
    // La vue trame ne dessine que les fils qui portent des onglets, et place un onglet sur
    // l'axe du temps par son age (secondes), comme l'hote le pousse.
    etat.fils = [
      { id: 1, nom: "Un", replie: false, onglets: [1, 2], domaine: "a.fr" },
      { id: 2, nom: "Deux", replie: false, onglets: [3], domaine: "c.fr" },
    ];
    etat.onglets = [
      // Le parent est plus ancien que son enfant : dans la vraie vie on ouvre un lien depuis
      // un onglet deja ouvert. La vue doit tolerer l'inverse aussi (voir le cas suivant).
      onglet(1, 1, "A", "https://a.fr/", { age: 120, parent: null }),
      onglet(2, 1, "B", "https://b.fr/", { age: 60, parent: 1 }),
      onglet(3, 2, "C", "https://c.fr/", { age: 30, parent: null }),
    ];
    await pousser(p, etat);
    egal(await p.$$eval("circle.noeud", (n) => n.length), 3, "un noeud par onglet");
    egal(await p.$$eval("rect.bande", (n) => n.length), 2, "une bande par fil");
    // Le lien de provenance (B vient de A) doit etre trace.
    egal(await p.$$eval("path", (n) => n.length), 1, "un lien parent -> enfant");
    const noms = await p.$$eval("text.nom-fil", (n) => n.map((x) => x.textContent));
    egal(noms, ["Un", "Deux"], "les fils sont nommes sur l'axe");

    // Un parent plus recent que son enfant (horodatage restaure) garde son lien : les
    // positions sont calculees avant le dessin, donc l'ordre ne compte plus.
    etat.onglets[0].age = 10;
    await pousser(p, etat);
    egal(await p.$$eval("path", (n) => n.length), 1, "le lien survit a un parent plus recent");
    await p.close();
  });

  // ---------------------------------------------------------------- zappeur

  await cas("zappeur : les regles se listent et se coupent", async () => {
    const p = await page("zap.html");
    const etat = etatDeBase();
    etat.zap = {
      ouvert: true,
      domaine: "youtube.com",
      regles: [
        { index: 0, domaine: "youtube.com", selecteur: "#masthead-ad", libelle: "Banniere", actif: true },
        { index: 1, domaine: "", selecteur: "ins.adsbygoogle", libelle: "Google Ads", actif: false },
      ],
    };
    await pousser(p, etat);
    const lignes = await p.$$eval("input[type=checkbox], .bascule, .regle", (n) => n.length);
    verifier(lignes >= 2, `deux regles attendues dans le DOM, trouve ${lignes}`);
    await p.close();
  });

  // ---------------------------------------------------------------- raccourcis clavier

  await cas("raccourcis : le tableau des touches envoie les bonnes commandes", async () => {
    const p = await page("newtab.html");
    await p.setContent("<body><p>page de test</p></body>");
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    const attendus = [
      ["Control+t", "nouvelOnglet"],
      ["Control+w", "fermerOnglet"],
      ["Control+Shift+t", "reouverture"],
      ["Control+Shift+n", "prive"],
      ["Control+k", "couture"],
      ["Control+l", "couture"],
      ["Control+r", "recharger"],
      ["Control+b", "rail"],
      ["Control+,", "reglages"],
      ["Control+f", "chercherPage"],
      ["Control+d", "favori"],
      ["Control+p", "imprimer"],
      ["Control+=", "zoomPlus"],
      ["Control+-", "zoomMoins"],
      ["Control+0", "zoomZero"],
      ["Control+Tab", "ongletSuivant"],
      ["Control+Shift+Tab", "ongletPrecedent"],
      ["Control+1", "onglet:1"],
      ["Control+9", "onglet:9"],
      ["F5", "recharger"],
    ];
    const manquants = [];
    for (const [touche, action] of attendus) {
      await vider(p);
      await p.keyboard.press(touche);
      const recus = await messages(p);
      const trouve = recus.some((m) => m.m === "raccourci" && m.action === action);
      if (!trouve) manquants.push(`${touche} -> ${action} (recu ${JSON.stringify(recus)})`);
    }
    egal(manquants, [], "toutes les touches doivent produire leur action");
    await p.close();
  });

  await cas("raccourcis : une touche non prevue ne fait rien", async () => {
    const p = await page("newtab.html");
    await p.setContent("<body><p>page de test</p></body>");
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    await vider(p);
    await p.keyboard.press("Control+q");
    await p.keyboard.press("a");
    egal((await messages(p)).length, 0, "aucun message pour une touche banale");
    await p.close();
  });

  await cas("raccourcis : le defilement est signale sans noyer l'hote", async () => {
    const p = await page("newtab.html");
    await p.setContent(
      "<body style='height:4000px'><p>haut</p><p style='margin-top:3000px'>bas</p></body>"
    );
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    await vider(p);
    // Dix defilements d'affilee : un seul message doit partir (delai de 400 ms). L'evenement
    // de defilement est asynchrone dans le navigateur, donc on attend qu'il arrive.
    for (let i = 1; i <= 10; i += 1) {
      await p.evaluate((y) => window.scrollTo(0, y), i * 40);
    }
    await p
      .waitForFunction(() => window.__ipc.some((m) => m.m === "defilement"), { timeout: 3000 })
      .catch(() => {});
    const recus = await messages(p);
    const defilements = recus.filter((m) => m.m === "defilement");
    verifier(defilements.length >= 1, "au moins un defilement signale");
    verifier(defilements.length <= 3, `${defilements.length} messages : trop de bruit`);
    await p.close();
  });

  // ---------------------------------------------------------------- video flottante

  // Une video sans reseau : un canevas anime, diffuse dans la balise <video>.
  const PAGE_VIDEO = `<body style="margin:0;background:#111">
    <canvas id="c" width="320" height="180" style="display:none"></canvas>
    <video id="v" muted autoplay playsinline style="position:absolute;left:100px;top:80px;width:640px;height:360px"></video>
    <video id="petite" muted style="position:absolute;left:900px;top:80px;width:120px;height:70px"></video>
    <script>
      const c = document.getElementById("c"), x = c.getContext("2d");
      let i = 0;
      setInterval(() => { x.fillStyle = "hsl(" + (i++ * 7) % 360 + ",70%,50%)"; x.fillRect(0, 0, 320, 180); }, 40);
      const v = document.getElementById("v");
      v.srcObject = c.captureStream(25);
      v.play().catch(() => {});
    </script></body>`;

  await cas("incrustation : la pastille apparait au survol d'une video, pas d'une petite", async () => {
    const p = await page("newtab.html");
    await p.setContent(PAGE_VIDEO);
    await p.waitForFunction(() => document.getElementById("v").readyState > 0);
    await p.addScriptTag({ path: path.join(UI, "incrustation.js") });
    await p.mouse.move(420, 260);
    await p.mouse.move(421, 262);
    await p.waitForTimeout(250);
    // La pastille est centree en haut de la video (x = 100 + 640 / 2, y = 80 + 12 + 15).
    const dessus = await p.evaluate(() => document.elementFromPoint(420, 107)?.tagName.toLowerCase());
    egal(dessus, "trame-incrustation", "la pastille doit etre sous le pointeur, en haut de la video");
    await p.mouse.move(960, 115);
    await p.waitForTimeout(250);
    const ailleurs = await p.evaluate(() => document.elementFromPoint(420, 107)?.tagName.toLowerCase());
    egal(ailleurs, "video", "la pastille disparait hors de la video (la petite n'en a pas)");
    await p.close();
  });

  await cas("incrustation : le clic sort la video sans la mettre en pause", async () => {
    const p = await page("newtab.html");
    await p.setContent(PAGE_VIDEO);
    await p.waitForFunction(() => document.getElementById("v").readyState > 0);
    await p.addScriptTag({ path: path.join(UI, "incrustation.js") });
    await p.evaluate(() => {
      window.__clicsLecteur = 0;
      document.getElementById("v").addEventListener("click", () => window.__clicsLecteur++);
    });
    await p.mouse.move(420, 260);
    await p.mouse.move(420, 200);
    await p.waitForTimeout(250);
    // Depuis la 0.4.62 le bouton de droite porte un texte (« Fenêtre à part ») : le centre de la
    // pastille tombe dessus. « Vidéo seule » est la moitie gauche.
    await p.mouse.click(372, 107);
    await p.waitForTimeout(500);
    const etat = await p.evaluate(() => ({
      clics: window.__clicsLecteur,
      enPause: document.getElementById("v").paused,
      permis: document.pictureInPictureEnabled,
      flottante: document.pictureInPictureElement?.id || null,
      erreurs: window.__ipc.filter((m) => m.m === "erreurUi").map((m) => m.message),
    }));
    egal(etat.clics, 0, "le lecteur ne doit pas recevoir le clic de la pastille");
    egal(etat.enPause, false, "la video continue de jouer");
    // Le Chromium sans fenetre de Playwright peut refuser la fenetre flottante : on ne juge
    // la sortie que s'il l'autorise (le vrai WebView2 est verifie a part).
    if (etat.permis) egal(etat.flottante, "v", `la video doit etre sortie (erreurs : ${etat.erreurs})`);
    await p.close();
  });

  await cas("incrustation : fermer la fenetre flottante ne laisse pas la video en pause", async () => {
    const p = await page("newtab.html");
    await p.setContent(PAGE_VIDEO);
    await p.waitForFunction(() => !document.getElementById("v").paused);
    await p.addScriptTag({ path: path.join(UI, "incrustation.js") });
    // La croix de Chromium : sortie d'incrustation, puis pause juste apres.
    const relancee = await p.evaluate(async () => {
      const v = document.getElementById("v");
      v.dispatchEvent(new Event("leavepictureinpicture"));
      v.pause();
      await new Promise((r) => setTimeout(r, 300));
      return !v.paused;
    });
    egal(relancee, true, "la video doit reprendre apres la fermeture");
    // Une pause voulue, bien avant la sortie, est respectee.
    const respectee = await p.evaluate(async () => {
      const v = document.getElementById("v");
      v.pause();
      await new Promise((r) => setTimeout(r, 1200));
      v.dispatchEvent(new Event("leavepictureinpicture"));
      await new Promise((r) => setTimeout(r, 300));
      return v.paused;
    });
    egal(respectee, true, "une pause faite avant reste une pause");
    await p.close();
  });

  await cas("incrustation : Alt+Maj+P demande la mini-fenetre, la page s'y etire puis revient", async () => {
    const p = await page("newtab.html");
    await p.setContent(PAGE_VIDEO);
    await p.waitForFunction(() => document.getElementById("v").readyState > 0);
    await p.addScriptTag({ path: path.join(UI, "incrustation.js") });
    await vider(p);
    await p.keyboard.press("Alt+Shift+P");
    const recus = await messages(p);
    verifier(
      recus.some((m) => m.m === "flottant" && m.action === "ouvrir"),
      `message flottant/ouvrir attendu, recu ${JSON.stringify(recus)}`
    );
    // L'hote appelle ensuite __trame_flottant(true) : la video remplit la fenetre.
    const vol = await p.evaluate(() => {
      window.__trame_flottant(true);
      const r = document.getElementById("v").getBoundingClientRect();
      return [r.left, r.top, r.width, r.height, innerWidth, innerHeight];
    });
    egal(vol, [0, 0, vol[4], vol[5], vol[4], vol[5]], "la video doit couvrir toute la fenetre");
    const retour = await p.evaluate(() => {
      window.__trame_flottant(false);
      const r = document.getElementById("v").getBoundingClientRect();
      return [r.left, r.top, r.width, r.height];
    });
    egal(retour, [100, 80, 640, 360], "la video retrouve sa place dans la page");
    await p.close();
  });

  // ---------------------------------------------------------------- recherche dans la page

  await cas("recherche : Ctrl+F installe la barre, une seule fois", async () => {
    const p = await page("newtab.html");
    await p.setContent(
      "<body><p>Trame est un navigateur a onglets tisses. Trame mesure l'attention.</p></body>"
    );
    await p.addScriptTag({ content: RECHERCHE_JS });
    await p.addScriptTag({ content: RECHERCHE_JS });
    egal(await p.$$eval("#trame-recherche", (n) => n.length), 1, "une seule barre");
    const actif = await p.evaluate(() => document.activeElement && document.activeElement.tagName);
    egal(actif, "INPUT", "le champ prend le focus");
    await p.close();
  });

  await cas("recherche : la barre cherche, compte et se referme sur Echap", async () => {
    const p = await page("newtab.html");
    await p.setContent(
      "<body><p>Trame est un navigateur a onglets tisses.</p><p>Trame mesure l'attention.</p></body>"
    );
    await p.addScriptTag({ content: RECHERCHE_JS });
    const aFind = await p.evaluate(() => typeof window.find === "function");
    await p.fill("#trame-recherche input", "Trame");
    await p.press("#trame-recherche input", "Enter");
    const compte = await p.$eval("#trame-recherche span", (n) => n.textContent);
    if (aFind) {
      verifier(/trouvé/.test(compte), `compteur attendu, obtenu « ${compte} »`);
    } else {
      // `window.find` absent : la barre doit au moins rester honnete et ne pas planter.
      verifier(/aucun|trouvé/.test(compte), `compteur inattendu : « ${compte} »`);
    }
    await p.press("#trame-recherche input", "Escape");
    egal(await p.$$eval("#trame-recherche", (n) => n.length), 0, "la barre disparait");
    await p.close();
  });

  // ---------------------------------------------------------------- rendu sans churn

  await cas("rail : une poussee d'etat identique ne reconstruit pas le DOM", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil", replie: false }];
    etat.onglets = [onglet(1, 1, "A", "https://a.fr/"), onglet(2, 1, "B", "https://b.fr/")];
    await pousser(p, etat);
    // On marque les noeuds rendus : s'ils sont reconstruits, la marque disparait.
    await p.evaluate(() => {
      for (const n of document.querySelectorAll(".onglet, .fil")) n.dataset.marque = "1";
    });
    // Trois poussees identiques (c'est ce qui arrive a chaque defilement de page).
    await pousser(p, etat);
    await pousser(p, etat);
    await pousser(p, etat);
    egal(
      await p.evaluate(() => document.querySelectorAll("[data-marque]").length),
      3,
      "les noeuds doivent survivre aux poussees identiques"
    );
    // Un vrai changement reconstruit : le titre de B change.
    etat.onglets[1].titre = "B modifie";
    await pousser(p, etat);
    egal(
      await p.evaluate(() => document.querySelectorAll("[data-marque]").length),
      0,
      "un changement visible doit reconstruire"
    );
    egal(
      await p.$$eval(".onglet .titre", (n) => n.map((x) => x.textContent)),
      ["A", "B modifie"],
      "le nouveau titre est affiche"
    );
    await p.close();
  });

  // ---------------------------------------------------------------- coup d'oeil

  await cas("coup d'oeil : Alt + clic sur un lien demande un apercu, un clic simple non", async () => {
    const p = await page("newtab.html");
    await p.setContent('<body><a id="l" href="https://exemple.fr/article">lien</a><a id="m" href="mailto:x@y.fr">mail</a></body>');
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    await vider(p);
    await p.click("#l", { modifiers: ["Alt"] });
    let recus = await messages(p);
    verifier(recus.some((m) => m.m === "coupDoeil" && m.url === "https://exemple.fr/article"), `coupDoeil : ${JSON.stringify(recus)}`);
    // Alt + clic sur un lien non web : rien.
    await vider(p);
    await p.click("#m", { modifiers: ["Alt"] });
    recus = await messages(p);
    verifier(!recus.some((m) => m.m === "coupDoeil"), "pas d'apercu pour mailto:");
    await p.close();
  });

  // ---------------------------------------------------------------- liens vers un autre site

  // Une page servie sous une vraie adresse (interceptee, rien ne sort) : la regle compare le site
  // de la page a celui du lien, il faut donc un `location.hostname` realiste.
  const pageSur = async (adresse, corps) => {
    const p = await navigateur.newPage({ viewport: { width: 1280, height: 800 } });
    p.on("pageerror", (e) => {
      throw new Error(`erreur JS sur ${adresse} : ${e.message}`);
    });
    await p.addInitScript(() => {
      window.__ipc = [];
      window.ipc = { postMessage: (brut) => window.__ipc.push(JSON.parse(brut)) };
    });
    await p.route("**/*", (r) =>
      r.request().url() === adresse
        ? r.fulfill({ contentType: "text/html; charset=utf-8", body: corps })
        : r.fulfill({ contentType: "text/html", body: "<p>ailleurs</p>" })
    );
    await p.goto(adresse);
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    return p;
  };
  const LIENS = `<body>
    <a id="meme" href="https://www.youtube.com/watch?v=autre">même site</a>
    <a id="mobile" href="https://m.youtube.com/@chaine">même site (m.)</a>
    <a id="court" href="https://youtu.be/abc">youtu.be</a>
    <a id="ext" href="https://exemple.fr/article">autre site</a>
    <a id="redir" href="https://www.youtube.com/redirect?event=video_description&amp;q=https%3A%2F%2Fboutique.fr%2Fproduit">redirection</a>
    <a id="connexion" href="https://accounts.google.com/ServiceLogin">se connecter</a>
    <a id="mail" href="mailto:x@y.fr">mail</a>
    <a id="dl" href="https://exemple.fr/f.zip" download>fichier</a>
    <iframe name="cadre"></iframe><a id="cadre" href="https://exemple.fr/c" target="cadre">dans le cadre</a>
    <div id="ombre"></div>
    <script>
      const r = document.getElementById("ombre").attachShadow({ mode: "open" });
      r.innerHTML = '<a id="dedans" href="https://exemple.fr/ombre"><span id="sous">dans l\\'ombre</span></a>';
      // Le site veut ouvrir lui-même (window.open puis repli) : il ne doit jamais s'en apercevoir.
      document.addEventListener("click", (e) => { window.__siteAVu = (window.__siteAVu || 0) + 1; });
    </script></body>`;

  await cas("liens : meme site dans l'onglet, autre site a cote, redirections lues jusqu'au bout", async () => {
    const p = await pageSur("https://www.youtube.com/watch?v=x", LIENS);
    const aCote = async (sel, attendu, options = {}) => {
      // Deux clics à moins de 400 ms ne font qu'un onglet (double-clic) : on espace.
      await new Promise((r) => setTimeout(r, 450));
      await vider(p);
      const avant = p.url();
      // Le clic reel suivrait le lien : on le bloque apres coup pour ne garder que la decision.
      await p.evaluate(() => {
        window.__navigue = false;
        document.addEventListener("click", (e) => { if (!e.defaultPrevented) { window.__navigue = true; e.preventDefault(); } }, { once: true });
      });
      if (options.milieu) await p.click(sel, { button: "middle" });
      else await p.click(sel, options.ctrl ? { modifiers: ["Control"] } : {});
      const recus = (await messages(p)).filter((m) => m.m === "lienACote");
      if (attendu) {
        egal(recus.length, 1, `${sel} : un seul onglet demandé (${JSON.stringify(recus)})`);
        egal(recus[0].url, attendu, `${sel} : adresse ouverte`);
        egal(recus[0].arrierePlan, !!(options.ctrl || options.milieu), `${sel} : premier plan / arrière-plan`);
        egal(p.url(), avant, `${sel} : la page d'origine reste`);
      } else {
        egal(recus.length, 0, `${sel} : pas d'onglet à côté (${JSON.stringify(recus)})`);
      }
    };
    await aCote("#meme", null);
    await aCote("#mobile", null);
    await aCote("#court", null);
    await aCote("#ext", "https://exemple.fr/article");
    await aCote("#redir", "https://boutique.fr/produit");
    await aCote("#connexion", null);
    await aCote("#mail", null);
    await aCote("#dl", null);
    await aCote("#cadre", null);
    await aCote("#sous", "https://exemple.fr/ombre");
    // Ctrl+clic et clic du milieu : toujours à côté, même sur le même site, en arrière-plan.
    await aCote("#meme", "https://www.youtube.com/watch?v=autre", { ctrl: true });
    await aCote("#meme", "https://www.youtube.com/watch?v=autre", { milieu: true });
    // Le site n'a pas vu le clic qui part à côté : pas de window.open, donc pas de repli en double.
    await vider(p);
    await new Promise((r) => setTimeout(r, 450));
    const vuAvant = await p.evaluate(() => window.__siteAVu || 0);
    await p.click("#ext");
    egal(await p.evaluate(() => window.__siteAVu || 0), vuAvant, "le site ne reçoit pas le clic vers l'autre site");
    await p.close();
  });

  await cas("liens : sur un moteur de recherche, un resultat reste dans l'onglet", async () => {
    const p = await pageSur("https://www.google.fr/search?q=essai", `<body><a id="r" href="https://exemple.fr/resultat">résultat</a></body>`);
    await vider(p);
    await p.evaluate(() => document.addEventListener("click", (e) => e.preventDefault(), { once: true }));
    await p.click("#r");
    egal((await messages(p)).filter((m) => m.m === "lienACote").length, 0, "pas d'onglet à côté depuis Google");
    await new Promise((r) => setTimeout(r, 450));
    await vider(p);
    await p.click("#r", { modifiers: ["Control"] });
    egal((await messages(p)).filter((m) => m.m === "lienACote").length, 1, "Ctrl+clic ouvre quand même à côté");
    await p.close();
  });

  await cas("coup d'oeil : la barre de l'apercu garde, ferme, et repond a Echap", async () => {
    const p = await page("newtab.html");
    await p.setContent("<body style='background:#fff'><p>page apercue</p></body>");
    await p.addScriptTag({ path: path.join(UI, "apercu.js") });
    await p.waitForTimeout(100);
    egal(await p.$$eval("#__trame-apercu", (n) => n.length), 1, "la barre est posee, une seule fois");
    // Le Shadow DOM est ferme : on clique a l'aveugle sur ses boutons par leur position.
    const boite = await p.$eval("#__trame-apercu", (n) => { const r = n.getBoundingClientRect(); return [r.left, r.top, r.width, r.height]; });
    await vider(p);
    await p.mouse.click(boite[0] + 30, boite[1] + boite[3] / 2);
    let recus = await messages(p);
    verifier(recus.some((m) => m.m === "apercuOuvrir"), `garder : ${JSON.stringify(recus)}`);
    await vider(p);
    await p.mouse.click(boite[0] + boite[2] - 14, boite[1] + boite[3] / 2);
    recus = await messages(p);
    verifier(recus.some((m) => m.m === "apercuFermer"), `fermer : ${JSON.stringify(recus)}`);
    await vider(p);
    await p.keyboard.press("Escape");
    recus = await messages(p);
    verifier(recus.some((m) => m.m === "apercuFermer"), `Echap : ${JSON.stringify(recus)}`);
    // Une page qui vide son body ne fait pas disparaitre la barre.
    await p.evaluate(() => { document.body.innerHTML = "<p>reconstruite</p>"; });
    await p.waitForTimeout(1700);
    egal(await p.$$eval("#__trame-apercu", (n) => n.length), 1, "la barre revient");
    await p.close();
  });

  await cas("reglages : navigateur par defaut — etat lu a l'ouverture, bouton vers Windows", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), parDefaut: false };
    await pousser(p, etat);
    verifier((await messages(p)).some((m) => m.m === "etatParDefaut"), "l'etat est demande a l'ouverture");
    verifier((await p.$eval("#txt-par-defaut", (n) => n.textContent)).includes("n'est pas"), "etat affiche");
    await vider(p);
    await p.click("#btn-par-defaut");
    verifier((await messages(p)).some((m) => m.m === "navigateurParDefaut"), "le bouton ouvre les Parametres");
    etat.reglages.parDefaut = true;
    await pousser(p, etat);
    verifier((await p.$eval("#txt-par-defaut", (n) => n.textContent)).includes("✓"), "etat confirme");
    await p.close();
  });

  await cas("rail : le bouton replie et deplie le menu", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await pousser(p, etatDeBase());
    await vider(p);
    await p.click("#btn-rail");
    verifier((await messages(p)).some((m) => m.m === "rail"), "le bouton envoie la bascule du rail");
    await p.close();
  });

  await cas("reglages : confidentialite et pause envoient leur choix", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), nettoyerAdresses: true, garderAffiliation: true, pauseOnglets: true, antiPistage: "stricte", adressesNettoyees: 3 };
    await pousser(p, etat);
    verifier((await p.$eval("#txt-nettoyage", (n) => n.textContent)).includes("3 adresse(s)"), "compteur affiche");
    egal(await p.$eval("#anti-pistage", (n) => n.value), "stricte", "niveau affiche");
    await vider(p);
    await rubrique(p, "onglets");
    await p.click("#bascule-pause");
    await rubrique(p, "confidentialite");
    await p.click("#bascule-affiliation");
    // Le niveau se choisit par les segments ; le <select> cache reste la source envoyee.
    await p.click('#seg-pistage button[data-pistage="equilibree"]');
    egal(await p.$eval("#anti-pistage", (n) => n.value), "equilibree", "le select suit le segment");
    const recus = (await messages(p)).filter((m) => m.m === "confidentialite");
    verifier(recus.some((m) => m.pauseOnglets === false), `pause : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.garderAffiliation === false), `affiliation : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.antiPistage === "equilibree"), `anti-pistage : ${JSON.stringify(recus)}`);
    await p.close();
  });

  // ---------------------------------------------------------------- outils (2026-11-22)

  await cas("barre : Entrée remplace, Alt+Entrée ouvre dans le fil, Maj+Entrée à côté", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    const essai = async (touche) => {
      await p.fill("#couture", "amazon.fr");
      // La frappe ouvre une requete ; aucune reponse ici, donc aucun resultat selectionne.
      await vider(p);
      await p.keyboard.press(touche);
      return (await messages(p)).find((m) => m.m === "naviguer");
    };
    const simple = await essai("Enter");
    verifier(simple && !simple.mode, `Entrée : ${JSON.stringify(simple)}`);
    const alt = await essai("Alt+Enter");
    egal(alt && alt.mode, "onglet", "Alt+Entrée : nouvel onglet du fil");
    const maj = await essai("Shift+Enter");
    egal(maj && maj.mode, "cote", "Maj+Entrée : à côté");
    await p.close();
  });

  await cas("barre : étoile, menu de la page (rendez-vous, côte à côte, guets)", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [onglet(1, 1, "Aliexpress", "https://aliexpress.com/x", { actif: true })];
    etat.carnet = {
      pageWeb: true,
      favori: false,
      favoris: [],
      precedent: { id: 2, titre: "Amazon" },
      guets: [{ id: 7, url: "https://a.fr", libelle: "Disque", texte: "34,90 €", ancien: "39,90 €", vu: false, echecs: 0 }],
      guetsNonVus: 1,
    };
    await pousser(p, etat);
    egal(await p.$eval("#page-badge", (n) => (n.hidden ? "" : n.textContent)), "1", "badge : un guet a changé");
    await vider(p);
    await p.click("#btn-favori");
    verifier((await messages(p)).some((m) => m.m === "favori"), "l'étoile demande le favori");
    await p.click("#btn-page");
    verifier(await p.$eval("#menu-page", (n) => !n.hidden), "le menu de la page s'ouvre");
    const texte = await p.$eval("#menu-page", (n) => n.textContent);
    for (const attendu of ["Imprimer", "Rendez-vous", "Amazon", "Guetter", "39,90 € → 34,90 €", "navigation privée"]) {
      verifier(texte.includes(attendu), `« ${attendu} » absent du menu`);
    }
    // La ligne « Ouvrir en navigation privee » envoie la meme commande que la touche.
    await vider(p);
    await p.click("#menu-page >> text=Ouvrir en navigation privée");
    egal(
      (await messages(p)).filter((m) => m.m === "prive"),
      [{ m: "prive" }],
      "le menu de la page demande la navigation privée"
    );
    await vider(p);
    await p.dispatchEvent(".puce >> text=Demain matin", "pointerdown", { button: 0 });
    const rdv = (await messages(p)).find((m) => m.m === "rendezVous");
    verifier(rdv && rdv.id === 1 && rdv.quand > Date.now() + 3600e3, `rendez-vous : ${JSON.stringify(rdv)}`);

    // Clic droit sur un onglet du rail : le menu se pose a gauche, a la hauteur du clic, et
    // annonce son haut a l'hote (region de fenetre) ; trop bas, il remonte dans la fenetre.
    await p.setViewportSize({ width: 1150, height: 1200 });
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200 }));
    await p.waitForTimeout(120);
    const pose = await p.$eval("#menu-page .menu-colonne", (n) => {
      const r = n.getBoundingClientRect();
      return { x: r.left, haut: r.top, bas: r.bottom, ancre: n.classList.contains("ancre") };
    });
    verifier(pose.ancre && pose.x < 20, `menu collé au rail : ${JSON.stringify(pose)}`);
    // Le menu se pose a la hauteur du clic S'IL TIENT ; sinon il remonte pour tenir dans la
    // fenetre. Le test ne fige donc pas une valeur : il exige les deux proprietes (mesure du
    // 2026-11-23, quand le menu d'onglet s'est enrichi : un haut fixe à 138 cassait).
    verifier(
      pose.haut <= 142 && pose.haut >= 0 && pose.bas <= 1196,
      `menu posé au clic, ou remonté pour tenir dans la fenêtre : ${JSON.stringify(pose)}`
    );
    const zone = (await messages(p)).filter((m) => m.m === "panneau").pop();
    verifier(zone && zone.zone && Math.abs(zone.zone.y - pose.haut) < 2, `haut annoncé : ${JSON.stringify(zone)}`);
    await p.evaluate(() => window.trame.menuPage({ y: 880, h: 900 }));
    await p.waitForTimeout(120);
    const bas = await p.$eval("#menu-page .menu-colonne", (n) => n.getBoundingClientRect().bottom);
    verifier(bas <= 893, `menu remonté dans la fenêtre : bas = ${bas}`);
    // Par le bouton « … » : de nouveau sous le bouton, a droite.
    await p.click("#btn-page");
    await p.click("#btn-page");
    const droite = await p.$eval("#menu-page .menu-colonne", (n) => ({ x: n.getBoundingClientRect().left, ancre: n.classList.contains("ancre") }));
    verifier(!droite.ancre && droite.x > 500, `bouton « … » : menu à droite : ${JSON.stringify(droite)}`);
    await p.close();
  });

  await cas("menu d'un onglet du rail : veille, réveil, son, adresse, fil — sans l'afficher", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [
      onglet(1, 1, "Aliexpress", "https://aliexpress.com/x", { actif: true }),
      onglet(2, 1, "Une vidéo", "https://www.youtube.com/watch?v=abc", { eveille: false }),
    ];
    etat.fils = [{ id: 1, nom: "Achats", onglets: [1, 2] }, { id: 3, nom: "Vidéos", onglets: [] }];
    etat.carnet = { pageWeb: true, favori: false, favoris: [], guets: [] };
    await pousser(p, etat);
    await p.setViewportSize({ width: 1150, height: 1200 });
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 2 }));
    await p.waitForTimeout(120);
    const texte = await p.$eval("#menu-page", (n) => n.textContent);
    verifier(texte.includes("Cet onglet — Une vidéo"), `titre : ${texte.slice(0, 80)}`);
    verifier(texte.includes("Réveiller") && !texte.includes("Mettre en veille"), "onglet endormi : Réveiller");
    verifier(!texte.includes("Ajouter aux favoris") && !texte.includes("Côte à côte"), "pas les lignes de la page affichée");
    verifier(texte.includes("Copier l'adresse") && texte.includes("Couper le son"), "adresse et son");
    // Tout ce qu'on peut faire d'un onglet doit etre la : fermer, fermer les autres du fil,
    // dupliquer, ancrer, a lire, acces rapide, copier le titre.
    for (const attendu of [
      "Fermer l'onglet",
      "Fermer les autres du fil",
      "Dupliquer l'onglet",
      "Ancrer",
      "À lire plus tard",
      "Ajouter aux accès rapides",
      "Copier le titre",
    ]) {
      verifier(texte.includes(attendu), `« ${attendu} » manque au menu d'un onglet`);
    }
    await vider(p);
    await p.click("#menu-page .menu-principal >> text=Dupliquer l'onglet");
    verifier(
      (await messages(p)).some((m) => m.m === "dupliquer" && m.id === 2),
      "dupliquer envoie l'onglet visé"
    );
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 2 }));
    await vider(p);
    await p.click("#menu-page .menu-principal >> text=Fermer les autres du fil");
    verifier(
      (await messages(p)).some((m) => m.m === "fermerAutres" && m.id === 2),
      "fermer les autres du fil envoie l'onglet visé"
    );
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 2 }));
    await vider(p);
    await p.click("#menu-page .menu-principal >> text=Réveiller");
    verifier((await messages(p)).some((m) => m.m === "reveillerFond" && m.id === 2), "réveil en arrière-plan");
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 2 }));
    await vider(p);
    await p.click("#menu-page .puce >> text=Vidéos");
    verifier((await messages(p)).some((m) => m.m === "tisser" && m.onglet === 2 && m.fil === 3), "déplacer vers « Vidéos »");
    // L'onglet affiché : Mettre en veille, et les lignes de la page reviennent.
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 1 }));
    const texte1 = await p.$eval("#menu-page", (n) => n.textContent);
    verifier(texte1.includes("Mettre en veille") && texte1.includes("Ajouter aux favoris"), "onglet affiché : veille et page");
    await p.close();
  });

  // Mesure du 2026-11-26 : dans un côte à côte, la page neuve prenait TOUJOURS la droite et la
  // page de gauche n'était jamais remplacée. Le menu dit désormais les deux côtés, et un onglet
  // du rail peut prendre l'une des deux places.
  await cas("menu de la page : « À gauche » et « À droite » remplacent la ligne unique", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [
      onglet(1, 1, "Aliexpress", "https://aliexpress.com/x", { actif: true }),
      onglet(2, 1, "Amazon", "https://amazon.fr/x"),
    ];
    etat.fils = [{ id: 1, nom: "Achats", onglets: [1, 2] }];
    etat.carnet = {
      pageWeb: true,
      favori: false,
      favoris: [],
      guets: [],
      cote: null,
      precedent: { id: 2, titre: "Amazon" },
    };
    await pousser(p, etat);
    await p.setViewportSize({ width: 1150, height: 1200 });
    await p.click("#btn-page");
    await p.waitForTimeout(120);
    const texte = await p.$eval("#menu-page", (n) => n.textContent);
    verifier(texte.includes("À gauche") && texte.includes("À droite"), `deux entrées de côté : ${texte.slice(-200)}`);
    verifier(!texte.includes("À côté de «"), "l'ancienne ligne unique a disparu");
    await vider(p);
    // Par le TITRE du bouton : « text=À droite » attrapait la ligne « À gauche », dont le détail
    // contient « à droite » (les deux libellés se citent l'un l'autre).
    await p.click('#menu-page .menu-principal[title="À gauche"]');
    egal(
      (await messages(p)).filter((m) => m.m === "coteAvec"),
      [{ m: "coteAvec", id: 2, cote: "gauche" }],
      "« À gauche » place Amazon à gauche de la page affichée"
    );
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200 }));
    await vider(p);
    await p.click('#menu-page .menu-principal[title="À droite"]');
    egal(
      (await messages(p)).filter((m) => m.m === "coteAvec"),
      [{ m: "coteAvec", id: 2, cote: "droite" }],
      "« À droite » place Amazon à droite"
    );
    await p.close();
  });

  await cas("menu d'un onglet du rail : remplacer la page de gauche ou de droite", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [
      onglet(1, 1, "Aliexpress", "https://aliexpress.com/x", { actif: true }),
      onglet(2, 1, "Amazon", "https://amazon.fr/x"),
      onglet(3, 1, "Une vidéo", "https://www.youtube.com/watch?v=abc"),
    ];
    etat.fils = [{ id: 1, nom: "Achats", onglets: [1, 2, 3] }];
    etat.carnet = { pageWeb: true, favori: false, favoris: [], guets: [], cote: { gauche: 1, droite: 2, affiche: true, lie: false }, precedent: null };
    await pousser(p, etat);
    await p.setViewportSize({ width: 1150, height: 1200 });
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 3 }));
    await p.waitForTimeout(120);
    const texte = await p.$eval("#menu-page", (n) => n.textContent);
    verifier(
      texte.includes("Remplacer la page de gauche") && texte.includes("Remplacer la page de droite"),
      `les deux places sont proposées : ${texte.slice(-260)}`
    );
    verifier(!texte.includes("Ajouter aux favoris"), "le menu d'un onglet ne prend pas les lignes de la page");
    await vider(p);
    await p.click('#menu-page .menu-principal[title="Remplacer la page de gauche"]');
    egal(
      (await messages(p)).filter((m) => m.m === "coteAvec"),
      [{ m: "coteAvec", id: 3, cote: "gauche" }],
      "remplacer la page de gauche envoie le côté demandé"
    );
    // Un onglet DÉJÀ dans la paire ne se « remplace » pas : seules les puces du côte à côte.
    await p.evaluate(() => window.trame.menuPage({ y: 150, h: 1200, cible: 1 }));
    const lignesPage = await p.$$eval("#menu-page .menu-ligne .n", (n) => n.map((x) => x.textContent));
    verifier(
      !lignesPage.some((t) => t.startsWith("Remplacer la page")),
      `une page déjà affichée ne se remplace pas elle-même : ${JSON.stringify(lignesPage)}`
    );
    const textePage = await p.$eval("#menu-page", (n) => n.textContent);
    verifier(textePage.includes("Inverser") && textePage.includes("Quitter"), "les puces du côte à côte sont là");
    await p.close();
  });

  await cas("barre : la pastille VPN dit si le tunnel est actif, et mène aux réglages", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    // 1. Rien d'exigé, aucun tunnel : la pastille est cachée.
    await pousser(p, etatDeBase());
    egal(await p.$eval("#btn-vpn", (n) => n.hidden), true, "aucun tunnel : pastille cachée");
    // 2. Tunnel monté : pastille verte, avec le nom dans la bulle.
    await pousser(p, {
      ...etatDeBase(),
      reglages: { tunnelOblige: true, tunnel: "Tunnel intégré (WireGuard)" },
    });
    egal(await p.$eval("#btn-vpn", (n) => n.hidden), false, "tunnel actif : pastille visible");
    egal(await p.$eval("#btn-vpn", (n) => n.className.includes("actif")), true, "classe « actif » (verte)");
    verifier(
      (await p.$eval("#btn-vpn", (n) => n.title)).includes("Tunnel intégré"),
      "la bulle donne le nom du tunnel"
    );
    // 3. Exigé mais absent : pastille rouge, et la bulle dit que la navigation est bloquée.
    await pousser(p, { ...etatDeBase(), reglages: { tunnelOblige: true, tunnel: null } });
    egal(await p.$eval("#btn-vpn", (n) => n.className.includes("bloque")), true, "classe « bloqué » (rouge)");
    verifier(
      (await p.$eval("#btn-vpn", (n) => n.title)).includes("bloquée"),
      "la bulle dit que la navigation est bloquée"
    );
    // 4. Un clic mène aux réglages, sur la rubrique Confidentialité.
    await vider(p);
    await p.click("#btn-vpn");
    egal(
      (await messages(p)).filter((m) => m.m === "ouvrirReglages"),
      [{ m: "ouvrirReglages", rubrique: "confidentialite" }],
      "le clic ouvre les réglages sur Confidentialité"
    );
    await p.close();
  });

  await cas("rail : les marques gauche/droite ne s'affichent que si la paire est à l'écran", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil" }];
    etat.onglets = [
      onglet(1, 1, "A", "https://a.fr/", { actif: true }),
      onglet(2, 1, "B", "https://b.fr/"),
      onglet(3, 1, "C", "https://c.fr/"),
    ];
    etat.carnet = { cote: { gauche: 1, droite: 2, affiche: true, lie: false } };
    await pousser(p, etat);
    egal(
      await p.$$eval(".onglet .marque-mini.cote", (n) => n.map((x) => x.textContent)),
      ["gauche", "droite"],
      "paire affichée : les deux marques sont là"
    );
    // L'onglet affiché n'est plus l'un des deux : la paire est en attente, les marques mentiraient.
    etat.carnet = { cote: { gauche: 1, droite: 2, affiche: false, lie: false } };
    await pousser(p, etat);
    egal(await p.$$eval(".onglet .marque-mini.cote", (n) => n.length), 0, "paire en attente : aucune marque");
    await p.close();
  });

  await cas("barre : les pages lues arrivent après coup, seulement pour la bonne requête", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.fill("#couture", "?batterie solide");
    await p.waitForTimeout(150);
    const r = await p.evaluate(() => {
      const dernier = window.__ipc.filter((m) => m.m === "requete").pop();
      return dernier && dernier.rid;
    });
    await p.evaluate((rid) => window.trame.resultats({ rid, q: "?batterie solide", items: [] }), r);
    verifier((await p.$eval("#resultats", (n) => n.textContent)).includes("Recherche dans ce que vous avez lu"), "attente affichée");
    const lecture = { genre: "lecture", id: "https://a.fr/x", titre: "Batteries", sousTitre: "…électrolyte solide…", action: { type: "lecture", url: "https://a.fr/x" } };
    await p.evaluate((rid) => window.trame.resultatsPlus({ rid: rid - 1, items: [{ genre: "lecture", id: "perime", titre: "Périmé", sousTitre: "", action: {} }] }), r);
    await p.evaluate(([rid, l]) => window.trame.resultatsPlus({ rid, items: [l] }), [r, lecture]);
    const titres = await p.$$eval("#resultats .resultat .t", (n) => n.map((x) => x.textContent));
    egal(titres, ["Batteries"], "seule la réponse à la requête en cours s'affiche");
    await p.close();
  });

  await cas("rail : rendez-vous hors du rail, Maj+clic à côté, clic droit = menu de la page", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const etat = etatDeBase();
    etat.fils = [{ id: 1, nom: "Fil", replie: false }];
    etat.onglets = [
      onglet(1, 1, "A", "https://a.fr/", { actif: true }),
      onglet(2, 1, "B", "https://b.fr/", { fermeture: Date.now() + 3600e3 }),
      onglet(3, 1, "C", "https://c.fr/", { rdv: Date.now() + 86400e3, eveille: false }),
    ];
    await pousser(p, etat);
    egal(await p.$$eval(".onglet .titre", (n) => n.map((x) => x.textContent)), ["A", "B"], "l'onglet en rendez-vous est caché");
    verifier((await p.$eval(".onglet:nth-child(2)", (n) => n.textContent)).includes("ferme"), "fermeture programmée marquée");
    await p.click(".attente");
    egal(await p.$eval("#volet-titre", (n) => n.textContent), "Rendez-vous", "le volet liste les rendez-vous");
    verifier((await p.$eval("#volet-corps", (n) => n.textContent)).includes("C"), "C est listé");
    await p.click("#volet-fermer");
    await vider(p);
    await p.click(".onglet:nth-child(2)", { modifiers: ["Shift"] });
    await p.click(".onglet:nth-child(2)", { button: "right" });
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "coteAvec" && m.id === 2), `Maj+clic : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.m === "menuPage" && m.id === 2), `clic droit : ${JSON.stringify(recus)}`);
    await p.close();
  });

  await cas("menu de la page : la poignée choisit la hauteur, le menu défile", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    // Cle `v2` : celle des versions d'avant la 0.4.57 est ignoree (elle pouvait plafonner le menu
    // a une hauteur absurde, defaut constate chez Clement le 2026-11-23).
    await p.evaluate(() => localStorage.removeItem("trame.menuPage.hauteur.v2"));
    const etat = etatDeBase();
    etat.url = "https://youtube.com/";
    etat.carnet = { pageWeb: true, precedent: { id: 2, titre: "Réglages" }, guets: [], memoireLecture: true };
    await pousser(p, etat);
    await p.evaluate(() => window.trame.menuPage({ y: 120, h: 800 }));
    await p.waitForTimeout(100);
    const g = await p.$eval("#menu-page .menu-poignee", (n) => {
      const r = n.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
    });
    // Un CLIC sans glissement ne doit rien enregistrer (il le faisait, et le menu restait
    // plafonne a cette hauteur a chaque ouverture suivante).
    await p.mouse.move(g.x, g.y);
    await p.mouse.down();
    await p.mouse.up();
    const apresClic = await p.$eval("#menu-page .menu-colonne", (c) => ({
      garde: localStorage.getItem("trame.menuPage.hauteur.v2"),
    }));
    egal(apresClic.garde, null, "un clic sur la poignée n'enregistre rien");
    // Une vraie traction, elle, choisit la hauteur.
    await p.mouse.move(g.x, g.y);
    await p.mouse.down();
    await p.mouse.move(g.x, g.y - 250, { steps: 4 });
    await p.mouse.up();
    const apres = await p.$eval("#menu-page .menu-colonne", (c) => ({
      h: c.getBoundingClientRect().height,
      defile: c.scrollHeight > c.clientHeight,
      garde: localStorage.getItem("trame.menuPage.hauteur.v2"),
    }));
    verifier(apres.defile && apres.h < 500 && apres.garde, `réduit, défilant et gardé : ${JSON.stringify(apres)}`);
    await p.evaluate(() => localStorage.removeItem("trame.menuPage.hauteur.v2"));
    await p.close();
  });

  await cas("fenêtre : boutons, glisser, double-clic, bord du haut, selon le style", async () => {
    const b = await page("chrome.html", { query: "?surface=barre" });
    const avec = (style, agrandie = false) => ({ ...etatDeBase(), titre: "YouTube", fenetre: { style, agrandie } });
    const visibles = () =>
      b.$$eval("button[data-fenetre]", (l) => l.filter((n) => n.offsetParent !== null).map((n) => n.dataset.fenetre));

    // Une seule bande : les trois boutons au bout de la barre d'adresse.
    await pousser(b, avec("bande"));
    egal(await visibles(), ["reduire", "agrandir", "fermer"], "bande : trois boutons visibles");
    const coin = await b.$eval(".boutons-fenetre.bande button.fermer", (n) => n.getBoundingClientRect().right);
    verifier(Math.abs(coin - 1280) <= 1, `la croix touche le bord droit (${coin})`);
    await vider(b);
    await b.click(".boutons-fenetre.bande button.reduire");
    await b.click(".boutons-fenetre.bande button.fermer");
    egal((await messages(b)).map((m) => m.action), ["reduire", "fermer"], "les boutons commandent la fenêtre");

    // Le vide de la barre deplace la fenetre ; un double-clic l'agrandit ; un bouton, non.
    const vide = await b.$eval(".barre-ligne", (n) => {
      const r = n.getBoundingClientRect();
      const btn = document.getElementById("btn-reglages").getBoundingClientRect();
      return { x: btn.right + 2, y: r.top + r.height / 2 };
    });
    await vider(b);
    await b.mouse.click(vide.x, vide.y);
    await b.mouse.dblclick(vide.x, vide.y);
    await b.click("#btn-recharger");
    const actions = (await messages(b)).filter((m) => m.m === "fenetre").map((m) => m.action);
    verifier(actions.includes("deplacer") && actions.includes("agrandir"), `glisser et double-clic : ${actions}`);

    // Bord du haut : redimensionne ; coin droit dans la barre.
    await vider(b);
    await b.mouse.click(600, 1);
    await b.mouse.click(1278, 1);
    egal((await messages(b)).filter((m) => m.m === "fenetre").map((m) => m.bord), ["haut", "haut-droite"], "bord du haut");

    // Agrandie : plus de bord du haut, l'icone devient « restaurer ».
    await pousser(b, avec("bande", true));
    verifier(!(await b.isVisible(".bord-haut")), "pas de bord du haut quand la fenêtre est agrandie");
    egal(await b.$eval(".boutons-fenetre.bande button.agrandir", (n) => n.title), "Restaurer", "bouton restaurer");

    // Bandeau : petite bande au-dessus avec le titre de l'onglet ; la barre d'adresse dessous.
    await pousser(b, avec("bandeau"));
    egal(await b.$eval(".titre-barre", (n) => Math.round(n.getBoundingClientRect().height)), 28, "bandeau de 28 px");
    egal(await b.$eval("#titre-texte", (n) => n.textContent), "YouTube", "titre de l'onglet dans le bandeau");
    egal(await visibles(), ["reduire", "agrandir", "fermer"], "bandeau : les boutons sont dans la bande");

    // Rail et Windows : aucun bouton dans la barre ; Windows : rien ne glisse.
    await pousser(b, avec("rail"));
    egal(await visibles(), [], "style rail : pas de bouton dans la barre");
    await pousser(b, avec("windows"));
    egal(await visibles(), [], "style windows : pas de bouton");
    await vider(b);
    await b.mouse.click(vide.x, vide.y);
    egal((await messages(b)).filter((m) => m.m === "fenetre").length, 0, "style windows : la barre ne déplace rien");

    // Rail : trois pastilles en haut du rail, deplie et replie.
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 292, height: 700 });
    await pousser(r, avec("rail"));
    await vider(r);
    await r.click(".pastille.fermer");
    egal((await messages(r)).filter((m) => m.m === "fenetre").map((m) => m.action), ["fermer"], "pastille fermer");
    await r.setViewportSize({ width: 64, height: 700 });
    const p = await r.$eval(".pastilles", (n) => {
      const b = n.getBoundingClientRect();
      return { visible: n.offsetParent !== null, droite: b.right };
    });
    verifier(p.visible && p.droite <= 64, `pastilles dans le rail replié : ${JSON.stringify(p)}`);
  });

  await cas("pied du rail : Veille, Privé et Capsules ; replié, la jauge, Veille et Privé", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 292, height: 700 });
    await pousser(r, etatDeBase());
    verifier(!(await r.$("#btn-reveiller")), "plus de bouton Réveiller");
    const visibles = () =>
      r.$$eval(".pied-actions button", (l) => l.filter((n) => n.offsetParent !== null).map((n) => n.id));
    egal(await visibles(), ["btn-veille", "btn-prive", "btn-capsules"], "déplié : Veille, Privé et Capsules");
    await vider(r);
    await r.click("#btn-veille");
    verifier((await messages(r)).some((m) => m.m === "endormirFroids"), "Veille endort les onglets froids");
    // La navigation privee se demande par un bouton, pas seulement par la touche.
    await vider(r);
    await r.click("#btn-prive");
    egal(
      (await messages(r)).filter((m) => m.m === "prive"),
      [{ m: "prive" }],
      "le bouton Privé demande la navigation privée"
    );
    await r.setViewportSize({ width: 64, height: 700 });
    // Le bouton prive reste visible rail replié : c'est tout l'objet de cette entrée (choix de
    // Clement, 2026-11-23) — l'écran de 64 px l'empile sous la jauge.
    egal(await visibles(), ["btn-veille", "btn-prive"], "replié : la jauge, Veille et Privé");
    // Replie, la jauge est l'anneau (conic-gradient) autour du bouton Veille.
    const pied = await r.$eval(".rail-pied", (n) => {
      const b = document.getElementById("btn-veille");
      const v = b.getBoundingClientRect();
      return {
        droite: v.right,
        anneau: getComputedStyle(b).backgroundImage.includes("conic-gradient"),
        jauge: b.style.getPropertyValue("--jauge"),
        texte: document.querySelector("#btn-veille span").offsetParent !== null,
      };
    });
    verifier(pied.droite <= 64 && pied.anneau && /%$/.test(pied.jauge) && !pied.texte, `replié : icône sans texte, jauge en anneau ${JSON.stringify(pied)}`);
    // Le detail vit dans le title, ou dans data-bulle une fois survole (bulles.js le deplace).
    const detail = await r.$eval("#btn-veille", (n) => n.getAttribute("title") || n.dataset.bulle || "");
    verifier(detail.includes("éveillé"), "l'anneau donne le détail au survol");
  });

  await cas("rail replié en dock : fils colorés, tuiles de 44 px, fil fermé en pile", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 64, height: 700 });
    const e = etatDeBase();
    e.fils = [
      { id: 1, nom: "Ciné", replie: false, compte: 2 },
      { id: 2, nom: "Musique", replie: false, compte: 4 },
    ];
    e.onglets = [
      onglet(10, 1, "Netflix", "https://www.netflix.com/", { actif: true }),
      onglet(11, 1, "Netflix 2", "https://www.netflix.com/browse"),
      onglet(12, 2, "Radio 1", "https://www.youtube.com/watch?v=a"),
      onglet(13, 2, "Radio 2", "https://www.youtube.com/watch?v=b"),
      onglet(14, 2, "Radio 3", "https://www.youtube.com/watch?v=c"),
      onglet(15, 2, "Radio 4", "https://www.youtube.com/watch?v=d"),
    ];
    await pousser(r, e);
    const vu = await r.$eval(".fil", (f) => {
      const o = f.querySelector(".onglet").getBoundingClientRect();
      const nom = f.querySelector(".fil-nom");
      return { c: f.style.getPropertyValue("--c"), l: o.width, h: o.height, droite: o.right, nom: nom.offsetParent !== null, cadre: getComputedStyle(f).borderTopWidth };
    });
    verifier(/^#/.test(vu.c) && vu.l === 44 && vu.h === 40 && vu.droite <= 64 && vu.nom && vu.cadre === "0px", `fil replié : ${JSON.stringify(vu)}`);
    await vider(r);
    await r.click(".fil-tete");
    const m = (await messages(r)).find((x) => x.m === "filReplier");
    verifier(m && m.replie === true, `un clic sur la tête replie le fil : ${JSON.stringify(m)}`);
    const f0 = e.fils[1];
    await pousser(r, { ...e, fils: e.fils.map((f) => (f.id === f0.id ? { ...f, replie: true } : f)) });
    const pile = await r.$eval(`.fil[data-fil="${f0.id}"] .fil-pile`, (n) => {
      const b = n.getBoundingClientRect();
      return { vis: n.offsetParent !== null, n: n.children.length, plus: n.lastChild.textContent, droite: b.right };
    });
    verifier(pile.vis && pile.n === 3 && pile.plus === "+2" && pile.droite <= 64, `fil fermé : 2 icônes et « +2 » ${JSON.stringify(pile)}`);
    await vider(r);
    await r.click(`.fil[data-fil="${f0.id}"] .fil-pile`);
    verifier((await messages(r)).some((x) => x.m === "filReplier" && x.replie === false), "un clic sur la pile déplie le fil");
  });

  // ------------------------------------------------------- ranger les groupes

  // Trois groupes comme dans le rail de Clément : leur ordre est celui de `session.fils`.
  const filsARanger = () => {
    const e = etatDeBase();
    e.fils = [
      { id: 1, nom: "Login Home", replie: false },
      { id: 2, nom: "Amazon Dvd", replie: false },
      { id: 3, nom: "Automated Day", replie: false },
    ];
    e.onglets = [
      onglet(10, 1, "localhost", "http://localhost/"),
      onglet(11, 2, "Amazon", "https://amazon.fr/"),
      onglet(12, 3, "YouTube", "https://youtube.com/"),
    ];
    return e;
  };

  await cas("rail : glisser la tête d'un groupe le range devant un autre", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    await pousser(p, filsARanger());
    await vider(p);
    const pts = await pointsRail(p, 3);
    const devant = pts.cartes.find((c) => c.id === 1);
    await geste(p, [
      ['.fil[data-fil="3"] .fil-tete', "pointerdown", pts.x, pts.y],
      [null, "pointermove", pts.x, devant.haut + 4],
    ]);
    // Le trait dit où le groupe tombera ; rien n'est encore demandé à l'hôte.
    verifier(await p.$('.fil[data-fil="1"].avant'), "la carte de destination porte le trait");
    // Il doit rester DANS la carte : `#fils` défile (`overflow: auto`) et coupe ce qui dépasse en
    // haut — un trait posé dans l'interstice au-dessus de la PREMIÈRE carte serait invisible, et
    // c'est justement le rang le plus demandé (constaté sur la capture du 2026-11-22).
    const trait = await p.$eval('.fil[data-fil="1"]', (c) => {
      const s = getComputedStyle(c, "::before");
      return { top: s.top, height: s.height, fond: s.backgroundColor };
    });
    egal(trait.top, "0px", "le trait de la première carte est dessiné dedans, pas au-dessus");
    verifier(parseFloat(trait.height) >= 2, `trait visible : ${JSON.stringify(trait)}`);
    egal(
      (await messages(p)).filter((m) => m.m === "filDeplacer"),
      [],
      "aucun rangement demandé pendant le geste"
    );
    await geste(p, [[null, "pointerup", pts.x, devant.haut + 4]]);
    egal(
      (await messages(p)).filter((m) => m.m === "filDeplacer"),
      [{ m: "filDeplacer", id: 3, avant: 1 }],
      "un seul message, au relâchement"
    );
    await p.close();
  });

  await cas("rail : glisser un groupe sous le dernier le range en dernier", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    await pousser(p, filsARanger());
    await vider(p);
    const pts = await pointsRail(p, 1);
    const sousTout = Math.max(...pts.cartes.map((c) => c.bas)) + 12;
    await geste(p, [
      ['.fil[data-fil="1"] .fil-tete', "pointerdown", pts.x, pts.y],
      [null, "pointermove", pts.x, sousTout],
    ]);
    verifier(await p.$('.fil[data-fil="3"].apres'), "le trait se pose sous la dernière carte");
    await geste(p, [[null, "pointerup", pts.x, sousTout]]);
    egal(
      (await messages(p)).filter((m) => m.m === "filDeplacer"),
      [{ m: "filDeplacer", id: 1, avant: 0 }],
      "avant: 0 veut dire « en dernier »"
    );
    await p.close();
  });

  await cas("rail : un appui sur la tête d'un groupe ne range rien", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    await pousser(p, filsARanger());
    await vider(p);
    const pts = await pointsRail(p, 2);
    // Appui et relâchement au même endroit, puis 3 px de tremblement : c'est un clic.
    await geste(p, [
      ['.fil[data-fil="2"] .fil-tete', "pointerdown", pts.x, pts.y],
      [null, "pointermove", pts.x, pts.y + 3],
      [null, "pointerup", pts.x, pts.y],
    ]);
    egal((await messages(p)).filter((m) => m.m === "filDeplacer"), [], "un appui ne range rien");
    await p.close();
  });

  await cas("rail replié : un glissement range sans replier le groupe", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await r.setViewportSize({ width: 64, height: 700 });
    await pousser(r, filsARanger());
    await vider(r);
    const pts = await pointsRail(r, 3);
    const devant = pts.cartes.find((c) => c.id === 1);
    await geste(r, [
      ['.fil[data-fil="3"] .fil-tete', "pointerdown", pts.x, pts.y],
      [null, "pointermove", pts.x, devant.haut + 4],
      [null, "pointerup", pts.x, devant.haut + 4],
    ]);
    // Le clic que le navigateur emet apres un glissement ne doit pas replier le groupe.
    await r.evaluate(() => document.querySelector('.fil[data-fil="3"] .fil-tete').click());
    const recus = await messages(r);
    egal(recus.filter((m) => m.m === "filReplier"), [], "un glissement ne replie pas le groupe");
    egal(
      recus.filter((m) => m.m === "filDeplacer"),
      [{ m: "filDeplacer", id: 3, avant: 1 }],
      "le groupe est rangé dans le rail replié"
    );
    await r.close();
  });

  await cas("rail : le groupe saisi survit à une poussée de l'hôte pendant le geste", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    await pousser(p, filsARanger());
    await vider(p);
    const pts = await pointsRail(p, 3);
    const devant = pts.cartes.find((c) => c.id === 1);
    await p.evaluate(() => {
      window.__carte = document.querySelector('.fil[data-fil="3"]');
    });
    await geste(p, [
      ['.fil[data-fil="3"] .fil-tete', "pointerdown", pts.x, pts.y],
      [null, "pointermove", pts.x, devant.haut + 4],
    ]);
    // Une poussee de l'hote change la signature du rail (titre d'onglet) : sans le gel du
    // redessin, la carte saisie serait detruite au milieu du geste.
    const etat2 = filsARanger();
    etat2.onglets[0].titre = "localhost (changé)";
    await pousser(p, etat2);
    const vu = await p.evaluate(() => ({
      meme: window.__carte === document.querySelector('.fil[data-fil="3"]'),
      saisi: !!document.querySelector('.fil[data-fil="3"].en-glissement'),
      titre: document.querySelector('.fil[data-fil="1"] .onglet .titre').textContent,
    }));
    verifier(vu.meme && vu.saisi, `la carte saisie survit à la poussée : ${JSON.stringify(vu)}`);
    egal(vu.titre, "localhost", "le rail n'est pas redessiné pendant le geste");
    await geste(p, [[null, "pointerup", pts.x, devant.haut + 4]]);
    egal(
      (await messages(p)).filter((m) => m.m === "filDeplacer"),
      [{ m: "filDeplacer", id: 3, avant: 1 }],
      "le relâchement range le groupe malgré la poussée"
    );
    await p.close();
  });

  await cas("logo : clic droit = menu des commandes, dessiné par la barre", async () => {
    const r = await page("chrome.html", { query: "?surface=rail" });
    await pousser(r, etatDeBase());
    await vider(r);
    await r.click(".rail-tete .marque", { button: "right" });
    verifier((await messages(r)).some((m) => m.m === "menuCommandes"), "le rail demande le menu des commandes");
    // Rail deplie : le menu se dessine sous le logo, entier, et se ferme d'un clic ailleurs.
    await r.evaluate(() =>
      window.trame.menuCommandes([
        { id: "nouvel-onglet", titre: "Nouvel onglet", raccourci: "Ctrl+T" },
        { id: "veille", titre: "Endormir les onglets froids", raccourci: "" },
      ])
    );
    const boite = await r.$eval("#commandes-rail .menu-colonne", (n) => {
      const b = n.getBoundingClientRect();
      const logo = document.querySelector(".rail-tete").getBoundingClientRect();
      return { haut: b.top, basLogo: logo.bottom, coupe: n.scrollHeight > n.clientHeight + 1, gauche: b.left };
    });
    verifier(boite.haut >= boite.basLogo && boite.haut - boite.basLogo < 20, `sous le logo : ${JSON.stringify(boite)}`);
    verifier(!boite.coupe && boite.gauche < 20, `entier et à gauche : ${JSON.stringify(boite)}`);
    await vider(r);
    await r.click("#commandes-rail .menu-ligne:nth-of-type(3) button");
    verifier(
      (await messages(r)).some((m) => m.m === "ouvrir" && m.action.id === "veille"),
      "un clic dans le rail lance la commande"
    );
    verifier(!(await r.$("#commandes-rail")), "le menu du rail se referme");
    await r.close();

    const b = await page("chrome.html", { query: "?surface=barre" });
    await pousser(b, etatDeBase());
    await b.evaluate(() =>
      window.trame.menuCommandes([
        { id: "nouvel-onglet", titre: "Nouvel onglet", raccourci: "Ctrl+T" },
        { id: "veille", titre: "Endormir les onglets froids", raccourci: "" },
      ])
    );
    egal(
      await b.$$eval("#menu-commandes .n", (n) => n.map((x) => x.textContent)),
      ["Nouvel onglet", "Endormir les onglets froids"],
      "les commandes sont listées"
    );
    await vider(b);
    await b.click("#menu-commandes .menu-ligne:nth-of-type(3) button");
    verifier(
      (await messages(b)).some((m) => m.m === "ouvrir" && m.action && m.action.type === "commande" && m.action.id === "veille"),
      "un clic lance la commande"
    );
    verifier(await b.$eval("#menu-commandes", (n) => n.hidden), "le menu se referme");
    await b.close();
  });

  await cas("rail : le « + » d'un groupe ouvre un onglet DANS ce groupe", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    const e = etatDeBase();
    e.fils = [{ id: 7, nom: "Achats", onglets: [1] }];
    e.onglets = [onglet(1, 7, "Un", "https://a.fr/")];
    await pousser(p, e);
    verifier(!!(await p.$('.fil[data-fil="7"] .ajouter-fil')), "le groupe a son bouton « + »");
    await vider(p);
    await p.click('.fil[data-fil="7"] .ajouter-fil');
    const envoi = (await messages(p)).find((m) => m.m === "nouvelOnglet");
    egal(envoi && envoi.fil, 7, `il demande le nouvel onglet dans SON fil (${JSON.stringify(envoi)})`);
    // Le « + » de la tete du rail, lui, ne nomme aucun fil : l'hote le met dans le fil affiche.
    await vider(p);
    await p.click("#btn-nouvel");
    const sansFil = (await messages(p)).find((m) => m.m === "nouvelOnglet");
    egal(sansFil && sansFil.fil, undefined, "la tête du rail ne force aucun fil");
    await p.close();
  });

  await cas("rail : glisser un onglet entre deux onglets demande sa position", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    await p.setViewportSize({ width: 292, height: 700 });
    const e = etatDeBase();
    e.fils = [{ id: 1, nom: "Achats", onglets: [1, 2, 3] }];
    e.onglets = [
      onglet(1, 1, "Un", "https://a.fr/"),
      onglet(2, 1, "Deux", "https://b.fr/"),
      onglet(3, 1, "Trois", "https://c.fr/"),
    ];
    await pousser(p, e);

    const glisser = async (source, cible, moitie) => {
      const pt = await p.$eval(
        `.onglet[data-onglet="${cible}"]`,
        (n, m) => {
          const r = n.getBoundingClientRect();
          return { x: Math.round(r.left + 20), y: Math.round(m === "haut" ? r.top + 4 : r.bottom - 4) };
        },
        moitie
      );
      await vider(p);
      await p.evaluate(
        ({ source, cible, x, y }) => {
          const dt = new DataTransfer();
          const d = document.querySelector(`.onglet[data-onglet="${source}"]`);
          const c = document.querySelector(`.onglet[data-onglet="${cible}"]`);
          d.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
          c.dispatchEvent(new DragEvent("dragover", { bubbles: true, dataTransfer: dt, clientX: x, clientY: y }));
          c.dispatchEvent(new DragEvent("drop", { bubbles: true, dataTransfer: dt, clientX: x, clientY: y }));
        },
        { source, cible, x: pt.x, y: pt.y }
      );
      return (await messages(p)).find((m) => m.m === "ongletDeplacer");
    };

    const dessus = await glisser(3, 1, "haut");
    verifier(
      dessus && dessus.id === 3 && dessus.avant === 1 && dessus.fil === 1,
      `moitié haute : devant l'onglet visé (${JSON.stringify(dessus)})`
    );
    const dessous = await glisser(3, 1, "bas");
    verifier(
      dessous && dessous.id === 3 && dessous.avant === 2 && dessous.fil === 1,
      `moitié basse : juste après lui (${JSON.stringify(dessous)})`
    );
    await p.close();
  });

  await cas("accueil : l'entrée Navigation privée demande la navigation privée", async () => {
    // Une touche qu'on ne connait pas n'existe pas : l'entree est sur la page ou l'on commence.
    const p = await page("newtab.html");
    await pousser(p, etatDeBase());
    const texte = await p.$eval("#btn-prive-newtab", (n) => n.textContent.trim());
    verifier(texte.includes("Navigation privée"), `le bouton annonce la navigation privée : ${texte}`);
    await vider(p);
    await p.click("#btn-prive-newtab");
    egal(
      (await messages(p)).filter((m) => m.m === "prive"),
      [{ m: "prive" }],
      "un clic sur l'entrée envoie la commande"
    );
    await p.close();
  });

  await cas("accueil : sans accès rapide, la section invite à en ajouter", async () => {
    // Les accès rapides (sites) ont leur section, toujours là ; les favoris (pages) à part.
    const p = await page("newtab.html");
    await pousser(p, etatDeBase());
    verifier(await p.$eval("#bloc-acces", (n) => !n.hidden), "la section des accès rapides est visible");
    verifier((await p.$eval("#tuiles", (n) => n.textContent)).includes("Ajouter"), "l'invite cite « Ajouter »");
    egal(await p.$$eval("#tuiles .tuile.ajout", (n) => n.length), 1, "tuile Ajouter");
    await p.close();
  });

  await cas("reglages : mémoire de lecture (bascule, sites exclus, compte)", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), memoireLecture: true, lectureExclus: ["banque.fr"] };
    await pousser(p, etat);
    verifier((await messages(p)).some((m) => m.m === "lecturesCompter"), "le compte est demandé à l'ouverture");
    await p.evaluate(() => window.trame.lecturesCompte({ n: 42, mo: 1.5 }));
    verifier((await p.$eval("#txt-lectures", (n) => n.textContent)).includes("42 page(s)"), "compte affiché");
    await vider(p);
    await rubrique(p, "confidentialite");
    await p.click("#bascule-lecture");
    await p.click(".exclu");
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "memoireLecture" && m.actif === false), `bascule : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.m === "lectureInclure" && m.domaine === "banque.fr"), `exclus : ${JSON.stringify(recus)}`);
    await p.close();
  });

  // ---------------------------------------------------------------- sauvegarde du profil

  await cas("reglages : sauvegarde — le catalogue vient de l'hote, et Sauvegarder envoie les cases", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.sauvegarde = {
      contenus: [
        { cle: "reglages", libelle: "Réglages", detail: "moteur, veille", defaut: true },
        { cle: "session", libelle: "Fils et onglets", detail: "les fils", defaut: true },
        { cle: "ext-storage", libelle: "Stockage des extensions", detail: "mémoire", defaut: false },
      ],
      restauration: null,
    };
    await pousser(p, etat);
    egal(await p.$$eval("#sauvegarde-contenus label", (n) => n.length), 3, "une case par contenu");
    egal(
      await p.$$eval("#sauvegarde-contenus input:checked", (n) => n.map((i) => i.value)),
      ["reglages", "session"],
      "les cases cochees sont celles que l'hote annonce par defaut"
    );
    // Une poussee de l'hote ne doit pas reconstruire les cases (cela decocherait un choix).
    await rubrique(p, "donnees");
    await p.click("#sauvegarde-contenus input[value=session]");
    await pousser(p, etat);
    egal(await p.$$eval("#sauvegarde-contenus label", (n) => n.length), 3, "pas de doublon apres une poussee");
    egal(
      await p.$$eval("#sauvegarde-contenus input:checked", (n) => n.map((i) => i.value)),
      ["reglages"],
      "un choix decoche survit a une poussee"
    );
    await vider(p);
    await p.click("#btn-sauvegarder");
    const recus = await messages(p);
    const envoi = recus.find((m) => m.m === "sauvegardeEnregistrer");
    verifier(!!envoi, `aucun envoi : ${JSON.stringify(recus)}`);
    egal(envoi.contenus, ["reglages"], "les contenus coches partent avec la commande");
    await p.close();
  });

  await cas("reglages : sauvegarde automatique — listes de l'hote, et chaque reglage envoie son message", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.sauvegardeAuto = {
      actif: true,
      dossier: "D:\\sauvegardes",
      heures: 12,
      garder: 10,
      heuresPossibles: [1, 3, 12, 24, 72, 168],
      garderPossibles: [3, 5, 10, 20],
      enCours: false,
      derniere: 1795000000000,
      fichier: "Trame-2026-11-23-10h05m07.trame",
      erreur: "",
      effaces: 2,
      pret: true,
    };
    await pousser(p, etat);
    await rubrique(p, "donnees");
    egal(await p.$eval("#bascule-auto", (n) => n.className), "bascule on", "la bascule suit l'hote");
    egal(await p.$$eval("#auto-heures option", (n) => n.length), 6, "les intervalles viennent de l'hote");
    egal(await p.$$eval("#auto-garder option", (n) => n.length), 4, "les nombres gardes viennent de l'hote");
    egal(await p.$eval("#auto-heures", (n) => n.value), "12", "l'intervalle en cours est selectionne");
    egal(await p.$eval("#auto-garder", (n) => n.value), "10", "le nombre garde en cours est selectionne");
    const derniere = await p.$eval("#c-auto-derniere", (n) => n.textContent);
    verifier(
      derniere.includes("Trame-2026-11-23-10h05m07.trame") && derniere.includes("2 ancienne"),
      `la derniere sauvegarde est annoncee avec sa rotation : ${derniere}`
    );
    verifier(
      (await p.$eval("#c-auto-dossier", (n) => n.textContent)).includes("sauvegardes"),
      "le dossier choisi est annonce"
    );

    // Allumee sans dossier : l'interface doit le dire, plutot que de laisser croire que c'est
    // en place. C'est la regle tenue cote hote (`sauvegarde_auto_prete`).
    etat.sauvegardeAuto = { ...etat.sauvegardeAuto, dossier: "", pret: false };
    await pousser(p, etat);
    const sansDossier = await p.$eval("#c-auto", (n) => n.textContent);
    verifier(sansDossier.includes("Choisissez un dossier"), `l'absence de dossier est dite : ${sansDossier}`);
    verifier(
      await p.$eval("#btn-auto-maintenant", (n) => n.disabled),
      "sans dossier, « Sauvegarder maintenant » est desactive"
    );
    etat.sauvegardeAuto = { ...etat.sauvegardeAuto, dossier: "D:\\sauvegardes", pret: true };
    await pousser(p, etat);

    await vider(p);
    await p.click("#bascule-auto");
    await p.selectOption("#auto-heures", "24");
    await p.selectOption("#auto-garder", "5");
    await p.click("#btn-auto-dossier");
    await p.click("#btn-auto-maintenant");
    const recus = await messages(p);
    const par = (m) => recus.filter((x) => x.m === m);
    egal(par("sauvegardeAuto")[0]?.actif, false, "la bascule envoie le nouvel etat (eteinte)");
    egal(par("sauvegardeAutoHeures")[0]?.heures, 24, "l'intervalle choisi part en heures");
    egal(par("sauvegardeAutoGarder")[0]?.garder, 5, "le nombre a garder part");
    egal(par("sauvegardeAutoDossier").length, 1, "le bouton dossier demande le choix");
    egal(par("sauvegardeMaintenant").length, 1, "« Sauvegarder maintenant » envoie sa commande");
    await p.close();
  });

  await cas("reglages : restauration — le resume s'affiche, rien ne s'applique sans confirmation", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.sauvegarde = { contenus: [], restauration: null };
    await pousser(p, etat);
    egal(await p.$eval("#resume-restauration", (n) => n.hidden), true, "pas de resume sans archive");
    egal(await p.$eval("#ligne-confirmer", (n) => n.hidden), true, "pas de bouton Appliquer sans archive");
    etat.sauvegarde.restauration = {
      fichier: "C:\\Temp\\trame-2026-11-22-2140.trame",
      resume:
        "Sauvegarde du 2026-11-22 21:40.\n\n• Favoris — 4 favori(s) (actuellement 2) — 2 ajouté(s)" +
        "\n\nJamais dans une sauvegarde : les connexions et les cookies.",
      contenus: ["favoris"],
    };
    await vider(p);
    await pousser(p, etat);
    egal(await p.$eval("#resume-restauration", (n) => n.hidden), false, "le resume s'affiche");
    verifier(
      (await p.$eval("#resume-restauration", (n) => n.textContent)).includes("2 ajouté(s)"),
      "le resume est lisible"
    );
    verifier(
      (await p.$eval("#c-restauration", (n) => n.textContent)).includes("trame-2026-11-22-2140"),
      "le fichier est nomme"
    );
    egal((await messages(p)).length, 0, "afficher un resume n'applique rien de lui-meme");
    await rubrique(p, "donnees");
    await p.click("#btn-restaurer-confirmer");
    verifier(
      (await messages(p)).some((m) => m.m === "sauvegardeRestaurerConfirmer"),
      "Appliquer envoie la confirmation"
    );
    await vider(p);
    await p.click("#btn-restaurer-annuler");
    verifier(
      (await messages(p)).some((m) => m.m === "sauvegardeRestaurerAnnuler"),
      "Annuler envoie l'annulation"
    );
    await p.close();
  });

  await cas("reglages : favoris — exporter et importer envoient leur commande", async () => {
    const p = await page("reglages.html");
    await pousser(p, etatDeBase());
    await rubrique(p, "donnees");
    await vider(p);
    await p.click("#btn-favoris-exporter");
    await p.click("#btn-favoris-importer");
    const recus = (await messages(p)).map((m) => m.m);
    verifier(recus.includes("favorisExporterHtml"), `export : ${JSON.stringify(recus)}`);
    verifier(recus.includes("favorisImporterHtml"), `import : ${JSON.stringify(recus)}`);
    await p.close();
  });

  await cas("reglages : rubriques, recherche, vignettes et historique confirmé", async () => {
    const p = await page("reglages.html", { query: "#apparence" });
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), barreFenetre: "bande", nettoyerAdresses: false, parDefaut: false };
    await pousser(p, etat);
    // L'ancre ouvre sa rubrique, et une seule rubrique est montree.
    egal(await p.$$eval(".rubrique.vue", (n) => n.map((x) => x.id)), ["r-apparence"], "l'ancre ouvre Apparence");
    // La vignette choisie suit l'etat, et un clic sur une autre part vers l'hote.
    egal(await p.$eval("#choix-barre-fenetre .option.choisi", (n) => n.dataset.valeur), "bande", "vignette courante");
    await vider(p);
    await p.click('#choix-barre-fenetre .option[data-valeur="rail"]');
    verifier((await messages(p)).some((m) => m.m === "barreFenetre" && m.mode === "rail"), "la vignette envoie son choix");
    // Point orange : Trame n'est pas le navigateur par defaut.
    egal(await p.$eval("#nav-alerte", (n) => n.hidden), false, "Général signale ce qui attend");
    // L'affiliation se grise quand le nettoyage est coupe.
    egal(await p.$eval("#ligne-affiliation", (n) => n.classList.contains("gris")), true, "affiliation grisée");
    // La recherche montre les groupes qui repondent, toutes rubriques confondues, sans accents.
    await p.fill("#recherche", "PISTAGE");
    const visibles = await p.$$eval(".groupe", (g) => g.filter((x) => x.offsetParent).map((x) => x.closest(".rubrique").id));
    verifier(visibles.length > 0 && visibles.every((id) => id === "r-confidentialite"), `recherche : ${visibles}`);
    await p.fill("#recherche", "zzzz");
    egal(await p.$eval("#vide-recherche", (n) => n.hidden), false, "rien ne correspond");
    await p.fill("#recherche", "");
    egal(await p.$$eval(".rubrique.vue", (n) => n.map((x) => x.id)), ["r-apparence"], "la recherche videe rend la rubrique");
    // Vider l'historique demande confirmation : refuse, rien ne part.
    await rubrique(p, "confidentialite");
    await vider(p);
    p.once("dialog", (d) => d.dismiss());
    await p.click("#btn-vider-historique");
    verifier(!(await messages(p)).some((m) => m.m === "historiqueVider"), "refus : l'historique reste");
    p.once("dialog", (d) => d.accept());
    await p.click("#btn-vider-historique");
    verifier((await messages(p)).some((m) => m.m === "historiqueVider"), "accord : l'historique est vidé");
    await p.close();
  });

  // ---------------------------------------------------------------- rangement (2026-11-27)

  const rangementDeBase = () => ({
    actif: false,
    parMois: false,
    racine: "C:\\Users\\x\\Downloads",
    categories: [
      { id: "images", nom: "Images", dossier: "Images", extensions: ["jpg", "png", "gif", "webp", "heic"], actif: true },
      { id: "documents", nom: "Documents", dossier: "Documents", extensions: ["pdf", "odt"], actif: false },
    ],
    regles: [
      { id: 11, actif: true, sites: ["pronote.fr"], types: [], nomContient: "", dossier: "École", renommer: "" },
      { id: 12, actif: true, sites: [], types: ["pdf"], nomContient: "facture", dossier: "Papiers\\{année}", renommer: "{date} {nom}" },
    ],
    compteurs: { "École": 3, "Papiers\\2026": 2, "Images": 7 },
    essai: { nom: "a.pdf", site: "pronote.fr", chemin: "École\\a.pdf", raison: "règle « pronote.fr »" },
  });

  await cas("reglages : rubrique Téléchargements, rangement par types et par règles", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), rangement: rangementDeBase() };
    await pousser(p, etat);
    await rubrique(p, "telechargements");
    egal(await p.$$eval(".rubrique.vue", (n) => n.map((x) => x.id)), ["r-telechargements"], "la rubrique s'ouvre");
    // Coupé : grisé, et l'interrupteur l'annonce.
    egal(await p.$eval("#r-telechargements", (n) => n.classList.contains("rg-coupe")), true, "coupé = grisé");
    await vider(p);
    await p.click("#rg-actif");
    verifier((await messages(p)).some((m) => m.m === "rangementActif" && m.actif === true), "allumer part vers l'hôte");
    // Les tuiles : une par type, la coupée est éteinte, sa bascule l'allume.
    egal(await p.$$eval(".rg-cat .nom", (n) => n.map((x) => x.textContent)), ["Images", "Documents"], "tuiles");
    egal(await p.$$eval(".rg-cat", (n) => n.map((x) => x.classList.contains("eteinte"))), [false, true], "tuile coupée");
    await vider(p);
    await p.click(".rg-cat:nth-child(2) .bascule");
    verifier((await messages(p)).some((m) => m.m === "rangementCategorie" && m.id === "documents" && m.actif === true), "bascule d'un type");
    // Les règles se lisent en phrase, avec ce qu'elles ont déjà rangé ({année} compris).
    const phrases = await p.$$eval(".rg-regle .phrase", (n) => n.map((x) => x.textContent.replace(/\s+/g, " ").trim()));
    verifier(phrases[0].includes("pronote.fr") && phrases[0].includes("École"), `phrase 1 : ${phrases[0]}`);
    verifier(phrases[1].includes("PDF") && phrases[1].includes("facture") && phrases[1].includes("{date} {nom}"), `phrase 2 : ${phrases[1]}`);
    egal(await p.$$eval(".rg-regle .rg-compte", (n) => n.map((x) => x.textContent)), ["3 fichiers", "2 fichiers"], "compteurs");
    // Ajouter une règle : le formulaire part entier vers l'hôte.
    await p.click("#rg-ajouter");
    const champs = await p.$$(".rg-edition input");
    await champs[0].fill("ameli.fr");
    await champs[3].fill("Papiers\\Santé");
    await vider(p);
    await p.click(".rg-edition button.principal");
    const envoi = (await messages(p)).find((m) => m.m === "rangementRegle");
    verifier(envoi && envoi.regle.id === 0 && envoi.regle.sites === "ameli.fr" && envoi.regle.dossier === "Papiers\\Santé", `nouvelle règle : ${JSON.stringify(envoi)}`);
    // L'essai affiche le chemin complet et la raison données par Trame.
    const essai = await p.$eval("#rg-e-res", (n) => n.textContent);
    verifier(essai.includes("Downloads\\École\\a.pdf") && essai.includes("pronote.fr"), `essai : ${essai}`);
    // L'arborescence montre les dossiers de premier niveau et leurs comptes.
    const arbre = await p.$eval("#rg-arbre", (n) => n.textContent);
    verifier(arbre.includes("Downloads") && arbre.includes("École") && arbre.includes("Papiers") && arbre.includes("Images") && !arbre.includes("Documents"), `arbre : ${arbre}`);
    // La recherche trouve la rubrique par « ranger ».
    await p.fill("#recherche", "ranger");
    const visibles = await p.$$eval(".groupe", (g) => g.filter((x) => x.offsetParent).map((x) => x.closest(".rubrique").id));
    verifier(visibles.includes("r-telechargements"), `recherche « ranger » : ${visibles}`);
    await p.close();
  });

  // Constat de Clément (2026-11-27) : après « Enregistrer » ou Entrée, la tuile restait ouverte sur
  // les anciennes valeurs — le focus restait dans la tuile, et `rendre` ne la redessinait jamais.
  await cas("reglages : modifier un type ou une règle s'enregistre et se voit tout de suite", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    etat.reglages = { ...(etat.reglages || {}), rangement: rangementDeBase() };
    await pousser(p, etat);
    await rubrique(p, "telechargements");
    const pousserRangement = async (modifier) => {
      const r = rangementDeBase();
      modifier(r);
      await pousser(p, { ...etat, reglages: { ...etat.reglages, rangement: r } });
    };
    for (const geste of ["bouton", "Entrée"]) {
      // Un type : dossier et extensions (plus de 20, rien ne doit être coupé côté page).
      await p.click(".rg-cat:nth-child(1) .modifier");
      const [dossier, ext] = await p.$$(".rg-cat:nth-child(1) input");
      const extensions = Array.from({ length: 25 }, (_, i) => "e" + i).join(", ");
      await dossier.fill("Photos");
      await ext.fill(extensions);
      await vider(p);
      if (geste === "bouton") await p.click(".rg-cat:nth-child(1) button.principal");
      else await ext.press("Enter");
      const envoi = (await messages(p)).find((m) => m.m === "rangementCategorie");
      verifier(envoi && envoi.id === "images" && envoi.dossier === "Photos" && envoi.extensions === extensions, `${geste} : envoi ${JSON.stringify(envoi)}`);
      egal(await p.$$eval(".rg-cat input", (n) => n.length), 0, `${geste} : la tuile se referme`);
      // Trame renvoie l'état enregistré : la tuile le montre.
      await pousserRangement((r) => {
        r.categories[0].dossier = "Photos";
        r.categories[0].extensions = extensions.split(", ");
      });
      egal(await p.$eval(".rg-cat:nth-child(1) .vers code", (n) => n.textContent), "Photos\\", `${geste} : nouveau dossier affiché`);
      egal(await p.$eval(".rg-cat:nth-child(1) .rg-puce.plus", (n) => n.textContent), "+21", `${geste} : toutes les extensions comptées`);
      await pousserRangement(() => {});
    }
    // Une règle existante : modifier, Entrée, l'état revient, la phrase suit.
    const modifierRegle = async () => {
      for (const b of await p.$$(".rg-regle:nth-child(1) button")) if ((await b.textContent()) === "Modifier") return b.click();
      throw new Error("bouton Modifier de la règle absent");
    };
    await modifierRegle();
    let champs = await p.$$(".rg-edition input");
    await champs[3].fill("Scolarité");
    await vider(p);
    await champs[3].press("Enter");
    const regle = (await messages(p)).find((m) => m.m === "rangementRegle");
    verifier(regle && regle.regle.id === 11 && regle.regle.dossier === "Scolarité", `règle : ${JSON.stringify(regle)}`);
    egal(await p.$$eval(".rg-edition", (n) => n.length), 0, "le formulaire de la règle se referme");
    await pousserRangement((r) => (r.regles[0].dossier = "Scolarité"));
    const phrase = await p.$eval(".rg-regle:nth-child(1) .phrase", (n) => n.textContent);
    verifier(phrase.includes("Scolarité"), `phrase mise à jour : ${phrase}`);
    // Une règle sans condition : Trame la refusera, le formulaire garde la saisie.
    await p.click("#rg-ajouter");
    champs = await p.$$(".rg-edition input");
    await champs[3].fill("Divers");
    await champs[3].press("Enter");
    await pousserRangement((r) => (r.regles[0].dossier = "Scolarité"));
    egal(await p.$$eval(".rg-edition", (n) => n.length), 1, "règle refusée : le formulaire reste");
    egal(await p.$eval(".rg-edition input:nth-of-type(1)", () => [...document.querySelectorAll(".rg-edition input")][3].value), "Divers", "règle refusée : la saisie reste");
    await p.close();
  });

  await cas("telechargements : un fichier rangé dit où, et se remet à la racine", async () => {
    const p = await page("telechargements.html");
    const d = {
      id: 5, url: "https://x/a.pdf", fichier: "a.pdf", chemin: "C:\\dl\\École\\a.pdf", site: "pronote.fr",
      total: 10, recu: 10, etat: "termine", raison: "", debut: 1, fin: 2, existe: true,
      range: "École", rangeRaison: "règle « pronote.fr »",
    };
    await pousser(p, { ...etatDeBase(), telechargements: { actifs: 0, liste: [d], dossier: "C:\\dl", rangement: true, dernierTermine: 2 } });
    egal(await p.$eval(".meta .range", (n) => n.textContent), "→ École", "pastille du rangement");
    await vider(p);
    let clique = false;
    for (const b of await p.$$(".actions button")) {
      if ((await b.textContent()) === "Remettre à la racine") { await b.click(); clique = true; }
    }
    verifier(clique, "bouton « Remettre à la racine » présent");
    verifier((await messages(p)).some((m) => m.m === "telechargementRacine" && m.id === 5), "remettre à la racine part vers l'hôte");
    await p.close();
  });

  await cas("reglages : doublons, ranger l'existant (aperçu, ranger, défaire) et crédit", async () => {
    const p = await page("reglages.html");
    const etat = etatDeBase();
    const rg = rangementDeBase();
    rg.doublons = true;
    rg.existant = { apercu: null, dernier: null };
    etat.reglages = { ...(etat.reglages || {}), rangement: rg };
    await pousser(p, etat);
    await rubrique(p, "telechargements");
    // Doublons : allumés, la bascule les coupe ; jamais grisés quand le rangement est coupé.
    egal(await p.$eval("#rg-doublons", (n) => n.classList.contains("on")), true, "doublons allumés");
    const opacite = await p.$eval("#rg-doublons", (n) => getComputedStyle(n.closest(".groupe")).opacity);
    egal(opacite, "1", "les options ne se grisent pas, rangement coupé");
    await vider(p);
    await p.click("#rg-doublons");
    verifier((await messages(p)).some((m) => m.m === "rangementDoublons" && m.actif === false), "couper les doublons");
    // Aperçu demandé : rien ne bouge avant « Ranger ».
    await vider(p);
    await p.click("#rg-existant button");
    verifier((await messages(p)).some((m) => m.m === "rangementApercu"), "aperçu demandé");
    rg.existant = {
      apercu: { total: 5, restent: 2, coupe: false, groupes: [
        { dossier: "Images", n: 3, exemples: ["a.png", "b.jpg", "c.gif"] },
        { dossier: "Documents", n: 2, exemples: ["d.pdf", "e.odt"] },
      ] },
      dernier: null,
    };
    await pousser(p, etat);
    const texte = await p.$eval("#rg-existant", (n) => n.textContent);
    verifier(texte.includes("5 fichiers") && texte.includes("2 restent") && texte.includes("Images") && texte.includes("d.pdf"), `aperçu : ${texte}`);
    await vider(p);
    await p.click("#rg-existant button.principal");
    verifier((await messages(p)).some((m) => m.m === "rangementValider"), "ranger après l'aperçu");
    // Le dernier rangement se défait, après confirmation.
    rg.existant = { apercu: null, dernier: { date: Date.now(), n: 5 } };
    await pousser(p, etat);
    await vider(p);
    p.once("dialog", (d) => d.accept());
    const boutons = await p.$$("#rg-existant button");
    await boutons[boutons.length - 1].click();
    verifier((await messages(p)).some((m) => m.m === "rangementDefaire"), "défaire le dernier rangement");
    // Le crédit ouvre FolderFresh dans un onglet.
    await vider(p);
    await p.click("#rg-credit");
    verifier((await messages(p)).some((m) => m.m === "nouvelOnglet" && m.url.includes("Trihedron1240/FolderFresh")), "crédit vers FolderFresh");
    await p.close();
  });

  await cas("telechargements : un doublon se garde ou part à la Corbeille", async () => {
    const p = await page("telechargements.html");
    const d = {
      id: 6, url: "https://x/a.png", fichier: "a (2).png", chemin: "C:\\dl\\a (2).png", site: "x.fr",
      total: 10, recu: 10, etat: "termine", raison: "", debut: 1, fin: 2, existe: true,
      doublon: "C:\\dl\\Images\\a.png",
    };
    await pousser(p, { ...etatDeBase(), telechargements: { actifs: 0, liste: [d], dossier: "C:\\dl", dernierTermine: 2 } });
    egal(await p.$eval(".meta .doublon", (n) => n.textContent), "Déjà là : a.png", "pastille du doublon");
    const cliquer = async (texte) => {
      for (const b of await p.$$(".actions button")) if ((await b.textContent()) === texte) return b.click();
      throw new Error(`bouton « ${texte} » absent`);
    };
    await vider(p);
    await cliquer("Garder les deux");
    await cliquer("Mettre celui-ci à la Corbeille");
    const recus = (await messages(p)).map((m) => m.m);
    egal(recus, ["telechargementDoublonGarder", "telechargementDoublonJeter"], "les deux choix partent vers l'hôte");
    await p.close();
  });

  await cas("teintes : chacune repeint fonds, surfaces et accents de la coquille, Défaut rend l'origine", async () => {
    const p = await page("chrome.html", { query: "?surface=rail" });
    const lire = () =>
      p.evaluate(() => {
        const cs = getComputedStyle(document.documentElement);
        const v = (n) => cs.getPropertyValue(n).trim();
        return { fond: v("--fond"), surface: v("--surface-2"), accent: v("--accent"), rgb: v("--accent-rgb"), corps: getComputedStyle(document.body).backgroundColor };
      });
    const origine = await lire();
    const vus = new Set();
    for (const id of ["menthe", "ciel", "violet", "ambre", "ardoise"]) {
      await pousser(p, { ...etatDeBase(), theme: id });
      const t = await lire();
      const attendu = await p.evaluate((i) => window.TrameTeintes.TEINTES[i], id);
      egal(t.fond, attendu.fond, `${id} : fond`);
      egal(t.surface, attendu.surface2, `${id} : surface`);
      egal(t.accent, attendu.accent, `${id} : accent`);
      verifier(t.corps !== origine.corps, `${id} : le fond peint doit changer (${t.corps})`);
      verifier(t.rgb !== origine.rgb, `${id} : les transparences d'accent doivent suivre`);
      vus.add(t.fond + t.accent);
    }
    egal(vus.size, 5, "cinq teintes, cinq aspects distincts");
    await pousser(p, { ...etatDeBase(), theme: "dynamique", teinteSite: "#e11d48" });
    egal((await lire()).rgb, "225, 29, 72", "dynamique : l'accent du site, transparences comprises");
    // Une couleur de site en rgb() se lit sans passer par le navigateur (1,7 ms -> 0,005 ms).
    await pousser(p, { ...etatDeBase(), theme: "dynamique", teinteSite: "rgb(12, 200, 99)" });
    egal((await lire()).rgb, "12, 200, 99", "dynamique : couleur rgb() du site");
    egal(await p.evaluate(() => [window.TrameTeintes.rgb("rgba(1, 2, 3, .5)"), window.TrameTeintes.rgb("rgb(1 2 3 / 50%)"), window.TrameTeintes.rgb("hsl(0, 100%, 50%)"), window.TrameTeintes.rgb("rgb(300, 1, 1)")]), ["1, 2, 3", "1, 2, 3", "255, 0, 0", null], "rgba, syntaxe moderne, hsl par le navigateur, valeur hors bornes refusée");
    await pousser(p, { ...etatDeBase(), theme: "defaut" });
    const retour = await lire();
    egal(retour.fond, origine.fond, "défaut : fond d'origine");
    egal(retour.accent, origine.accent, "défaut : accent d'origine");
    egal(retour.rgb, origine.rgb, "défaut : transparences d'origine");
    await p.close();

    // Les pastilles des réglages : les mêmes couleurs, et un clic annonce la teinte à l'hôte.
    const r = await page("reglages.html", { query: "#apparence" });
    await pousser(r, { ...etatDeBase(), theme: "ambre" });
    egal(await r.$$eval(".teinte", (n) => n.map((x) => x.dataset.teinte)),
      ["defaut", "menthe", "ciel", "violet", "ambre", "ardoise", "dynamique"], "sept pastilles");
    await r.waitForFunction(() => document.querySelector(".teinte.choisi")?.dataset.teinte === "ambre");
    egal(await r.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim()), "#fbbf24", "la page prend la teinte");
    await vider(r);
    await r.click('.teinte[data-teinte="ciel"]');
    verifier((await messages(r)).some((m) => m.m === "theme" && m.valeur === "ciel"), "clic : teinte envoyée");
    await r.close();

    // Les pages internes suivent la même teinte, et reviennent à l'origine avec « Défaut ».
    for (const fichier of ["newtab.html", "telechargements.html", "zap.html", "trame.html"]) {
      const q = await page(fichier);
      const fondOrigine = await q.evaluate(() => getComputedStyle(document.body).backgroundColor);
      await pousser(q, { ...etatDeBase(), theme: "menthe" });
      const vu = await q.evaluate(() => {
        const cs = getComputedStyle(document.documentElement);
        return { fond: cs.getPropertyValue("--fond").trim(), rgb: cs.getPropertyValue("--accent-rgb").trim() };
      });
      egal(vu.fond, "#07100e", `${fichier} : fond menthe`);
      egal(vu.rgb, "52, 211, 153", `${fichier} : accent menthe`);
      await pousser(q, { ...etatDeBase(), theme: "defaut" });
      egal(await q.evaluate(() => getComputedStyle(document.body).backgroundColor), fondOrigine, `${fichier} : retour à l'origine`);
      await q.close();
    }
  });

  await cas("teintes : les scripts posés dans les pages prennent la palette que Trame leur donne", async () => {
    // Ce que fait src/teinte.rs : `__TRAME_TEINTE__` remplacé par l'objet de couleurs.
    const menthe = { fond: "#07100e", surface: "#0d1a18", surface2: "#122321", accent: "#34d399", rgb: "52, 211, 153", clair: "#aeedd6" };
    const avec = (fichier) => fs.readFileSync(path.join(UI, fichier), "utf8").replaceAll("__TRAME_TEINTE__", JSON.stringify(menthe));
    const p = await page("newtab.html");
    // Recherche dans la page (Ctrl+F)
    await p.evaluate(avec("recherche.js"));
    const barre = await p.$eval("#trame-recherche", (n) => ({ fond: getComputedStyle(n).backgroundColor, bord: getComputedStyle(n).borderTopColor }));
    egal(barre.fond, "rgb(13, 26, 24)", "recherche : fond menthe");
    verifier(barre.bord.startsWith("rgba(52, 211, 153"), `recherche : bord menthe (${barre.bord})`);
    // Sélecteur de Zap : le halo prend l'accent
    await p.evaluate(avec("zap-choisir.js"));
    const halo = await p.evaluate(() => [...document.querySelectorAll("div")].map((d) => d.style.borderColor || d.style.border).find((b) => b && b.includes("52, 211, 153")));
    verifier(!!halo, "zap : le halo prend l'accent menthe");
    // Sans palette (banc d'essai), les scripts gardent l'aspect d'origine au lieu de planter
    const q = await page("newtab.html");
    await q.evaluate(fs.readFileSync(path.join(UI, "recherche.js"), "utf8"));
    egal(await q.$eval("#trame-recherche", (n) => getComputedStyle(n).backgroundColor), "rgb(16, 21, 33)", "sans palette : fond d'origine");
    await q.close();
    await p.close();
  });

  // ---------------------------------------------------------------- contrat UI <-> hôte

  await cas("contrat : tout message envoye par l'interface est traite par l'hote", async () => {
    // Test statique, sans navigateur : il attrape la faute qui ne se voit qu'au clic —
    // un bouton qui envoie `chromeInstallerMagasin` alors que l'hote attend un autre nom.
    const fichiers = ["chrome.js", "newtab.html", "trame.html", "reglages.html", "zap.html", "raccourcis.js", "apercu.js", "aide.html"];
    const envoyes = new Set();
    for (const f of fichiers) {
      const texte = fs.readFileSync(path.join(UI, f), "utf8");
      for (const m of texte.matchAll(/m:\s*["']([a-zA-Z]+)["']/g)) envoyes.add(m[1]);
    }
    // Le coeur, ses outils (favoris, rendez-vous, cote a cote, guets : `src/app/outils.rs`) et
    // la barre de la fenetre (`src/app/fenetre.rs`).
    const app = ["app.rs", path.join("app", "outils.rs"), path.join("app", "fenetre.rs"), path.join("app", "aide.rs"), path.join("app", "depannage.rs")]
      .map((f) => fs.readFileSync(path.join(RACINE, "src", f), "utf8"))
      .join("\n");
    const traites = new Set();
    // Bras de match : on prend chaque ligne qui porte un `=>` et tous les noms qu'elle cite.
    for (const ligne of app.split("\n")) {
      if (!ligne.includes("=>")) continue;
      for (const nom of ligne.matchAll(/"([a-zA-Z]+)"/g)) traites.add(nom[1]);
    }
    verifier(envoyes.size > 30, `extraction suspecte : ${envoyes.size} messages`);
    const orphelins = [...envoyes].filter((m) => !traites.has(m)).sort();
    egal(orphelins, [], "ces messages ne sont traites par aucun bras de l'hote");
  });

  // ---------------------------------------------------------------- aide (2026-11-24)

  // `ui/notice.js` : du JSON strict apres `window.NOTICE =`.
  const lireNotice = () => {
    const t = fs.readFileSync(path.join(UI, "notice.js"), "utf8");
    return JSON.parse(t.slice(t.indexOf("{"), t.lastIndexOf("}") + 1));
  };

  await cas("aide : chaque raccourci reel de raccourcis.js figure dans la notice", async () => {
    // Le calque (Ctrl+/) et la Notice lisent `notice.js` : un raccourci ajoute dans
    // `raccourcis.js` sans ligne dans la notice serait un raccourci que personne ne connait.
    const reels = new Set();
    for (const m of fs.readFileSync(path.join(UI, "raccourcis.js"), "utf8").matchAll(/action = (?:e\.shiftKey \? )?"([a-zA-Z:]+)"/g)) reels.add(m[1]);
    for (const m of fs.readFileSync(path.join(UI, "raccourcis.js"), "utf8").matchAll(/action = "onglet:"/g)) reels.add("onglet:");
    const notice = lireNotice();
    const documentes = new Set(notice.raccourcis.map((r) => r.action).filter(Boolean));
    // Variantes d'une meme ligne : precedent avec suivant, zoom arriere avec zoom avant.
    for (const [variante, ligne] of [["ongletPrecedent", "ongletSuivant"], ["zoomMoins", "zoomPlus"]]) if (documentes.has(ligne)) documentes.add(variante);
    verifier(reels.size > 20, `extraction suspecte : ${reels.size} actions`);
    egal([...reels].filter((a) => !documentes.has(a)).sort(), [], "raccourcis absents de ui/notice.js");
  });

  await cas("aide : la Notice ouvre la fiche demandee, ses touches en <kbd>, et « Essayer » envoie la commande", async () => {
    const p = await page("aide.html", { query: "#cote-a-cote" });
    await pousser(p, { ...etatDeBase(), version: "0.4.62", reglages: { astuces: true } });
    egal(await p.$eval(".fiche h2", (n) => n.textContent), "Côte à côte : deux pages dans la fenêtre", "fiche du lien");
    verifier((await p.$$eval(".etape kbd", (n) => n.map((k) => k.textContent))).includes("Maj + clic"), "« Maj + clic » en touche");
    egal(await p.$eval("#version", (n) => n.textContent), "Version 0.4.62", "version affichee");
    verifier(await p.$eval("#bascule-astuces", (n) => n.classList.contains("on")), "astuces allumees");
    await vider(p);
    await p.click("text=Essayer maintenant");
    egal(await messages(p), [{ m: "ouvrir", action: { type: "commande", id: "cote" } }], "Essayer = commande cote");
    // Recherche : « split » ne garde que la fiche Cote a cote.
    await p.fill("#filtre", "split");
    egal(await p.$$eval("nav a", (n) => n.length), 1, "une seule fiche pour « split »");
    // Alt+F4 reste une seule touche.
    await p.evaluate(() => { location.hash = "video"; });
    await p.waitForFunction(() => document.querySelector(".fiche h2").textContent.startsWith("Vidéo"));
    verifier((await p.$$eval(".fiche kbd", (n) => n.map((k) => k.textContent))).includes("Alt+F4"), "Alt+F4 entier");
    await vider(p);
    await p.click("#btn-pdf");
    await p.click("#bascule-astuces");
    egal(await messages(p), [{ m: "aideNoticePdf" }, { m: "astuces", actif: false }], "PDF et bascule des astuces");
    await p.close();
  });

  await cas("aide : la version imprimable met toutes les fiches a la suite", async () => {
    const p = await page("aide.html", { query: "?impression&pdf&version=0.4.62" });
    egal(await p.$$eval(".document .fiche", (n) => n.length), lireNotice().chapitres.length, "une section par fiche");
    verifier((await p.$eval(".couverture .version", (n) => n.textContent)).startsWith("Version 0.4.62"), "version sur la couverture");
    await p.close();
  });

  await cas("aide : Ctrl+/ dessine le calque dans la barre, Echap le referme", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.evaluate(() => window.trame.calqueRaccourcis());
    verifier(await p.$eval("#calque-raccourcis", (n) => !n.hidden), "calque visible");
    const texte = await p.$eval("#calque-raccourcis", (n) => n.textContent);
    for (const t of ["Ctrl+Maj+T", "Afficher l'onglet à côté", "Vidéo dans une fenêtre à part", "Notice"]) {
      verifier(texte.includes(t), `« ${t} » absent du calque`);
    }
    verifier(await p.$eval("#btn-aide", (n) => n.classList.contains("ouvert")), "bouton ? allume");
    egal(await p.evaluate(() => window.__trame_echap()), true, "Echap consomme par le calque");
    verifier(await p.$eval("#calque-raccourcis", (n) => n.hidden), "calque ferme");
    egal(await p.evaluate(() => window.__trame_echap()), false, "Echap rendu a la page ensuite");
    await vider(p);
    await p.click("#btn-aide");
    egal(await messages(p), [{ m: "raccourci", action: "aide" }], "le bouton ? ouvre la Notice");
    await p.close();
  });

  await cas("aide : une fiche dans la Couture garde sa reponse lisible et son bouton Faire", async () => {
    const p = await page("chrome.html", { query: "?surface=barre" });
    await pousser(p, etatDeBase());
    await p.click("#couture");
    await p.type("#couture", "split", { delay: 10 });
    await p.waitForTimeout(150);
    const r = await p.evaluate(() => window.__ipc.filter((m) => m.m === "requete").pop().rid);
    await p.evaluate((rid) => window.trame.resultats({ rid, q: "split", items: [
      { genre: "aide", id: "aide:cote-a-cote", titre: "Côte à côte", sousTitre: "Maj + clic sur un onglet du rail, ou Maj + Entrée dans la barre.",
        action: { type: "aide", chapitre: "cote-a-cote", commande: "cote", faire: "Essayer maintenant" } },
    ] }), r);
    egal(await p.$eval(".resultat.aide .s", (n) => getComputedStyle(n).whiteSpace), "normal", "reponse sur deux lignes");
    await vider(p);
    await p.click(".resultat.aide .faire");
    egal((await messages(p)).filter((m) => m.m === "ouvrir"), [{ m: "ouvrir", action: { type: "commande", id: "cote" } }], "Faire = la commande, pas la fiche");
    await p.close();
  });

  await cas("aide : F1 et Ctrl+/ partent de n'importe quelle page", async () => {
    const p = await page("newtab.html");
    await p.addScriptTag({ path: path.join(UI, "raccourcis.js") });
    await vider(p);
    await p.keyboard.press("F1");
    await p.keyboard.press("Control+/");
    const recus = (await messages(p)).filter((m) => m.m === "raccourci").map((m) => m.action);
    verifier(recus.includes("aide") && recus.includes("raccourcisCalque"), `recu ${JSON.stringify(recus)}`);
    await p.close();
  });

  await cas("incrustation : libelles explicites, et l'hote apprend qu'une video flotte", async () => {
    const p = await page("newtab.html");
    await p.setContent(PAGE_VIDEO);
    await p.waitForFunction(() => document.getElementById("v").readyState > 0);
    await p.addScriptTag({ path: path.join(UI, "incrustation.js") });
    await p.mouse.move(420, 260);
    await p.mouse.move(421, 262);
    await p.waitForTimeout(250);
    await vider(p);
    await p.evaluate(() => {
      document.getElementById("v").dispatchEvent(new Event("enterpictureinpicture", { bubbles: true }));
    });
    const recus = (await messages(p)).filter((m) => m.m === "flottant");
    verifier(recus.some((m) => m.action === "incrustation"), `annonce attendue, recu ${JSON.stringify(recus)}`);
    // La pastille vit dans une racine fantome fermee : ses libelles se verifient dans la source.
    const source = fs.readFileSync(path.join(UI, "incrustation.js"), "utf8");
    verifier(source.includes('"Vidéo seule"') && source.includes('"Fenêtre à part"'), "libelles « Vidéo seule » et « Fenêtre à part »");
    await p.close();
  });

  await cas("menus : bornés à la fenêtre réelle, refermés quand elle change de taille", async () => {
    // Le menu ⋯ supposait 720 px de fenetre : sur un portable, son bas (Côte à côte → Quitter)
    // sortait de l'ecran. Et une fenetre redimensionnee laissait un panneau vide (2026-11-24).
    const p = await page("chrome.html", { query: "?surface=barre" });
    const etat = etatDeBase();
    etat.actif = 1;
    etat.onglets = [onglet(1, 1, "Page", "https://exemple.fr/", { actif: true })];
    etat.carnet = { pageWeb: true, favori: false, favoris: [], guets: [], guetsNonVus: 0 };
    etat.hauteurFenetre = 560;
    etat.largeurFenetre = 880;
    await pousser(p, etat);
    await p.click("#btn-page");
    const borne = await p.$eval("#menu-page .menu-colonne", (n) => parseFloat(n.style.maxHeight));
    verifier(borne > 150 && borne <= 560 - 40, `menu ⋯ borné à la fenêtre de 560 px : ${borne}`);
    await vider(p);
    await pousser(p, { ...etat, hauteurFenetre: 600, largeurFenetre: 1000 });
    verifier(await p.$eval("#menu-page", (n) => n.hidden), "la fenêtre change de taille : le menu se referme");
    verifier((await messages(p)).some((m) => m.m === "panneau" && m.open === false), "et l'hôte rend la place");
    // Meme taille : un etat ordinaire ne ferme rien.
    await p.click("#btn-page");
    await pousser(p, { ...etat, hauteurFenetre: 600, largeurFenetre: 1000 });
    verifier(await p.$eval("#menu-page", (n) => !n.hidden), "un état sans changement de taille laisse le menu ouvert");
    await p.close();
  });

  await cas("reglages : le portier tunnel s'allume, et son état se relit", async () => {
    const p = await page("reglages.html");
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: false, tunnel: null },
    });
    await rubrique(p, "confidentialite");
    egal(await p.$eval("#bascule-tunnel", (n) => n.className.includes("on")), false, "portier éteint au départ");
    verifier(
      (await p.$eval("#txt-tunnel", (n) => n.textContent)).includes("Aucun tunnel"),
      "l'état du tunnel est dit quand il n'y en a pas"
    );
    await vider(p);
    await p.click("#bascule-tunnel");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelOblige"),
      [{ m: "tunnelOblige", actif: true }],
      "la bascule demande le portier"
    );
    await vider(p);
    await p.click("#btn-tunnel-relire");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelReessayer"),
      [{ m: "tunnelReessayer", url: "" }],
      "Relire redemande l'état du réseau"
    );
    // Tunnel monté : le nom est affiché.
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: true, tunnel: "WireGuard Tunnel: wg0" },
    });
    egal(await p.$eval("#bascule-tunnel", (n) => n.className.includes("on")), true, "portier allumé");
    verifier(
      (await p.$eval("#txt-tunnel", (n) => n.textContent)).includes("WireGuard"),
      "le nom du tunnel détecté est affiché"
    );
    await p.close();
  });

  await cas("reglages : changer de tunnel propose de redémarrer Trame", async () => {
    const p = await page("reglages.html");
    const conf = { nom: "mullvad-fr", resume: "10.66.163.176/32 → 193.32.126.68:51820" };
    const questions = [];
    let reponse = false;
    p.on("dialog", async (d) => {
      questions.push(d.message());
      if (reponse) await d.accept();
      else await d.dismiss();
    });
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: false, tunnel: null },
    });
    await rubrique(p, "confidentialite");

    // 1. Aucune configuration importée : rien à monter, donc aucune question — le portier se
    //    contente de bloquer les sites.
    await vider(p);
    await p.click("#bascule-tunnel");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelOblige"),
      [{ m: "tunnelOblige", actif: true }],
      "sans configuration, la bascule passe seule"
    );
    egal(questions.length, 0, "sans configuration, aucune proposition de redémarrage");

    // 2. Configuration importée, proposition refusée : le réglage part, Trame reste ouvert.
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: false, tunnel: null, tunnelImporte: conf },
    });
    await vider(p);
    await p.click("#bascule-tunnel");
    await p.waitForTimeout(150);
    egal(questions.length, 1, "une configuration : la question du redémarrage est posée");
    verifier(questions[0].includes("Redémarrer Trame"), "la question parle de redémarrer Trame");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelOblige"),
      [{ m: "tunnelOblige", actif: true }],
      "le portier est allumé malgré le refus"
    );
    egal(
      (await messages(p)).filter((m) => m.m === "redemarrerTrame"),
      [],
      "refusé : Trame ne redémarre pas"
    );

    // 3. Tunnel armé, on le coupe, proposition acceptée : le réglage part et Trame redémarre.
    reponse = true;
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: "Tunnel intégré (WireGuard)",
        tunnelImporte: conf,
      },
    });
    await vider(p);
    await p.click("#bascule-tunnel");
    await p.waitForTimeout(150);
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelOblige"),
      [{ m: "tunnelOblige", actif: false }],
      "la coupure du portier part"
    );
    egal(
      (await messages(p)).filter((m) => m.m === "redemarrerTrame"),
      [{ m: "redemarrerTrame" }],
      "accepté : Trame redémarre pour retirer le tunnel"
    );

    // 4. Import alors que le portier est armé : c'est l'arrivée de la configuration qui
    //    déclenche la proposition, pas le clic (le sélecteur de Windows peut être annulé).
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: true, tunnel: null, tunnelImporte: conf },
    });
    await vider(p);
    const avant = questions.length;
    await p.click("#btn-tunnel-importer");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelImporter"),
      [{ m: "tunnelImporter" }],
      "Importer demande le fichier"
    );
    await p.waitForTimeout(150);
    egal(questions.length, avant, "un clic sur Importer ne pose pas encore la question");
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: null,
        tunnelImporte: { nom: "mullvad-se", resume: "10.66.1.2/32 → 185.65.135.1:51820" },
      },
    });
    await p.waitForTimeout(150);
    egal(questions.length, avant + 1, "la configuration arrivée déclenche la proposition");
    egal(
      (await messages(p)).filter((m) => m.m === "redemarrerTrame"),
      [{ m: "redemarrerTrame" }],
      "accepté : Trame redémarre pour monter le nouveau tunnel"
    );
    await p.close();
  });

  await cas("reglages : plusieurs configurations de tunnel — on bascule et on en supprime une", async () => {
    const p = await page("reglages.html");
    const questions = [];
    p.on("dialog", async (d) => {
      questions.push(d.message());
      await d.accept();
    });
    const tunnels = {
      actif: "mullvad-fr",
      liste: [
        { id: "mullvad-fr", nom: "mullvad-fr", resume: "10.66.0.2/32 → 193.32.126.68:51820", endpoint: "193.32.126.68:51820", actif: true },
        { id: "nord-fr", nom: "nord-fr", resume: "10.5.0.2/16 → 185.61.156.20:51820", endpoint: "185.61.156.20:51820", actif: false },
      ],
    };
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: "Tunnel intégré (WireGuard)",
        tunnelImporte: { nom: "mullvad-fr", resume: "10.66.0.2/32 → 193.32.126.68:51820" },
        tunnels,
      },
    });
    await rubrique(p, "confidentialite");

    // Une rangée par configuration ; seule l'active porte la marque et n'a pas « Utiliser ».
    egal(
      await p.$$eval("#liste-tunnels .tunnel-rangee", (n) => n.length),
      2,
      "les deux configurations sont listées"
    );
    egal(
      (await p.$eval("#liste-tunnels .tunnel-rangee.actif b", (n) => n.textContent)).includes("mullvad-fr"),
      true,
      "l'active est marquée"
    );
    egal(
      await p.$$eval("#liste-tunnels [data-utiliser]", (n) => n.map((b) => b.dataset.utiliser)),
      ["nord-fr"],
      "seule la configuration inactive propose « Utiliser »"
    );
    egal(await p.$eval("#txt-tunnel-conf", (n) => n.hidden), true, "le texte d'aide disparaît");

    // « Utiliser » : le choix part, et le redémarrage est proposé (le portier est armé).
    await vider(p);
    await p.click('[data-utiliser="nord-fr"]');
    await p.waitForTimeout(150);
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelChoisir"),
      [{ m: "tunnelChoisir", id: "nord-fr" }],
      "« Utiliser » demande cette configuration"
    );
    egal(
      (await messages(p)).filter((m) => m.m === "redemarrerTrame"),
      [{ m: "redemarrerTrame" }],
      "portier armé : le redémarrage est proposé"
    );
    egal(questions.length, 1, "une question de redémarrage, une seule");

    // « Supprimer » : la confirmation nomme la configuration, puis la commande porte son id.
    await vider(p);
    await p.click('[data-supprimer="nord-fr"]');
    await p.waitForTimeout(150);
    verifier(questions[1].includes("nord-fr"), "la confirmation nomme la configuration");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelSupprimer"),
      [{ m: "tunnelSupprimer", id: "nord-fr" }],
      "« Supprimer » vise cette configuration"
    );
    await p.close();
  });

  await cas("reglages : le tunnel qui n'a pas monté dit pourquoi", async () => {
    const p = await page("reglages.html");
    const raison = "aucune poignée de main — le pair ne répond sur aucune adresse ; adresses essayées : 187.14.108.216:51820";
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: null,
        tunnelImporte: { nom: "nord-fr", resume: "10.5.0.2/16 → 187.14.108.216:51820" },
        tunnelErreur: raison,
      },
    });
    await rubrique(p, "confidentialite");
    egal(await p.$eval("#ligne-tunnel-erreur", (n) => n.hidden), false, "la raison est affichée");
    egal(
      (await p.$eval("#txt-tunnel-erreur", (n) => n.textContent)).includes("187.14.108.216"),
      true,
      "la raison nomme l'adresse essayée"
    );
    // Le tunnel monte : la ligne disparaît.
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: "Tunnel intégré (WireGuard)",
        tunnelImporte: { nom: "nord-fr", resume: "10.5.0.2/16 → 187.14.108.216:51820" },
        tunnelErreur: null,
      },
    });
    egal(await p.$eval("#ligne-tunnel-erreur", (n) => n.hidden), true, "elle disparaît quand le tunnel monte");
    await p.close();
  });

  await cas("page du tunnel : elle dit pourquoi le tunnel n'a pas monté", async () => {
    const raison = "aucune poignée de main — adresses essayées : 187.14.108.216:51820";
    const p = await page("tunnel.html", {
      query: `?url=${encodeURIComponent("https://example.com/x")}&erreur=${encodeURIComponent(raison)}`,
      sansEtat: true,
    });
    egal(
      (await p.$eval("#suite", (n) => n.textContent)).includes("Le tunnel n'a pas pu se monter"),
      true,
      "la page annonce l'échec"
    );
    egal(
      (await p.$eval("#suite", (n) => n.textContent)).includes("187.14.108.216"),
      true,
      "et donne la raison du moteur, adresse comprise"
    );
    await p.close();
  });

  await cas("reglages : importer un profil OpenVPN envoie aussi les identifiants", async () => {
    const p = await page("reglages.html");
    await pousser(p, {
      ...etatDeBase(),
      reglages: { astuces: true, tunnelOblige: false, tunnel: null },
    });
    await rubrique(p, "confidentialite");
    // Sans identifiants : le message reste nu (un `.conf` WireGuard n'en veut pas).
    await vider(p);
    await p.click("#btn-tunnel-importer");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelImporter"),
      [{ m: "tunnelImporter" }],
      "sans identifiants, l'import ne les envoie pas"
    );
    // Avec identifiants (profil .ovpn Nord) : ils partent avec l'import, jamais affichés ailleurs.
    await vider(p);
    await p.fill("#tunnel-utilisateur", "service-nord");
    await p.fill("#tunnel-motdepasse", "secret-de-test");
    await p.click("#btn-tunnel-importer");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelImporter"),
      [{ m: "tunnelImporter", utilisateur: "service-nord", motdepasse: "secret-de-test" }],
      "les identifiants accompagnent l'import"
    );
    // La liste distingue les deux genres : « openvpn udp serveur ».
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: null,
        tunnels: {
          actif: "nord-fr",
          liste: [
            { id: "nord-fr", nom: "nord-fr", resume: "openvpn udp 193.32.126.68", endpoint: "193.32.126.68", actif: true },
            { id: "mullvad-fr", nom: "mullvad-fr", resume: "10.66.0.2/32 → 193.32.126.68:51820", endpoint: "193.32.126.68:51820", actif: false },
          ],
        },
      },
    });
    egal(
      (await p.$$eval("#liste-tunnels .tunnel-rangee", (n) => n.length)),
      2,
      "les deux genres sont listés ensemble"
    );
    egal(
      (await p.$eval("#liste-tunnels .tunnel-rangee.actif", (n) => n.textContent)).includes("openvpn udp"),
      true,
      "l'actif dit son protocole"
    );
    await p.close();
  });

  await cas("reglages : le bouton Enregistrer sauve les identifiants du profil actif", async () => {
    const p = await page("reglages.html");
    const questions = [];
    p.on("dialog", async (d) => {
      questions.push(d.message());
      await d.accept();
    });
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: null,
        tunnels: {
          actif: "nord-fr",
          liste: [{ id: "nord-fr", nom: "nord-fr", resume: "openvpn udp 187.14.106.91", endpoint: "187.14.106.91", actif: true }],
        },
      },
    });
    await rubrique(p, "confidentialite");
    // Sans utilisateur : rien ne part, et le focus revient sur le champ.
    await vider(p);
    await p.click("#btn-tunnel-identifiants");
    await p.waitForTimeout(150);
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelIdentifiants"),
      [],
      "sans utilisateur, rien n'est envoyé"
    );
    egal(questions.length, 1, "l'interface le dit");
    // Avec les deux champs : le message porte les identifiants, puis la proposition de redémarrage.
    await vider(p);
    await p.fill("#tunnel-utilisateur", "service-nord");
    await p.fill("#tunnel-motdepasse", "secret-de-test");
    await p.click("#btn-tunnel-identifiants");
    await p.waitForTimeout(150);
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelIdentifiants"),
      [{ m: "tunnelIdentifiants", utilisateur: "service-nord", motdepasse: "secret-de-test" }],
      "Enregistrer envoie les identifiants"
    );
    // L'hôte répond en poussant l'état avec un nouvel instant d'import : c'est ce témoin qui
    // déclenche la proposition, l'identité de la configuration n'ayant pas changé.
    await pousser(p, {
      ...etatDeBase(),
      reglages: {
        astuces: true,
        tunnelOblige: true,
        tunnel: null,
        tunnelImporteLe: 1234567,
        tunnels: {
          actif: "nord-fr",
          liste: [{ id: "nord-fr", nom: "nord-fr", resume: "openvpn udp 187.14.106.91", endpoint: "187.14.106.91", actif: true }],
        },
      },
    });
    await p.waitForTimeout(200);
    egal(
      (await messages(p)).filter((m) => m.m === "redemarrerTrame"),
      [{ m: "redemarrerTrame" }],
      "portier armé : le redémarrage est proposé"
    );
    egal(await p.$eval("#tunnel-motdepasse", (n) => n.value), "", "le mot de passe est vidé du champ");
    await p.close();
  });

  await cas("page du tunnel : elle dit l'adresse visée, et « Réessayer » la redemande", async () => {
    const p = await page("tunnel.html", { query: "?url=https%3A%2F%2Fexample.com%2Fx", sansEtat: true });
    verifier(
      (await p.$eval("#cible", (n) => n.textContent)).includes("https://example.com/x"),
      "l'adresse bloquée est affichée"
    );
    await vider(p);
    await p.click("#btn-reessayer");
    egal(
      (await messages(p)).filter((m) => m.m === "tunnelReessayer"),
      [{ m: "tunnelReessayer", url: "https://example.com/x" }],
      "« Réessayer » redemande l'adresse bloquée"
    );
    await vider(p);
    await p.click("#btn-reglages");
    verifier((await messages(p)).some((m) => m.m === "ouvrirReglages"), "le bouton mène aux réglages");
    await p.close();
  });

  await cas("reglages : dépannage — réparer demande confirmation, le rapport part tout de suite", async () => {
    const p = await page("reglages.html");
    await pousser(p, { ...etatDeBase(), reglages: { astuces: true } });
    await rubrique(p, "general");
    await vider(p);
    // Refus : rien ne part.
    p.once("dialog", (d) => d.dismiss());
    await p.click("#btn-reparer");
    await p.waitForTimeout(100);
    egal((await messages(p)).filter((m) => m.m === "trameReparer").length, 0, "réparation refusée");
    p.once("dialog", (d) => d.accept());
    await p.click("#btn-reparer");
    await p.click("#btn-rapport");
    await p.waitForTimeout(100);
    egal((await messages(p)).map((m) => m.m).filter((m) => m.startsWith("trame")), ["trameReparer", "trameRapport"], "réparation puis rapport");
    await p.close();
  });

  // Saute-pub : le voile gris d'une fenetre YouTube cachee bloquait la video (2026-11-25).
  const PAGE_VOILE = (fenetre) => `<!doctype html><meta charset="utf-8"><body style="margin:0;height:900px">
    <div class="html5-video-player" style="position:absolute;left:100px;top:80px;width:640px;height:360px;background:#123"><video></video></div>
    <tp-yt-iron-overlay-backdrop opened style="position:fixed;inset:0;background:rgba(0,0,0,.6);z-index:2201;display:block"></tp-yt-iron-overlay-backdrop>
    <ytd-popup-container>${fenetre}</ytd-popup-container></body>`;
  const DIALOGUE = (dedans) =>
    `<tp-yt-paper-dialog style="position:fixed;left:900px;top:100px;width:300px;height:200px;z-index:2202;display:block;background:#fff">${dedans}</tp-yt-paper-dialog>`;
  const ouvrirVoile = async (html) => {
    const p = await page("newtab.html");
    await p.setContent(html);
    await p.addStyleTag({ path: path.join(RACINE, "extensions", "saute-pub", "style.css") });
    await p.addScriptTag({ path: path.join(RACINE, "extensions", "saute-pub", "contenu.js") });
    return p;
  };
  const sousLaVideo = (p) => p.evaluate(() => document.elementFromPoint(420, 260)?.tagName.toLowerCase());

  await cas("saute-pub : le voile gris de la fenêtre « bloqueurs » est retiré, la vidéo redevient cliquable", async () => {
    const p = await ouvrirVoile(PAGE_VOILE(DIALOGUE("<ytd-enforcement-message-view-model>Les bloqueurs de publicités ne sont pas autorisés</ytd-enforcement-message-view-model>")));
    egal(await sousLaVideo(p), "tp-yt-iron-overlay-backdrop", "au départ, le voile prend les clics");
    await p.waitForTimeout(2000);
    egal(await sousLaVideo(p), "div", "le voile ne recouvre plus la vidéo");
    egal(await p.evaluate(() => window.__trameSautePub.etat().voilesFermes), 1, "un voile fermé, compté");
    await p.close();
  });

  await cas("saute-pub : une vraie fenêtre de YouTube garde son voile", async () => {
    const p = await ouvrirVoile(PAGE_VOILE(DIALOGUE("<div>Partager</div>")));
    await p.waitForTimeout(2200);
    egal(await sousLaVideo(p), "tp-yt-iron-overlay-backdrop", "le voile de « Partager » reste");
    await p.close();
  });

  await cas("saute-pub : un voile qui précède de peu sa fenêtre n'est pas touché", async () => {
    const p = await ouvrirVoile(PAGE_VOILE(""));
    await p.waitForTimeout(500);
    await p.evaluate((html) => document.querySelector("ytd-popup-container").insertAdjacentHTML("beforeend", html), DIALOGUE("<div>Enregistrer</div>"));
    await p.waitForTimeout(2000);
    egal(await sousLaVideo(p), "tp-yt-iron-overlay-backdrop", "le voile de la fenêtre arrivée en retard reste");
    await p.close();
  });

  // ---------------------------------------------------------------- fin

  await navigateur.close();
  serveur.close();

  const echecs = resultats.filter((r) => !r.ok);
  console.log(
    `\n${resultats.length - echecs.length}/${resultats.length} cas passes` +
      (echecs.length ? `, ${echecs.length} echec(s)` : "")
  );
  for (const e of echecs) console.log(`  - ${e.nom}: ${e.erreur}`);
  process.exit(echecs.length ? 1 : 0);
}

suite().catch((e) => {
  console.error("suite interrompue :", e);
  process.exit(2);
});
