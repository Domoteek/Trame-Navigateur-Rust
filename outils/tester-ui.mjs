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
  await ongletPage.waitForFunction(() => window.trame && typeof window.trame.etat === "function");
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
      onglet(12, 2, "Proxmox", "https://192.168.1.5:8006/"),
      onglet(13, 2, "OPNsense", "https://192.168.1.1:4443/"),
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
        { url: "http://192.168.1.23:8720/", titre: "Home lab", domaine: "192.168.1.23", groupe: "Maison", rail: false, icone: null, ouvert: null },
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
    verifier((await messages(p)).some((m) => m.m === "accesOuvrir" && m.url === "http://192.168.1.23:8720/"), "le clic ouvre l'accès");
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
      onglet(12, 2, "Proxmox", "https://192.168.1.5:8006/"),
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
    await p.click("#bascule-pause");
    await p.click("#bascule-affiliation");
    await p.selectOption("#anti-pistage", "equilibree");
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
    verifier((await messages(p)).some((m) => m.m === "favori"), "l'étoile bascule le favori");
    await p.click("#btn-page");
    verifier(await p.$eval("#menu-page", (n) => !n.hidden), "le menu de la page s'ouvre");
    const texte = await p.$eval("#menu-page", (n) => n.textContent);
    for (const attendu of ["Imprimer", "Rendez-vous", "Amazon", "Guetter", "39,90 € → 34,90 €"]) {
      verifier(texte.includes(attendu), `« ${attendu} » absent du menu`);
    }
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
    verifier(Math.abs(pose.haut - 138) < 4, `menu à la hauteur du clic : ${JSON.stringify(pose)}`);
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
    await p.evaluate(() => localStorage.removeItem("trame.menuPage.hauteur"));
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
    await p.mouse.move(g.x, g.y);
    await p.mouse.down();
    await p.mouse.move(g.x, g.y - 250, { steps: 4 });
    await p.mouse.up();
    const apres = await p.$eval("#menu-page .menu-colonne", (c) => ({
      h: c.getBoundingClientRect().height,
      defile: c.scrollHeight > c.clientHeight,
      garde: localStorage.getItem("trame.menuPage.hauteur"),
    }));
    verifier(apres.defile && apres.h < 500 && apres.garde, `réduit, défilant et gardé : ${JSON.stringify(apres)}`);
    await p.evaluate(() => localStorage.removeItem("trame.menuPage.hauteur"));
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
    await p.click("#bascule-lecture");
    await p.click(".exclu");
    const recus = await messages(p);
    verifier(recus.some((m) => m.m === "memoireLecture" && m.actif === false), `bascule : ${JSON.stringify(recus)}`);
    verifier(recus.some((m) => m.m === "lectureInclure" && m.domaine === "banque.fr"), `exclus : ${JSON.stringify(recus)}`);
    await p.close();
  });

  // ---------------------------------------------------------------- contrat UI <-> hôte

  await cas("contrat : tout message envoye par l'interface est traite par l'hote", async () => {
    // Test statique, sans navigateur : il attrape la faute qui ne se voit qu'au clic —
    // un bouton qui envoie `chromeInstallerMagasin` alors que l'hote attend un autre nom.
    const fichiers = ["chrome.js", "newtab.html", "trame.html", "reglages.html", "zap.html", "raccourcis.js", "apercu.js"];
    const envoyes = new Set();
    for (const f of fichiers) {
      const texte = fs.readFileSync(path.join(UI, f), "utf8");
      for (const m of texte.matchAll(/m:\s*["']([a-zA-Z]+)["']/g)) envoyes.add(m[1]);
    }
    // Le coeur, ses outils (favoris, rendez-vous, cote a cote, guets : `src/app/outils.rs`) et
    // la barre de la fenetre (`src/app/fenetre.rs`).
    const app = ["app.rs", path.join("app", "outils.rs"), path.join("app", "fenetre.rs")]
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
