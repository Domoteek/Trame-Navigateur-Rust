#!/usr/bin/env node
/**
 * Banc d'essai de TOUS les menus de Trame, dans le vrai binaire caché, à plusieurs tailles de
 * fenêtre — puis le redimensionnement d'une fenêtre dont un menu est ouvert.
 *
 * Demande de Clément du 2026-11-24 : le menu ⋯ « ne marche pas » sur le portable de son fils
 * (il sortait coupé d'une fenêtre de moins de 720 px), et « ouvre un menu et redimensionne la
 * fenêtre : le menu reste affiché mais avec rien dedans ».
 *
 * Pour chaque taille, chaque menu est ouvert comme le ferait la souris, puis on exige :
 *   - il est visible et a un vrai contenu ;
 *   - il tient dans la webview qui le porte (bas et droite) — sinon le bas est coupé ;
 *   - la région découpée par l'hôte le contient (sinon : panneau vide, menu invisible).
 * Puis, menu ⋯ ouvert, la fenêtre est redimensionnée (plus étroite, puis plus large) : le menu
 * doit soit se refermer proprement, soit rester entier et visible — jamais un panneau vide.
 *
 *   node outils/eprouver-menus.mjs        (TAILLES=1440x900,880x560 pour choisir)
 */
import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { attendreCible, connecter, pause } from "./cdp.mjs";

const ICI = path.dirname(fileURLToPath(import.meta.url));
const RACINE = path.resolve(ICI, "..");
const PORT = Number(process.env.TRAME_PORT_ESSAI || 19464);
const PORT_PAGE = 8128;
const BINAIRE = process.env.TRAME_BINAIRE || path.join(RACINE, "target", "release", "trame.exe");
const TAILLES = (process.env.TAILLES ?? "1440x900,1280x720,1100x650,880x560").split(",").filter(Boolean);

let echecs = 0;
const ok = (condition, quoi) => {
  console.log(`  ${condition ? "ok  " : "ECHEC"} ${quoi}`);
  if (!condition) echecs += 1;
};

const serveur = http.createServer((_, rep) => {
  rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  rep.end(`<!doctype html><meta charset="utf-8"><title>Page d'essai</title><p>Une page web ordinaire.</p>`);
});
await new Promise((r) => serveur.listen(PORT_PAGE, "127.0.0.1", r));

function redimensionner(pid, l, h) {
  const r = spawnSync("powershell", ["-NoProfile", "-File", path.join(ICI, "redimensionner-fenetre.ps1"), "-ProcessId", String(pid), "-Largeur", String(l), "-Hauteur", String(h)], { encoding: "utf8" });
  return (r.stdout || "").trim();
}

// Mesure d'un menu dans la barre : la colonne visible du panneau, et la region de l'hote.
const MESURE_BARRE = `(() => {
  const zones = ["menu-page", "extensions", "menu-commandes", "calque-raccourcis", "resultats"]
    .map((id) => document.getElementById(id)).filter((z) => z && !z.hidden);
  const z = zones[0];
  if (!z) return { visible: false };
  const col = z.querySelector(".menu-colonne, .menu-acces-rail") || z;
  const r = col.getBoundingClientRect();
  return { visible: true, zone: z.id, contenu: col.children.length, texte: (col.textContent || "").trim().length,
    haut: Math.round(r.height), bas: Math.round(r.bottom), gauche: Math.round(r.left), droite: Math.round(r.right),
    viewport: window.innerHeight, largeur: window.innerWidth };
})()`;

async function fermerTout(barre, rail) {
  await barre.evaluer(`window.dispatchEvent(new Event('blur')), true`);
  await rail.evaluer(`window.dispatchEvent(new Event('blur')), true`);
  await pause(500);
}

function juger(nom, m) {
  ok(m.visible && m.contenu > 0 && m.texte > 10, `${nom} : ouvert, avec du contenu (${m.zone || "rien"})`);
  if (!m.visible) return;
  ok(m.haut > 40 && m.bas <= m.viewport + 1, `${nom} : tient en hauteur (bas ${m.bas} ≤ ${m.viewport})`);
  ok(m.gauche >= 0 && m.droite <= m.largeur + 1, `${nom} : tient en largeur (${m.gauche}…${m.droite} / ${m.largeur})`);
}

async function cliquer(cdp, selecteur) {
  const c = await cdp.evaluer(`(() => { const r = document.querySelector(${JSON.stringify(selecteur)}).getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; })()`);
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.envoyer("Input.dispatchMouseEvent", { type, x: c.x, y: c.y, button: "left", clickCount: 1 });
  }
}

async function lancer(taille) {
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-menus-"));
  const enfant = spawn(BINAIRE, [], {
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      TRAME_TAILLE: taille,
      TRAME_NAV: `http://127.0.0.1:${PORT_PAGE}/`,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT} --remote-debugging-address=127.0.0.1`,
    },
    stdio: "ignore",
  });
  await attendreCible(PORT, `127.0.0.1:${PORT_PAGE}`, 45000);
  const barre = await connecter((await attendreCible(PORT, "surface=barre")).webSocketDebuggerUrl);
  const rail = await connecter((await attendreCible(PORT, "surface=rail")).webSocketDebuggerUrl);
  await barre.envoyer("Runtime.enable");
  await rail.envoyer("Runtime.enable");
  await barre.attendre(`!!(window.trame && document.getElementById('btn-page'))`, { quoi: "la barre" });
  await pause(1500);
  const arreter = async () => {
    barre.fermer();
    rail.fermer();
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    await pause(1500);
    try {
      fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch {
      /* profil garde */
    }
  };
  return { barre, rail, pid: enfant.pid, arreter };
}

async function tousLesMenus(taille) {
  console.log(`\nfenêtre ${taille} — tous les menus`);
  const { barre, rail, arreter } = await lancer(taille);
  try {
    const hauteurRail = await rail.evaluer(`window.innerHeight`);
    const essais = [
      ["menu ⋯", () => cliquer(barre, "#btn-page")],
      ["puzzle des extensions", () => cliquer(barre, "#btn-ext")],
      ["menu d'un onglet (clic droit, haut du rail)", () => rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuPage", id: 1, y: 120 })), true`)],
      ["menu d'un onglet (clic droit, bas du rail)", () => rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuPage", id: 1, y: ${hauteurRail - 40} })), true`)],
      ["commandes (rail replié)", () => barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuCommandes", ici: false })), true`)],
      ["calque des raccourcis", () => barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "raccourci", action: "raccourcisCalque" })), true`)],
      ["résultats de la Couture", async () => {
        await cliquer(barre, "#couture");
        await barre.evaluer(`(() => { const c = document.getElementById('couture'); c.value = 'a'; c.dispatchEvent(new Event('input', { bubbles: true })); return true; })()`);
      }],
    ];
    for (const [nom, ouvrir] of essais) {
      await ouvrir();
      await pause(1200);
      juger(nom, await barre.evaluer(MESURE_BARRE));
      await fermerTout(barre, rail);
    }
    // Commandes du rail deplie : le rail les dessine lui-meme, sous le logo.
    await rail.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "menuCommandes", ici: true })), true`);
    await pause(900);
    const c = await rail.evaluer(`(() => { const m = document.getElementById('commandes-rail'); if (!m) return { visible: false }; const r = m.getBoundingClientRect();
      return { visible: true, zone: 'commandes-rail', contenu: m.children.length, texte: m.textContent.trim().length, haut: Math.round(r.height), bas: Math.round(r.bottom),
        gauche: Math.round(r.left), droite: Math.round(r.right), viewport: window.innerHeight, largeur: window.innerWidth }; })()`);
    juger("commandes (rail déplié)", c);
    await fermerTout(barre, rail);
  } finally {
    await arreter();
  }
}

async function redimensionnerMenuOuvert() {
  console.log(`\nmenu ouvert puis fenêtre redimensionnée`);
  const { barre, rail, pid, arreter } = await lancer("1280x800");
  try {
    for (const [nom, ouvrir] of [
      ["menu ⋯", () => cliquer(barre, "#btn-page")],
      ["puzzle des extensions", () => cliquer(barre, "#btn-ext")],
      ["calque des raccourcis", () => barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "raccourci", action: "raccourcisCalque" })), true`)],
    ]) {
      for (const [l, h] of [[1000, 600], [1400, 860]]) {
        // Journal pris sur le moment : qui ouvre, qui ferme, quels etats arrivent.
        await barre.evaluer(`(() => {
          window.__journal = [];
          const t0 = performance.now();
          const note = (q) => window.__journal.push(Math.round(performance.now() - t0) + ' ' + q);
          if (!window.__espion) {
            window.__espion = true;
            const e = window.trame.etat;
            window.trame.etat = (s) => { (window.__note || (() => {}))('etat h=' + s.hauteurFenetre); e(s); };
            new MutationObserver(() => (window.__note || (() => {}))('panneau ' + (document.getElementById('panneau').hidden ? 'cache' : 'montre')))
              .observe(document.getElementById('panneau'), { attributes: true, attributeFilter: ['hidden'] });
            window.addEventListener('resize', () => (window.__note || (() => {}))('resize ' + innerWidth + 'x' + innerHeight));
            window.addEventListener('blur', () => (window.__note || (() => {}))('blur'));
          }
          window.__note = note;
          return true;
        })()`);
        await ouvrir();
        await pause(1200);
        const avant = await barre.evaluer(MESURE_BARRE);
        if (!avant.visible) console.log("      journal : " + (await barre.evaluer("window.__journal.join(' | ')")));
        const r = redimensionner(pid, l, h);
        await pause(1500);
        const apres = await barre.evaluer(MESURE_BARRE);
        // Ce que l'on voit vraiment : la barre est-elle restee grande (panneau) alors que le
        // menu n'est plus la, ou plus dans la region decoupee ?
        const vide = apres.viewport > 120 && (!apres.visible || apres.texte < 10);
        const coupe = apres.visible && (apres.bas > apres.viewport + 1 || apres.droite > apres.largeur + 1);
        ok(r === "ok" && avant.visible, `${nom} → ${l}x${h} : menu ouvert puis fenêtre redimensionnée (${r}${avant.visible ? "" : ", menu pas ouvert : " + JSON.stringify(await barre.evaluer("({ panneau: !document.getElementById('panneau').hidden, cache: document.getElementById('calque-raccourcis').hidden, h: window.innerHeight, w: window.innerWidth })"))})`);
        ok(!vide, `${nom} → ${l}x${h} : pas de panneau vide (barre ${apres.viewport} px, menu ${apres.visible ? "visible" : "fermé"})`);
        ok(!coupe, `${nom} → ${l}x${h} : rien de coupé (bas ${apres.bas} / ${apres.viewport}, droite ${apres.droite} / ${apres.largeur})`);
        await fermerTout(barre, rail);
        redimensionner(pid, 1280, 800);
        await pause(1000);
      }
    }
  } finally {
    await arreter();
  }
}

try {
  for (const t of TAILLES) await tousLesMenus(t);
  await redimensionnerMenuOuvert();
} catch (e) {
  console.error(`ECHEC : ${e.message}`);
  echecs += 1;
} finally {
  serveur.close();
}
console.log(echecs ? `\n${echecs} échec(s)` : "\ntout tient");
process.exit(echecs ? 1 : 0);
