/**
 * Éprouve le rangement des groupes du rail dans la **vraie application**.
 *
 * Le geste vit dans `ui/chrome.js` (« ranger les groupes ») et l'ordre dans `session.fils`
 * (`model::Session::deplacer_fil`, message `filDeplacer`). Ce banc d'essai mesure ce que fait le
 * binaire, pas ce que le code a l'intention de faire :
 *
 *   1. Trame d'essai, profil jetable, fenêtre cachée (`TRAME_CACHEE`), port CDP ouvert par
 *      `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` ;
 *   2. une session de trois groupes écrite à la main (des pages internes : aucun réseau) ;
 *   3. un **vrai** glissement de souris envoyé par CDP (`Input.dispatchMouseEvent`) sur la tête
 *      du troisième groupe, déposé au-dessus du premier — c'est `chrome.js` qui le reçoit ;
 *   4. on relit l'ordre des cartes du rail, puis `session.json` pour la persistance.
 *
 * Usage :
 *   node outils/eprouver-rangement.mjs
 *   node outils/eprouver-rangement.mjs --voir    (affiche la fenêtre de Trame pendant l'essai)
 *
 * Sans dépendance : CDP parle par le WebSocket natif de Node (>= 22).
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const RACINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (nom, defaut) => {
  const i = args.indexOf(`--${nom}`);
  return i >= 0 && args[i + 1] ? args[i + 1] : defaut;
};
// `--exe` : essayer un autre binaire (une construction `cargo build` de debug, par exemple,
// quand la construction de release est occupee par une autre session).
const EXE = path.resolve(RACINE, option("exe", path.join("target", "release", "trame.exe")));
const PORT = 9335;
const VOIR = args.includes("--voir");
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/** Trois groupes comme dans le rail de Clément, pour un glissement mesurable. */
const SESSION = {
  onglets: [
    { id: 1, url: "trame://localhost/reglages.html", titre: "Réglages un", fil: 1, cree_le: 1_700_000_000_000 },
    { id: 2, url: "trame://localhost/reglages.html", titre: "Réglages deux", fil: 2, cree_le: 1_700_000_001_000 },
    { id: 3, url: "trame://localhost/reglages.html", titre: "Réglages trois", fil: 3, cree_le: 1_700_000_002_000 },
  ],
  fils: [
    { id: 1, nom: "Login Home", cree_le: 1_700_000_000_000 },
    { id: 2, nom: "Amazon Dvd", cree_le: 1_700_000_001_000 },
    { id: 3, nom: "Automated Day", cree_le: 1_700_000_002_000 },
  ],
  actif: 1,
  prochain_onglet: 4,
  prochain_fil: 4,
};

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
  async evaluer(expression) {
    const r = await this.envoyer("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (r.exceptionDetails) {
      throw new Error(`page : ${r.exceptionDetails.text} ${r.exceptionDetails.exception?.description || ""}`);
    }
    return r.result.value;
  }
  async attendre(expression, { delai = 30000, pas = 300, quoi = expression } = {}) {
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

/** L'ordre des cartes du rail, tel que le DOM le montre. */
const ORDRE = `JSON.stringify([...document.querySelectorAll("#fils .fil")].map((f) => Number(f.dataset.fil)))`;

/** Un vrai geste de souris : appui sur la tete du groupe `id`, depot a `y` dans la liste. */
async function glisser(cdp, { depart, arrivee }) {
  const commun = { button: "left", buttons: 1 };
  await cdp.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x: depart.x, y: depart.y, buttons: 0 });
  await pause(120);
  await cdp.envoyer("Input.dispatchMouseEvent", { type: "mousePressed", x: depart.x, y: depart.y, clickCount: 1, ...commun });
  await pause(120);
  // Le geste est reconnu au-dela de 6 px, et chaque pas doit etre un vrai mouvement.
  for (let i = 1; i <= 8; i += 1) {
    const t = i / 8;
    const x = Math.round(depart.x + (arrivee.x - depart.x) * t);
    const y = Math.round(depart.y + (arrivee.y - depart.y) * t);
    await cdp.envoyer("Input.dispatchMouseEvent", { type: "mouseMoved", x, y, ...commun });
    await pause(45);
  }
  await pause(200);
  await cdp.envoyer("Input.dispatchMouseEvent", { type: "mouseReleased", x: arrivee.x, y: arrivee.y, clickCount: 1, button: "left", buttons: 0 });
}

async function principale() {
  if (!fs.existsSync(EXE)) {
    console.error(`Binaire absent : ${EXE}\nConstruire d'abord : cargo build --release`);
    process.exit(2);
  }
  const profil = fs.mkdtempSync(path.join(os.tmpdir(), "trame-rangement-"));
  fs.writeFileSync(path.join(profil, "session.json"), JSON.stringify(SESSION, null, 2), "utf8");
  const enfant = spawn(EXE, [], {
    cwd: RACINE,
    env: {
      ...process.env,
      TRAME_PROFIL: profil,
      // `TRAME_CACHEE` vaut « cachee » des qu'elle existe, meme vide : on la retire pour voir.
      ...(VOIR ? {} : { TRAME_CACHEE: "1" }),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}`,
    },
    stdio: "ignore",
  });
  let cdp = null;
  let ok = true;
  try {
    cdp = await connecter((await attendreCible("surface=rail")).webSocketDebuggerUrl);
    await cdp.envoyer("Runtime.enable");
    await cdp.attendre(`document.querySelectorAll("#fils .fil").length === 3`, {
      delai: 30000,
      quoi: "les trois groupes du rail",
    });

    const avant = JSON.parse(await cdp.evaluer(ORDRE));
    console.log(`rail     : ${JSON.stringify(avant)}  ${avant.join() === "1,2,3" ? "(ordre de la session)" : "(INATTENDU)"}`);
    if (avant.join() !== "1,2,3") ok = false;

    // Les coordonnees sont celles du rail : le glissement part de la tete du troisieme groupe et
    // se depose dans la moitie haute de la premiere carte.
    const pts = await cdp.evaluer(`(() => {
      const tete = document.querySelector('.fil[data-fil="3"] .fil-tete').getBoundingClientRect();
      const un = document.querySelector('.fil[data-fil="1"]').getBoundingClientRect();
      return { depart: { x: Math.round(tete.left + 60), y: Math.round(tete.top + tete.height / 2) },
               arrivee: { x: Math.round(un.left + 60), y: Math.round(un.top + 5) } };
    })()`);
    console.log(`geste    : tete du groupe 3 (${pts.depart.x},${pts.depart.y}) -> dessus du groupe 1 (${pts.arrivee.x},${pts.arrivee.y})`);

    await glisser(cdp, pts);
    await cdp.attendre(`JSON.parse(${ORDRE}).join() === "3,1,2"`, {
      delai: 8000,
      quoi: "le rail range 3 devant 1",
    });
    const apres = JSON.parse(await cdp.evaluer(ORDRE));
    console.log(`rail     : ${JSON.stringify(apres)}`);
    if (apres.join() !== "3,1,2") ok = false;

    // Rien ne doit avoir ete replie au passage : le clic qui suit un glissement est avale.
    const replie = await cdp.evaluer(`!!document.querySelector('.fil[data-fil="3"].replie')`);
    console.log(`replie   : ${replie ? "OUI (le clic a replie le groupe)" : "non"}`);
    if (replie) ok = false;

    // Persistance : la session est ecrite au plus toutes les 8 s.
    const fichier = path.join(profil, "session.json");
    const fin = Date.now() + 15000;
    let ordreDisque = [];
    while (Date.now() < fin) {
      try {
        ordreDisque = JSON.parse(fs.readFileSync(fichier, "utf8")).fils.map((f) => f.id);
      } catch (e) {
        ordreDisque = [];
      }
      if (ordreDisque.join() === "3,1,2") break;
      await pause(500);
    }
    console.log(`session  : ${JSON.stringify(ordreDisque)}`);
    if (ordreDisque.join() !== "3,1,2") ok = false;

    console.log(ok ? "\nRECAP : le rangement tient dans la vraie application" : "\nRECAP : le rangement NE tient PAS");
    return ok;
  } finally {
    try {
      cdp?.ws.close();
    } catch {
      /* deja ferme */
    }
    spawn("taskkill", ["/PID", String(enfant.pid), "/T", "/F"], { stdio: "ignore" });
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
