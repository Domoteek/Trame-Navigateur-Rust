#!/usr/bin/env node
/**
 * Banc d'essai du CLIC SUR L'ICÔNE d'une extension sans fenêtre d'action (Joko), sur le vrai
 * binaire, en fenêtre cachée (ne vole pas le focus).
 *
 * Constat de Clément (2026-11-28) : Joko épinglée répondait « Cette extension n'a pas de fenêtre
 * d'action ». Joko n'a pas de `default_popup` : elle écoute `chrome.action.onClicked`, que WebView2
 * ne déclenche jamais. Trame charge la page relais `trame-relais/action.html`, l'adaptateur v2 du
 * service worker appelle les écouteurs avec l'onglet actif.
 *
 * Vérifie : le service worker porte l'adaptateur v2, le clic est transmis, et Joko réagit dans la
 * page (nouveaux éléments ou cadre Joko dans le DOM de la page marchande).
 *
 * Usage : node outils/eprouver-clic-action.mjs   (réseau requis ; Joko copiée depuis le profil
 * de Clément, ou depuis TRAME_EXT_SOURCE)
 */
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, cpSync, existsSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { attendreCible, cibles, connecter, pause } from "./cdp.mjs";

const ID = "jigflhhckdjdefdjmodlkomnmdonfbbn"; // Joko
const PORT_CDP = Number(process.env.TRAME_PORT_ESSAI || 19495);
const BINAIRE = process.env.TRAME_BINAIRE || join(process.cwd(), "target", "release", "trame.exe");
const SOURCE = process.env.TRAME_EXT_SOURCE || join(process.env.LOCALAPPDATA, "Trame", "extensions-chrome", ID);
const PAGE = "https://www.fnac.com/";
const profil = mkdtempSync(join(tmpdir(), "trame-clic-action-"));
cpSync(SOURCE, join(profil, "extensions-chrome", ID), { recursive: true });

let echecs = 0;
const ok = (c, quoi) => {
  console.log(`  ${c ? "ok  " : "ECHEC"} ${quoi}`);
  if (!c) echecs += 1;
};

const enfant = spawn(BINAIRE, [], {
  env: {
    ...process.env,
    TRAME_PROFIL: profil,
    TRAME_CACHEE: "1",
    TRAME_NAV: PAGE,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT_CDP} --remote-debugging-address=127.0.0.1`,
  },
  stdio: "ignore",
});

// Ce que Joko pose dans la page : tout élément dont l'id, la classe, l'adresse ou la balise
// parle de Joko, plus les racines d'ombre (shadow DOM) posées sur <html>/<body>.
const traces = `(() => {
  const tout = [...document.querySelectorAll("*")];
  const joko = tout.filter((e) => /joko/i.test((e.id || "") + " " + (e.className && e.className.baseVal === undefined ? e.className : "") + " " + (e.getAttribute("src") || "") + " " + e.tagName));
  return { joko: joko.length, ombres: tout.filter((e) => e.shadowRoot).length, cadres: document.querySelectorAll("iframe").length };
})()`;

try {
  const cible = await attendreCible(PORT_CDP, "fnac.com");
  await pause(8000); // le service worker adapté est rechargé une fois au démarrage
  const sw = (await cibles(PORT_CDP)).find((t) => t.type === "service_worker" && t.url.includes(ID));
  ok(!!sw, "service worker de Joko lancé");
  if (sw) {
    const c = await connecter(sw.webSocketDebuggerUrl);
    ok(await c.evaluer(`!!(chrome.action.onClicked.__trame)`), "adaptateur v2 actif dans le service worker");
    c.fermer();
  }
  const page = await connecter(cible.webSocketDebuggerUrl);
  const avant = await page.evaluer(traces);
  console.log(`      avant le clic : ${JSON.stringify(avant)}`);

  const barre = await connecter((await attendreCible(PORT_CDP, "surface=barre")).webSocketDebuggerUrl);
  await barre.evaluer(`window.ipc.postMessage(JSON.stringify({ m: "chromePopup", id: "${ID}" })), true`);
  barre.fermer();

  // Joko réagit soit dans la page (connecté), soit en menant l'onglet à sa connexion
  // (`app.joko.com/auth/sign-in`, mesuré le 2026-11-28 sur un profil neuf) — comme dans Chrome.
  let apres = avant;
  let connexion = false;
  const fin = Date.now() + 15000;
  while (Date.now() < fin) {
    await pause(500);
    connexion = (await cibles(PORT_CDP)).some((t) => t.type === "page" && /joko\.com/.test(t.url));
    if (connexion) break;
    apres = await page.evaluer(traces).catch(() => apres);
    if (apres.joko > avant.joko || apres.ombres > avant.ombres || apres.cadres > avant.cadres) break;
  }
  console.log(`      après le clic : ${JSON.stringify(apres)}${connexion ? " — onglet mené à joko.com" : ""}`);
  ok(connexion || apres.joko > avant.joko || apres.ombres > avant.ombres || apres.cadres > avant.cadres, "Joko réagit au clic");
  page.fermer();
} catch (e) {
  ok(false, e.message);
}

const journal = existsSync(join(profil, "trame.log")) ? readFileSync(join(profil, "trame.log"), "utf8") : "";
ok(/clic d'action transmis/.test(journal), "clic transmis (journal)");
ok(!/n'a pas de fenêtre d'action/.test(journal), "pas de « pas de fenêtre d'action »");
for (const l of journal.split("\n").filter((l) => /clic d'action|adaptateur|relais|recharg/i.test(l)).slice(-8)) {
  console.log(`      ${l.trim()}`);
}
try {
  execFileSync("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
} catch {
  /* deja parti */
}
await pause(1500);
try {
  rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
} catch {
  console.log(`      (profil gardé : ${profil})`);
}
console.log(echecs ? `\n${echecs} échec(s)` : "\nTout passe.");
process.exit(echecs ? 1 : 0);
