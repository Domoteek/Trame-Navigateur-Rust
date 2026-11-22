/**
 * Éprouve la reprise de lecture après un rechargement (F5 d'une vraie page).
 *
 * Le mecanisme vit dans `ui/raccourcis.js` : la position de la video principale est notee dans
 * `sessionStorage`, et sur YouTube l'adresse recoit `t=…s` avant le rechargement. Ce banc
 * d'essai mesure ce que fait la **vraie application**, pas ce que le code a l'intention de faire :
 *
 *   1. Trame d'essai, profil jetable, fenetre cachee (`TRAME_CACHEE`), port CDP ouvert par
 *      `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` ;
 *   2. une vraie page chargee (`TRAME_NAV`) ;
 *   3. la video est placee a une position connue (95 s par defaut) ;
 *   4. un **vrai** F5 est envoye par CDP (`Input.dispatchKeyEvent`) — c'est `raccourcis.js` qui
 *      le recoit et qui demande le rechargement a l'hote, comme sous un doigt humain ;
 *   5. on relit la position apres le rechargement, l'adresse, la note de reprise.
 *
 * Usage :
 *   node outils/eprouver-reprise.mjs                                  # YouTube, 95 s
 *   node outils/eprouver-reprise.mjs --position 40 --boucles 3
 *   node outils/eprouver-reprise.mjs --url https://…/BigBuckBunny.mp4  # hors YouTube
 *   node outils/eprouver-reprise.mjs --env TRAME_SANS_INTERCEPTION=1   # mecanisme coupe
 *
 * Sans dependance : CDP parle par le WebSocket natif de Node (>= 22).
 */
import { spawn } from "node:child_process";
import http from "node:http";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const EXE = path.join(RACINE, "target", "release", "trame.exe");
const PORT = 9333;

const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
const URL_ESSAI = option("url", "https://www.youtube.com/watch?v=kLcTqfXvlY8");
const POSITION = Number(option("position", 95));
const BOUCLES = Number(option("boucles", 1));
// `--lecture` : la video joue au moment du F5 (le cas de tous les jours) ; sinon elle est en pause
// avant le rechargement, ce qui donne une mesure exacte (rien n'avance pendant le rechargement).
const EN_LECTURE = args.includes("--lecture");
// `--env CLE=valeur` (repetable) : compare avec un mecanisme coupe (sonde de diagnostic).
const ENV_SUPPLEMENTAIRE = {};
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === "--env" && args[i + 1]) {
    const [cle, ...reste] = args[i + 1].split("=");
    ENV_SUPPLEMENTAIRE[cle] = reste.join("=") || "1";
    i += 1;
  }
}
const surYouTube = (() => {
  try {
    return /(^|\.)youtube\.com$/.test(new URL(URL_ESSAI).hostname);
  } catch {
    return false;
  }
})();
// `--local` : la video est posee dans une page servie localement (le pont `window.ipc` n'existe
// pas en `file://`). Utile pour éprouver le mecanisme de note hors YouTube (MP4 direct, par ex.).
const LOCAL = args.includes("--local");
const PORT_LOCAL = 8123;
// Duree minimale de la video de contenu : une pub dure quelques dizaines de secondes.
const DUREE_MIN = Number(option("duree-min", surYouTube ? 120 : 30));
const CIBLE = option("cible", LOCAL ? `127.0.0.1:${PORT_LOCAL}` : surYouTube ? "youtube.com/watch" : new URL(URL_ESSAI).host);

/** Une page locale minimale autour de la video : DOM ordinaire, donc mesurable. */
function servirPageLocale() {
  const html = `<!doctype html><meta charset="utf-8">
<style>html,body{margin:0;height:100%;background:#000}video{width:100%;height:100%}</style>
<video id="v" controls muted playsinline src="${URL_ESSAI}"></video>`;
  const serveur = http.createServer((_, rep) => {
    rep.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    rep.end(html);
  });
  return new Promise((r) => serveur.listen(PORT_LOCAL, "127.0.0.1", () => r(serveur)));
}

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/** Un client CDP minimal : une promesse par message, par identifiant. */
class Cdp {
  constructor(ws) {
    this.ws = ws;
    this.id = 0;
    this.attente = new Map();
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(e.data);
      if (m.id && this.attente.has(m.id)) {
        const { resoudre, rejeter } = this.attente.get(m.id);
        this.attente.delete(m.id);
        if (m.error) rejeter(new Error(JSON.stringify(m.error)));
        else resoudre(m.result);
      }
    });
  }
  envoyer(method, params = {}) {
    this.id += 1;
    const id = this.id;
    return new Promise((resoudre, rejeter) => {
      this.attente.set(id, { resoudre, rejeter });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.attente.has(id)) {
          this.attente.delete(id);
          rejeter(new Error(`CDP ${method} : pas de reponse`));
        }
      }, 20000);
    });
  }
  /** Evalue une expression dans la page et rend sa valeur. */
  async evaluer(expression) {
    const r = await this.envoyer("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) {
      throw new Error(`page : ${r.exceptionDetails.text} ${r.exceptionDetails.exception?.description || ""}`);
    }
    return r.result.value;
  }
  /** Attend qu'une expression devienne vraie. */
  async attendre(expression, { delai = 60000, pas = 500, quoi = expression } = {}) {
    const fin = Date.now() + delai;
    let derniere = "";
    while (Date.now() < fin) {
      try {
        if (await this.evaluer(expression)) return true;
      } catch (e) {
        derniere = e.message;
      }
      await pause(pas);
    }
    throw new Error(`attente expiree : ${quoi}${derniere ? ` (${derniere})` : ""}`);
  }
}

async function cibles() {
  try {
    const r = await fetch(`http://127.0.0.1:${PORT}/json/list`);
    return await r.json();
  } catch {
    return [];
  }
}

async function attendreCible(contient, delai = 45000) {
  const fin = Date.now() + delai;
  while (Date.now() < fin) {
    const c = (await cibles()).find((t) => t.type === "page" && t.url.includes(contient));
    if (c) return c;
    await pause(400);
  }
  throw new Error(`aucune cible CDP ne contient ${contient} (Trame a-t-elle demarre ?)`);
}

function connecter(url) {
  return new Promise((resoudre, rejeter) => {
    const ws = new WebSocket(url);
    ws.addEventListener("open", () => resoudre(new Cdp(ws)));
    ws.addEventListener("error", () => rejeter(new Error(`WebSocket refuse : ${url}`)));
  });
}

/** La video principale : la plus grande, duree longue (une pub dure quelques dizaines de s). */
const LECTEUR = `(() => {
  let v = null, aire = 0;
  for (const x of document.querySelectorAll('video')) {
    if (!Number.isFinite(x.duration) || x.duration < 5) continue;
    const r = x.getBoundingClientRect();
    const a = r.width * r.height;
    if (a > aire) { aire = a; v = x; }
  }
  const pub = !!document.querySelector('.ad-showing, .ad-interrupting');
  return v ? { t: Math.round(v.currentTime * 10) / 10, d: Math.round(v.duration), paused: v.paused, pub } : { t: null, d: null, paused: null, pub };
})()`;

/** Le clic de depart : sans geste utilisateur, un lecteur refuse de jouer. */
async function cliquer(cdp) {
  for (const type of ["mousePressed", "mouseReleased"]) {
    await cdp.envoyer("Input.dispatchMouseEvent", { type, x: 440, y: 300, button: "left", clickCount: 1 });
  }
  await pause(800);
}

/** Vrai F5 : `raccourcis.js` le recoit, l'hote recharge la webview. */
async function envoyerF5(cdp) {
  for (const type of ["rawKeyDown", "keyUp"]) {
    await cdp.envoyer("Input.dispatchKeyEvent", {
      type,
      key: "F5",
      code: "F5",
      windowsVirtualKeyCode: 116,
      nativeVirtualKeyCode: 116,
    });
  }
}

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  const serveur = LOCAL ? await servirPageLocale() : null;
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-reprise-"));
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      TRAME_CACHEE: "1",
      TRAME_NAV: LOCAL ? `http://127.0.0.1:${PORT_LOCAL}/` : URL_ESSAI,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
      ...ENV_SUPPLEMENTAIRE,
    },
    stdio: "ignore",
  });
  let cdp = null;
  let ok = true;
  try {
    console.log(`page     : ${URL_ESSAI}`);
    console.log(`cible    : ${CIBLE}`);
    console.log(`position : ${POSITION} s, ${BOUCLES} rechargement(s)`);
    if (Object.keys(ENV_SUPPLEMENTAIRE).length) console.log(`env      : ${JSON.stringify(ENV_SUPPLEMENTAIRE)}`);
    cdp = await connecter((await attendreCible(CIBLE)).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");

    const pret = `(() => { const v = ${LECTEUR}; return v.d > ${DUREE_MIN} && !v.pub; })()`;
    try {
      await cdp.attendre(pret, { delai: 90000, quoi: "la video principale" });
    } catch (e) {
      // Un echec d'attente ne dit pas pourquoi : on demande l'etat de la page.
      const etat = await cdp.evaluer(
        `(() => ({ pret: document.readyState, videos: document.querySelectorAll('video').length, detail: ${LECTEUR} }))()`
      );
      console.error(`etat de la page : ${JSON.stringify(etat)}`);
      throw e;
    }
    await cliquer(cdp);
    await cdp.attendre(pret, { delai: 60000, quoi: "la video principale apres le clic" });

    for (let tour = 1; tour <= BOUCLES; tour += 1) {
      // Une publicite peut demarrer entre l'attente et le placement : on remet jusqu'a ce que
      // la video de contenu soit bien la.
      const placer = () =>
        cdp.evaluer(`(() => {
          const v = [...document.querySelectorAll('video')].find((x) => Number.isFinite(x.duration) && x.duration > ${DUREE_MIN});
          if (!v) return false;
          v.currentTime = ${POSITION};
          if (${EN_LECTURE}) v.play().catch(() => {});
          else v.pause();
          return true;
        })()`);
      const finPlacement = Date.now() + 60000;
      while (!(await placer())) {
        if (Date.now() > finPlacement) throw new Error("aucune video de contenu a placer (publicite ?)");
        await pause(700);
      }
      await pause(1500);
      const avant = await cdp.evaluer(LECTEUR);
      const note = await cdp.evaluer("sessionStorage.getItem('__trame_reprise')");
      const origine = await cdp.evaluer("performance.timeOrigin");
      console.log(`\n--- rechargement ${tour} ---`);
      console.log(`avant F5  : ${avant.t} s (${avant.paused ? "en pause" : "en lecture"}, duree ${avant.d} s)`);
      await envoyerF5(cdp);
      await cdp.attendre(`performance.timeOrigin !== ${origine}`, { delai: 30000, quoi: "le rechargement" });
      const adresse = await cdp.evaluer("location.href");
      const noteApres = await cdp.evaluer("sessionStorage.getItem('__trame_reprise')");
      await cdp.attendre(pret, { delai: 90000, quoi: "la video principale apres rechargement" });
      await pause(2000);
      const apres = await cdp.evaluer(LECTEUR);
      // Si le lecteur ne joue pas de lui-meme, on relance : une reprise peut n'apparaitre qu'au
      // demarrage de la lecture (le lecteur de YouTube place sa position a ce moment-la).
      let relance = null;
      if (apres.paused) {
        await cliquer(cdp);
        await pause(2500);
        relance = await cdp.evaluer(LECTEUR);
      }
      const ecart = Math.round((apres.t - POSITION) * 10) / 10;
      console.log(`note      : ${note === null ? "(absente)" : note}`);
      console.log(`adresse   : ${adresse}`);
      console.log(`apres F5  : ${apres.t} s (${apres.paused ? "en pause" : "en lecture"})  -> ecart ${ecart > 0 ? "+" : ""}${ecart} s`);
      console.log(`note apres : ${noteApres === null ? "consommee" : noteApres}`);
      if (relance) console.log(`apres relance de la lecture : ${relance.t} s (${relance.paused ? "en pause" : "en lecture"})`);
      // Le verdict porte sur la position reposee, pas sur la lecture relancee : un lecteur qui
      // avance apres coup est normal, un lecteur qui se place ailleurs ne l'est pas.
      const tolerance = EN_LECTURE ? 6 : 1.5;
      if (apres.t === null || Math.abs(apres.t - POSITION) > tolerance) ok = false;
    }
    console.log(ok ? "\nRECAP : la reprise tombe juste" : "\nRECAP : la reprise NE tombe PAS juste");
    return ok;
  } finally {
    try {
      cdp?.ws.close();
    } catch {
      /* deja ferme */
    }
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
    serveur?.close();
    await pause(1500);
    try {
      fs.rmSync(profil, { recursive: true, force: true, maxRetries: 12, retryDelay: 250 });
    } catch (e) {
      console.warn(`(profil de test garde : ${profil} — ${e.code || e.message})`);
    }
  }
}

principale()
  .then((ok) => process.exit(ok ? 0 : 1))
  .catch((e) => {
    console.error(`ECHEC : ${e.message}`);
    process.exit(1);
  });
